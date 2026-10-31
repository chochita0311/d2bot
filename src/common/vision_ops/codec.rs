use super::VisionError;
use crate::common::frame::{BgrFrame, GrayFrame, Image, Size};
use image::{DynamicImage, ImageEncoder};
use std::path::Path;

#[derive(Debug)]
pub enum DecodedImage {
    Gray(GrayFrame),
    Bgr(BgrFrame),
    Bgra(Image<u8, 4>),
}

fn convert(image: DynamicImage) -> Result<DecodedImage, VisionError> {
    let size = Size::new(image.width() as usize, image.height() as usize)?;
    match image {
        DynamicImage::ImageLuma8(image) => Ok(DecodedImage::Gray(GrayFrame::from_vec(
            size,
            image.into_raw(),
        )?)),
        DynamicImage::ImageRgb8(image) => {
            let mut pixels = image.into_raw();
            for pixel in pixels.as_chunks_mut::<3>().0 {
                pixel.swap(0, 2);
            }
            Ok(DecodedImage::Bgr(BgrFrame::from_vec(size, pixels)?))
        }
        DynamicImage::ImageRgba8(image) => {
            let mut pixels = image.into_raw();
            for pixel in pixels.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
            Ok(DecodedImage::Bgra(Image::from_vec(size, pixels)?))
        }
        _ => Err(VisionError::Invalid("unsupported image channel/depth mode")),
    }
}

struct DecodedAsset {
    format: Option<image::ImageFormat>,
    gamma: Option<f64>,
    image: DecodedImage,
}

fn load_with_format(path: impl AsRef<Path>) -> Result<DecodedAsset, VisionError> {
    use std::io::Read;
    let reader = image::ImageReader::open(path)?.with_guessed_format()?;
    let format = reader.format();
    let mut gamma = None;
    let image = if format == Some(image::ImageFormat::Jpeg) {
        let mut bytes = Vec::new();
        reader.into_inner().read_to_end(&mut bytes)?;
        decode_jpeg(&bytes)?
    } else if format == Some(image::ImageFormat::Png) {
        let (metadata, image) = decode_png(reader.into_inner())?;
        gamma = metadata;
        image
    } else {
        convert(reader.decode()?)?
    };
    Ok(DecodedAsset {
        format,
        gamma,
        image,
    })
}

pub fn load_unchanged(path: impl AsRef<Path>) -> Result<DecodedImage, VisionError> {
    load_with_format(path).map(|asset| asset.image)
}

pub fn decode_color(bytes: &[u8]) -> Result<BgrFrame, VisionError> {
    color(match image::guess_format(bytes)? {
        image::ImageFormat::Jpeg => decode_jpeg(bytes)?,
        image::ImageFormat::Png => decode_png(std::io::Cursor::new(bytes))?.1,
        _ => convert(image::load_from_memory(bytes)?)?,
    })
}

fn decode_png<R: std::io::BufRead + std::io::Seek>(
    input: R,
) -> Result<(Option<f64>, DecodedImage), VisionError> {
    let mut decoder = png::Decoder::new_with_limits(
        input,
        png::Limits {
            bytes: 512 * 1024 * 1024,
        },
    );
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info()?;
    // image0.25's gamma_value reads an obsolete png Info field. Read effective
    // gAMA/sRGB metadata from png0.18's gamma() together with the same pixel stream.
    let gamma = reader
        .info()
        .gamma()
        .map(|value| f64::from(value.into_scaled()) / 100000.0);
    let (color, depth) = reader.output_color_type();
    if depth != png::BitDepth::Eight
        || !matches!(
            color,
            png::ColorType::Grayscale | png::ColorType::Rgb | png::ColorType::Rgba
        )
    {
        return Err(VisionError::Invalid("unsupported PNG channel/depth mode"));
    }
    let len = reader
        .output_buffer_size()
        .ok_or(VisionError::Invalid("PNG byte size overflow"))?;
    if len > 512 * 1024 * 1024 {
        return Err(VisionError::Invalid("decoded image exceeds 512 MiB"));
    }
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(len)
        .map_err(crate::common::frame::FrameError::Allocation)?;
    pixels.resize(len, 0);
    let info = reader.next_frame(&mut pixels)?;
    pixels.truncate(info.buffer_size());
    let size = Size::new(info.width as usize, info.height as usize)?;
    let image = match info.color_type {
        png::ColorType::Grayscale => DecodedImage::Gray(GrayFrame::from_vec(size, pixels)?),
        png::ColorType::Rgb => {
            for pixel in pixels.as_chunks_mut::<3>().0 {
                pixel.swap(0, 2);
            }
            DecodedImage::Bgr(BgrFrame::from_vec(size, pixels)?)
        }
        png::ColorType::Rgba => {
            for pixel in pixels.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
            DecodedImage::Bgra(Image::from_vec(size, pixels)?)
        }
        _ => return Err(VisionError::Invalid("unsupported PNG channel/depth mode")),
    };
    Ok((gamma, image))
}

fn decode_jpeg(bytes: &[u8]) -> Result<DecodedImage, VisionError> {
    let mut decoder = jpeg_decoder::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_max_decoding_buffer_size(512 * 1024 * 1024);
    decoder.read_info()?;
    let info = decoder
        .info()
        .ok_or(VisionError::Invalid("JPEG dimensions unavailable"))?;
    let size = Size::new(info.width as usize, info.height as usize)?;
    match info.pixel_format {
        jpeg_decoder::PixelFormat::L8 => Ok(DecodedImage::Gray(GrayFrame::from_vec(
            size,
            decoder.decode()?,
        )?)),
        jpeg_decoder::PixelFormat::RGB24 => {
            let mut pixels = decoder.decode()?;
            for pixel in pixels.as_chunks_mut::<3>().0 {
                pixel.swap(0, 2);
            }
            Ok(DecodedImage::Bgr(BgrFrame::from_vec(size, pixels)?))
        }
        _ => Err(VisionError::Invalid("unsupported JPEG channel/depth mode")),
    }
}

pub fn color(image: DecodedImage) -> Result<BgrFrame, VisionError> {
    match image {
        DecodedImage::Bgr(image) => Ok(image),
        DecodedImage::Gray(image) => {
            let mut output = BgrFrame::zeros(image.size())?;
            for (pixel, value) in output
                .pixels_mut()
                .as_chunks_mut::<3>()
                .0
                .iter_mut()
                .zip(image.bytes())
            {
                pixel.fill(*value);
            }
            Ok(output)
        }
        DecodedImage::Bgra(image) => {
            let mut output = BgrFrame::zeros(image.size())?;
            for (pixel, value) in output
                .pixels_mut()
                .as_chunks_mut::<3>()
                .0
                .iter_mut()
                .zip(image.bytes().as_chunks::<4>().0)
            {
                pixel.copy_from_slice(&value[..3]);
            }
            Ok(output)
        }
    }
}

pub fn load_color(path: impl AsRef<Path>) -> Result<BgrFrame, VisionError> {
    color(load_unchanged(path)?)
}

pub fn load_gray(path: impl AsRef<Path>) -> Result<GrayFrame, VisionError> {
    let asset = load_with_format(path)?;
    match asset.image {
        DecodedImage::Gray(image) => Ok(image),
        image if asset.format == Some(image::ImageFormat::Png) => {
            let image = color(image)?;
            let mut gray = GrayFrame::zeros(image.size())?;
            let gamma_tables = asset
                .gamma
                .filter(|gamma| *gamma < 0.95 || *gamma > 1.05)
                .map(png_gamma_tables)
                .transpose()?;
            // PNG grayscale decoding truncates the weighted sum. Runtime cvtColor
            // rounds instead, so these are deliberately separate operations.
            for (output, pixel) in gray
                .pixels_mut()
                .iter_mut()
                .zip(image.bytes().as_chunks::<3>().0)
            {
                *output = if let Some((linear, encoded)) = &gamma_tables {
                    if pixel[0] == pixel[1] && pixel[1] == pixel[2] {
                        pixel[0]
                    } else {
                        let value = (u32::from(linear[pixel[0] as usize]) * 3737
                            + u32::from(linear[pixel[1] as usize]) * 19234
                            + u32::from(linear[pixel[2] as usize]) * 9797
                            + 16384)
                            >> 15;
                        encoded[value as usize]
                    }
                } else {
                    ((u32::from(pixel[0]) * 3737
                        + u32::from(pixel[1]) * 19234
                        + u32::from(pixel[2]) * 9797)
                        >> 15) as u8
                };
            }
            Ok(gray)
        }
        image => super::color::gray(color(image)?.view()),
    }
}

fn png_gamma_tables(gamma: f64) -> Result<([u8; 256], [u8; 256]), VisionError> {
    if !gamma.is_finite() || gamma <= 0.0 {
        return Err(VisionError::Invalid("positive finite PNG gamma required"));
    }
    let (mut linear, mut encoded) = ([0; 256], [0; 256]);
    for value in 0..256 {
        let input = value as f64 / 255.0;
        linear[value] = (input.powf(1.0 / gamma) * 255.0 + 0.5)
            .floor()
            .clamp(0.0, 255.0) as u8;
        encoded[value] = (input.powf(gamma) * 255.0 + 0.5).floor().clamp(0.0, 255.0) as u8;
    }
    Ok((linear, encoded))
}

pub fn file_sha256(path: impl AsRef<Path>) -> Result<String, VisionError> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 32768];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub fn encode_png(frame: &BgrFrame) -> Result<Vec<u8>, VisionError> {
    let mut rgb = frame.bytes().to_vec();
    for pixel in rgb.as_chunks_mut::<3>().0 {
        pixel.swap(0, 2);
    }
    let mut output = Vec::new();
    let width = u32::try_from(frame.size().width())
        .map_err(|_| VisionError::Invalid("PNG dimensions exceed u32"))?;
    let height = u32::try_from(frame.size().height())
        .map_err(|_| VisionError::Invalid("PNG dimensions exceed u32"))?;
    image::codecs::png::PngEncoder::new(&mut output).write_image(
        &rgb,
        width,
        height,
        image::ExtendedColorType::Rgb8,
    )?;
    Ok(output)
}

pub fn save_png(path: impl AsRef<Path>, frame: &BgrFrame) -> Result<(), VisionError> {
    std::fs::write(path, encode_png(frame)?)?;
    Ok(())
}
pub fn encode_gray_png(frame: &GrayFrame) -> Result<Vec<u8>, VisionError> {
    let width = u32::try_from(frame.size().width())
        .map_err(|_| VisionError::Invalid("PNG dimensions exceed u32"))?;
    let height = u32::try_from(frame.size().height())
        .map_err(|_| VisionError::Invalid("PNG dimensions exceed u32"))?;
    let mut output = Vec::new();
    image::codecs::png::PngEncoder::new(&mut output).write_image(
        frame.pixels(),
        width,
        height,
        image::ExtendedColorType::L8,
    )?;
    Ok(output)
}

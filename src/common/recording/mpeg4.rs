//! 필요한 MPEG-4 Visual I/P 부호화와 로컬 참조 복원. 게임 판단 경로와 독립이다.
mod bits;
mod coding;
mod motion;
mod picture;
mod syntax;
mod tables;
mod transform;
use self::{
    bits::Bits,
    coding::{Macroblock, PictureCoding, encode_picture},
    picture::Picture,
};
use crate::common::frame::{BgrFrame, Size};
#[cfg(test)]
use coding::position;
use std::io;
pub const DEFAULT_QUANTIZER: u8 = 4;
const KEYFRAME_INTERVAL: u64 = 16;
pub struct EncodedFrame<'a> {
    pub bytes: &'a [u8],
    pub keyframe: bool,
}
pub struct Mpeg4Encoder {
    size: Size,
    q: u8,
    frames: u64,
    bits: Bits,
    reference: Option<Picture>,
    previous_input: Option<Picture>,
    input_scratch: Option<Picture>,
    output_scratch: Option<Picture>,
    blocks: Vec<Macroblock>,
    compute: Option<crate::common::compute::ComputeLease>,
    failed: bool,
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

#[cfg(test)]
mod efficiency_profile {
    use super::*;
    #[test]
    #[ignore = "explicit input-free performance measurement"]
    fn efficiency_profile_codec_stages() -> Result<(), Box<dyn std::error::Error>> {
        use std::{hint::black_box, path::Path, time::Instant};
        let frame = crate::common::vision_ops::codec::load_color(Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/navigation/north-step-before.png",
        ))?;
        let picture = Picture::from_bgr(&frame)?;
        let mut encoder = Mpeg4Encoder::new(frame.size(), 4)?;
        encoder.encode(&frame)?;
        let reference = encoder.reference.as_ref().ok_or("reference missing")?;
        let samples = picture.planes[0].block(960, 560).map(f64::from);
        let coefficients = transform::quantize(&samples, 4, true, true).1;
        let global = motion::global(&picture, reference, frame.size());
        let mut bits = Bits::new(16 * 1024 * 1024)?;
        for name in [
            "color",
            "global_motion",
            "quantize_1000",
            "inverse_1000",
            "choose_all",
            "prediction_all",
            "encode_p",
            "encode_identical",
            "encode_i",
        ] {
            let mut times = Vec::new();
            for iteration in 0..33 {
                let started = Instant::now();
                match name {
                    "color" => {
                        black_box(Picture::from_bgr(&frame)?);
                    }
                    "global_motion" => {
                        black_box(motion::global(&picture, reference, frame.size()));
                    }
                    "quantize_1000" => {
                        for _ in 0..1000 {
                            black_box(transform::quantize(black_box(&samples), 4, true, true));
                        }
                    }
                    "inverse_1000" => {
                        for _ in 0..1000 {
                            black_box(transform::inverse(black_box(&coefficients)));
                        }
                    }
                    "choose_all" => {
                        let mut vectors = motion::Vectors::new(
                            picture.planes[0].width / 16,
                            picture.planes[0].height / 16,
                        )?;
                        for y in 0..picture.planes[0].height / 16 {
                            for x in 0..picture.planes[0].width / 16 {
                                let vector = motion::choose(
                                    &picture.planes[0],
                                    &reference.planes[0],
                                    frame.size(),
                                    x * 16,
                                    y * 16,
                                    global,
                                    vectors.predict(x, y),
                                );
                                vectors.store(x, y, black_box(vector));
                            }
                        }
                    }
                    "prediction_all" => {
                        for y in 0..picture.planes[0].height / 16 {
                            for x in 0..picture.planes[0].width / 16 {
                                for b in 0..6 {
                                    let (p, bx, by) = position(x, y, b);
                                    black_box(motion::block(
                                        &reference.planes[p],
                                        bx,
                                        by,
                                        [0, 0],
                                        p != 0,
                                    ));
                                }
                            }
                        }
                    }
                    _ => {
                        bits.clear();
                        let mut output = Picture::blank(frame.size())?;
                        let count = picture.planes[0].width / 16 * (picture.planes[0].height / 16);
                        let mut blocks = Vec::new();
                        blocks.try_reserve_exact(count)?;
                        blocks.resize_with(count, Macroblock::default);
                        encode_picture(
                            &mut bits,
                            PictureCoding {
                                current: &picture,
                                reference: Some(if name == "encode_identical" {
                                    &picture
                                } else {
                                    reference
                                }),
                                previous_input: None,
                                key: name == "encode_i",
                                q: 4,
                                visible: frame.size(),
                            },
                            &mut output,
                            &mut blocks,
                        )?;
                        black_box(output);
                        black_box(bits.bytes()?);
                    }
                }
                if iteration >= 3 {
                    times.push(started.elapsed().as_secs_f64() * 1000.0);
                }
            }
            times.sort_by(f64::total_cmp);
            println!(
                "{}",
                serde_json::json!({"profile":"codec","stage":name,"samples":30,"warmups":3,"p50_ms":times[14],"p95_ms":times[28]})
            );
        }
        Ok(())
    }
}
impl Mpeg4Encoder {
    pub fn new(size: Size, quantizer: u8) -> io::Result<Self> {
        if size.width() > 8191 || size.height() > 8191 {
            return Err(invalid("MPEG-4 VOL dimensions must not exceed 8191 pixels"));
        }
        if !(2..=31).contains(&quantizer) {
            return Err(invalid("MPEG-4 quantizer must be from 2 to 31"));
        }
        let padded = size.width().div_ceil(16) * 16 * size.height().div_ceil(16) * 16;
        let packet_limit = padded
            .checked_mul(6)
            .and_then(|n| n.checked_add(65_536))
            .ok_or_else(|| invalid("MPEG-4 packet size overflow"))?;
        Ok(Self {
            size,
            q: quantizer,
            frames: 0,
            bits: Bits::new(packet_limit)?,
            reference: None,
            previous_input: None,
            input_scratch: None,
            output_scratch: None,
            blocks: Vec::new(),
            compute: None,
            failed: false,
        })
    }
    pub fn encode(&mut self, frame: &BgrFrame) -> io::Result<EncodedFrame<'_>> {
        if self.failed {
            return Err(invalid("MPEG-4 encoder has a previous encoding failure"));
        }
        if frame.size() != self.size {
            return Err(invalid(
                "Recording frame dimensions changed; recording stopped before writing a mismatched frame.",
            ));
        }
        let next = self
            .frames
            .checked_add(1)
            .ok_or_else(|| invalid("MPEG-4 frame counter exhausted"))?;
        let current = if let Some(mut buffer) = self.input_scratch.take() {
            buffer.update(frame)?;
            buffer
        } else {
            Picture::from_bgr(frame)?
        };
        let keyframe = self.reference.is_none()
            || self.frames.is_multiple_of(KEYFRAME_INTERVAL)
            || self
                .reference
                .as_ref()
                .is_some_and(|reference| scene_cut(&current, reference));
        self.bits.clear();
        if keyframe {
            syntax::vol(&mut self.bits, self.size);
        }
        syntax::vop(&mut self.bits, self.frames, keyframe, self.q);
        let count = current.planes[0].width / 16 * (current.planes[0].height / 16);
        let repeated = !keyframe
            && self
                .previous_input
                .as_ref()
                .is_some_and(|p| current.same_pixels(p));
        let result = (|| -> io::Result<()> {
            if repeated {
                // 입력이 같으면 직전 복원 화소를 재양자화하지 않고 표준P skip으로 유지한다.
                for _ in 0..count {
                    self.bits.bit(true);
                }
            } else {
                if self.blocks.len() != count {
                    self.blocks
                        .try_reserve_exact(count - self.blocks.len())
                        .map_err(io::Error::other)?;
                    self.blocks.resize_with(count, Macroblock::default);
                }
                if self.compute.is_none()
                    && current.planes[0].width * current.planes[0].height >= 256_000
                {
                    self.compute = Some(crate::common::compute::ComputeLease::acquire()?);
                }
                let mut reconstructed = match self.output_scratch.take() {
                    Some(p) => p,
                    None => Picture::blank(self.size)?,
                };
                encode_picture(
                    &mut self.bits,
                    PictureCoding {
                        current: &current,
                        reference: self.reference.as_ref(),
                        previous_input: self.previous_input.as_ref(),
                        key: keyframe,
                        q: self.q,
                        visible: self.size,
                    },
                    &mut reconstructed,
                    &mut self.blocks,
                )?;
                self.output_scratch = self.reference.replace(reconstructed);
            }
            Ok(())
        })();
        match result {
            Ok(()) => {}
            Err(error) => {
                self.failed = true;
                return Err(error);
            }
        }
        self.bits.align();
        if let Err(error) = self.bits.bytes() {
            self.failed = true;
            return Err(error);
        }
        self.input_scratch = self.previous_input.replace(current);
        self.frames = next;
        Ok(EncodedFrame {
            bytes: self.bits.bytes()?,
            keyframe,
        })
    }
}
fn scene_cut(current: &Picture, reference: &Picture) -> bool {
    let a = &current.planes[0];
    let b = &reference.planes[0];
    let mut difference = 0u64;
    let mut count = 0u64;
    for y in (0..a.height).step_by(16) {
        for x in (0..a.width).step_by(16) {
            difference += u64::from(a.sample(x, y).abs_diff(b.sample(x, y)));
            count += 1;
        }
    }
    difference > count * 40
}

#[cfg(test)]
mod reuse_tests {
    use super::*;
    #[test]
    fn unchanged_frames_keep_reference_and_active_buffers_are_reused()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::collections::HashSet;
        let size = Size::new(512, 512)?;
        let mut frame = BgrFrame::zeros(size)?;
        for (i, p) in frame
            .pixels_mut()
            .as_chunks_mut::<3>()
            .0
            .iter_mut()
            .enumerate()
        {
            p.copy_from_slice(&[(i % 251) as u8, ((i / size.width()) % 251) as u8, 90]);
        }
        let mut encoder = Mpeg4Encoder::new(size, 4)?;
        assert!(encoder.encode(&frame)?.keyframe);
        let reference = encoder
            .reference
            .as_ref()
            .ok_or("reference missing")?
            .planes
            .iter()
            .map(|p| p.data.clone())
            .collect::<Vec<_>>();
        for _ in 0..6 {
            assert!(!encoder.encode(&frame)?.keyframe);
            for (plane, expected) in encoder
                .reference
                .as_ref()
                .ok_or("reference missing")?
                .planes
                .iter()
                .zip(&reference)
            {
                assert_eq!(&plane.data, expected);
            }
        }
        let mut input = HashSet::new();
        let mut output = HashSet::new();
        let mut blocks = None;
        for i in 0..40 {
            for y in 0..8 {
                for x in 0..8 {
                    frame.pixels_mut()[(y * size.width() + x) * 3..(y * size.width() + x) * 3 + 3]
                        .fill((i * 5) as u8);
                }
            }
            encoder.encode(&frame)?;
            for picture in [
                encoder.previous_input.as_ref(),
                encoder.input_scratch.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                input.insert(picture.planes[0].data.as_ptr() as usize);
            }
            for picture in [encoder.reference.as_ref(), encoder.output_scratch.as_ref()]
                .into_iter()
                .flatten()
            {
                output.insert(picture.planes[0].data.as_ptr() as usize);
            }
            if let Some(previous) = blocks {
                assert_eq!(previous, encoder.blocks.as_ptr() as usize);
            } else {
                blocks = Some(encoder.blocks.as_ptr() as usize);
            }
        }
        assert_eq!(input.len(), 2);
        assert_eq!(output.len(), 2);
        Ok(())
    }
}

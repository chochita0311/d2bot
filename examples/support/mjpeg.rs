//! MJPEG 크기 비교와 RIFF 테스트에서만 사용하는 개발용 인코더.
use d2bot::common::{
    frame::{BgrFrame, Size},
    recording::{
        OutputMode,
        avi::{self, AviWriter},
    },
};
use jpeg_encoder::{ColorType, Encoder, SamplingFactor};
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::Path,
};
pub struct JpegFrameEncoder {
    size: Size,
    quality: u8,
    bytes: Vec<u8>,
}
impl JpegFrameEncoder {
    pub fn new(size: Size, quality: u8) -> io::Result<Self> {
        if !(1..=100).contains(&quality) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "JPEG quality must be from 1 to 100",
            ));
        }
        if size.width() > i16::MAX as usize || size.height() > i16::MAX as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "AVI dimensions must not exceed 32767 pixels",
            ));
        }
        Ok(Self {
            size,
            quality,
            bytes: Vec::new(),
        })
    }
    pub fn encode(&mut self, frame: &BgrFrame) -> io::Result<&[u8]> {
        if frame.size() != self.size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Recording frame dimensions changed; recording stopped before writing a mismatched frame.",
            ));
        }
        self.bytes.clear();
        let mut encoder = Encoder::new(&mut self.bytes, self.quality);
        // 품질 단계와 별개로 4:2:2를 고정해 크기·색상 비교의 조건을 유지한다.
        encoder.set_sampling_factor(SamplingFactor::F_2_1);
        encoder
            .encode(
                frame.bytes(),
                self.size.width() as u16,
                self.size.height() as u16,
                ColorType::Bgr,
            )
            .map_err(io::Error::other)?;
        Ok(&self.bytes)
    }
}
pub struct MjpegWriter {
    encoder: JpegFrameEncoder,
    avi: AviWriter<File>,
}
impl MjpegWriter {
    pub fn open(path: &Path, size: Size, quality: u8, mode: OutputMode) -> io::Result<Self> {
        let encoder = JpegFrameEncoder::new(size, quality)?;
        let avi = open_avi(path, size, mode, avi::AviCodec::Mjpeg)?;
        Ok(Self { encoder, avi })
    }
    pub fn write(&mut self, frame: &BgrFrame) -> io::Result<()> {
        self.avi.write_jpeg(self.encoder.encode(frame)?)
    }
    pub fn finish(&mut self) -> io::Result<()> {
        self.avi.finish()
    }
}
fn open_avi(
    path: &Path,
    size: Size,
    mode: OutputMode,
    codec: avi::AviCodec,
) -> io::Result<AviWriter<File>> {
    if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let mut options = OpenOptions::new();
    options.write(true);
    match mode {
        OutputMode::CreateNew => {
            options.create_new(true);
        }
        OutputMode::Replace => {
            options.create(true).truncate(true);
        }
    }
    AviWriter::for_codec(options.open(path)?, size, codec)
}
impl Drop for MjpegWriter {
    fn drop(&mut self) {
        if let Err(error) = self.finish() {
            let _ = writeln!(
                io::stderr().lock(),
                "MJPEG trial finalization failed: {error}"
            );
        }
    }
}

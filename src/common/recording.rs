//! 직접 구현한 MPEG-4와 무음 AVI 저장을 제공한다. 파일 타임베이스는 캡처 주기와 독립이다.
pub mod avi;
pub mod mpeg4;
use crate::common::frame::{BgrFrame, Size};
use avi::AviWriter;
use std::{
    fs::{File, OpenOptions},
    io,
    path::Path,
};
#[derive(Clone, Copy)]
pub enum OutputMode {
    CreateNew,
    Replace,
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
pub struct Mpeg4Writer {
    encoder: mpeg4::Mpeg4Encoder,
    avi: AviWriter<File>,
}
impl Mpeg4Writer {
    pub fn open(path: &Path, size: Size, quantizer: u8, mode: OutputMode) -> io::Result<Self> {
        let encoder = mpeg4::Mpeg4Encoder::new(size, quantizer)?;
        let avi = open_avi(path, size, mode, avi::AviCodec::Mpeg4)?;
        Ok(Self { encoder, avi })
    }
    pub fn write(&mut self, frame: &BgrFrame) -> io::Result<()> {
        let packet = self.encoder.encode(frame)?;
        self.avi.write_packet(packet.bytes, packet.keyframe)
    }
    pub fn frames(&self) -> u32 {
        self.avi.frames()
    }
    pub fn finish(&mut self) -> io::Result<()> {
        self.avi.finish()
    }
}
impl Drop for Mpeg4Writer {
    fn drop(&mut self) {
        crate::common::error::best_effort_cleanup(|| {
            self.finish().map_err(|e| {
                crate::common::error::ControlError::Runtime(format!("AVI finalization failed: {e}"))
            })
        });
    }
}

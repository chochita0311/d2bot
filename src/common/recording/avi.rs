//! RIFF/OpenDML 구조만 작성한다. 압축과 캡처는 이 모듈의 책임이 아니다.
use crate::common::frame::Size;
use std::{
    fs::File,
    io::{self, Cursor, Seek, SeekFrom, Write},
};
pub const FPS: u32 = 8;
const SEGMENT_BYTES: u64 = 1 << 30;
const SEGMENT_FRAMES: usize = 65_536;
const INDEX_SEGMENTS: usize = 1024;
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AviCodec {
    Mjpeg,
    Mpeg4,
}
impl AviCodec {
    fn fourcc(self) -> &'static [u8; 4] {
        match self {
            Self::Mjpeg => b"MJPG",
            Self::Mpeg4 => b"XVID",
        }
    }
}
pub trait AviStorage: Write + Seek {
    fn truncate(&mut self, length: u64) -> io::Result<()>;
}
impl AviStorage for File {
    fn truncate(&mut self, length: u64) -> io::Result<()> {
        self.set_len(length)
    }
}
impl AviStorage for Cursor<Vec<u8>> {
    fn truncate(&mut self, length: u64) -> io::Result<()> {
        self.get_mut().truncate(
            usize::try_from(length)
                .map_err(|_| invalid("AVI length exceeds memory address range"))?,
        );
        Ok(())
    }
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn u32_size(value: u64) -> io::Result<u32> {
    u32::try_from(value).map_err(|_| invalid("AVI chunk exceeds 32-bit size"))
}
fn word(w: &mut impl Write, value: u16) -> io::Result<()> {
    w.write_all(&value.to_le_bytes())
}
fn dword(w: &mut impl Write, value: u32) -> io::Result<()> {
    w.write_all(&value.to_le_bytes())
}
fn qword(w: &mut impl Write, value: u64) -> io::Result<()> {
    w.write_all(&value.to_le_bytes())
}
fn chunk(w: &mut impl Write, id: &[u8; 4], size: u32) -> io::Result<()> {
    w.write_all(id)?;
    dword(w, size)
}
fn list(w: &mut (impl Write + Seek), id: &[u8; 4]) -> io::Result<u64> {
    let start = w.stream_position()?;
    chunk(w, b"LIST", 0)?;
    w.write_all(id)?;
    Ok(start)
}
fn patch(w: &mut (impl Write + Seek), at: u64, value: u32) -> io::Result<()> {
    let end = w.stream_position()?;
    w.seek(SeekFrom::Start(at))?;
    dword(w, value)?;
    w.seek(SeekFrom::Start(end))?;
    Ok(())
}
fn end_list(w: &mut (impl Write + Seek), start: u64) -> io::Result<()> {
    let size = u32_size(
        w.stream_position()?
            .checked_sub(start + 8)
            .ok_or_else(|| invalid("invalid AVI list position"))?,
    )?;
    patch(w, start + 4, size)
}
struct Segment {
    riff: u64,
    movi: u64,
    entries: Vec<(u32, u32, bool)>,
}
struct SubIndex {
    offset: u64,
    bytes: u32,
    frames: u32,
}
struct Header {
    avih: u64,
    strh: u64,
    indx: u64,
    dmlh: u64,
}
pub struct AviWriter<W: AviStorage> {
    storage: W,
    header: Header,
    segment: Option<Segment>,
    indexes: Vec<SubIndex>,
    committed_end: u64,
    frames: u32,
    maximum_frame: u32,
    segment_bytes: u64,
    finished: bool,
    failed: bool,
    codec: AviCodec,
}
impl<W: AviStorage> AviWriter<W> {
    pub fn new(storage: W, size: Size) -> io::Result<Self> {
        Self::with_segment_target(storage, size, SEGMENT_BYTES)
    }
    pub fn for_codec(storage: W, size: Size, codec: AviCodec) -> io::Result<Self> {
        Self::with_codec_and_segment_target(storage, size, codec, SEGMENT_BYTES)
    }
    /// 작은 구간에서도 동일한 OpenDML 마감·다음 구간 전환을 사용한다.
    pub fn with_segment_target(storage: W, size: Size, segment_bytes: u64) -> io::Result<Self> {
        Self::with_codec_and_segment_target(storage, size, AviCodec::Mjpeg, segment_bytes)
    }
    pub fn with_codec_and_segment_target(
        mut storage: W,
        size: Size,
        codec: AviCodec,
        segment_bytes: u64,
    ) -> io::Result<Self> {
        if !(1..=SEGMENT_BYTES).contains(&segment_bytes) {
            return Err(invalid("AVI segment target must be from 1 byte to 1 GiB"));
        }
        let width = u16::try_from(size.width())
            .ok()
            .filter(|n| *n <= i16::MAX as u16)
            .ok_or_else(|| invalid("AVI width exceeds 32767 pixels"))?;
        let height = u16::try_from(size.height())
            .ok()
            .filter(|n| *n <= i16::MAX as u16)
            .ok_or_else(|| invalid("AVI height exceeds 32767 pixels"))?;
        let image_bytes = u32::from(width)
            .checked_mul(u32::from(height))
            .and_then(|v| v.checked_mul(3))
            .ok_or_else(|| invalid("AVI image size exceeds 32-bit size"))?;
        storage.seek(SeekFrom::Start(0))?;
        storage.truncate(0)?;
        chunk(&mut storage, b"RIFF", 0)?;
        storage.write_all(b"AVI ")?;
        let hdrl = list(&mut storage, b"hdrl")?;
        chunk(&mut storage, b"avih", 56)?;
        let avih = storage.stream_position()?;
        for value in [
            1_000_000 / FPS,
            0,
            0,
            0x10,
            0,
            0,
            1,
            0,
            u32::from(width),
            u32::from(height),
            0,
            0,
            0,
            0,
        ] {
            dword(&mut storage, value)?;
        }
        let strl = list(&mut storage, b"strl")?;
        chunk(&mut storage, b"strh", 56)?;
        let strh = storage.stream_position()?;
        storage.write_all(b"vids")?;
        storage.write_all(codec.fourcc())?;
        dword(&mut storage, 0)?;
        word(&mut storage, 0)?;
        word(&mut storage, 0)?;
        for value in [0, 1, FPS, 0, 0, 0, u32::MAX, 0] {
            dword(&mut storage, value)?;
        }
        for value in [0, 0, width, height] {
            word(&mut storage, value)?;
        }
        chunk(&mut storage, b"strf", 40)?;
        for value in [40, u32::from(width), u32::from(height)] {
            dword(&mut storage, value)?;
        }
        word(&mut storage, 1)?;
        word(&mut storage, 24)?;
        storage.write_all(codec.fourcc())?;
        for value in [image_bytes, 0, 0, 0, 0] {
            dword(&mut storage, value)?;
        }
        chunk(&mut storage, b"indx", (24 + INDEX_SEGMENTS * 16) as u32)?;
        let indx = storage.stream_position()?;
        word(&mut storage, 4)?;
        storage.write_all(&[0, 0])?;
        dword(&mut storage, 0)?;
        storage.write_all(b"00dc")?;
        storage.write_all(&[0; 12])?;
        storage.write_all(&vec![0; INDEX_SEGMENTS * 16])?;
        end_list(&mut storage, strl)?;
        let odml = list(&mut storage, b"odml")?;
        chunk(&mut storage, b"dmlh", 248)?;
        let dmlh = storage.stream_position()?;
        storage.write_all(&[0; 248])?;
        end_list(&mut storage, odml)?;
        end_list(&mut storage, hdrl)?;
        let movi = list(&mut storage, b"movi")?;
        let committed_end = storage.stream_position()?;
        Ok(Self {
            storage,
            header: Header {
                avih,
                strh,
                indx,
                dmlh,
            },
            segment: Some(Segment {
                riff: 0,
                movi,
                entries: Vec::new(),
            }),
            indexes: Vec::with_capacity(INDEX_SEGMENTS),
            committed_end,
            frames: 0,
            maximum_frame: 0,
            segment_bytes,
            finished: false,
            failed: false,
            codec,
        })
    }
    pub fn frames(&self) -> u32 {
        self.frames
    }
    pub fn into_inner(mut self) -> io::Result<W> {
        self.finish()?;
        Ok(self.storage)
    }
    pub fn write_jpeg(&mut self, jpeg: &[u8]) -> io::Result<()> {
        if self.codec != AviCodec::Mjpeg {
            return Err(invalid("JPEG data requires an MJPEG AVI stream"));
        }
        self.write_packet(jpeg, true)
    }
    pub fn write_packet(&mut self, jpeg: &[u8], keyframe: bool) -> io::Result<()> {
        if self.finished || self.failed {
            return Err(invalid(
                "AVI writer is closed or has a previous write failure",
            ));
        }
        if self.codec == AviCodec::Mjpeg
            && (!jpeg.starts_with(&[0xff, 0xd8]) || !jpeg.ends_with(&[0xff, 0xd9]))
        {
            return Err(invalid("AVI frame must be a complete JPEG image"));
        }
        if self.codec == AviCodec::Mpeg4 && (!jpeg.starts_with(&[0, 0, 1]) || jpeg.len() < 5) {
            return Err(invalid("AVI MPEG-4 packet requires a video start code"));
        }
        let size = u32::try_from(jpeg.len())
            .ok()
            .filter(|n| *n < 0x8000_0000)
            .ok_or_else(|| invalid("AVI frame exceeds index size"))?;
        if u64::from(size) + 64 > SEGMENT_BYTES {
            return Err(invalid(
                "A single AVI frame exceeds the 1 GiB segment limit",
            ));
        }
        if self.frames == u32::MAX {
            return Err(invalid("AVI frame count exceeds 32-bit header"));
        }
        let result = self.append(jpeg, size, keyframe);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn append(&mut self, jpeg: &[u8], size: u32, keyframe: bool) -> io::Result<()> {
        let rollover = self.segment.as_ref().is_some_and(|s| {
            !s.entries.is_empty()
                && (s.entries.len() >= SEGMENT_FRAMES
                    || self.committed_end - s.movi - 12
                        + u64::from(size)
                        + 9
                        + 32
                        + (s.entries.len() as u64 + 1) * 8
                        > self.segment_bytes)
        });
        if rollover {
            if self.indexes.len() + 1 >= INDEX_SEGMENTS {
                return Err(invalid(
                    "AVI reached its 1024-segment index capacity; completed frames remain finalizable",
                ));
            }
            self.finish_segment()?;
            self.storage.seek(SeekFrom::Start(self.committed_end))?;
            let riff = self.committed_end;
            chunk(&mut self.storage, b"RIFF", 0)?;
            self.storage.write_all(b"AVIX")?;
            let movi = list(&mut self.storage, b"movi")?;
            self.segment = Some(Segment {
                riff,
                movi,
                entries: Vec::new(),
            });
            self.committed_end = self.storage.stream_position()?;
        }
        let s = self
            .segment
            .as_mut()
            .ok_or_else(|| invalid("AVI has no active segment"))?;
        let offset = u32_size(self.committed_end - s.movi - 8)?;
        s.entries.try_reserve(1).map_err(io::Error::other)?;
        self.storage.seek(SeekFrom::Start(self.committed_end))?;
        chunk(&mut self.storage, b"00dc", size)?;
        self.storage.write_all(jpeg)?;
        if !size.is_multiple_of(2) {
            self.storage.write_all(&[0])?;
        }
        self.committed_end = self.storage.stream_position()?;
        s.entries.push((offset, size, keyframe));
        self.frames += 1;
        self.maximum_frame = self.maximum_frame.max(size);
        Ok(())
    }
    fn finish_segment(&mut self) -> io::Result<()> {
        let Some(s) = &self.segment else {
            return Ok(());
        };
        self.storage.seek(SeekFrom::Start(self.committed_end))?;
        let offset = self.committed_end;
        let frames = u32::try_from(s.entries.len())
            .map_err(|_| invalid("AVI segment frame count overflow"))?;
        let index_bytes = 24 + frames * 8;
        chunk(&mut self.storage, b"ix00", index_bytes)?;
        word(&mut self.storage, 2)?;
        self.storage.write_all(&[0, 1])?;
        dword(&mut self.storage, frames)?;
        self.storage.write_all(b"00dc")?;
        qword(&mut self.storage, s.movi + 8)?;
        dword(&mut self.storage, 0)?;
        for (relative, size, keyframe) in &s.entries {
            dword(&mut self.storage, relative + 8)?;
            dword(
                &mut self.storage,
                *size | if *keyframe { 0 } else { 0x8000_0000 },
            )?;
        }
        end_list(&mut self.storage, s.movi)?;
        if self.indexes.is_empty() {
            chunk(&mut self.storage, b"idx1", frames * 16)?;
            for (relative, size, keyframe) in &s.entries {
                self.storage.write_all(b"00dc")?;
                dword(&mut self.storage, if *keyframe { 0x10 } else { 0 })?;
                dword(&mut self.storage, *relative)?;
                dword(&mut self.storage, *size)?;
            }
        }
        let end = self.storage.stream_position()?;
        patch(&mut self.storage, s.riff + 4, u32_size(end - s.riff - 8)?)?;
        self.indexes.push(SubIndex {
            offset,
            bytes: index_bytes + 8,
            frames,
        });
        self.segment = None;
        self.committed_end = end;
        Ok(())
    }
    pub fn finish(&mut self) -> io::Result<()> {
        if self.finished {
            return Ok(());
        }
        self.finish_segment()?;
        let first_frames = self.indexes.first().map_or(0, |index| index.frames);
        patch(
            &mut self.storage,
            self.header.avih + 4,
            self.maximum_frame.saturating_mul(FPS),
        )?;
        patch(&mut self.storage, self.header.avih + 16, first_frames)?;
        patch(&mut self.storage, self.header.avih + 28, self.maximum_frame)?;
        patch(&mut self.storage, self.header.strh + 32, self.frames)?;
        patch(&mut self.storage, self.header.strh + 36, self.maximum_frame)?;
        patch(&mut self.storage, self.header.dmlh, self.frames)?;
        self.storage.seek(SeekFrom::Start(self.header.indx + 4))?;
        dword(&mut self.storage, self.indexes.len() as u32)?;
        self.storage.seek(SeekFrom::Start(self.header.indx + 24))?;
        for index in &self.indexes {
            qword(&mut self.storage, index.offset)?;
            dword(&mut self.storage, index.bytes)?;
            dword(&mut self.storage, index.frames)?;
        }
        self.storage.truncate(self.committed_end)?;
        self.storage.seek(SeekFrom::Start(self.committed_end))?;
        self.storage.flush()?;
        self.finished = true;
        Ok(())
    }
}

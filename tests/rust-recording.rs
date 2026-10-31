use d2bot::{
    actions::recording::RecordingSession,
    common::{
        config::BotConfig,
        error::ControlError,
        frame::{BgrFrame, Size},
        frame_geometry::PhysicalTarget,
        realtime::{CapturedFrame, ScreenSource},
        recording::{
            OutputMode,
            avi::{AviStorage, AviWriter},
        },
    },
    runs::RunError,
};
#[path = "../examples/support/mjpeg.rs"]
mod mjpeg;
use mjpeg::{JpegFrameEncoder, MjpegWriter};
use serde_json::json;
use std::{
    error::Error,
    fs::File,
    io::{self, Cursor, Seek, SeekFrom, Write},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
#[path = "support/directory.rs"]
mod directory;
mod support;
type TestResult = Result<(), Box<dyn Error>>;
fn solid(width: usize, height: usize, color: [u8; 3]) -> Result<BgrFrame, Box<dyn Error>> {
    Ok(BgrFrame::from_vec(
        Size::new(width, height)?,
        color.repeat(width * height),
    )?)
}
fn decode(path: &Path, count: usize, width: usize, height: usize, fourcc: &[u8; 4]) -> TestResult {
    let value =
        support::legacy_expectation("recording-oracle", &json!({"paths":[path],"pixels":true}))?;
    let result = &value[0];
    assert_eq!(result["width"], width);
    assert_eq!(result["height"], height);
    assert_eq!(result["fps"], 8.0);
    assert_eq!(result["reported_frames"], count);
    assert_eq!(result["decoded_frames"], count);
    assert_eq!(result["fourcc"], u32::from_le_bytes(*fourcc));
    for seek in result["seek"].as_array().ok_or("seek results")? {
        assert_eq!(seek["matches_sequential"], true);
    }
    for frame in result["pixels"].as_array().ok_or("decoded pixels")? {
        assert_eq!(frame.as_array().ok_or("pixel rows")?.len(), height);
        assert_eq!(frame[0].as_array().ok_or("pixel columns")?.len(), width);
    }
    Ok(())
}
#[test]
fn independent_decoder_reads_odd_dimensions_colors_frames_and_segment_indexes() -> TestResult {
    let directory = directory::TestDirectory::new()?;
    for (width, height, roll) in [(64, 48, false), (65, 49, false), (64, 48, true)] {
        let path = directory
            .path()
            .join(format!("{width}-{height}-{roll}.avi"));
        let size = Size::new(width, height)?;
        let mut avi = AviWriter::with_segment_target(
            File::create(&path)?,
            size,
            if roll { 1 } else { 1 << 30 },
        )?;
        let mut encoder = JpegFrameEncoder::new(size, 80)?;
        let colors = [[20, 40, 220], [200, 100, 30], [90, 200, 100], [60, 80, 100]];
        for color in colors {
            avi.write_jpeg(encoder.encode(&solid(width, height, color)?)?)?;
        }
        avi.finish()?;
        avi.finish()?;
        assert!(avi.write_jpeg(&[0xff, 0xd8, 0xff, 0xd9]).is_err());
        drop(avi);
        decode(&path, 4, width, height, b"MJPG")?;
        let value = support::legacy_expectation(
            "recording-oracle",
            &json!({"paths":[path],"pixels":true}),
        )?;
        for (index, color) in colors.iter().enumerate() {
            for (channel, expected) in color.iter().enumerate() {
                assert!(
                    (value[0]["pixels"][index][0][0][channel]
                        .as_i64()
                        .ok_or("channel")?
                        - i64::from(*expected))
                    .abs()
                        <= 4
                );
            }
        }
        if roll {
            let bytes = std::fs::read(&path)?;
            let mut position = 0usize;
            let mut segments = 0;
            while position < bytes.len() {
                assert_eq!(&bytes[position..position + 4], b"RIFF");
                assert_eq!(
                    &bytes[position + 8..position + 12],
                    if segments == 0 { b"AVI " } else { b"AVIX" }
                );
                let length = u32::from_le_bytes(bytes[position + 4..position + 8].try_into()?);
                position += usize::try_from(length)? + 8;
                segments += 1;
            }
            assert_eq!(segments, 4);
            assert_eq!(position, bytes.len());
        }
    }
    Ok(())
}
#[derive(Clone, Copy)]
enum Failure {
    WriteAfter(usize),
    Seek,
    Flush,
    Truncate,
}
struct FaultStorage {
    inner: Cursor<Vec<u8>>,
    fault: Arc<Mutex<Option<Failure>>>,
}
impl Write for FaultStorage {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut fault = self
            .fault
            .lock()
            .map_err(|_| io::Error::other("fault lock"))?;
        if let Some(Failure::WriteAfter(remaining)) = *fault {
            if remaining == 0 {
                *fault = None;
                return Err(io::Error::other("injected write failure"));
            }
            let count = remaining.min(bytes.len());
            *fault = Some(Failure::WriteAfter(remaining - count));
            return self.inner.write(&bytes[..count]);
        }
        self.inner.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        let mut fault = self
            .fault
            .lock()
            .map_err(|_| io::Error::other("fault lock"))?;
        if matches!(*fault, Some(Failure::Flush)) {
            *fault = None;
            return Err(io::Error::other("injected flush failure"));
        }
        Ok(())
    }
}
impl Seek for FaultStorage {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let mut fault = self
            .fault
            .lock()
            .map_err(|_| io::Error::other("fault lock"))?;
        if matches!(*fault, Some(Failure::Seek)) {
            *fault = None;
            return Err(io::Error::other("injected seek failure"));
        }
        self.inner.seek(position)
    }
}
impl AviStorage for FaultStorage {
    fn truncate(&mut self, length: u64) -> io::Result<()> {
        let mut fault = self
            .fault
            .lock()
            .map_err(|_| io::Error::other("fault lock"))?;
        if matches!(*fault, Some(Failure::Truncate)) {
            *fault = None;
            return Err(io::Error::other("injected truncate failure"));
        }
        self.inner
            .get_mut()
            .truncate(usize::try_from(length).map_err(io::Error::other)?);
        Ok(())
    }
}
#[test]
fn partial_frame_is_rolled_back_and_finish_failures_remain_retryable() -> TestResult {
    let directory = directory::TestDirectory::new()?;
    let size = Size::new(64, 48)?;
    let mut encoder = JpegFrameEncoder::new(size, 80)?;
    let jpeg = encoder.encode(&solid(64, 48, [10, 90, 180])?)?.to_vec();
    for (index, failure) in [
        Failure::WriteAfter(30),
        Failure::Seek,
        Failure::Flush,
        Failure::Truncate,
    ]
    .into_iter()
    .enumerate()
    {
        let fault = Arc::new(Mutex::new(None));
        let mut avi = AviWriter::new(
            FaultStorage {
                inner: Cursor::new(vec![]),
                fault: fault.clone(),
            },
            size,
        )?;
        avi.write_jpeg(&jpeg)?;
        *fault.lock().map_err(|_| "fault lock")? = Some(failure);
        if matches!(failure, Failure::WriteAfter(_)) {
            assert!(avi.write_jpeg(&jpeg).is_err());
            assert_eq!(avi.frames(), 1);
            assert!(avi.write_jpeg(&jpeg).is_err());
        } else {
            assert!(avi.finish().is_err());
        }
        avi.finish()?;
        let path = directory.path().join(format!("repair-{index}.avi"));
        // 저수준 저장기를 닫은 뒤 결과만 독립 디코더에 제공한다.
        let bytes = avi.into_inner()?.inner.into_inner();
        std::fs::write(&path, bytes)?;
        decode(&path, 1, 64, 48, b"MJPG")?;
    }
    Ok(())
}
#[test]
fn writer_rejects_dimension_changes_and_existing_generated_paths() -> TestResult {
    let directory = directory::TestDirectory::new()?;
    let path = directory.path().join("retained.avi");
    let mut writer = MjpegWriter::open(&path, Size::new(64, 48)?, 80, OutputMode::CreateNew)?;
    writer.write(&solid(64, 48, [40, 80, 120])?)?;
    assert!(writer.write(&solid(65, 48, [40, 80, 120])?).is_err());
    writer.finish()?;
    drop(writer);
    let before = std::fs::read(&path)?;
    assert!(MjpegWriter::open(&path, Size::new(64, 48)?, 80, OutputMode::CreateNew).is_err());
    assert_eq!(std::fs::read(&path)?, before);
    decode(&path, 1, 64, 48, b"MJPG")?;
    Ok(())
}
struct Capture {
    frame: Arc<BgrFrame>,
    grabs: usize,
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
    closes: Arc<AtomicUsize>,
    panic: bool,
}
impl ScreenSource for Capture {
    fn grab(&mut self) -> Result<CapturedFrame, ControlError> {
        self.grabs += 1;
        if self.grabs > 1 {
            self.entered
                .send(())
                .map_err(|e| ControlError::Runtime(e.to_string()))?;
            self.release
                .recv()
                .map_err(|e| ControlError::Runtime(e.to_string()))?;
            if self.panic {
                panic!("injected recording capture panic");
            }
            return Err(ControlError::Runtime("injected capture failure".into()));
        }
        Ok(CapturedFrame {
            frame: self.frame.clone(),
            target: PhysicalTarget::new(0, 0, 64, 48)
                .map_err(|e| ControlError::Runtime(e.to_string()))?,
            wall_captured_at: None,
            geometry: None,
        })
    }
    fn close(&mut self) -> Result<(), ControlError> {
        self.closes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}
#[test]
fn cancellation_and_panic_retain_worker_finalize_video_and_close_capture() -> TestResult {
    let directory = directory::TestDirectory::new()?;
    for panic in [false, true] {
        let (entered, started) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let closes = Arc::new(AtomicUsize::new(0));
        let capture = Arc::new(Mutex::new(Some(Capture {
            frame: Arc::new(solid(64, 48, [10, 90, 180])?),
            grabs: 0,
            entered,
            release: gate,
            closes: closes.clone(),
            panic,
        })));
        let config = BotConfig::from_value(
            json!({"recording":{"directory":directory.path()},"capture":{"fps":8}}),
        )?;
        let factory = Arc::new(
            move |settings: d2bot::common::capture::CaptureSettings, _| {
                assert!(settings.reference_layout.is_none());
                capture
                    .lock()
                    .map_err(|_| RunError::Runtime("capture lock".into()))?
                    .take()
                    .map(|c| Box::new(c) as Box<dyn ScreenSource>)
                    .ok_or_else(|| RunError::Runtime("fake capture consumed".into()))
            },
        );
        let mut session =
            RecordingSession::new(&config, env!("CARGO_MANIFEST_DIR").into(), factory, || {
                "20261023_010203".into()
            })?;
        let path = directory.path().join(format!("cancel-{panic}.avi"));
        session.start_to(path.clone())?;
        started.recv_timeout(Duration::from_secs(3))?;
        let started = Instant::now();
        session.request_stop();
        assert!(started.elapsed() < Duration::from_millis(100));
        assert!(session.finish(Duration::from_millis(1)).is_err());
        assert!(session.is_running());
        assert!(session.start_to(path.clone()).is_err());
        assert_eq!(closes.load(Ordering::SeqCst), 0);
        release.send(())?;
        session.finish(Duration::from_secs(3))?;
        assert!(!session.is_running());
        assert_eq!(closes.load(Ordering::SeqCst), 1);
        let events = session.drain_events();
        assert!(events.iter().any(|e| e.message.contains(if panic {
            "injected recording capture panic"
        } else {
            "injected capture failure"
        })));
        drop(session);
        decode(&path, 1, 64, 48, b"FMP4")?;
    }
    Ok(())
}

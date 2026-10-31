//! 캡처·인코더·파일은 한 녹화 작업이 소유하며 취소 후에도 실제 마감까지 유지한다.
use super::{ActionEvent, EventLevel, runtime::ActionEvents};
use crate::{
    common::{
        capture::CaptureSettings,
        config::{BotConfig, ConfigError},
        realtime::ScreenSource,
        recording::{Mpeg4Writer, OutputMode, mpeg4::DEFAULT_QUANTIZER},
        workers::{Cancellation, WorkerGroup},
    },
    runs::RunError,
};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
pub type CaptureFactory =
    Arc<dyn Fn(CaptureSettings, PathBuf) -> Result<Box<dyn ScreenSource>, RunError> + Send + Sync>;
pub struct RecordingSession {
    settings: CaptureSettings,
    fps: Value,
    directory: PathBuf,
    root: PathBuf,
    capture: CaptureFactory,
    timestamp: Box<dyn Fn() -> String>,
    worker: Option<WorkerGroup>,
    stop: Cancellation,
    events: ActionEvents,
    fault_reported: AtomicBool,
    stop_requested: AtomicBool,
    output: Option<PathBuf>,
    stop_reported: bool,
}
impl RecordingSession {
    pub fn new(
        config: &BotConfig,
        root: PathBuf,
        capture: CaptureFactory,
        timestamp: impl Fn() -> String + 'static,
    ) -> Result<Self, RunError> {
        let mut value = Self {
            settings: CaptureSettings::default(),
            fps: Value::Null,
            directory: PathBuf::new(),
            root,
            capture,
            timestamp: Box::new(timestamp),
            worker: None,
            stop: Cancellation::default(),
            events: ActionEvents::default(),
            fault_reported: AtomicBool::new(false),
            stop_requested: AtomicBool::new(false),
            output: None,
            stop_reported: false,
        };
        value.configure(config)?;
        Ok(value)
    }
    #[cfg(windows)]
    pub fn windows(config: &BotConfig, root: PathBuf) -> Result<Self, RunError> {
        Self::new(
            config,
            root,
            Arc::new(|settings, root| {
                Ok(Box::new(crate::common::capture::ScreenCapture::new(
                    crate::platform::windows::Windows::new()?,
                    settings,
                    &root,
                )?))
            }),
            || crate::platform::windows::local_timestamp().replace('-', "_"),
        )
    }
    pub fn configure(&mut self, config: &BotConfig) -> Result<(), RunError> {
        let mut settings = CaptureSettings::from_config(config)?;
        settings.reference_layout = None;
        let directory = config
            .section("recording")
            .and_then(|v| v["directory"].as_str())
            .ok_or_else(|| ConfigError::value("Recording directory must be a path string."))?;
        self.settings = settings;
        self.directory = directory.into();
        self.fps = config
            .section("capture")
            .map(|v| v["fps"].clone())
            .unwrap_or(Value::Null);
        Ok(())
    }
    pub fn is_running(&self) -> bool {
        self.worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
    }
    pub fn output_path(&self) -> Option<&Path> {
        self.output.as_deref()
    }
    pub fn start(&mut self) -> Result<PathBuf, RunError> {
        let path = self
            .directory
            .join(format!("diablo2_{}.avi", (self.timestamp)()));
        self.start_to(path)
    }
    pub fn start_to(&mut self, output: PathBuf) -> Result<PathBuf, RunError> {
        if self.is_running() {
            return Err(RunError::Runtime("Recording is already running.".into()));
        }
        self.finish(Duration::ZERO)?;
        let stop = Cancellation::default();
        let mut worker = WorkerGroup::new(stop.clone(), 1);
        let settings = self.settings.clone();
        let root = self.root.clone();
        let capture = self.capture.clone();
        let path = output.clone();
        let fps = self.fps.clone();
        let events = self.events.clone();
        worker.spawn("gui-recording", move |stop| {
            if let Err(error) = record(capture, settings, root, &path, &fps, &stop) {
                events.push(ActionEvent {
                    level: EventLevel::Error,
                    message: format!("Recording failed: {error}"),
                });
            }
            Ok(())
        })?;
        self.worker = Some(worker);
        self.stop = stop;
        self.output = Some(output.clone());
        self.stop_reported = false;
        self.stop_requested.store(false, Ordering::Release);
        self.fault_reported.store(false, Ordering::Release);
        self.events.push(ActionEvent {
            level: EventLevel::Info,
            message: format!(
                "Recording started: {}",
                output.file_name().unwrap_or_default().to_string_lossy()
            ),
        });
        Ok(output)
    }
    pub fn request_stop(&self) {
        self.stop_requested.store(true, Ordering::Release);
        self.stop.cancel();
    }
    pub fn drain_events(&self) -> Vec<ActionEvent> {
        if let Some(worker) = self.worker.as_ref().filter(|worker| worker.is_finished())
            && !self.fault_reported.swap(true, Ordering::AcqRel)
        {
            let faults = worker
                .faults()
                .map(|faults| {
                    faults
                        .into_iter()
                        .map(|fault| fault.message)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|error| vec![error.to_string()]);
            for error in faults {
                self.events.push(ActionEvent {
                    level: EventLevel::Error,
                    message: format!("Recording failed: {error}"),
                });
            }
        }
        self.events.drain()
    }
    pub fn finish(&mut self, timeout: Duration) -> Result<(), RunError> {
        if let Some(worker) = &mut self.worker {
            worker.finish(timeout)?;
        }
        if self.worker.is_some() {
            // 큐의 오류를 없애지 않은 채 패닉도 명시적인 녹화 오류로 남긴다.
            if !self.fault_reported.swap(true, Ordering::AcqRel) {
                for fault in self
                    .worker
                    .as_ref()
                    .map(|w| w.faults())
                    .transpose()?
                    .unwrap_or_default()
                {
                    self.events.push(ActionEvent {
                        level: EventLevel::Error,
                        message: format!("Recording failed: {}", fault.message),
                    });
                }
            }
            if self.stop_requested.load(Ordering::Acquire) && !self.stop_reported {
                self.events.push(ActionEvent {
                    level: EventLevel::Info,
                    message: "Recording stopped.".into(),
                });
                self.stop_reported = true;
            }
        }
        self.worker = None;
        Ok(())
    }
}
fn period(fps: &Value) -> Result<Duration, RunError> {
    let fps = match fps {
        Value::Bool(value) => f64::from(u8::from(*value)),
        Value::Number(value) => value
            .as_f64()
            .ok_or_else(|| ConfigError::value("Capture FPS exceeds floating-point range."))?,
        _ => return Err(ConfigError::value("Capture FPS must be numeric.").into()),
    };
    let seconds = if fps > 1.0 { 1.0 / fps } else { 1.0 };
    Duration::try_from_secs_f64(seconds)
        .map_err(|_| ConfigError::value("Capture interval exceeds the clock range.").into())
}
fn record(
    factory: CaptureFactory,
    settings: CaptureSettings,
    root: PathBuf,
    output: &Path,
    fps: &Value,
    stop: &Cancellation,
) -> Result<(), RunError> {
    let mut capture = CaptureOwner(Some(factory(settings, root)?));
    let mut writer: Option<Mpeg4Writer> = None;
    let result = (|| {
        let first = capture.grab()?;
        writer = Some(Mpeg4Writer::open(
            output,
            first.frame.size(),
            DEFAULT_QUANTIZER,
            OutputMode::CreateNew,
        )?);
        let writer = writer
            .as_mut()
            .ok_or_else(|| RunError::Runtime("Recording writer unavailable.".into()))?;
        writer.write(first.frame.as_ref())?;
        let delay = period(fps)?;
        while !stop.is_cancelled() {
            let started = Instant::now();
            let packet = capture.grab()?;
            writer.write(packet.frame.as_ref())?;
            stop.wait(delay.saturating_sub(started.elapsed()))?;
        }
        Ok(())
    })();
    let finish = writer
        .as_mut()
        .map(Mpeg4Writer::finish)
        .transpose()
        .map(|_| ())
        .map_err(RunError::from);
    let close = capture.close().map_err(RunError::from);
    [finish, close]
        .into_iter()
        .fold(result, |previous, cleanup| match (previous, cleanup) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
            (Err(error), Err(cleanup)) => Err(error.with_cleanup(cleanup)),
        })
}
struct CaptureOwner(Option<Box<dyn ScreenSource>>);
impl CaptureOwner {
    fn grab(
        &mut self,
    ) -> Result<crate::common::realtime::CapturedFrame, crate::common::error::ControlError> {
        self.0
            .as_mut()
            .ok_or_else(|| {
                crate::common::error::ControlError::Runtime("Recording capture is closed.".into())
            })?
            .grab()
    }
    fn close(&mut self) -> Result<(), crate::common::error::ControlError> {
        match self.0.take() {
            Some(mut capture) => capture.close(),
            None => Ok(()),
        }
    }
}
impl Drop for CaptureOwner {
    fn drop(&mut self) {
        crate::common::error::best_effort_cleanup(|| self.close());
    }
}

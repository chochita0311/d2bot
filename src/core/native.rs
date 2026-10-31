//! 레거시 미리보기는 모니터/영역도 지원한다. 필드 전용 포커스 정책을 섞지 않는다.
use super::PreviewEnvironment;
use crate::{
    common::{
        capture::{CaptureSettings, ScreenCapture},
        config::BotConfig,
        controller::{InputGate, normalize_pointer},
        frame::{BgrFrame, Size},
        hotkeys::{HotkeyAction, HotkeyBinding},
        recording::{Mpeg4Writer, OutputMode},
        time::{MonotonicClock, SystemClock},
        workers::Cancellation,
    },
    platform::{
        InputEvent, InputSystem,
        windows::{HotkeyListener, Windows},
    },
    runs::RunError,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

pub struct NativePreview {
    capture: ScreenCapture<Windows>,
    system: Windows,
    stop: Cancellation,
    gate: InputGate,
    clock: SystemClock,
    keys: Option<HotkeyListener>,
    bindings: Vec<HotkeyBinding>,
    binding_message: String,
    recorder: Option<Mpeg4Writer>,
    recording: Option<PathBuf>,
    emit: Arc<dyn Fn(&str) + Send + Sync>,
    level: u8,
}
impl NativePreview {
    pub fn load(
        config: &BotConfig,
        root: &Path,
        stop: Cancellation,
        gate: InputGate,
        emit: Arc<dyn Fn(&str) + Send + Sync>,
        level: u8,
    ) -> Result<Self, RunError> {
        let hotkeys = config
            .section("hotkeys")
            .ok_or_else(|| RunError::Runtime("hotkeys required".into()))?;
        let key = |name: &str| {
            hotkeys[name]
                .as_str()
                .ok_or_else(|| RunError::Runtime(format!("{name} hotkey must be text")))
        };
        let bindings = vec![
            HotkeyBinding::parse(key("pause")?, HotkeyAction::TogglePause)?,
            HotkeyBinding::parse(key("stop")?, HotkeyAction::Stop)?,
        ];
        let binding_message = format!(
            "Hotkeys active: pause={} stop={}",
            key("pause")?,
            key("stop")?
        );
        let recording = config
            .section("recording")
            .ok_or_else(|| RunError::Runtime("recording settings required".into()))?;
        let recording =
            if recording["enabled"] == true {
                Some(root.join(recording["output_path"].as_str().ok_or_else(|| {
                    RunError::Runtime("recording output path must be text".into())
                })?))
            } else {
                None
            };
        let system = Windows::new()?;
        let capture = ScreenCapture::new(system, CaptureSettings::from_config(config)?, root)?;
        Ok(Self {
            capture,
            system,
            stop,
            gate,
            clock: SystemClock::default(),
            keys: None,
            bindings,
            binding_message,
            recorder: None,
            recording,
            emit,
            level,
        })
    }
    fn send(&self, event: InputEvent) -> Result<(), RunError> {
        if self.system.send(event)? != 1 {
            return Err(RunError::Runtime(
                "SendInput did not insert preview input".into(),
            ));
        }
        Ok(())
    }
}
impl PreviewEnvironment for NativePreview {
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError> {
        Ok(self.capture.grab()?.frame)
    }
    fn target(&self) -> String {
        format!("{:?}", self.capture.target())
    }
    fn to_desktop(&self, point: [f64; 2]) -> Result<[i64; 2], RunError> {
        if let Some(geometry) = self.capture.geometry() {
            if self.capture.current_target()? != geometry.target()
                || self.capture.current_client_box()? != geometry.source_client()
            {
                return Err(RunError::Runtime(
                    "window geometry changed after capture".into(),
                ));
            }
            Ok(geometry
                .to_desktop(point)
                .map_err(|e| RunError::Runtime(e.to_string()))?)
        } else {
            let target = self.capture.target();
            Ok([
                (point[0] + f64::from(target.left)) as i64,
                (point[1] + f64::from(target.top)) as i64,
            ])
        }
    }
    fn click(&mut self, point: [i64; 2]) -> Result<(), RunError> {
        if self.gate.interrupted() || self.stop.is_cancelled() {
            return Ok(());
        }
        let point = [i32::try_from(point[0]), i32::try_from(point[1])];
        let point = [
            point[0]
                .as_ref()
                .copied()
                .map_err(|_| RunError::Runtime("preview pointer x exceeds range".into()))?,
            point[1]
                .as_ref()
                .copied()
                .map_err(|_| RunError::Runtime("preview pointer y exceeds range".into()))?,
        ];
        let [dx, dy] = normalize_pointer(self.system.desktop()?, point)?;
        self.send(InputEvent::Mouse {
            dx,
            dy,
            flags: 0xc001,
        })?;
        if self.gate.wait(Duration::from_millis(50))? || self.stop.is_cancelled() {
            return Ok(());
        }
        if self.system.pointer()? == [0, 0] {
            return Err(RunError::Runtime(
                "PyDirectInput fail-safe point (0, 0) reached".into(),
            ));
        }
        self.send(InputEvent::Mouse {
            dx: 0,
            dy: 0,
            flags: 6,
        })?;
        self.gate.wait(Duration::from_millis(100))?;
        Ok(())
    }
    fn start(&mut self, size: Size) -> Result<(), RunError> {
        if let Some(path) = &self.recording {
            self.recorder = Some(Mpeg4Writer::open(path, size, 4, OutputMode::Replace)?);
        }
        let gate = self.gate.clone();
        let stop = self.stop.clone();
        let emit = self.emit.clone();
        let level = self.level;
        self.keys = Some(HotkeyListener::start(
            self.bindings.clone(),
            Arc::new(move |action| {
                match action {
                    HotkeyAction::TogglePause => {
                        gate.pause(!gate.paused());
                        if level <= 20 {
                            emit(if gate.paused() {
                                "Bot is now paused."
                            } else {
                                "Bot is now running."
                            });
                        }
                    }
                    HotkeyAction::Stop => {
                        gate.stop();
                        stop.cancel();
                        if level <= 20 {
                            emit("Stop requested.");
                        }
                    }
                }
                Ok(())
            }),
        )?);
        if self.level <= 20 {
            (self.emit)(&self.binding_message);
        }
        Ok(())
    }
    fn record(&mut self, frame: &BgrFrame) -> Result<(), RunError> {
        if let Some(recorder) = &mut self.recorder {
            recorder.write(frame)?;
        }
        Ok(())
    }
    fn now(&self) -> f64 {
        self.clock.now()
    }
    fn wait(&mut self, seconds: f64) -> Result<(), RunError> {
        self.stop.wait(
            Duration::try_from_secs_f64(seconds)
                .map_err(|_| RunError::Runtime("preview wait exceeds clock range".into()))?,
        )?;
        Ok(())
    }
    fn close(&mut self) -> Result<(), RunError> {
        self.stop.cancel();
        self.gate.stop();
        let mut errors = Vec::new();
        if let Some(keys) = &mut self.keys
            && let Err(e) = keys.stop(Duration::from_secs(1))
        {
            errors.push(e.to_string());
        }
        if let Some(recorder) = &mut self.recorder
            && let Err(e) = recorder.finish()
        {
            errors.push(e.to_string());
        }
        if let Err(e) = self.capture.close() {
            errors.push(e.to_string());
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(RunError::Runtime(errors.join("; ")))
        }
    }
}
impl Drop for NativePreview {
    fn drop(&mut self) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.close()));
    }
}

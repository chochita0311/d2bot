//! 화면 열거·캡처는 기존 Windows 경계에서 수행하고 게임 입력은 액션 세션에 남긴다.
use super::{runtime::PanelEffects, snapshot::SnapshotEffect};
use crate::{
    actions::{ActionError, desktop::DesktopActionBackend},
    common::{
        capture::{CaptureSettings, ScreenCapture},
        vision_ops::codec,
        workers::Cancellation,
    },
    platform::{CaptureBackend, windows::Windows},
    runs::RunError,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub type NativeActionBackend = DesktopActionBackend<Windows, Windows>;
pub fn runtime(
    config: crate::common::config::BotConfig,
    root: PathBuf,
) -> Result<
    super::runtime::PanelRuntime<NativeActionBackend, crate::actions::recording::RecordingSession>,
    RunError,
> {
    let recorder = crate::actions::recording::RecordingSession::windows(&config, root.clone())?;
    super::runtime::PanelRuntime::new(config, root, recorder, effects())
}

pub fn effects() -> PanelEffects<NativeActionBackend> {
    PanelEffects {
        backend: Arc::new(|settings, root| DesktopActionBackend::windows(settings, &root)),
        summoner: Box::new(|session, root, request| session.start(root, request)),
        snapshot: snapshot_effect(),
        windows: Box::new(|| {
            Ok(Windows::new()?
                .windows()?
                .into_iter()
                .map(|window| window.title)
                .collect())
        }),
    }
}
pub fn snapshot_effect() -> SnapshotEffect {
    Arc::new(|settings, root, stop| save_snapshot(Windows::new()?, settings, &root, &stop))
}
pub fn save_snapshot<B: CaptureBackend>(
    backend: B,
    settings: CaptureSettings,
    root: &Path,
    stop: &Cancellation,
) -> Result<PathBuf, RunError> {
    if stop.is_cancelled() {
        return Err(RunError::Runtime("Snapshot cancelled.".into()));
    }
    let mut capture = ScreenCapture::new(backend, settings, root)?;
    let result = capture.grab().map_err(RunError::from);
    let cleanup = capture.close().map_err(RunError::from);
    let packet = match (result, cleanup) {
        (Ok(packet), Ok(())) => packet,
        (Err(error), Ok(())) | (Ok(_), Err(error)) => return Err(error),
        (Err(error), Err(cleanup)) => return Err(error.with_cleanup(cleanup)),
    };
    if stop.is_cancelled() {
        return Err(RunError::Runtime("Snapshot cancelled.".into()));
    }
    let directory = PathBuf::from("assets/private");
    std::fs::create_dir_all(&directory)?;
    let output = directory.join("gui_snapshot.png");
    codec::save_png(&output, packet.frame.as_ref()).map_err(ActionError::from)?;
    Ok(output)
}

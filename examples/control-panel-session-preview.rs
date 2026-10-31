//! 실제 GUI/세션 종료 연결을 게임·파일 효과 없이 확인하는 개발용 창.
use d2bot::{
    actions::{ActionError, ActionEvent, EventLevel, runtime::ActionEvents},
    common::{
        config::BotConfig,
        workers::{Cancellation, WorkerGroup},
    },
    runs::RunError,
    ui::{
        host::{ControlPanel, native_options},
        native::NativeActionBackend,
        recording::RecordingPort,
        runtime::{PanelEffects, PanelRuntime},
        view,
    },
};
use std::{path::PathBuf, sync::Arc, time::Duration};
#[derive(Default)]
struct Recorder {
    worker: Option<WorkerGroup>,
    events: ActionEvents,
}
impl RecordingPort for Recorder {
    fn configure(&mut self, _: &BotConfig) -> Result<(), RunError> {
        Ok(())
    }
    fn start(&mut self) -> Result<PathBuf, RunError> {
        if self.is_running() {
            return Err(RunError::Runtime(
                "Preview recorder already running.".into(),
            ));
        }
        self.finish(Duration::ZERO)?;
        let events = self.events.clone();
        let mut worker = WorkerGroup::new(Cancellation::default(), 1);
        worker.spawn("preview-recorder", move |stop| {
            stop.wait(Duration::from_secs(3600))?;
            std::thread::sleep(Duration::from_millis(500));
            events.push(ActionEvent {
                level: EventLevel::Info,
                message: "Preview recorder cleanup completed; no file was written.".into(),
            });
            Ok(())
        })?;
        self.worker = Some(worker);
        Ok("(preview only: no recording file)".into())
    }
    fn request_stop(&self) {
        if let Some(worker) = &self.worker {
            worker.cancellation().cancel();
        }
    }
    fn is_running(&self) -> bool {
        self.worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
    }
    fn drain_events(&self) -> Vec<ActionEvent> {
        self.events.drain()
    }
    fn finish(&mut self, timeout: Duration) -> Result<(), RunError> {
        if let Some(worker) = &mut self.worker {
            worker.finish(timeout)?;
        }
        self.worker = None;
        Ok(())
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let effects: PanelEffects<NativeActionBackend> = PanelEffects {
        backend: Arc::new(|_, _| {
            Err(ActionError::runtime(
                "Preview backend: game capture and input are disabled.",
            ))
        }),
        summoner: Box::new(|session, _, request| {
            session.start_with(request, || Ok("(preview only: no evidence files)".into()), |packet| {
            (packet.emit)("미리보기입니다. 실제 작업 스레드의 취소·정리만 확인하며 게임 입력·파일 저장은 수행하지 않습니다.");
            packet.stop.wait(Duration::from_secs(3600))?;
            std::thread::sleep(Duration::from_millis(500));
            Ok(2)
        })
        }),
        snapshot: Arc::new(|_, _, _| {
            Err(RunError::Runtime(
                "Preview snapshot: no game capture or file write.".into(),
            ))
        }),
        windows: Box::new(|| Ok(vec!["Diablo II: Resurrected".into()])),
    };
    let mut runtime = PanelRuntime::new(
        BotConfig::load(root.join("config"))?,
        root,
        Recorder::default(),
        effects,
    )?;
    runtime.state.append_log(
        "info",
        "세션 연결 미리보기: 게임 캡처·입력·파일 저장 없음. 보통 · 악몽 · 지옥",
    );
    let panel = ControlPanel::new(runtime);
    let options = native_options()?;
    eframe::run_native(
        "Diablo2 Helper — session preview",
        options,
        Box::new(move |context| {
            view::configure(&context.egui_ctx);
            Ok(Box::new(panel))
        }),
    )?;
    Ok(())
}

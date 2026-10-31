//! GUI 스레드는 그리기와 짧은 폴링만 하며 실제 종료가 끝난 뒤 창을 닫는다.
use super::{
    model::{INITIAL_SIZE, MINIMUM_SIZE, TITLE},
    recording::RecordingPort,
    runtime::{PanelRuntime, UiFault},
    view::{self, PanelView},
};
use crate::{actions::runtime::ActionBackend, runs::RunError};
use eframe::egui;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

const POLL_INTERVAL: Duration = Duration::from_millis(250);
pub struct ControlPanel<B: ActionBackend + 'static, R: RecordingPort> {
    runtime: PanelRuntime<B, R>,
    view: PanelView,
    faults: VecDeque<UiFault>,
    last_poll: Option<Instant>,
}
impl<B: ActionBackend + 'static, R: RecordingPort> ControlPanel<B, R> {
    pub fn new(mut runtime: PanelRuntime<B, R>) -> Self {
        let mut faults = VecDeque::new();
        if let Err(fault) = runtime.refresh_windows(true) {
            runtime.state.append_log("error", &fault.message);
            faults.push_back(fault);
        }
        Self {
            runtime,
            view: PanelView::default(),
            faults,
            last_poll: None,
        }
    }
    fn push_fault(&mut self, fault: UiFault) {
        if self
            .faults
            .iter()
            .any(|f| f.title == fault.title && f.message == fault.message)
        {
            return;
        }
        if self.faults.len() == 16 {
            self.faults.pop_front();
        }
        self.faults.push_back(fault);
    }
}
impl<B: ActionBackend + 'static, R: RecordingPort> eframe::App for ControlPanel<B, R> {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        if context.input(|input| input.viewport().close_requested()) {
            self.runtime.request_close();
            if !self.runtime.ready_to_close() {
                context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }
        if self
            .last_poll
            .is_none_or(|last| last.elapsed() >= POLL_INTERVAL)
        {
            self.last_poll = Some(Instant::now());
            match self.runtime.poll() {
                Ok(faults) => {
                    for fault in faults {
                        self.push_fault(fault);
                    }
                }
                Err(error) => {
                    let message = error.to_string();
                    if !self.faults.iter().any(|f| f.message == message) {
                        self.runtime.state.append_log("error", &message);
                    }
                    self.runtime.state.status = "Error".into();
                    self.push_fault(UiFault {
                        title: "Action Error",
                        message,
                    });
                    self.runtime.request_close();
                }
            }
        }
        for intent in self.view.show(ui, &mut self.runtime.state) {
            if let Err(fault) = self.runtime.intent(intent) {
                self.push_fault(fault);
            }
        }
        if context.input(|input| input.key_pressed(egui::Key::F10))
            && let Err(fault) = self.runtime.intent(super::view::Intent::StopAction)
        {
            self.push_fault(fault);
        }
        if let Some(fault) = self.faults.front() {
            let response =
                egui::Modal::new(egui::Id::unique("action-error")).show(&context, |ui| {
                    ui.set_max_width(540.0);
                    ui.heading(fault.title);
                    ui.label(&fault.message);
                    if ui.button("OK").clicked() {
                        ui.close();
                    }
                });
            if response.should_close() {
                self.faults.pop_front();
            }
        }
        if self.runtime.ready_to_close() {
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        context.request_repaint_after(POLL_INTERVAL);
    }
}
pub fn run<B: ActionBackend + 'static, R: RecordingPort>(
    runtime: PanelRuntime<B, R>,
) -> Result<(), RunError> {
    let panel = ControlPanel::new(runtime);
    let options = native_options()?;
    eframe::run_native(
        TITLE,
        options,
        Box::new(move |context| {
            view::configure(&context.egui_ctx);
            Ok(Box::new(panel))
        }),
    )
    .map_err(|error| RunError::Runtime(format!("Control panel failed: {error}")))
}
pub fn native_options() -> Result<eframe::NativeOptions, RunError> {
    #[cfg(windows)]
    let scale = crate::platform::windows::Windows::new()?.system_ui_scale()?;
    #[cfg(not(windows))]
    let scale = 1.0;
    Ok(eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(INITIAL_SIZE.map(|pixels| pixels / scale))
            .with_min_inner_size(MINIMUM_SIZE.map(|pixels| pixels / scale)),
        renderer: eframe::Renderer::Glow,
        persist_window: false,
        ..Default::default()
    })
}

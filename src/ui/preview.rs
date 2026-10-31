//! 최신 프레임 한 장만 표시한다. Escape/닫기는 취소를 요청하고 작업 종료를 기다린다.
use crate::{common::workers::Cancellation, core::PreviewFrames, runs::RunError};
use eframe::egui;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
struct Preview {
    frames: PreviewFrames,
    stop: Cancellation,
    finished: Arc<AtomicBool>,
    texture: Option<egui::TextureHandle>,
    initial: bool,
    error: Option<String>,
}
impl eframe::App for Preview {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        if context.input(|i| i.viewport().close_requested() || i.key_pressed(egui::Key::Escape)) {
            self.stop.cancel();
            if !self.finished.load(Ordering::Acquire) {
                context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }
        match self.frames.take() {
            Ok(Some(frame)) => {
                let size = frame.size();
                let image = egui::ColorImage::new(
                    [size.width(), size.height()],
                    frame
                        .pixels()
                        .as_chunks::<3>()
                        .0
                        .iter()
                        .map(|p| egui::Color32::from_rgb(p[2], p[1], p[0]))
                        .collect(),
                );
                if let Some(texture) = &mut self.texture {
                    texture.set(image, egui::TextureOptions::NEAREST);
                } else {
                    self.texture =
                        Some(context.load_texture("preview", image, egui::TextureOptions::NEAREST));
                }
                if self.initial {
                    self.initial = false;
                    context.send_viewport_cmd(egui::ViewportCommand::InnerSize(
                        egui::vec2(size.width() as f32, size.height() as f32)
                            / context.pixels_per_point(),
                    ));
                }
            }
            Ok(None) => {}
            Err(error) => {
                self.error = Some(error.to_string());
                self.stop.cancel();
            }
        }
        if let Some(texture) = &self.texture {
            ui.image((
                texture.id(),
                texture.size_vec2() / context.pixels_per_point(),
            ));
        } else {
            ui.label("Waiting for preview frame…");
        }
        if let Some(error) = &self.error {
            ui.label(error);
        }
        if self.stop.is_cancelled() {
            ui.label("Stopping…");
        }
        if self.finished.load(Ordering::Acquire) {
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        context.request_repaint_after(Duration::from_millis(20));
    }
}
pub fn run(
    frames: PreviewFrames,
    stop: Cancellation,
    finished: Arc<AtomicBool>,
) -> Result<(), RunError> {
    let _ = crate::platform::windows::Windows::new()?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        renderer: eframe::Renderer::Glow,
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        "Diablo2 Preview",
        options,
        Box::new(move |_| {
            Ok(Box::new(Preview {
                frames,
                stop,
                finished,
                texture: None,
                initial: true,
                error: None,
            }))
        }),
    )
    .map_err(|e| RunError::Runtime(format!("Preview window failed: {e}")))
}

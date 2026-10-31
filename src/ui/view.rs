//! 기존 배치의 렌더링과 사용자 의도 수집. 실제 캡처·입력은 실행 소유자가 맡는다.
#[cfg(test)]
mod tests;
use super::model::{INITIAL_SIZE, MINIMUM_SIZE, PanelState};
use eframe::egui::{
    self, Button, Color32, ComboBox, Frame, Margin, Panel, ScrollArea, TextEdit, TextWrapMode, Ui,
    vec2,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    Record,
    StopRecording,
    Snapshot,
    RefreshWindows,
    Gems,
    StopAction,
    Summoner,
    Loot,
    Lifecycle,
    WindowChanged,
    CharacterChanged,
}
#[derive(Default)]
pub struct PanelView {
    native_scale: Option<f32>,
}

fn log_area(ui: &mut Ui, log: &str) -> egui::scroll_area::ScrollAreaOutput<egui::Response> {
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.add(
                egui::Label::new(log)
                    .wrap_mode(TextWrapMode::Wrap)
                    .selectable(true),
            )
        })
}
#[derive(Clone, Copy)]
struct Pixels(f32);
impl Pixels {
    fn new(context: &egui::Context) -> Self {
        Self(
            context
                .native_pixels_per_point()
                .filter(|scale| scale.is_finite() && *scale > 0.0)
                .unwrap_or(1.0),
        )
    }
    fn points(self, pixels: f32) -> f32 {
        pixels / self.0
    }
    fn margin(self, margin: Margin) -> Margin {
        let convert = |value| self.points(f32::from(value)).round() as i8;
        Margin {
            left: convert(margin.left),
            right: convert(margin.right),
            top: convert(margin.top),
            bottom: convert(margin.bottom),
        }
    }
    fn inset(self, pixels: i8) -> Margin {
        self.margin(Margin::same(pixels))
    }
}
fn button(ui: &mut Ui, label: &str, enabled: bool, intent: Intent, events: &mut Vec<Intent>) {
    let units = Pixels::new(ui.ctx());
    if ui
        .add_enabled(
            enabled,
            Button::new(label).min_size(vec2(ui.available_width(), units.points(30.0))),
        )
        .clicked()
    {
        events.push(intent);
    }
}
pub fn configure(context: &egui::Context) {
    context.set_theme(egui::Theme::Light);
    #[cfg(windows)]
    if let Some(root) = std::env::var_os("WINDIR")
        && let Ok(bytes) = std::fs::read(std::path::PathBuf::from(root).join("Fonts/segoeui.ttf"))
    {
        // OS 글꼴은 배포에 복제하지 않는다. 없는 환경에서는 번들·시스템 fallback을 유지한다.
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "windows-ui".into(),
            egui::FontData::from_owned(bytes).into(),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "windows-ui".into());
        context.set_fonts(fonts);
    }
    let units = Pixels::new(context);
    configure_style(context, units);
    // Tk geometry는 물리 픽셀이고 egui viewport는 논리 점이다. 실제 초기 모니터 값으로 보정한다.
    context.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(
        MINIMUM_SIZE.map(|pixels| units.points(pixels)).into(),
    ));
    context.send_viewport_cmd(egui::ViewportCommand::InnerSize(
        INITIAL_SIZE.map(|pixels| units.points(pixels)).into(),
    ));
}
fn configure_style(context: &egui::Context, units: Pixels) {
    context.global_style_mut(|style| {
        style.spacing.item_spacing = vec2(units.points(8.0), units.points(8.0));
        style.spacing.button_padding = vec2(units.points(8.0), units.points(5.0));
        style.spacing.interact_size.y = units.points(30.0);
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(12.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(12.0));
    });
}
impl PanelView {
    pub fn show(&mut self, ui: &mut Ui, state: &mut PanelState) -> Vec<Intent> {
        let units = Pixels::new(ui.ctx());
        if self.native_scale != Some(units.0) {
            configure_style(ui.ctx(), units);
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::MinInnerSize(
                    MINIMUM_SIZE.map(|pixels| units.points(pixels)).into(),
                ));
            self.native_scale = Some(units.0);
        }
        ui.painter()
            .rect_filled(ui.max_rect(), 0.0, ui.visuals().panel_fill);
        let controls = state.controls();
        let mut events = vec![];
        Panel::top("capture-header")
            .frame(Frame::NONE.inner_margin(units.margin(Margin {
                left: 16,
                right: 16,
                top: 16,
                bottom: 12,
            })))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Window Title");
                    ui.add_enabled_ui(controls.selectors, |ui| {
                        ComboBox::from_id_salt("window-title")
                            .width(
                                (ui.available_width() - units.points(275.0))
                                    .max(units.points(120.0)),
                            )
                            .truncate()
                            .selected_text(&state.window_title)
                            .show_ui(ui, |ui| {
                                for title in &state.window_titles {
                                    if ui
                                        .selectable_value(
                                            &mut state.window_title,
                                            title.clone(),
                                            title,
                                        )
                                        .changed()
                                    {
                                        events.push(Intent::WindowChanged);
                                    }
                                }
                            });
                    });
                    ui.label("Capture Backend");
                    ui.add_enabled_ui(controls.selectors, |ui| {
                        ComboBox::from_id_salt("capture-backend")
                            .width(units.points(90.0))
                            .selected_text(&state.backend)
                            .show_ui(ui, |ui| {
                                for backend in ["auto", "window", "screen"] {
                                    ui.selectable_value(
                                        &mut state.backend,
                                        backend.into(),
                                        backend,
                                    );
                                }
                            });
                    });
                });
                ui.add_space(units.points(3.0));
                ui.columns(4, |columns| {
                    button(
                        &mut columns[0],
                        "Start Recording",
                        controls.record,
                        Intent::Record,
                        &mut events,
                    );
                    button(
                        &mut columns[1],
                        "Stop Recording",
                        controls.stop_record,
                        Intent::StopRecording,
                        &mut events,
                    );
                    button(
                        &mut columns[2],
                        "Capture Snapshot",
                        !state.closing,
                        Intent::Snapshot,
                        &mut events,
                    );
                    button(
                        &mut columns[3],
                        "Refresh Windows",
                        !state.closing,
                        Intent::RefreshWindows,
                        &mut events,
                    );
                });
            });
        Panel::bottom("status-output")
            .frame(Frame::NONE.inner_margin(units.margin(Margin {
                left: 16,
                right: 16,
                top: 12,
                bottom: 16,
            })))
            .show(ui, |ui| {
                for (label, value) in [("Status", &state.status), ("Output", &state.output)] {
                    ui.horizontal(|ui| {
                        ui.add_sized([48.0, units.points(20.0)], egui::Label::new(label));
                        ui.add(egui::Label::new(value).wrap_mode(TextWrapMode::Wrap));
                    });
                }
            });
        let natural_width = ui
            .painter()
            .layout_no_wrap(
                "Select the matching character in the game.".into(),
                egui::TextStyle::Body.resolve(ui.style()),
                ui.visuals().text_color(),
            )
            .size()
            .x
            + units.points(64.0);
        let control_width = natural_width.max(units.points(360.0));
        Panel::right("play-controls")
            .default_size(control_width).min_size(control_width).max_size(control_width)
            .resizable(false).show_separator_line(false)
            .frame(Frame::NONE.inner_margin(units.margin(Margin {left:16,right:16,top:0,bottom:0})))
            .show(ui, |ui| {
                Frame::group(ui.style()).inner_margin(units.inset(12)).show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    ui.label("Play Controls");
                    ui.add_space(units.points(3.0));
                    button(ui, "Start Gem Summing", controls.actions, Intent::Gems, &mut events);
                    button(ui, "Stop Action (F10)", controls.stop_action, Intent::StopAction, &mut events);
                    // 큰 배율에서도 중단 버튼은 고정하고 나머지 조작부를 끝까지 접근하게 한다.
                    ui.style_mut().spacing.scroll = egui::style::ScrollStyle::solid();
                    ScrollArea::vertical().id_salt("play-controls-scroll").auto_shrink([false,false]).show(ui, |ui| {
                    ui.add_space(units.points(10.0));
                    ui.horizontal(|ui| {
                        ui.label("Repeat Count");
                        let previous = state.repeat_count.clone();
                        let response = ui.add_sized([ui.available_width(), units.points(25.0)], TextEdit::singleline(&mut state.repeat_count));
                        if response.changed() && !PanelState::validate_repeat(&state.repeat_count) {
                            state.repeat_count = previous;
                        }
                    });
                    ui.add_space(units.points(8.0));
                    ui.label("Difficulty");
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut state.difficulty, "normal".into(), "보통");
                        ui.radio_value(&mut state.difficulty, "nightmare".into(), "악몽");
                        ui.radio_value(&mut state.difficulty, "hell".into(), "지옥");
                    });
                    ui.separator();
                    ui.add_space(units.points(4.0));
                    ui.label("Character");
                    ui.add_enabled_ui(controls.selectors, |ui| {
                        ComboBox::from_id_salt("character").width(ui.available_width()).truncate()
                            .selected_text(&state.character).show_ui(ui, |ui| {
                                for character in &state.characters {
                                    if ui.selectable_value(&mut state.character, character.clone(), character).changed() {
                                        events.push(Intent::CharacterChanged);
                                    }
                                }
                            });
                    });
                    ui.add_space(units.points(5.0));
                    button(ui, "Summoner Run", controls.actions, Intent::Summoner, &mut events);
                    ui.label("1–5 fresh north attempts (blank = 1).\nSelect the matching character in the game.");
                    ui.add_space(units.points(5.0));
                    ui.checkbox(&mut state.town_loop, "Live town loop (1–3 runs)");
                    button(ui, "Item Looting", controls.actions, Intent::Loot, &mut events);
                    button(ui, "Start Room Lifecycle", controls.actions, Intent::Lifecycle, &mut events);
                    });
                });
            });
        egui::CentralPanel::default()
            .frame(Frame::NONE.inner_margin(units.margin(Margin {
                left: 16,
                right: 0,
                top: 0,
                bottom: 0,
            })))
            .show(ui, |ui| {
                Frame::group(ui.style())
                    .inner_margin(units.inset(12))
                    .show(ui, |ui| {
                        ui.set_min_size(ui.available_size());
                        ui.label("Log");
                        Frame::NONE
                            .fill(Color32::WHITE)
                            .inner_margin(units.inset(6))
                            .show(ui, |ui| {
                                log_area(ui, state.log());
                            });
                    });
            });
        events
    }
}

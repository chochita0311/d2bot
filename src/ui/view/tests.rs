use super::*;
use crate::common::config::BotConfig;
use std::{collections::BTreeSet, error::Error, path::Path};

#[test]
fn log_follows_bottom_but_preserves_reader_scroll_until_bottom_again() {
    let context = egui::Context::default();
    let mut log = (0..100).map(|i| format!("line {i}\n")).collect::<String>();
    let mut tick = 0;
    let mut draw = |log: &str, wheel: Option<f32>| {
        tick += 1;
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(400.0, 200.0),
            )),
            time: Some(f64::from(tick) / 30.0),
            ..Default::default()
        };
        input
            .events
            .push(egui::Event::PointerMoved(egui::pos2(100.0, 100.0)));
        if let Some(delta) = wheel {
            input.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, delta),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
                modifiers: Default::default(),
            });
        }
        let mut position = (0.0, 0.0);
        let mut output = context.run_ui(input, |ui| {
            let area = log_area(ui, log);
            position = (
                area.state.clamped_offset().y,
                (area.content_size.y - area.inner_rect.height()).max(0.0),
            );
        });
        output.textures_delta.clear();
        position
    };
    for _ in 0..4 {
        draw(&log, None);
    }
    log.push_str("new at bottom\n");
    let bottom = draw(&log, None);
    assert!(bottom.1 > 500.0);
    assert!((bottom.0 - bottom.1).abs() < 1.0, "{bottom:?}");
    draw(&log, Some(180.0));
    for _ in 0..20 {
        draw(&log, None);
    }
    let reading = draw(&log, None);
    assert!(reading.0 < reading.1 - 50.0, "{reading:?}");
    log.push_str("arrived while reading history\n");
    let stayed = draw(&log, None);
    assert!(
        (stayed.0 - reading.0).abs() < 1.0,
        "{reading:?} -> {stayed:?}"
    );
    draw(&log, Some(-10000.0));
    for _ in 0..30 {
        draw(&log, None);
    }
    log.push_str("follow resumed\n");
    let resumed = draw(&log, None);
    assert!((resumed.0 - resumed.1).abs() < 1.0, "{resumed:?}");
}

#[test]
fn source_pixel_viewports_keep_controls_and_literal_logs_at_multiple_dpi()
-> Result<(), Box<dyn Error>> {
    let config = BotConfig::load(Path::new(env!("CARGO_MANIFEST_DIR")).join("config"))?;
    for scale in [1.0f32, 1.25, 1.5, 2.0] {
        for pixels in [INITIAL_SIZE, MINIMUM_SIZE] {
            let context = egui::Context::default();
            context.input_mut(|input| {
                input
                    .raw
                    .viewports
                    .entry(egui::ViewportId::ROOT)
                    .or_default()
                    .native_pixels_per_point = Some(scale);
                input.pixels_per_point = scale;
            });
            configure(&context);
            let mut state = PanelState::new(&config)?;
            let literal = r"[$rate] _value_ <raw> C:\folder\notes [1]";
            for _ in 0..100 {
                state.append_log("info", literal);
            }
            let mut view = PanelView::default();
            let rect = egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(pixels[0] / scale, pixels[1] / scale),
            );
            let mut seen = BTreeSet::new();
            let mut literal_seen = false;
            for pass in 0..8 {
                let mut input = egui::RawInput {
                    screen_rect: Some(rect),
                    time: Some(f64::from(pass) / 60.0),
                    ..Default::default()
                };
                input
                    .viewports
                    .entry(egui::ViewportId::ROOT)
                    .or_default()
                    .native_pixels_per_point = Some(scale);
                input.events.push(egui::Event::PointerMoved(egui::pos2(
                    rect.max.x - 50.0 / scale,
                    rect.center().y,
                )));
                if pass == 3 {
                    input.events.push(egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: vec2(0.0, -1000.0),
                        phase: egui::TouchPhase::Move,
                        source: egui::MouseWheelSource::Wheel,
                        modifiers: Default::default(),
                    });
                }
                let mut output = context.run_ui(input, |ui| {
                    view.show(ui, &mut state);
                });
                // Renderer 없는 배치 검사는 GPU delta를 적용하지 않으므로 명시적으로 소비한다.
                output.textures_delta.clear();
                let mut stop_visible = false;
                for clipped in output.shapes {
                    let egui::Shape::Text(text) = clipped.shape else {
                        continue;
                    };
                    let content = text.galley.text();
                    if content.is_empty() {
                        continue;
                    }
                    if content.starts_with("[INFO]") {
                        literal_seen |= content.contains(literal);
                        assert!(rect.expand(1.0).contains_rect(clipped.clip_rect));
                        continue;
                    }
                    let bounds = text.visual_bounding_rect();
                    if !clipped.clip_rect.expand(1.0).contains_rect(bounds) {
                        continue;
                    }
                    assert!(
                        rect.expand(1.0).contains_rect(bounds),
                        "scale={scale}, pixels={pixels:?}, text={content:?}, bounds={bounds:?}, viewport={rect:?}"
                    );
                    stop_visible |= content == "Stop Action (F10)";
                    seen.insert(content.to_owned());
                }
                assert!(stop_visible, "Stop control moved outside the visible frame");
            }
            for label in [
                "Window Title",
                "Capture Backend",
                "Start Recording",
                "Start Room Lifecycle",
                "Summoner Run",
                "No recording yet",
                "보통",
                "악몽",
                "지옥",
            ] {
                assert!(
                    seen.contains(label),
                    "Missing visible control {label}; scale={scale}, pixels={pixels:?}, seen={seen:?}"
                );
            }
            assert!(literal_seen, "Literal log content was altered");
        }
    }
    Ok(())
}

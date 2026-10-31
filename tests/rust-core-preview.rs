#[path = "support/directory.rs"]
mod directory;
mod support;
use d2bot::{
    common::{
        config::BotConfig,
        controller::InputGate,
        frame::{BgrFrame, PixelRect, Size},
        vision_ops::{codec, resize},
        workers::Cancellation,
    },
    core::{self, PreviewConfig, PreviewEnvironment, PreviewFrames},
    runs::RunError,
};
use serde_json::{Value, json};
use std::{error::Error, path::Path, sync::Arc};
type TestResult = Result<(), Box<dyn Error>>;
struct Fake {
    frames: Vec<Arc<BgrFrame>>,
    index: usize,
    paused: Vec<bool>,
    gate: InputGate,
    stop: Cancellation,
    recorded: Vec<BgrFrame>,
    clicks: Vec<[i64; 2]>,
    waits: Vec<f64>,
    size: Option<Size>,
    time: f64,
    closed: bool,
    failure: Option<&'static str>,
}
impl PreviewEnvironment for Fake {
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError> {
        if self.failure == Some("capture") {
            return Err(RunError::Runtime("capture failed".into()));
        }
        let frame = self
            .frames
            .get(self.index)
            .ok_or_else(|| RunError::Runtime("frame bound exceeded".into()))?
            .clone();
        self.gate.pause(self.paused[self.index]);
        self.index += 1;
        Ok(frame)
    }
    fn target(&self) -> String {
        "fake physical target".into()
    }
    fn to_desktop(&self, p: [f64; 2]) -> Result<[i64; 2], RunError> {
        Ok([p[0] as i64 + 100, p[1] as i64 - 200])
    }
    fn click(&mut self, p: [i64; 2]) -> Result<(), RunError> {
        self.clicks.push(p);
        Ok(())
    }
    fn start(&mut self, size: Size) -> Result<(), RunError> {
        self.size = Some(size);
        if self.failure == Some("start") {
            Err(RunError::Runtime("start failed".into()))
        } else {
            Ok(())
        }
    }
    fn record(&mut self, f: &BgrFrame) -> Result<(), RunError> {
        self.recorded.push(f.clone());
        if self.failure == Some("record") {
            Err(RunError::Runtime("record failed".into()))
        } else {
            Ok(())
        }
    }
    fn now(&self) -> f64 {
        self.time
    }
    fn wait(&mut self, seconds: f64) -> Result<(), RunError> {
        self.waits.push(seconds);
        self.time += seconds;
        if self.index == self.frames.len() {
            self.stop.cancel();
        }
        Ok(())
    }
    fn close(&mut self) -> Result<(), RunError> {
        self.closed = true;
        if self.failure == Some("close") || self.failure == Some("capture") {
            Err(RunError::Runtime("close failed".into()))
        } else {
            Ok(())
        }
    }
}
fn fake(frames: Vec<Arc<BgrFrame>>, paused: Vec<bool>, failure: Option<&'static str>) -> Fake {
    Fake {
        frames,
        paused,
        index: 0,
        gate: InputGate::default(),
        stop: Cancellation::default(),
        recorded: vec![],
        clicks: vec![],
        waits: vec![],
        size: None,
        time: 0.0,
        closed: false,
        failure,
    }
}
fn patterned(width: usize, height: usize, seed: u32) -> Result<BgrFrame, Box<dyn Error>> {
    let mut state = seed;
    let size = Size::new(width, height)?;
    Ok(BgrFrame::from_vec(
        size,
        (0..size.byte_len()?)
            .map(|_| {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                (state >> 24) as u8
            })
            .collect(),
    )?)
}
#[test]
fn explicit_bgr_preview_scale_matches_source_all_pixels() -> TestResult {
    for (width, height) in [(33, 29), (64, 48), (1, 3), (3, 1), (383, 287)] {
        let frame = patterned(width, height, 13)?;
        for scale in [0.5, 0.7, 0.33, 1.0, 1.25, 2.0] {
            if (width as f64 * scale).round_ties_even() < 1.0
                || (height as f64 * scale).round_ties_even() < 1.0
            {
                continue;
            }
            let expected = support::legacy_expectation(
                "core-oracle",
                &json!({"op":"scale","size":[width,height],"pixels":frame.pixels(),"scale":scale}),
            )?;
            let actual = resize::scale_linear(frame.view(), scale)?;
            assert_eq!(
                json!([actual.size().width(), actual.size().height()]),
                expected["size"]
            );
            assert_eq!(
                json!(actual.pixels()),
                expected["pixels"],
                "{width}x{height} {scale}"
            );
        }
    }
    Ok(())
}
#[test]
fn preview_preserves_order_pause_full_size_recording_and_cleanup() -> TestResult {
    let directory = directory::TestDirectory::new()?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let frames = vec![
        Arc::new(patterned(151, 111, 1)?),
        Arc::new(patterned(151, 111, 2)?),
        Arc::new(patterned(151, 111, 3)?),
    ];
    let template = frames[1]
        .view()
        .crop(PixelRect::new(41, 51, 54, 64)?)?
        .to_owned()?;
    let path = directory.path().join("template.png");
    codec::save_png(&path, &template)?;
    let frame_paths = frames
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let p = directory.path().join(format!("stored-{i}.png"));
            codec::save_png(&p, f)?;
            Ok(p)
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    for overlay in [false, true] {
        for paused in [false, true] {
            let raw = json!({"dry_run":false,"overlay":overlay,"capture":{"fps":8,"preview_scale":0.7},"farm":{"name":"한글","templates":[{"name":"first","path":path,"threshold":-1.0,"action":"click_center"},{"name":"second","path":path,"threshold":-1.0,"action":"click_center"}]}});
            let config_path = directory.path().join("config.json");
            std::fs::write(&config_path, serde_json::to_vec(&raw)?)?;
            let config = BotConfig::from_value(raw)?;
            let settings = PreviewConfig::from_config(&config, root)?;
            let expected = support::legacy_expectation(
                "core-oracle",
                &json!({"op":"run","config":config_path,"frames":frame_paths,"paused":[false,paused,false]}),
            )?;
            let mut env = fake(frames.clone(), vec![false, paused, false], None);
            let stop = env.stop.clone();
            let gate = env.gate.clone();
            let latest = PreviewFrames::default();
            let logs = std::cell::RefCell::new(Vec::new());
            assert_eq!(
                core::run(&mut env, &settings, &stop, &gate, &latest, |_, s| logs
                    .borrow_mut()
                    .push(s.to_owned()))?,
                0
            );
            assert!(env.closed);
            assert_eq!(json!(env.clicks), expected["clicks"]);
            assert_eq!(json!(env.waits), expected["sleeps"]);
            assert_eq!(
                json!(
                    env.recorded
                        .iter()
                        .map(|f| f.pixels().to_vec())
                        .collect::<Vec<_>>()
                ),
                expected["records"]
            );
            assert_eq!(
                json!([
                    env.size.ok_or("start size")?.width(),
                    env.size.ok_or("start size")?.height()
                ]),
                expected["record_size"]
            );
            let source_logs = expected["logs"]
                .as_array()
                .ok_or("logs")?
                .iter()
                .filter_map(Value::as_str)
                .filter(|l| l.starts_with("Matched "))
                .collect::<Vec<_>>();
            assert_eq!(
                logs.borrow()
                    .iter()
                    .filter(|l| l.starts_with("Matched "))
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                source_logs
            );
            if overlay {
                let preview = latest.take()?.ok_or("preview")?;
                assert_eq!(
                    json!(preview.pixels()),
                    expected["previews"]
                        .as_array()
                        .ok_or("previews")?
                        .last()
                        .ok_or("last")?
                        .clone()
                );
            } else {
                assert!(latest.take()?.is_none());
            }
        }
    }
    let settings =
        PreviewConfig::from_config(&BotConfig::from_value(json!({"overlay":false}))?, root)?;
    let mut dry = fake(frames.clone(), vec![false; 3], None);
    let stop = dry.stop.clone();
    let gate = dry.gate.clone();
    let mut dry_settings = PreviewConfig::from_config(
        &BotConfig::from_value(
            json!({"dry_run":true,"overlay":false,"farm":{"templates":[{"name":"dry","path":path,"threshold":-1.0,"action":"click_center"}]}}),
        )?,
        root,
    )?;
    let dry_logs = std::cell::RefCell::new(Vec::new());
    core::run(
        &mut dry,
        &dry_settings,
        &stop,
        &gate,
        &PreviewFrames::default(),
        |logger, s| {
            dry_logs
                .borrow_mut()
                .push((logger.to_owned(), s.to_owned()))
        },
    )?;
    assert!(dry.clicks.is_empty());
    assert!(dry.closed);
    assert_eq!(
        dry_logs
            .borrow()
            .iter()
            .filter(|(logger, s)| logger == "diablo2.controller" && s.starts_with("Dry-run click"))
            .count(),
        2
    );
    dry_settings.log_level = 40;
    let mut dry = fake(frames.clone(), vec![false; 3], None);
    let stop = dry.stop.clone();
    let gate = dry.gate.clone();
    core::run(
        &mut dry,
        &dry_settings,
        &stop,
        &gate,
        &PreviewFrames::default(),
        |_, _| panic!("INFO must be filtered"),
    )?;
    for failure in ["capture", "start", "record", "close"] {
        let mut env = fake(frames.clone(), vec![false; 3], Some(failure));
        let stop = env.stop.clone();
        let gate = env.gate.clone();
        let result = core::run(
            &mut env,
            &settings,
            &stop,
            &gate,
            &PreviewFrames::default(),
            |_, _| {},
        );
        assert!(result.is_err());
        assert!(env.closed);
        if failure == "capture" {
            assert!(
                result
                    .err()
                    .ok_or("error")?
                    .to_string()
                    .contains("cleanup also failed")
            );
        }
    }
    let mut settings = settings;
    settings.rules.push(d2bot::common::detectors::TemplateRule {
        name: "missing".into(),
        path: directory.path().join("absent.png"),
        threshold: 0.9,
        action: "log".into(),
    });
    let mut env = fake(frames, vec![false; 3], None);
    let stop = env.stop.clone();
    let gate = env.gate.clone();
    let result = core::run(
        &mut env,
        &settings,
        &stop,
        &gate,
        &PreviewFrames::default(),
        |_, _| {},
    );
    assert!(result.is_err());
    assert!(env.closed);
    assert!(env.size.is_none());
    Ok(())
}

//! 템플릿 미리보기의 순서/화면 선택을 보존하고 효과와 종료 소유권을 주입한다.
pub mod native;
use crate::{
    common::{
        config::BotConfig,
        controller::InputGate,
        detectors::{TemplateMatcher, TemplateRule, draw_overlay},
        frame::{BgrFrame, Size},
        vision_ops::resize,
        workers::Cancellation,
    },
    runs::RunError,
};
use serde_json::Value;
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

pub trait PreviewEnvironment {
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError>;
    fn target(&self) -> String;
    fn to_desktop(&self, point: [f64; 2]) -> Result<[i64; 2], RunError>;
    fn click(&mut self, point: [i64; 2]) -> Result<(), RunError>;
    fn start(&mut self, size: Size) -> Result<(), RunError>;
    fn record(&mut self, frame: &BgrFrame) -> Result<(), RunError>;
    fn now(&self) -> f64;
    fn wait(&mut self, seconds: f64) -> Result<(), RunError>;
    fn close(&mut self) -> Result<(), RunError>;
}
#[derive(Clone, Default)]
pub struct PreviewFrames(Arc<Mutex<Option<Arc<BgrFrame>>>>);
impl PreviewFrames {
    pub fn publish(&self, frame: BgrFrame) -> Result<(), RunError> {
        *self
            .0
            .lock()
            .map_err(|_| RunError::Runtime("preview frame lock poisoned".into()))? =
            Some(Arc::new(frame));
        Ok(())
    }
    pub fn take(&self) -> Result<Option<Arc<BgrFrame>>, RunError> {
        Ok(self
            .0
            .lock()
            .map_err(|_| RunError::Runtime("preview frame lock poisoned".into()))?
            .take())
    }
}
pub struct PreviewConfig {
    pub name: String,
    pub dry_run: bool,
    pub overlay: bool,
    pub scale: f64,
    pub interval: f64,
    pub rules: Vec<TemplateRule>,
    pub introduction: Vec<String>,
    pub log_level: u8,
}
fn required<'a>(value: &'a Value, name: &str) -> Result<&'a str, RunError> {
    value
        .as_str()
        .ok_or_else(|| RunError::Runtime(format!("{name} must be text")))
}
impl PreviewConfig {
    pub fn from_config(config: &BotConfig, root: &Path) -> Result<Self, RunError> {
        let fail = || RunError::Runtime("preview settings missing or invalid".into());
        let capture = config.section("capture").ok_or_else(fail)?;
        let fps = capture["fps"]
            .as_f64()
            .filter(|n| n.is_finite())
            .ok_or_else(fail)?;
        let scale = capture["preview_scale"]
            .as_f64()
            .filter(|n| n.is_finite() && *n > 0.0)
            .ok_or_else(fail)?;
        let farm = config.farm();
        let rules = farm["templates"]
            .as_array()
            .ok_or_else(fail)?
            .iter()
            .map(|v| {
                Ok(TemplateRule {
                    name: required(&v["name"], "template name")?.into(),
                    path: root.join(required(&v["path"], "template path")?),
                    threshold: v["threshold"]
                        .as_f64()
                        .filter(|n| n.is_finite())
                        .ok_or_else(fail)?,
                    action: required(&v["action"], "template action")?.into(),
                })
            })
            .collect::<Result<Vec<_>, RunError>>()?;
        let mut labels = Vec::new();
        if let Some(items) = config
            .section("shared_loot")
            .and_then(|v| v["fixed_items"].as_array())
        {
            for item in items {
                labels.push(required(&item["label"], "fixed item label")?.to_owned());
            }
        }
        for label in farm["loot"]["keep_labels"].as_array().ok_or_else(fail)? {
            labels.push(required(label, "loot label")?.to_owned());
        }
        let name = required(&farm["name"], "farm name")?.to_owned();
        let level =
            required(config.section("log_level").ok_or_else(fail)?, "log level")?.to_uppercase();
        let log_level = match level.as_str() {
            "NOTSET" => 0,
            "DEBUG" => 10,
            "WARNING" | "WARN" => 30,
            "ERROR" => 40,
            "CRITICAL" | "FATAL" => 50,
            _ => 20,
        };
        Ok(Self {
            introduction: vec![
                format!("Loaded farm profile: {name}"),
                format!("Farm goal: {}", required(&farm["goal"], "farm goal")?),
                format!(
                    "Hunting objective: {}",
                    required(&farm["hunting"]["objective"], "hunting objective")?
                ),
                format!("Loot keep labels: {}", labels.join(", ")),
                format!("Run-specific rules: {}", farm["run_specific_rules"]),
            ],
            name,
            dry_run: config
                .section("dry_run")
                .and_then(Value::as_bool)
                .ok_or_else(fail)?,
            overlay: config
                .section("overlay")
                .and_then(Value::as_bool)
                .ok_or_else(fail)?,
            scale,
            interval: 1.0 / fps.max(1.0),
            rules,
            log_level,
        })
    }
    pub fn status(&self, gate: &InputGate) -> String {
        format!(
            "{} | watching | {} | {}",
            self.name,
            if self.dry_run { "DRY-RUN" } else { "LIVE" },
            if gate.paused() { "PAUSED" } else { "RUNNING" }
        )
    }
}
pub fn run(
    environment: &mut impl PreviewEnvironment,
    settings: &PreviewConfig,
    stop: &Cancellation,
    gate: &InputGate,
    frames: &PreviewFrames,
    emit: impl Fn(&str, &str),
) -> Result<u8, RunError> {
    // 초기화 실패도 동일한 소유자의 정리 경로를 지난다.
    let result = (|| {
        let matcher = TemplateMatcher::load(settings.rules.clone())
            .map_err(crate::actions::ActionError::from)?;
        let first = environment.capture()?;
        environment.start(first.size())?;
        if settings.log_level <= 20 {
            emit(
                "diablo2.bot",
                &format!("Watching monitor region: {}", environment.target()),
            );
            for line in &settings.introduction {
                emit("diablo2.bot", line);
            }
        }
        while !stop.is_cancelled() && !gate.stopped() {
            let started = environment.now();
            let frame = environment.capture()?;
            let matches = matcher
                .scan(frame.view())
                .map_err(crate::actions::ActionError::from)?;
            for found in &matches {
                if settings.log_level <= 20 {
                    emit(
                        "diablo2.bot",
                        &format!(
                            "Matched {} at {:.3} -> {}",
                            found.name, found.confidence, found.action
                        ),
                    );
                }
                if gate.interrupted() || stop.is_cancelled() {
                    break;
                }
                if found.action == "click_center" {
                    let point = [
                        (found.top_left[0] + found.bottom_right[0]) / 2,
                        (found.top_left[1] + found.bottom_right[1]) / 2,
                    ]
                    .map(|n| n as f64);
                    let point = environment.to_desktop(point)?;
                    if settings.dry_run {
                        if settings.log_level <= 20 {
                            emit(
                                "diablo2.controller",
                                &format!("Dry-run click at ({}, {})", point[0], point[1]),
                            );
                        }
                    } else {
                        environment.click(point)?;
                    }
                }
            }
            let display = if settings.overlay {
                draw_overlay(&frame, &matches, &settings.status(gate))
                    .map_err(crate::actions::ActionError::from)?
            } else {
                (*frame).clone()
            };
            if settings.overlay {
                frames.publish(
                    resize::scale_linear(display.view(), settings.scale)
                        .map_err(crate::actions::ActionError::from)?,
                )?;
            }
            environment.record(&display)?;
            environment.wait((settings.interval - (environment.now() - started)).max(0.0))?;
        }
        Ok(0)
    })();
    let close = environment.close();
    match (result, close) {
        (Ok(code), Ok(())) => Ok(code),
        (Err(e), Ok(())) | (Ok(_), Err(e)) => Err(e),
        (Err(e), Err(c)) => Err(e.with_cleanup(c)),
    }
}

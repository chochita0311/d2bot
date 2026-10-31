use super::*;
use crate::{
    actions::town_loop::{TownOptions, TownPhase, TownPlanner},
    common::{frame_geometry::PhysicalTarget, realtime::CapturedFrame},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TownOutcome {
    Stopped,
    Passed,
    Failed,
}
#[derive(Debug, Clone, Serialize)]
pub struct TownRunReport {
    pub result: TownOutcome,
    pub planner: crate::actions::town_loop::TownState,
    pub geometry: (usize, i32, i32, usize, usize),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorShift {
    Unrelated,
    Pending,
    Confirmed,
}

pub fn accept_waypoint_cursor_shift<B: ActionBackend>(
    context: &mut ActionContext<B>,
    vision: &mut TownVision,
    planner: &TownPlanner,
    frame: Arc<BgrFrame>,
    window_width: usize,
) -> Result<CursorShift, ActionError> {
    use TownPhase::*;
    let phase = planner.data.phase;
    let opening = matches!(phase, WaitAct1Panel | ReturnPanel | Act2Spawn);
    let closing = matches!(phase, DestinationArrival | Act1Return);
    let Some(last) = context.last_pointer() else {
        return Ok(CursorShift::Unrelated);
    };
    if !(opening || closing) {
        return Ok(CursorShift::Unrelated);
    }
    let current = context.pointer()?;
    let dx = (current[0] - last[0]) as f64;
    let dy = current[1] - last[1];
    if (dx - window_width as f64 * 0.1875 * if opening { 1.0 } else { -1.0 }).abs() > 8.0
        || dy.abs() > 3
    {
        return Ok(CursorShift::Unrelated);
    }
    vision.set_frame(frame);
    vision.normalize()?;
    let menu = vision.find("waypoint_header")?.is_some();
    let arrival = if phase == Act1Return {
        "area_act1"
    } else if planner.data.destination == super::super::town_loop::Destination::Town {
        "area_act2"
    } else {
        "area_arcane"
    };
    let confirmed = if opening {
        menu && (vision.find("list_act1")?.is_some() || vision.find("list_act2")?.is_some())
    } else {
        !menu && vision.find("hud")?.is_some() && vision.find(arrival)?.is_some()
    };
    if confirmed {
        context.emit(
            EventLevel::Info,
            format!(
                "Confirmed waypoint-panel cursor shift: ({}, {}) -> ({}, {})",
                last[0], last[1], current[0], current[1]
            ),
        );
        context.accept_pointer(current);
        Ok(CursorShift::Confirmed)
    } else {
        Ok(CursorShift::Pending)
    }
}
struct Evidence {
    directory: PathBuf,
}
impl Evidence {
    fn create(directory: &Path) -> Result<Self, ActionError> {
        if let Some(parent) = directory.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir(directory)?;
        Ok(Self {
            directory: directory.into(),
        })
    }
    fn event(&self, event: &Value) -> Result<(), ActionError> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.directory.join("events.jsonl"))?;
        serde_json::to_writer(&mut file, event)?;
        file.write_all(b"\n")?;
        Ok(())
    }
    fn frame(&self, name: &str, frame: &BgrFrame) -> Result<(), ActionError> {
        Ok(codec::save_png(self.directory.join(name), frame)?)
    }
    fn json(&self, name: &str, value: &impl Serialize) -> Result<(), ActionError> {
        fs::write(self.directory.join(name), serde_json::to_vec_pretty(value)?)?;
        Ok(())
    }
}

/// The caller selects the same timestamped recording directory as the existing
/// entry point. Creation is exclusive; a prior session's evidence is never reused.
pub fn run_town<B: ActionBackend>(
    context: &mut ActionContext<B>,
    vision: &mut TownVision,
    options: TownOptions,
    resume_town: bool,
    directory: &Path,
) -> Result<TownRunReport, ActionError> {
    let result = (|| {
        let mut planner = TownPlanner::new(options, |name| vision.calibrated(name))?;
        context.prepare()?;
        if resume_town {
            planner.data.phase = TownPhase::Act1Spawn;
        }
        context.start_town_guard()?;
        let (id, target) = context.town_geometry()?;
        let evidence = Evidence::create(directory)?;
        town_loop(context, vision, &mut planner, &evidence, (id, target))
    })();
    context.finish_logged(result, "Run lifecycle")
}
fn town_loop<B: ActionBackend>(
    context: &mut ActionContext<B>,
    vision: &mut TownVision,
    planner: &mut TownPlanner,
    evidence: &Evidence,
    (id, target): (usize, PhysicalTarget),
) -> Result<TownRunReport, ActionError> {
    let mut previous_phase = None;
    let mut timings = Vec::new();
    let mut cursor_pending_since = None;
    let mut previous_sample_start = None;
    let mut last_packet: Option<CapturedFrame> = None;
    let mut outcome = TownOutcome::Stopped;
    let result: Result<(), ActionError> = (|| {
        while !context.cancelled() {
            context.town_guard()?;
            let sample_start = context.monotonic_now()?;
            let interval_ms =
                previous_sample_start.map(|previous| (sample_start - previous) * 1000.0);
            previous_sample_start = Some(sample_start);
            last_packet = Some(context.capture()?);
            let packet = last_packet
                .as_ref()
                .ok_or_else(|| ActionError::runtime("town frame missing"))?;
            let observed_at = context.monotonic_now()?;
            let shift = accept_waypoint_cursor_shift(
                context,
                vision,
                planner,
                packet.frame.clone(),
                target.size().width(),
            )?;
            if shift == CursorShift::Pending {
                let now = context.monotonic_now()?;
                let pending = *cursor_pending_since.get_or_insert(now);
                if now - pending < 0.45 {
                    context.wait(0.04)?;
                    continue;
                }
            } else {
                cursor_pending_since = None;
            }
            if context.user_interrupted()? {
                break;
            }
            vision.set_frame(packet.frame.clone());
            let action = planner.step(vision, context.now()?)?;
            let planned_at = context.monotonic_now()?;
            let phase = planner.data.phase;
            let mut dispatch_at = None;
            if let Some(action) = &action {
                context.town_guard()?;
                dispatch_at = Some(context.monotonic_now()?);
                dispatch_town_action(context, packet.frame.size(), action)?;
            }
            let dispatch = dispatch_at.filter(|t| *t != 0.0);
            let sample = json!({"interval_ms":interval_ms,"capture_ms":(observed_at-sample_start)*1000.0,"vision_ms":(planned_at-observed_at)*1000.0,"input_ms":if let Some(at)=dispatch{(context.monotonic_now()?-at)*1000.0}else{0.0},"observation_to_input_ms":dispatch.map(|at|(at-observed_at)*1000.0)});
            timings.push(sample.clone());
            if action.is_some() || Some(phase) != previous_phase {
                let filename = format!("{:03}-{}.png", planner.data.action_number, phase.name());
                evidence.frame(&filename, &packet.frame)?;
                evidence.event(&json!({"time":context.now()?,"phase":phase,"completed":planner.data.completed,"town_layout":planner.data.town_layout,"action":action,"frame":filename,"timing":sample,"pointer":context.last_pointer()}))?;
                context.emit(
                    EventLevel::Info,
                    format!(
                        "Town loop: {}, completed {}/{}.",
                        phase.name(),
                        planner.data.completed,
                        planner.data.repeat_count
                    ),
                );
                previous_phase = Some(phase);
            }
            if phase == TownPhase::Done {
                outcome = TownOutcome::Passed;
                let label = if planner.data.arrival_only {
                    "Arcane entry"
                } else {
                    "Town loop"
                };
                context.emit(
                    EventLevel::Info,
                    format!(
                        "{label} completed {} run(s). Evidence: {}",
                        planner.data.completed,
                        evidence.directory.display()
                    ),
                );
                return Ok(());
            }
            context.wait(0.04)?;
        }
        Ok(())
    })();
    let mut cleanup_errors = Vec::new();
    if let Err(error) = &result {
        outcome = TownOutcome::Failed;
        if let Some(packet) = &last_packet {
            record_cleanup(
                evidence.frame("failure-frame.png", &packet.frame),
                &mut cleanup_errors,
            );
        }
        record_cleanup(
            context.now().and_then(|now| {
                evidence.event(
                    &json!({"time":now,"error":error.to_string(),"phase":planner.data.phase}),
                )
            }),
            &mut cleanup_errors,
        );
    }
    record_cleanup(context.release_town_mouse(), &mut cleanup_errors);
    if outcome == TownOutcome::Stopped
        && let Some(packet) = &last_packet
    {
        record_cleanup(
            evidence.frame("stopped-frame.png", &packet.frame),
            &mut cleanup_errors,
        );
    }
    if !cleanup_errors.is_empty() {
        outcome = TownOutcome::Failed;
    }
    let report = TownRunReport {
        result: outcome,
        planner: planner.data.clone(),
        geometry: (
            id,
            target.left,
            target.top,
            target.size().width(),
            target.size().height(),
        ),
    };
    record_cleanup(evidence.json("result.json", &report), &mut cleanup_errors);
    record_cleanup(evidence.json("timings.json", &timings), &mut cleanup_errors);
    if let Some(reason) = context.stop_reason() {
        record_cleanup(
            context
                .now()
                .and_then(|now| evidence.event(&json!({"time":now,"stop_reason":reason}))),
            &mut cleanup_errors,
        );
    }
    match (result, cleanup_errors.is_empty()) {
        (Ok(()), true) => Ok(report),
        (Err(error), true) => Err(error),
        (result, false) => Err(ActionError::runtime(format!(
            "{}town finalization failed: {}",
            result.err().map(|e| format!("{e}; ")).unwrap_or_default(),
            cleanup_errors.join("; ")
        ))),
    }
}
fn record_cleanup(result: Result<(), ActionError>, errors: &mut Vec<String>) {
    if let Err(error) = result {
        errors.push(error.to_string());
    }
}

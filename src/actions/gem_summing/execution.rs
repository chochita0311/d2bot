use super::{
    FrameCounts, GemCounts, GemRecognition, build_plan, consensus_counts, successful_combine,
};
use crate::actions::{
    ActionError, EventLevel, MatchResult,
    runtime::{ActionBackend, ActionContext},
};
use serde::Serialize;
use std::collections::BTreeSet;

const UI_DELAY: [f64; 2] = [0.10, 0.18];
const ACTION_DELAY: [f64; 2] = [0.12, 0.22];
const TRANSMUTE_DELAY: [f64; 2] = [0.16, 0.26];
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GemOutcome {
    Completed,
    Interrupted,
}

pub fn run_session<B: ActionBackend>(
    vision: &dyn GemRecognition,
    context: &mut ActionContext<B>,
) -> Result<GemOutcome, ActionError> {
    let result = context.prepare().and_then(|_| run(vision, context));
    context.finish_logged(result, "Gem summing")
}
pub fn scan_counts<B: ActionBackend>(
    vision: &dyn GemRecognition,
    context: &mut ActionContext<B>,
) -> Result<GemCounts, ActionError> {
    let result = (|| {
        let packet = context.capture()?;
        let panel = vision
            .locate_stash_panel(packet.frame.view())?
            .ok_or_else(|| {
                ActionError::runtime(
                    "Could not find the opened gem stash panel. Open stash + cube, then try again.",
                )
            })?;
        consensus(vision, context, panel, None)
    })();
    context.finish(result)
}
fn run<B: ActionBackend>(
    vision: &dyn GemRecognition,
    ctx: &mut ActionContext<B>,
) -> Result<GemOutcome, ActionError> {
    let initial = ctx.capture()?;
    let mut panel=vision.locate_stash_panel(initial.frame.view())?.ok_or_else(||ActionError::runtime("Could not find the opened gem stash panel in the starting screen. Open stash + cube first, then start again."))?;
    vision.locate_cube_button(initial.frame.view())?.ok_or_else(||ActionError::runtime("Could not find the Horadric Cube transmute button in the starting screen. Open stash + cube first, then start again."))?;
    drop(initial);
    ctx.click_frame(
        [
            panel.top_left[0] as f64 + 260.0,
            panel.top_left[1] as f64 + 91.0,
        ],
        "left",
        false,
    )?;
    ctx.sleep_range(UI_DELAY)?;
    let mut tracked = None;
    let mut needs_resync = true;
    let mut successful_steps = 0;
    let mut blocked = BTreeSet::new();
    loop {
        if ctx.cancelled() {
            ctx.emit(EventLevel::Warning, "Gem summing interrupted by user.");
            return Ok(GemOutcome::Interrupted);
        }
        park(ctx, panel)?;
        ctx.sleep_range(UI_DELAY)?;
        restash(vision, ctx, panel)?;
        park(ctx, panel)?;
        ctx.sleep_range(UI_DELAY)?;
        let packet = ctx.capture()?;
        panel = vision
            .locate_stash_panel(packet.frame.view())?
            .ok_or_else(|| {
                ActionError::runtime("Lost the opened gem stash panel during gem summing.")
            })?;
        let button = vision
            .locate_cube_button(packet.frame.view())?
            .ok_or_else(|| {
                ActionError::runtime("Lost the Horadric Cube transmute button during gem summing.")
            })?;
        drop(packet);
        if needs_resync || tracked.is_none() || successful_steps >= 3 {
            tracked = Some(consensus(vision, ctx, panel, tracked.as_ref())?);
            successful_steps = 0;
        }
        let counts = tracked
            .as_ref()
            .ok_or_else(|| ActionError::runtime("tracked gem counts missing"))?;
        let plan = build_plan(counts, &blocked)?;
        let Some(step) = plan.first() else {
            ctx.emit(
                EventLevel::Info,
                "All non-perfect gem stacks are now below 10.",
            );
            return Ok(GemOutcome::Completed);
        };
        ctx.emit(
            EventLevel::Info,
            format!(
                "{} {}: running {} combine(s) from tracked counts {}",
                step.gem_name, step.level_name, step.repeats, counts[step.row][step.col]
            ),
        );
        let mut completed = true;
        for repeat in 0..step.repeats {
            if ctx.user_interrupted()? {
                ctx.emit(EventLevel::Warning, "Gem summing interrupted by user.");
                return Ok(GemOutcome::Interrupted);
            }
            let slot = [
                (panel.top_left[0] + 82 + step.col * 61 + 23) as f64,
                (panel.top_left[1] + 117 + step.row * 50 + 23) as f64,
            ];
            ctx.click_frame(slot, "right", true)?;
            ctx.sleep_range(ACTION_DELAY)?;
            ctx.click_frame(
                [
                    (button.top_left[0] + button.width / 2) as f64,
                    (button.top_left[1] + button.height / 2) as f64,
                ],
                "left",
                false,
            )?;
            ctx.sleep_range(TRANSMUTE_DELAY)?;
            let occupied = occupied(vision, ctx, panel)?;
            if occupied.len() != 1 {
                ctx.emit(EventLevel::Warning,format!("{} {}: cube had {} item(s) after transmute on combine {}/{}; skipping this slot.",step.gem_name,step.level_name,occupied.len(),repeat+1,step.repeats));
                blocked.insert((step.col, step.row));
                restash(vision, ctx, panel)?;
                completed = false;
                break;
            }
            let result_desktop = ctx.frame_point(occupied[0].map(|n| n as f64))?;
            let packet = ctx.capture()?;
            let result_frame = ctx.desktop_point_to_frame(result_desktop)?;
            let icon = vision.cube_result_icon(packet.frame.view(), panel, result_frame)?;
            drop(packet);
            if !vision.expected_result(step.col, step.row + 1, icon.view())? {
                ctx.emit(EventLevel::Warning,format!("{} {}: result did not match the expected next gem on combine {}/{}; skipping this slot.",step.gem_name,step.level_name,repeat+1,step.repeats));
                blocked.insert((step.col, step.row));
                restash(vision, ctx, panel)?;
                completed = false;
                break;
            }
            ctx.click_absolute(result_desktop, "left", true)?;
            ctx.sleep_range(ACTION_DELAY)?;
            restash(vision, ctx, panel)?;
            tracked = Some(successful_combine(
                tracked
                    .as_ref()
                    .ok_or_else(|| ActionError::runtime("tracked gem counts missing"))?,
                step.row,
                step.col,
            )?);
        }
        needs_resync = !completed;
        if completed {
            successful_steps += 1;
            ctx.emit(
                EventLevel::Info,
                format!(
                    "Tracked counts after {} {}: {:?}",
                    step.gem_name,
                    step.level_name,
                    tracked
                        .as_ref()
                        .ok_or_else(|| ActionError::runtime("tracked gem counts missing"))?
                ),
            );
        }
    }
}
fn consensus<B: ActionBackend>(
    vision: &dyn GemRecognition,
    ctx: &mut ActionContext<B>,
    panel: MatchResult,
    fallback: Option<&GemCounts>,
) -> Result<GemCounts, ActionError> {
    let mut samples = Vec::new();
    for index in 0..3 {
        let packet = ctx.capture()?;
        samples.push(vision.read_frame_counts(packet.frame.view(), panel)?);
        drop(packet);
        if index < 2 {
            ctx.sleep_range([0.02, 0.05])?;
        }
    }
    let samples: [FrameCounts; 3] = samples
        .try_into()
        .map_err(|_| ActionError::runtime("three gem scan samples required"))?;
    let (counts, events) = consensus_counts(&samples, fallback)?;
    for event in events {
        ctx.emit(event.level, event.message);
    }
    Ok(counts)
}
fn park<B: ActionBackend>(
    ctx: &mut ActionContext<B>,
    panel: MatchResult,
) -> Result<(), ActionError> {
    let point = ctx.frame_point([
        panel.top_left[0] as f64 + 455.0,
        panel.top_left[1] as f64 + 610.0,
    ])?;
    ctx.move_absolute(point)
}
fn occupied<B: ActionBackend>(
    vision: &dyn GemRecognition,
    ctx: &mut ActionContext<B>,
    panel: MatchResult,
) -> Result<Vec<[usize; 2]>, ActionError> {
    let packet = ctx.capture()?;
    Ok(vision
        .occupied_cube_cells(packet.frame.view(), panel)?
        .into_iter()
        .map(|(point, _)| point)
        .collect())
}
fn restash<B: ActionBackend>(
    vision: &dyn GemRecognition,
    ctx: &mut ActionContext<B>,
    panel: MatchResult,
) -> Result<(), ActionError> {
    for _ in 0..6 {
        let cells = occupied(vision, ctx, panel)?;
        if cells.is_empty() {
            return Ok(());
        }
        for point in cells {
            if ctx.user_interrupted()? {
                return Ok(());
            }
            ctx.click_frame(point.map(|n| n as f64), "left", true)?;
            ctx.sleep_range(ACTION_DELAY)?;
        }
    }
    ctx.emit(
        EventLevel::Warning,
        "Cube cleanup left items behind after several attempts.",
    );
    Ok(())
}

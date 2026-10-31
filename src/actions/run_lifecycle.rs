//! Existing room/menu lifecycle over owned action effects. Town evidence and
//! native dispatch are connected separately from these input-free contracts.
use super::{
    ActionError, EventLevel, MatchResult, locate,
    runtime::{ActionBackend, ActionContext},
    session::SessionOutcome,
    town_loop::{Difficulty, REFERENCE_SIZE, TownAction, TownActionKind, TownSight, TownVision},
};
mod town;
use crate::common::{
    frame::{BgrFrame, BgrView, Size},
    vision_ops::codec,
};
use std::{collections::HashMap, path::Path, sync::Arc};
pub use town::{CursorShift, TownOutcome, TownRunReport, accept_waypoint_cursor_shift, run_town};

pub trait LifecycleSight {
    fn menu_label(
        &mut self,
        frame: Arc<BgrFrame>,
        label: &str,
        exclude_difficulty: bool,
    ) -> Result<bool, ActionError>;
}
impl LifecycleSight for TownVision {
    fn menu_label(
        &mut self,
        frame: Arc<BgrFrame>,
        label: &str,
        exclude_difficulty: bool,
    ) -> Result<bool, ActionError> {
        self.set_frame(frame);
        self.normalize()?;
        Ok(self.find(label)?.is_some()
            && (!exclude_difficulty || self.find("difficulty_header")?.is_none()))
    }
}
pub struct RoomTemplates {
    pub difficulty: HashMap<String, BgrFrame>,
    pub loading: BgrFrame,
    pub exit: BgrFrame,
}
impl RoomTemplates {
    pub fn load(root: impl AsRef<Path>) -> Result<Self, ActionError> {
        let root = root.as_ref();
        let load = |relative: &str| -> Result<BgrFrame, ActionError> {
            let relative = format!("assets/ui/town-loop/remastered-ko-window/{relative}");
            let path = root.join(&relative);
            if !path.exists() {
                return Err(ActionError::runtime(format!(
                    "Required asset is missing: {relative}"
                )));
            }
            codec::load_color(path).map_err(|error| match error {
                crate::common::vision_ops::VisionError::Io(error) => ActionError::Io(error),
                _ => ActionError::runtime(format!("Failed to load asset: {relative}")),
            })
        };
        let difficulty = [
            ("normal", "difficulty-normal-button-241x64.png"),
            ("nightmare", "difficulty-nightmare-button-250x66.png"),
            ("hell", "difficulty-hell-button-245x69.png"),
        ]
        .into_iter()
        .map(|(name, file)| Ok((name.into(), load(&format!("templates/{file}"))?)))
        .collect::<Result<_, ActionError>>()?;
        Ok(Self {
            difficulty,
            loading: load("references/loading-window.png")?,
            exit: load("references/exit-menu-window.png")?,
        })
    }
}
pub fn locate_template(
    frame: BgrView<'_>,
    template: BgrView<'_>,
    threshold: f64,
) -> Result<Option<MatchResult>, ActionError> {
    if frame.size().width() < template.size().width()
        || frame.size().height() < template.size().height()
    {
        return Ok(None);
    }
    locate(frame, template, threshold)
}
pub fn wait_for_template<B: ActionBackend>(
    context: &mut ActionContext<B>,
    template: BgrView<'_>,
    threshold: f64,
    timeout: f64,
    label: &str,
) -> Result<MatchResult, ActionError> {
    let deadline = context.now()? + timeout;
    let mut best: Option<MatchResult> = None;
    while context.now()? < deadline {
        if context.user_interrupted()? {
            return Err(ActionError::runtime("stopped by user interference"));
        }
        let packet = context.capture()?;
        if let Some(found) = locate_template(packet.frame.view(), template, threshold)? {
            return Ok(found);
        }
        if let Some(found) = locate_template(packet.frame.view(), template, 0.0)?
            && best.is_none_or(|b| found.score > b.score)
        {
            best = Some(found);
        }
        context.raw_sleep(0.1)?;
    }
    Err(ActionError::runtime(format!(
        "Timed out waiting for {label}. Best match score was {:.3}.",
        best.map_or(-1.0, |b| b.score)
    )))
}
pub fn wait_until_template_missing<B: ActionBackend>(
    context: &mut ActionContext<B>,
    template: BgrView<'_>,
    threshold: f64,
    timeout: f64,
    label: &str,
) -> Result<(), ActionError> {
    let deadline = context.now()? + timeout;
    while context.now()? < deadline {
        if context.user_interrupted()? {
            return Err(ActionError::runtime("stopped by user interference"));
        }
        let packet = context.capture()?;
        if locate_template(packet.frame.view(), template, threshold)?.is_none() {
            return Ok(());
        }
        context.raw_sleep(0.1)?;
    }
    Err(ActionError::runtime(format!(
        "Timed out waiting for {label} to disappear."
    )))
}
pub fn wait_for_menu<B: ActionBackend>(
    context: &mut ActionContext<B>,
    vision: &mut impl LifecycleSight,
    label: &str,
    exclude_difficulty: bool,
) -> Result<(), ActionError> {
    let deadline = context.monotonic_now()? + 20.0;
    while context.monotonic_now()? < deadline {
        context.ensure_input_allowed()?;
        let packet = context.capture()?;
        if vision.menu_label(packet.frame, label, exclude_difficulty)? {
            return Ok(());
        }
        context.wait(0.05)?;
    }
    Err(ActionError::runtime(format!(
        "Required shared menu label was not observed: {label}"
    )))
}
pub fn create_room<B: ActionBackend>(
    context: &mut ActionContext<B>,
    vision: &mut impl LifecycleSight,
    templates: &RoomTemplates,
    difficulty: Difficulty,
    run: impl std::fmt::Display,
) -> Result<(), ActionError> {
    context.emit(
        EventLevel::Info,
        format!("Lifecycle run {run}: creating room."),
    );
    wait_for_menu(context, vision, "play", true)?;
    context.lifecycle_click_frame([808.0, 1010.0])?;
    wait_for_menu(context, vision, "difficulty_header", false)?;
    let button = templates
        .difficulty
        .get(difficulty.name())
        .ok_or_else(|| ActionError::runtime("difficulty template missing"))?;
    let found = wait_for_template(
        context,
        button.view(),
        0.88,
        10.0,
        &format!("{} difficulty button", difficulty.name()),
    )?;
    context.lifecycle_click_frame([
        (found.top_left[0] + found.width / 2) as f64,
        (found.top_left[1] + found.height / 2) as f64,
    ])?;
    wait_for_template(
        context,
        templates.loading.view(),
        0.9,
        10.0,
        "loading screen",
    )?;
    wait_until_template_missing(
        context,
        templates.loading.view(),
        0.9,
        20.0,
        "loading screen",
    )?;
    context.sleep_range([0.8, 1.2])?;
    context.emit(
        EventLevel::Info,
        format!("Lifecycle run {run}: room created."),
    );
    Ok(())
}
pub fn exit_room<B: ActionBackend>(
    context: &mut ActionContext<B>,
    vision: &mut impl LifecycleSight,
    templates: &RoomTemplates,
    run: impl std::fmt::Display,
) -> Result<(), ActionError> {
    context.emit(
        EventLevel::Info,
        format!("Lifecycle run {run}: exiting room."),
    );
    context.lifecycle_press("esc")?;
    wait_for_template(context, templates.exit.view(), 0.88, 8.0, "exit menu")?;
    context.lifecycle_click_frame([959.0, 520.0])?;
    wait_for_menu(context, vision, "play", true)
}
pub fn run_rooms<B: ActionBackend>(
    context: &mut ActionContext<B>,
    vision: &mut impl LifecycleSight,
    templates: &RoomTemplates,
    repeat: Option<u64>,
    difficulty: Difficulty,
) -> Result<SessionOutcome, ActionError> {
    run_rooms_exact(
        context,
        vision,
        templates,
        repeat.map(serde_json::Number::from),
        difficulty,
    )
}
pub fn run_rooms_exact<B: ActionBackend>(
    context: &mut ActionContext<B>,
    vision: &mut impl LifecycleSight,
    templates: &RoomTemplates,
    repeat: Option<serde_json::Number>,
    difficulty: Difficulty,
) -> Result<SessionOutcome, ActionError> {
    let repeat = repeat
        .map(|number| {
            crate::common::numeric_input::integer(&number.to_string())
                .map(|n| n.to_string())
                .ok_or_else(|| ActionError::runtime("Repeat Count must be a whole number."))
        })
        .transpose()?;
    let result = (|| {
        context.prepare()?;
        let mut completed = "0".to_string();
        while !context.cancelled() {
            if repeat
                .as_ref()
                .is_some_and(|limit| reached(&completed, limit))
            {
                context.emit(
                    EventLevel::Info,
                    format!("Completed {completed} lifecycle run(s)."),
                );
                return Ok(SessionOutcome::Completed);
            }
            let run = increment(&completed);
            create_room(context, vision, templates, difficulty, &run)?;
            exit_room(context, vision, templates, &run)?;
            completed = run.clone();
            context.emit(
                EventLevel::Info,
                format!("Lifecycle run {run}: exited room and returned to character select."),
            );
        }
        Ok(SessionOutcome::Interrupted)
    })();
    context.finish_logged(result, "Run lifecycle")
}
fn reached(completed: &str, limit: &str) -> bool {
    limit.starts_with('-')
        || completed.len() > limit.len()
        || (completed.len() == limit.len() && completed >= limit)
}
fn increment(count: &str) -> String {
    let mut carry = true;
    let mut digits = count
        .bytes()
        .rev()
        .map(|digit| {
            if carry && digit == b'9' {
                b'0'
            } else if carry {
                carry = false;
                digit + 1
            } else {
                digit
            }
        })
        .collect::<Vec<_>>();
    if carry {
        digits.push(b'1');
    }
    digits.into_iter().rev().map(char::from).collect()
}
pub fn dispatch_town_action<B: ActionBackend>(
    context: &mut ActionContext<B>,
    frame_size: Size,
    action: &TownAction,
) -> Result<(), ActionError> {
    if action.kind == TownActionKind::Release {
        return context.release_town_mouse();
    }
    context.ensure_input_allowed()?;
    if !context.geometry_is_current()? {
        return Err(ActionError::runtime(
            "window geometry changed after capture",
        ));
    }
    if action.kind == TownActionKind::Key {
        context.release_town_mouse()?;
        return context.press_with_pause(
            action
                .key
                .as_deref()
                .ok_or_else(|| ActionError::runtime("town key missing"))?,
            false,
        );
    }
    let p = action
        .point
        .ok_or_else(|| ActionError::runtime("town pointer point missing"))?;
    let scaled = [
        (p[0] as f64 * frame_size.width() as f64 / REFERENCE_SIZE[0] as f64).round_ties_even(),
        (p[1] as f64 * frame_size.height() as f64 / REFERENCE_SIZE[1] as f64).round_ties_even(),
    ];
    let point = context.frame_point(scaled)?;
    context.move_town_absolute(point)?;
    context.ensure_input_allowed()?;
    if action.kind == TownActionKind::Hold {
        context.mouse_button_with_pause("left", false, false)
    } else {
        context.release_town_mouse()?;
        context.wait(0.06)?;
        context.ensure_input_allowed()?;
        context.click_with_pause("left", false)
    }
}

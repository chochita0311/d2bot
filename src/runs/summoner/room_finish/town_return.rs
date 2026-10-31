use crate::{
    actions::{
        run_lifecycle::{CursorShift, accept_waypoint_cursor_shift, dispatch_town_action},
        runtime::{ActionBackend, ActionContext},
        town_loop::{
            Destination, Difficulty, InspectionStatus, TownAction, TownActionKind, TownOptions,
            TownPhase, TownPlanner, TownSight, TownVision,
        },
    },
    common::frame::BgrFrame,
    runs::{
        RunError,
        summoner::{encounter_execution::EncounterEvidence, navigation_execution::StepEnvironment},
    },
};
use serde_json::{Value, json};
use std::sync::Arc;
#[cfg(test)]
mod tests;
pub trait TownFinishRuntime {
    fn begin(&mut self) -> Result<(), RunError>;
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError>;
    fn cursor_shift(&mut self, frame: &Arc<BgrFrame>) -> Result<CursorShift, RunError>;
    fn step(&mut self, frame: &Arc<BgrFrame>) -> Result<Option<TownAction>, RunError>;
    fn phase(&self) -> &str;
    fn snapshot(&self) -> Result<Value, RunError>;
    fn dispatch(&mut self, frame: &Arc<BgrFrame>, action: &TownAction) -> Result<(), RunError>;
    fn hidden_map(&self) -> bool;
    fn clear_hud(&mut self, frame: &Arc<BgrFrame>) -> Result<bool, RunError>;
    fn map_restore_requested(&mut self);
    fn restore_map(&mut self, frame: &Arc<BgrFrame>) -> Result<(), RunError>;
    fn release_mouse(&mut self) -> Result<(), RunError>;
}
/// 이미 선택한 필드/창과 입력 소유자를 빌린다. 이 어댑터는 별도 세션을 시작하지 않는다.
pub struct NativeTownFinish<'a, B: ActionBackend> {
    context: &'a mut ActionContext<B>,
    vision: &'a mut TownVision,
    planner: TownPlanner,
    width: Option<usize>,
}
impl<'a, B: ActionBackend> NativeTownFinish<'a, B> {
    pub fn new(
        context: &'a mut ActionContext<B>,
        vision: &'a mut TownVision,
        destination: Destination,
    ) -> Result<Self, RunError> {
        let mut planner = TownPlanner::new(
            TownOptions {
                repeat_count: 1,
                difficulty: Difficulty::Hell,
                destination,
                inspect_arcane_map: destination == Destination::Arcane,
                ..Default::default()
            },
            |name| vision.calibrated(name),
        )?;
        planner.data.phase = TownPhase::DestinationArrival;
        Ok(Self {
            context,
            vision,
            planner,
            width: None,
        })
    }
}
impl<B: ActionBackend> TownFinishRuntime for NativeTownFinish<'_, B> {
    fn begin(&mut self) -> Result<(), RunError> {
        self.context.reset_pointer();
        self.context.start_town_guard()?;
        self.width = Some(self.context.town_geometry()?.1.size().width());
        Ok(())
    }
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError> {
        Ok(self.context.capture()?.frame)
    }
    fn cursor_shift(&mut self, frame: &Arc<BgrFrame>) -> Result<CursorShift, RunError> {
        Ok(accept_waypoint_cursor_shift(
            self.context,
            self.vision,
            &self.planner,
            frame.clone(),
            self.width
                .ok_or_else(|| RunError::Runtime("town return not initialized".into()))?,
        )?)
    }
    fn step(&mut self, frame: &Arc<BgrFrame>) -> Result<Option<TownAction>, RunError> {
        self.vision.set_frame(frame.clone());
        Ok(self.planner.step(self.vision, self.context.now()?)?)
    }
    fn phase(&self) -> &str {
        self.planner.data.phase.name()
    }
    fn snapshot(&self) -> Result<Value, RunError> {
        serde_json::to_value(&self.planner.data)
            .map_err(|e| crate::common::error::ObservationError::from(e).into())
    }
    fn dispatch(&mut self, frame: &Arc<BgrFrame>, action: &TownAction) -> Result<(), RunError> {
        Ok(dispatch_town_action(self.context, frame.size(), action)?)
    }
    fn hidden_map(&self) -> bool {
        self.planner
            .data
            .arcane_map_inspection
            .as_ref()
            .is_some_and(|s| s.status == InspectionStatus::Hidden)
    }
    fn clear_hud(&mut self, frame: &Arc<BgrFrame>) -> Result<bool, RunError> {
        self.vision.set_frame(frame.clone());
        self.vision.normalize()?;
        if self.vision.find("hud")?.is_none() {
            return Ok(false);
        }
        for name in ["waypoint_header", "save_exit", "npc_cancel"] {
            if self.vision.find(name)?.is_some() {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn map_restore_requested(&mut self) {
        if let Some(i) = self.planner.data.arcane_map_inspection.as_mut() {
            i.status = InspectionStatus::RestoreRequestedAfterError;
        }
    }
    fn release_mouse(&mut self) -> Result<(), RunError> {
        Ok(self.context.release_town_mouse()?)
    }
    fn restore_map(&mut self, frame: &Arc<BgrFrame>) -> Result<(), RunError> {
        // 이 정리 입력은 planner의 새 명령이 아니며 actions 보고에 추가하지 않는다.
        let action = TownAction {
            kind: TownActionKind::Key,
            key: Some("tab".into()),
            point: None,
            reason: RESTORE_REASON.into(),
            phase: self.planner.data.phase,
            number: self.planner.data.action_number,
        };
        self.dispatch(frame, &action)
    }
}
const RESTORE_REASON: &str = "Restore temporary Arcane map after finish interruption";
fn guard(g: &impl StepEnvironment) -> Result<(), RunError> {
    if g.stopped() || !g.focused()? {
        Err(RunError::Runtime(
            "north-room finish interrupted or unfocused".into(),
        ))
    } else {
        Ok(())
    }
}
pub fn return_to_act1_and_exit(
    g: &impl StepEnvironment,
    runtime: &mut impl TownFinishRuntime,
    evidence: &mut Vec<EncounterEvidence>,
    mut emit: impl FnMut(&str),
) -> Result<Value, RunError> {
    let result = (|| {
        runtime.begin()?;
        let mut previous = None;
        let mut actions = vec![];
        let mut pending = None;
        while runtime.phase() != "done" {
            guard(g)?;
            let acquired = g.clock();
            let frame = runtime.capture()?;
            if runtime.cursor_shift(&frame)? == CursorShift::Pending {
                let start = *pending.get_or_insert(acquired);
                if acquired - start < 0.45 {
                    g.wait(0.04)?;
                    continue;
                }
            } else {
                pending = None;
            }
            let action = runtime.step(&frame)?;
            if g.clock() - acquired >= 0.25 {
                if action.is_some() {
                    return Err(RunError::Runtime(
                        "north-room finish action frame stale".into(),
                    ));
                }
                continue;
            }
            if let Some(action) = action {
                guard(g)?;
                runtime.dispatch(&frame, &action)?;
                emit(&format!("NORTH_ROOM_FINISH: {}", action.reason));
                actions.push(action);
            }
            let phase = runtime.phase().to_owned();
            if previous.as_ref() != Some(&phase) {
                evidence.push(EncounterEvidence {
                    filename: format!("room-finish-{}-{phase}.png", actions.len()),
                    frame,
                });
                previous = Some(phase);
            }
            g.wait(0.04)?;
        }
        Ok(
            json!({"status":"act1_then_character_select_observed","actions":actions,"planner":runtime.snapshot()?}),
        )
    })();
    if result.is_err() && runtime.hidden_map() && !g.stopped() {
        let restore = (|| {
            if !g.focused()? {
                return Ok(());
            }
            let acquired = g.clock();
            let frame = runtime.capture()?;
            if runtime.clear_hud(&frame)? && (0.0..0.25).contains(&(g.clock() - acquired)) {
                guard(g)?;
                runtime.restore_map(&frame)?;
                runtime.map_restore_requested();
                emit(&format!("NORTH_ROOM_FINISH: {RESTORE_REASON}"));
                evidence.push(EncounterEvidence {
                    filename: "room-finish-map-restore-after-error.png".into(),
                    frame,
                });
            }
            Ok::<_, RunError>(())
        })();
        if let Err(error) = restore {
            emit(&format!("NORTH_ROOM_FINISH_MAP_RESTORE: {error}"));
        }
    }
    let cleanup = runtime.release_mouse();
    match (result, cleanup) {
        (Ok(v), Ok(())) => Ok(v),
        (Err(e), Ok(())) | (Ok(_), Err(e)) => Err(e),
        (Err(e), Err(cleanup)) => Err(e.with_cleanup(cleanup)),
    }
}

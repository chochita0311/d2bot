use super::{AutomaticField, BuffRecipe, FieldWorld, RecordedInput};
use crate::{
    common::{
        automap_vision::AutomapObserver,
        error::ControlError,
        field_control::{DesktopPoint, FieldInputSession},
        field_preparation::AutomaticBuffDriver,
        frame::BgrFrame,
        survival_vision::HudReading,
    },
    platform::{CaptureBackend, InputSystem},
    runs::{
        RunError,
        summoner::{
            appearance_vision::SummonerAppearanceObserver,
            encounter_execution::{EncounterEnvironment, EncounterEvidence},
            field_defense::PausedEnvironment,
            objective_vision::ArcaneObjectiveObserver,
            scene_vision::HostileTarget,
        },
    },
};
use serde_json::Value;
use std::sync::Arc;
pub struct EncounterRecheck {
    pub objective: Arc<ArcaneObjectiveObserver>,
    pub appearance: Arc<SummonerAppearanceObserver>,
    pub next_at: f64,
}
/// 대기 중 갱신은 보고서/증거를 호출 지점에서 빌려 이중 mutable 참조나 장시간 잠금을 피한다.
pub struct PausedField<'a, 'b, S: CaptureBackend + InputSystem> {
    pub world: FieldWorld<'b, S>,
    pub driver: &'a mut AutomaticBuffDriver,
    pub area: &'a mut AutomapObserver,
    pub recipe: &'a BuffRecipe,
    pub retained_inputs: &'a mut Vec<FieldInputSession<RecordedInput<S>>>,
    pub encounter_recheck: Option<EncounterRecheck>,
}
impl<S: CaptureBackend + InputSystem> EncounterEnvironment for PausedField<'_, '_, S> {
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError> {
        EncounterEnvironment::capture(&mut self.world)
    }
    fn hud(&mut self, f: &Arc<BgrFrame>) -> Result<HudReading, RunError> {
        EncounterEnvironment::hud(&mut self.world, f)
    }
    fn wait(&self, s: f64) -> Result<(), ControlError> {
        EncounterEnvironment::wait(&self.world, s)
    }
    fn frame_to_desktop(&self, p: [i32; 2]) -> Result<DesktopPoint, ControlError> {
        self.world.frame_to_desktop(p)
    }
    fn emit(&mut self, text: &str) {
        self.world.emit(text);
    }
    fn loot_movement_allowed(&self) -> Option<bool> {
        self.world.loot_movement_allowed()
    }
    fn recover_resources(
        &mut self,
        f: &Arc<BgrFrame>,
        hud: &HudReading,
        at: f64,
    ) -> Result<bool, RunError> {
        self.world.recover_resources(f, hud, at)
    }
    fn has_probe(&self) -> bool {
        self.world.has_probe()
    }
    fn probe_pending(&self) -> bool {
        self.world.probe_pending()
    }
    fn reset_probe(&mut self) {
        self.world.reset_probe();
    }
    fn actor_proposals(
        &mut self,
        f: &Arc<BgrFrame>,
        p: [i32; 2],
    ) -> Result<Vec<[i32; 2]>, RunError> {
        self.world.actor_proposals(f, p)
    }
    fn probe(
        &mut self,
        f: &Arc<BgrFrame>,
        p: [i32; 2],
        at: f64,
        points: Option<&[[i32; 2]]>,
    ) -> Result<Option<HostileTarget>, RunError> {
        self.world.probe(f, p, at, points)
    }
    fn inspect_loot(
        &mut self,
        started: f64,
        dispatches: u64,
        buff: Option<f64>,
    ) -> Result<Option<Value>, RunError> {
        self.world.inspect_loot(started, dispatches, buff)
    }
}
impl<S: CaptureBackend + InputSystem> PausedEnvironment for PausedField<'_, '_, S> {
    fn defense_probe_points(
        &mut self,
        frame: &Arc<BgrFrame>,
        player: [i32; 2],
    ) -> Result<Option<Vec<[i32; 2]>>, RunError> {
        if self.encounter_recheck.is_some() {
            Ok(Some(self.world.actor_proposals(frame, player)?))
        } else {
            Ok(None)
        }
    }
    fn encounter_recheck(
        &mut self,
        frame: &Arc<BgrFrame>,
        acquired: f64,
    ) -> Result<bool, RunError> {
        let Some(check) = self.encounter_recheck.as_mut() else {
            return Ok(false);
        };
        if acquired < check.next_at {
            return Ok(false);
        }
        check.next_at = acquired + 0.25;
        let platform = check.objective.observe(frame.view())?;
        Ok(check
            .appearance
            .observe_corpse(frame.view(), platform.as_ref())?
            .is_some())
    }
    fn has_renewal(&self) -> bool {
        true
    }
    fn renew_buffs(
        &mut self,
        report: &mut Value,
        evidence: &mut Vec<EncounterEvidence>,
    ) -> Result<f64, RunError> {
        if !self.retained_inputs.is_empty() {
            return Err(RunError::Runtime(
                "previous field input session cleanup incomplete".into(),
            ));
        }
        let trace_number = report
            .get("buff_renewals")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        let policy = self.world.loot.policy.clone();
        let mut env = AutomaticField {
            world: &mut self.world,
            area: self.area,
            recipe: self.recipe,
            evidence,
            retained_inputs: self.retained_inputs,
            trace_number,
        };
        let record = self.driver.execute(&mut env, &self.recipe.order, &policy)?;
        let deadline = record["preparation_at"]
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or_else(|| RunError::Runtime("automatic preparation deadline required".into()))?;
        if report.get("buff_renewals").is_none() {
            report["buff_renewals"] = serde_json::json!([]);
        }
        report["buff_renewals"]
            .as_array_mut()
            .ok_or_else(|| RunError::Runtime("buff renewal report must be an array".into()))?
            .push(record);
        self.world.loot.deadline = Some(deadline);
        Ok(deadline)
    }
}

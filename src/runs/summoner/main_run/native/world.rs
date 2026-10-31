use super::{FieldCore, FieldLoot, PanelObservers};
use crate::{
    common::{
        automap_vision::AutomapObserver,
        conditional_loot::{ConditionalLootController, ConditionalLootEnvironment},
        error::ControlError,
        field_control::DesktopPoint,
        field_preparation::{
            AutomapSample, BuffSiteEnvironment, PreparationEnvironment, PreparationScreen,
            ensure_automap_on,
        },
        frame::{BgrFrame, BgrView},
        hostile_probe::{NearbyHostileProbe, nearby_actor_proposals_with_radius},
        survival_vision::HudReading,
    },
    platform::{CaptureBackend, InputSystem},
    runs::{
        RunError,
        summoner::{
            continuous_navigation::ContinuousEnvironment,
            encounter_execution::EncounterEnvironment,
            legacy_segment::LegacyEnvironment,
            scene_vision::{ArcaneHazardHandle, HostileTarget},
        },
    },
};
use serde_json::{Value, json};
use std::sync::Arc;
pub(super) type TargetReader =
    Box<dyn for<'a> FnMut(BgrView<'a>) -> Result<Option<HostileTarget>, RunError> + Send>;
pub type TargetProbe = NearbyHostileProbe<TargetReader>;
#[derive(Clone, Copy)]
pub enum LootContext {
    Navigation,
    Encounter,
    Paused,
}
impl LootContext {
    fn name(self) -> &'static str {
        match self {
            Self::Navigation => "navigation",
            Self::Encounter => "encounter",
            Self::Paused => "paused",
        }
    }
    fn reason(self) -> &'static str {
        match self {
            Self::Paused => "paused_defense_ended_scan",
            _ => "combat_ended_scan",
        }
    }
}
pub struct MapScreen<'a> {
    pub map: &'a mut AutomapObserver,
    pub panels: PanelObservers,
}
impl<S: CaptureBackend + InputSystem> PreparationScreen<FieldCore<S>> for MapScreen<'_> {
    fn panels_open(&mut self, frame: &Arc<BgrFrame>) -> Result<bool, RunError> {
        Ok(self.panels.check(frame)?)
    }
    fn automap(&mut self, frame: &Arc<BgrFrame>) -> Result<AutomapSample, RunError> {
        let visible = self.map.visible(frame.view(), "arcane_sanctuary")?;
        Ok(AutomapSample {
            visible,
            obscured: self.map.last_obscured(),
            evidence: json!(self.map.last_evidence()),
        })
    }
}
/// 필드 절차에 필요한 소유자를 서로 겹치지 않게 빌린다. 줍기 중 회복도 같은 core를 사용한다.
pub struct FieldWorld<'a, S: CaptureBackend + InputSystem> {
    pub loot: FieldLoot<'a, S>,
    pub controller: &'a mut ConditionalLootController,
    pub hazards: ArcaneHazardHandle,
    pub map: Option<&'a mut AutomapObserver>,
    pub probe: Option<&'a mut TargetProbe>,
    pub context: LootContext,
}
impl<S: CaptureBackend + InputSystem> PreparationEnvironment for FieldWorld<'_, S> {
    type Error = RunError;
    fn clock(&self) -> f64 {
        self.loot.clock()
    }
    fn wall_clock(&self) -> f64 {
        self.loot.wall_clock()
    }
    fn stopped(&self) -> bool {
        self.loot.stopped()
    }
    fn focused(&self) -> Result<bool, RunError> {
        self.loot.focused()
    }
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError> {
        self.loot.capture()
    }
    fn hud(&mut self, f: &Arc<BgrFrame>) -> Result<HudReading, RunError> {
        self.loot.hud(f)
    }
    fn press(&mut self, k: &str) -> Result<(), RunError> {
        self.loot.press(k)
    }
    fn wait(&mut self, s: f64) -> Result<(), RunError> {
        self.loot.wait(s)
    }
}
impl<S: CaptureBackend + InputSystem> BuffSiteEnvironment for FieldWorld<'_, S> {
    fn prepare_ground(
        &mut self,
        f: &Arc<BgrFrame>,
        at: f64,
        hud: &HudReading,
    ) -> Result<(), RunError> {
        let focused = self.loot.focused()?;
        Ok(self
            .loot
            .camera
            .with(|c| c.prepare_frame(f, at, hud, focused))?)
    }
    fn ground(&mut self, f: &Arc<BgrFrame>, at: f64) -> Result<Option<[i32; 2]>, RunError> {
        Ok(self.loot.camera.with(|c| c.observe(f, at))?.point)
    }
    fn hazard_grounds(
        &mut self,
        f: &Arc<BgrFrame>,
        p: [i32; 2],
        at: f64,
        now: f64,
    ) -> Result<Vec<Option<[f64; 2]>>, RunError> {
        Ok(self
            .hazards
            .observe(f.view(), p, at, now, None)?
            .into_iter()
            .map(|c| c.ground())
            .collect())
    }
}
impl<S: CaptureBackend + InputSystem> EncounterEnvironment for FieldWorld<'_, S> {
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError> {
        self.loot.capture()
    }
    fn hud(&mut self, f: &Arc<BgrFrame>) -> Result<HudReading, RunError> {
        self.loot.hud(f)
    }
    fn wait(&self, s: f64) -> Result<(), ControlError> {
        crate::runs::summoner::navigation_execution::StepEnvironment::wait(self.loot.core, s)
    }
    fn frame_to_desktop(&self, p: [i32; 2]) -> Result<DesktopPoint, ControlError> {
        self.loot.core.to_desktop(p.map(f64::from))
    }
    fn emit(&mut self, text: &str) {
        self.loot.core.emit(text);
    }
    fn loot_movement_allowed(&self) -> Option<bool> {
        Some(self.controller.movement_allowed())
    }
    fn recover_resources(
        &mut self,
        f: &Arc<BgrFrame>,
        hud: &HudReading,
        at: f64,
    ) -> Result<bool, RunError> {
        ConditionalLootEnvironment::recover_resources(&mut self.loot, f, hud, at)
    }
    fn has_probe(&self) -> bool {
        self.probe.is_some()
    }
    fn probe_pending(&self) -> bool {
        self.probe.as_ref().is_some_and(|p| p.has_pending())
    }
    fn encounter_map_visible(&mut self, frame: &Arc<BgrFrame>) -> Result<Option<bool>, RunError> {
        self.map
            .as_mut()
            .map(|map| {
                map.visible(frame.view(), "arcane_sanctuary")
                    .map_err(RunError::from)
            })
            .transpose()
    }
    fn restore_map(&mut self) -> Result<Value, RunError> {
        ContinuousEnvironment::confirm_map_restored(self)
    }
    fn encounter_panels_open(&mut self, frame: &Arc<BgrFrame>) -> Result<bool, RunError> {
        Ok(self.loot.core.panels().check(frame)?)
    }
    fn reset_probe(&mut self) {
        if let Some(p) = self.probe.as_mut() {
            p.reset();
        }
    }
    fn actor_proposals(
        &mut self,
        f: &Arc<BgrFrame>,
        p: [i32; 2],
    ) -> Result<Vec<[i32; 2]>, RunError> {
        let radius = self.probe.as_ref().map_or([300.0, 180.0], |p| p.radius());
        Ok(nearby_actor_proposals_with_radius(
            f.view(),
            Some(p.map(f64::from)),
            radius,
        )?)
    }
    fn probe(
        &mut self,
        f: &Arc<BgrFrame>,
        p: [i32; 2],
        at: f64,
        points: Option<&[[i32; 2]]>,
    ) -> Result<Option<HostileTarget>, RunError> {
        match self.probe.as_mut() {
            Some(probe) => probe.observe(f.view(), Some(p.map(f64::from)), at, points),
            None => Ok(None),
        }
    }
    fn inspect_loot(
        &mut self,
        started: f64,
        dispatches: u64,
        buff: Option<f64>,
    ) -> Result<Option<Value>, RunError> {
        let report = self.controller.after_combat(
            &mut self.loot,
            buff,
            self.context.reason(),
            Some(json!([self.context.name(), started, dispatches])),
        )?;
        Ok(Some(serde_json::to_value(report)?))
    }
}
impl<S: CaptureBackend + InputSystem> LegacyEnvironment for FieldWorld<'_, S> {
    fn wall_clock(&self) -> f64 {
        self.loot.wall_clock()
    }
}
impl<S: CaptureBackend + InputSystem> ContinuousEnvironment for FieldWorld<'_, S> {
    fn evidence_streaming(&self) -> bool {
        self.loot.core.evidence_channel().is_some()
    }
    fn drain_evidence(
        &mut self,
        frames: &mut Vec<crate::runs::summoner::encounter_execution::EncounterEvidence>,
    ) -> Vec<String> {
        self.loot.core.drain_navigation_evidence(frames)
    }
    fn reaction_radius(&self) -> [f64; 2] {
        self.probe.as_ref().map_or([300.0, 180.0], |p| p.radius())
    }
    fn capture_for_control(&mut self) -> Result<Arc<BgrFrame>, RunError> {
        Ok(self.loot.core.grab_latest()?.frame)
    }
    fn capture_source(&self) -> Option<(u64, f64, f64)> {
        self.loot.core.capture_source()
    }
    fn capture_method(&self) -> Option<&'static str> {
        self.loot.core.capture_method()
    }
    fn wall_clock(&self) -> f64 {
        self.loot.wall_clock()
    }
    fn desktop_to_frame(&self, p: DesktopPoint) -> Result<[i32; 2], ControlError> {
        self.loot.core.guards().window.from_desktop(p)
    }
    fn has_map(&self) -> bool {
        self.map.is_some()
    }
    fn map_visible(&mut self, f: &Arc<BgrFrame>) -> Result<bool, RunError> {
        Ok(self
            .map
            .as_mut()
            .ok_or_else(|| RunError::Runtime("automap observer unavailable".into()))?
            .visible(f.view(), "arcane_sanctuary")?)
    }
    fn panels_open(&mut self, f: &Arc<BgrFrame>) -> Result<bool, RunError> {
        Ok(self.loot.core.panels().check(f)?)
    }
    fn confirm_map_restored(&mut self) -> Result<Value, RunError> {
        let map = self
            .map
            .as_deref_mut()
            .ok_or_else(|| RunError::Runtime("automap observer unavailable".into()))?;
        let mut screen = MapScreen {
            map,
            panels: self.loot.core.panels(),
        };
        let preparation = ensure_automap_on(
            self.loot.core,
            &self.loot.policy,
            &mut screen,
            None,
            false,
            0.5,
        )?;
        Ok(serde_json::to_value(preparation)?)
    }
    fn probe_next_proposal_at(&self) -> f64 {
        self.probe
            .as_ref()
            .and_then(|p| p.snapshot().next_proposal_at)
            .unwrap_or(0.0)
    }
    fn probe_requested_at(&self) -> Option<f64> {
        self.probe
            .as_ref()
            .and_then(|p| p.snapshot().pending.map(|v| v.1))
    }
}

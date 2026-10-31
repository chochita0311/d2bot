use super::FieldCore;
use crate::{
    actions::loot_pickup::{LootCandidate, LootScanner},
    common::{
        conditional_loot::{ConditionalLootEnvironment, LootGround, LootLabelHit},
        emergency_recovery::EmergencyResourceRecovery,
        error::ControlError,
        field_control::{DesktopPoint, FieldInputBackend},
        field_preparation::PreparationEnvironment,
        frame::BgrFrame,
        survival::CharacterSurvivalPolicy,
        survival_vision::HudReading,
    },
    platform::{CaptureBackend, InputSystem},
    runs::{
        RunError,
        summoner::{camera_ground::CameraGroundHandle, canyon_finish::CanyonBackend},
    },
};
use std::sync::Arc;
/// 줍기는 메인 캡처/입력과 회복 소유자를 빌리며 별도 입력 세션을 시작하지 않는다.
pub struct FieldLoot<'a, S: CaptureBackend + InputSystem> {
    pub core: &'a mut FieldCore<S>,
    pub scanner: &'a mut LootScanner,
    pub recovery: &'a mut EmergencyResourceRecovery,
    pub camera: CameraGroundHandle,
    pub policy: Arc<CharacterSurvivalPolicy>,
    pub deadline: Option<f64>,
}
impl<S: CaptureBackend + InputSystem> PreparationEnvironment for FieldLoot<'_, S> {
    type Error = RunError;
    fn clock(&self) -> f64 {
        PreparationEnvironment::clock(self.core)
    }
    fn wall_clock(&self) -> f64 {
        PreparationEnvironment::wall_clock(self.core)
    }
    fn stopped(&self) -> bool {
        PreparationEnvironment::stopped(self.core)
    }
    fn focused(&self) -> Result<bool, RunError> {
        PreparationEnvironment::focused(self.core)
    }
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError> {
        PreparationEnvironment::capture(self.core)
    }
    fn hud(&mut self, f: &Arc<BgrFrame>) -> Result<HudReading, RunError> {
        self.core.read_hud(f)
    }
    fn press(&mut self, k: &str) -> Result<(), RunError> {
        PreparationEnvironment::press(self.core, k)
    }
    fn wait(&mut self, s: f64) -> Result<(), RunError> {
        PreparationEnvironment::wait(self.core, s)
    }
}
impl<S: CaptureBackend + InputSystem> ConditionalLootEnvironment for FieldLoot<'_, S> {
    type Candidate = LootCandidate;
    fn candidate_label(c: &LootCandidate) -> &str {
        &c.label
    }
    fn build_candidates(&mut self) -> Result<Vec<LootCandidate>, RunError> {
        Ok(self.scanner.candidates()?.to_vec())
    }
    fn scan_frame(
        &mut self,
        f: &Arc<BgrFrame>,
        allowed: &[LootCandidate],
    ) -> Result<Option<LootLabelHit>, RunError> {
        self.scanner
            .scan_frame(f.view(), Some(allowed))?
            .map(|hit| {
                let m = hit.match_result;
                let convert = |n| {
                    i64::try_from(n).map_err(|_| {
                        ControlError::Runtime("loot label outside coordinate range".into())
                    })
                };
                Ok(LootLabelHit {
                    label: hit.label,
                    top_left: [convert(m.top_left[0])?, convert(m.top_left[1])?],
                    width: m.width,
                    height: m.height,
                })
            })
            .transpose()
    }
    fn panels_open(&mut self, f: &Arc<BgrFrame>) -> Result<bool, RunError> {
        Ok(self.core.panels().check(f)?)
    }
    fn frame_to_desktop(&mut self, p: [i64; 2]) -> Result<DesktopPoint, RunError> {
        Ok(self.core.to_desktop(p.map(|n| n as f64))?)
    }
    fn supports_click_confirmation(&self) -> bool {
        self.core.input().supports_pointer_confirmation()
    }
    fn move_pointer(&mut self, p: DesktopPoint) -> Result<(), RunError> {
        Ok(self.core.input().move_pointer(p)?)
    }
    fn pointer_matches(&mut self, p: DesktopPoint) -> Result<bool, RunError> {
        Ok(self.core.input().pointer_matches(p)?)
    }
    fn click(&mut self) -> Result<(), RunError> {
        Ok(self.core.input().click()?)
    }
    fn buff_deadline(&mut self) -> Result<Option<f64>, RunError> {
        Ok(self.deadline)
    }
    fn has_recovery(&self) -> bool {
        true
    }
    fn recover_resources(
        &mut self,
        f: &Arc<BgrFrame>,
        reading: &HudReading,
        at: f64,
    ) -> Result<bool, RunError> {
        self.recovery
            .recover(self.core, &self.policy, f, reading, at)
    }
    fn ground_available(&mut self) -> Result<bool, RunError> {
        Ok(true)
    }
    fn observe_ground(&mut self, f: &Arc<BgrFrame>, at: f64) -> Result<LootGround, RunError> {
        let hud = self.core.read_hud(f)?;
        let focused = PreparationEnvironment::focused(self.core)?;
        Ok(self.camera.with(|camera| {
            camera.prepare_frame(f, at, &hud, focused)?;
            let g = camera.observe(f, at)?;
            Ok(LootGround {
                point: g.point,
                reason: Some(g.reason),
            })
        })?)
    }
}

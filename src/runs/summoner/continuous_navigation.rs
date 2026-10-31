//! 현재 화면을 기준으로 한 연속 이동·근접 방어·복귀 실행.
mod engine;
mod native;
use super::{
    continuation::*,
    encounter_execution::{EncounterEnvironment, EncounterEvidence},
    navigation::{ChooseOptions, PathChoice, Wing},
    objective_vision::ObjectiveReading,
    scene_vision::{GroundReading, HostileTarget, HoverCandidate, HubEvidence},
    terminal_vision::TerminalReading,
    terrain_patterns::TerrainLanding,
};
use crate::{
    common::{
        encounters::VisualCandidate,
        error::{ControlError, ObservationError},
        field_control::{DesktopPoint, FieldInputBackend, FieldInputSession},
        frame::BgrFrame,
        scene_motion::CameraMotion,
        survival::CharacterSurvivalPolicy,
        survival_vision::HudReading,
    },
    runs::RunError,
};
pub use engine::run_continuous_route;
pub use native::{ContinuousObservers, NativeContinuousVision};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

pub enum Continuation<'a> {
    Initial,
    Current,
    Transition(&'a Value),
}
pub struct ContinuousRequest<'a> {
    pub policy: &'a CharacterSurvivalPolicy,
    pub proof: &'a Value,
    pub seconds: f64,
    pub reference_seconds: f64,
    pub wing: Wing,
    pub return_from: Option<Wing>,
    pub engage_monsters: bool,
    pub buff_expires_at: Option<f64>,
    pub continuation: Continuation<'a>,
}
pub trait ContinuousEnvironment: EncounterEnvironment {
    fn evidence_streaming(&self) -> bool {
        false
    }
    /// Native 진단은 판독 경로를 막지 않는 제한된 저장 큐로 소유권을 넘긴다.
    /// 재생/모의 환경은 원래 메모리 evidence 경로를 유지한다.
    fn drain_evidence(&mut self, _frames: &mut Vec<EncounterEvidence>) -> Vec<String> {
        vec![]
    }
    fn reaction_radius(&self) -> [f64; 2] {
        NEAR_GROUND_RADIUS
    }
    fn capture_for_control(&mut self) -> Result<Arc<BgrFrame>, RunError> {
        self.capture()
    }
    fn capture_source(&self) -> Option<(u64, f64, f64)> {
        None
    }
    fn capture_method(&self) -> Option<&'static str> {
        None
    }
    fn wall_clock(&self) -> f64;
    fn desktop_to_frame(&self, point: DesktopPoint) -> Result<[i32; 2], ControlError>;
    fn has_map(&self) -> bool;
    fn map_visible(&mut self, frame: &Arc<BgrFrame>) -> Result<bool, RunError>;
    fn panels_open(&mut self, frame: &Arc<BgrFrame>) -> Result<bool, RunError>;
    /// Tab 복원 요청 뒤 새 제목을 확인하며 다시 토글하지 않는다.
    fn confirm_map_restored(&mut self) -> Result<Value, RunError>;
    fn probe_next_proposal_at(&self) -> f64;
    fn probe_requested_at(&self) -> Option<f64>;
}
#[derive(Default)]
pub struct CurrentThreats {
    pub candidates: Vec<VisualCandidate>,
    pub hover: Vec<HoverCandidate>,
    pub target: Option<HostileTarget>,
}
pub struct PlanningJobs<J> {
    pub jobs: J,
    pub hub: Option<[i32; 2]>,
    pub hub_evidence: Option<HubEvidence>,
}
pub struct CurrentPlan<V> {
    pub choice: PathChoice,
    pub ground: GroundReading,
    pub view: V,
}
pub struct PlanningRequest<'a> {
    pub wing: Wing,
    pub acquired: f64,
    pub position: [f64; 2],
    pub excluded: &'a [[f64; 4]],
    pub ground: &'a GroundReading,
    pub outbound: bool,
}
#[derive(Default)]
pub struct TrackedLandmarks {
    pub terminal: Option<TerminalReading>,
    pub approach: Option<TerminalReading>,
    pub approach_displacement: Option<[f64; 2]>,
    pub approach_static_pairs: u64,
    pub hub: Option<[i32; 2]>,
    pub hub_evidence: Option<HubEvidence>,
}
pub struct GraphLocation {
    pub relocated: bool,
    pub localized: bool,
    pub position: [f64; 2],
    pub places: Vec<[f64; 2]>,
}
pub struct LandmarkObservation<'a> {
    pub frame: &'a Arc<BgrFrame>,
    pub terminal: Option<TerminalReading>,
    pub hub: Option<[i32; 2]>,
    pub evidence: Option<HubEvidence>,
    pub acquired: f64,
    pub displacement: Option<[f64; 2]>,
}
pub struct RouteChoiceRequest<'a> {
    pub wing: Wing,
    pub now: f64,
    pub excluded: &'a [[f64; 4]],
    pub landings: &'a [TerrainLanding],
    pub hub: Option<[i32; 2]>,
    pub returning: bool,
}
/// 엔진은 판독 결과와 현재 시한만 소비하고 영상 작업의 소유권은 이 구현이 보존한다.
pub trait ContinuousVision {
    type View;
    type Jobs;
    fn initialize(
        &mut self,
        reference: f64,
        last_aim: Option<[i32; 2]>,
        detour: Option<[f64; 2]>,
    ) -> Result<(), RunError>;
    fn clearance(&self) -> f64;
    fn has_hub(&self) -> bool;
    fn has_graph(&self) -> bool;
    fn graph_position_current(&self) -> bool {
        true
    }
    /// 복귀 이력이 소진돼도 입력 없이 현재 관측을 새 지역 기준으로 삼을 수 있다.
    fn reobserve_route_graph(
        &mut self,
        _view: &Self::View,
        _player: [i32; 2],
        _acquired: f64,
    ) -> Result<bool, RunError> {
        Ok(false)
    }
    fn prepares_ground(&self) -> bool;
    fn begin_frame(&mut self, _frame: &Arc<BgrFrame>) -> Result<(), RunError> {
        Ok(())
    }
    fn panel_timing(&self) -> Option<(f64, f64)> {
        None
    }
    fn prepare(
        &mut self,
        frame: &Arc<BgrFrame>,
        acquired: f64,
        hud: &HudReading,
        focused: bool,
    ) -> Result<(), RunError>;
    fn ground(&mut self, frame: &Arc<BgrFrame>, acquired: f64) -> Result<GroundReading, RunError>;
    fn threats(
        &mut self,
        frame: &Arc<BgrFrame>,
        player: [i32; 2],
        acquired: f64,
        aim: Option<[i32; 2]>,
        engage: bool,
    ) -> Result<CurrentThreats, RunError>;
    fn near_preempts_hover(&self) -> bool {
        false
    }
    fn near_threats(
        &mut self,
        frame: &Arc<BgrFrame>,
        player: [i32; 2],
        acquired: f64,
        aim: Option<[i32; 2]>,
        engage: bool,
    ) -> Result<CurrentThreats, RunError> {
        self.threats(frame, player, acquired, aim, engage)
    }
    fn finish_threats(&mut self, _threats: &mut CurrentThreats) -> Result<(), RunError> {
        Ok(())
    }
    fn hover(
        &mut self,
        frame: &Arc<BgrFrame>,
        aim: [i32; 2],
    ) -> Result<Vec<HoverCandidate>, RunError>;
    fn hazards(
        &mut self,
        frame: &Arc<BgrFrame>,
        player: [i32; 2],
        acquired: f64,
        aim: Option<[i32; 2]>,
    ) -> Result<Vec<VisualCandidate>, RunError>;
    fn start_plan(
        &mut self,
        frame: &Arc<BgrFrame>,
        request: PlanningRequest<'_>,
    ) -> Result<PlanningJobs<Self::Jobs>, RunError>;
    fn objective(&mut self, jobs: &mut Self::Jobs) -> Result<Option<ObjectiveReading>, RunError>;
    fn slow_confirmation_pending(&self) -> bool {
        false
    }
    fn planning_diagnostics(&self) -> Value {
        Value::Null
    }
    fn require_current_goals(&mut self, _required: bool) {}
    /// 전체 지도 갱신 사이에는 현재 바닥으로만 local aim을 고른다.
    fn local_field_phase(&mut self, _eligible: bool) {}
    fn plan(&mut self, jobs: &mut Self::Jobs) -> Result<CurrentPlan<Self::View>, RunError>;
    fn terminal(&mut self, jobs: &mut Self::Jobs) -> Result<Option<TerminalReading>, RunError>;
    fn set_sequence(&mut self, view: &mut Self::View, sequence: u64);
    fn choose(
        &mut self,
        view: &Self::View,
        wing: Wing,
        options: ChooseOptions<'_>,
    ) -> Result<PathChoice, RunError>;
    fn floor_supports(&self, view: &Self::View, aim: [i32; 2]) -> Result<bool, RunError>;
    fn cell_pixels(&self, view: &Self::View) -> f64;
    fn measure(
        &mut self,
        frame: &Arc<BgrFrame>,
        view: &Self::View,
        hint: Option<[f64; 2]>,
    ) -> Result<(CameraMotion, Vec<TerrainLanding>), RunError>;
    fn reset_motion(&mut self) -> Result<(), RunError>;
    fn seed_motion(&mut self, frame: &Arc<BgrFrame>, view: &Self::View) -> Result<(), RunError>;
    fn track(&mut self, observation: LandmarkObservation<'_>)
    -> Result<TrackedLandmarks, RunError>;
    fn reset_terminal(&mut self);
    fn relocalize_terminal(&mut self);
    fn terminal_reference(&self) -> (bool, Option<f64>);
    fn reset_hub(&mut self);
    fn current_pair_near(
        &mut self,
        frame: &Arc<BgrFrame>,
        player: [i32; 2],
    ) -> Result<Option<[f64; 2]>, RunError>;
    fn camera_diagnostics(&self) -> Result<Value, RunError>;
    fn graph_observe(
        &mut self,
        player: [i32; 2],
        displacement: Option<[f64; 2]>,
        acquired: f64,
    ) -> Result<GraphLocation, RunError>;
    fn graph_choose(
        &mut self,
        view: &Self::View,
        request: RouteChoiceRequest<'_>,
    ) -> Result<PathChoice, RunError>;
    fn graph_fail_landing(
        &mut self,
        aim: Option<[i32; 2]>,
        player: [i32; 2],
    ) -> Result<(), RunError>;
    fn graph_summary(&self) -> Option<Value>;
    fn flow_choose(
        &mut self,
        view: &Self::View,
        wing: Wing,
        global: &PathChoice,
        now: f64,
        options: super::steering::RouteFlowOptions<'_>,
    ) -> Result<PathChoice, RunError>;
    fn flow_state(&self) -> (Option<[f64; 2]>, String);
    fn clear_steering_aim(&mut self);
    fn steering_aim(&self) -> Option<[i32; 2]>;
    fn steer(
        &mut self,
        view: &Self::View,
        choice: &PathChoice,
        now: f64,
    ) -> Result<(bool, &'static str), RunError>;
    fn comparison(&mut self, frame: &Arc<BgrFrame>) -> Result<Vec<u8>, RunError>;
    fn finish(&mut self, timeout: Duration) -> Result<(), RunError>;
}
fn number(value: &Value) -> Option<f64> {
    value.as_f64().filter(|n| n.is_finite())
}
fn pair(value: &Value) -> Option<[f64; 2]> {
    let values = value.as_array()?;
    (values.len() == 2)
        .then(|| Some([number(&values[0])?, number(&values[1])?]))
        .flatten()
}
fn pixel(value: &Value) -> Option<[i32; 2]> {
    let v = value.as_array()?;
    if v.len() != 2 {
        return None;
    }
    Some([
        i32::try_from(v[0].as_i64()?).ok()?,
        i32::try_from(v[1].as_i64()?).ok()?,
    ])
}
fn norm(point: [f64; 2]) -> f64 {
    point[0].hypot(point[1])
}
fn delta(left: [f64; 2], right: [f64; 2]) -> [f64; 2] {
    std::array::from_fn(|i| left[i] - right[i])
}
fn fresh(now: f64, acquired: f64) -> bool {
    (0.0..0.25).contains(&(now - acquired))
}
fn valid_terminal(terminal: Option<&TerminalReading>) -> bool {
    terminal.is_some_and(|t| {
        matches!(
            t.kind.as_str(),
            "nonboss_chest_trio" | "nonboss_chest_platform"
        )
    })
}
fn truth(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64() != Some(0.0),
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
    }
}
fn unavailable(error: &RunError) -> bool {
    matches!(
        error,
        RunError::Runtime(_)
            | RunError::Observation(ObservationError::Invalid(_))
            | RunError::Control(ControlError::Runtime(_) | ControlError::Backend(_))
    )
}

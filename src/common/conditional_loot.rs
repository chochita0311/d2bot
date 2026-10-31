//! 전투 후에만 라벨을 켜는 한정된 줍기 절차. 입력 성공은 획득 증거가 아니다.
mod read;
mod scan;
use super::{
    config::ConfigError, field_control::DesktopPoint, field_preparation::PreparationEnvironment,
    frame::BgrFrame, survival::CharacterSurvivalPolicy, survival_vision::HudReading,
    unicode_casefold::casefold,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeSet, VecDeque},
    fmt::Display,
    sync::Arc,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LootLabelHit {
    pub label: String,
    pub top_left: [i64; 2],
    pub width: usize,
    pub height: usize,
}
impl LootLabelHit {
    fn center(&self) -> Option<[i64; 2]> {
        let x = i128::from(self.top_left[0]) + (self.width / 2) as i128;
        let y = i128::from(self.top_left[1]) + (self.height / 2) as i128;
        Some([i64::try_from(x).ok()?, i64::try_from(y).ok()?])
    }
    fn contains(&self, point: [i64; 2]) -> bool {
        self.top_left[0] <= point[0]
            && i128::from(point[0]) < i128::from(self.top_left[0]) + self.width as i128
            && self.top_left[1] <= point[1]
            && i128::from(point[1]) < i128::from(self.top_left[1]) + self.height as i128
    }
}
pub struct LootGround {
    pub point: Option<[i32; 2]>,
    pub reason: Option<String>,
}
/// 영상 판독과 일반 입력 구현은 호출자가 소유하며 공통 절차는 action을 참조하지 않는다.
pub trait ConditionalLootEnvironment: PreparationEnvironment {
    type Candidate: Clone;
    fn candidate_label(candidate: &Self::Candidate) -> &str;
    fn build_candidates(&mut self) -> Result<Vec<Self::Candidate>, Self::Error>;
    fn scan_frame(
        &mut self,
        frame: &Arc<BgrFrame>,
        allowed: &[Self::Candidate],
    ) -> Result<Option<LootLabelHit>, Self::Error>;
    fn panels_open(&mut self, frame: &Arc<BgrFrame>) -> Result<bool, Self::Error>;
    fn frame_to_desktop(&mut self, point: [i64; 2]) -> Result<DesktopPoint, Self::Error>;
    fn supports_click_confirmation(&self) -> bool;
    fn move_pointer(&mut self, point: DesktopPoint) -> Result<(), Self::Error>;
    fn pointer_matches(&mut self, point: DesktopPoint) -> Result<bool, Self::Error>;
    fn click(&mut self) -> Result<(), Self::Error>;
    fn buff_deadline(&mut self) -> Result<Option<f64>, Self::Error> {
        Ok(None)
    }
    fn potion_count(&mut self) -> Result<Option<Value>, Self::Error> {
        Ok(None)
    }
    fn has_recovery(&self) -> bool {
        false
    }
    fn recover_resources(
        &mut self,
        _frame: &Arc<BgrFrame>,
        _reading: &HudReading,
        _acquired: f64,
    ) -> Result<bool, Self::Error> {
        Ok(false)
    }
    fn ground_available(&mut self) -> Result<bool, Self::Error>;
    /// 필요하면 같은 화면의 HUD·포커스로 카메라를 준비한 뒤 현재 발밑을 읽는다.
    fn observe_ground(
        &mut self,
        frame: &Arc<BgrFrame>,
        acquired: f64,
    ) -> Result<LootGround, Self::Error>;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LabelsState {
    Unknown,
    On,
    Off,
}
#[derive(Debug, Clone, Copy)]
pub struct LootLimits {
    scan_seconds: f64,
    scan_frames: usize,
    clicks: usize,
    cast_reference_seconds: f64,
}
impl LootLimits {
    pub fn new(
        scan_seconds: f64,
        scan_frames: usize,
        clicks: usize,
        cast_reference_seconds: f64,
    ) -> Result<Self, ConfigError> {
        if !scan_seconds.is_finite() || !(0.0..=3.0).contains(&scan_seconds) || scan_seconds == 0.0
        {
            return Err(ConfigError::value(
                "loot scan duration must be within three seconds",
            ));
        }
        if !(1..=30).contains(&scan_frames) {
            return Err(ConfigError::value("bounded loot frame count required"));
        }
        if clicks > 4 {
            return Err(ConfigError::value("bounded loot click count required"));
        }
        if !(0.0..1.5).contains(&cast_reference_seconds) || cast_reference_seconds == 0.0 {
            return Err(ConfigError::value(
                "configured teleport cast reference required",
            ));
        }
        Ok(Self {
            scan_seconds,
            scan_frames,
            clicks,
            cast_reference_seconds,
        })
    }
}
impl Default for LootLimits {
    fn default() -> Self {
        Self {
            scan_seconds: 3.0,
            scan_frames: 12,
            clicks: 2,
            cast_reference_seconds: 0.32,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct HudFailure {
    acquired_at: f64,
    life_ratio: Option<f64>,
    mana_ratio: Option<f64>,
    belt_visibility: super::buffing::BeltVisibility,
    restoring: bool,
    deadline: f64,
    recovered_at: Option<f64>,
}
pub struct LootFrameEvidence<T> {
    pub frame: Arc<BgrFrame>,
    pub metadata: T,
}
#[derive(Debug, Clone, Serialize)]
pub struct LandingObservation {
    acquired_at: f64,
    processed_at: Option<f64>,
    label: Option<String>,
    item_center: Option<[i64; 2]>,
    player: Option<[i32; 2]>,
    ground_reason: Option<String>,
    missing: Vec<&'static str>,
    status: &'static str,
}
#[derive(Debug, Clone, Serialize)]
pub struct LootClick {
    label: String,
    aim: DesktopPoint,
    requested_at: f64,
    status: &'static str,
    pickup_confirmed: bool,
    dispatch_returned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    observed_after_at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    label_visible_after: Option<bool>,
}
#[derive(Debug, Clone, Serialize)]
pub struct LootApproach {
    label: String,
    aim: DesktopPoint,
    requested_at: Option<f64>,
    dispatch_returned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    landing_observation: Option<LandingObservation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    observed_after_at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    player: Option<[i32; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    item_center: Option<[i64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<&'static str>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ScanObservation {
    acquired_at: f64,
    processed_at: f64,
    label: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct LootScanReport {
    pub status: String,
    reason: String,
    kill_confirmed: bool,
    pickups_confirmed: usize,
    pub clicks: Vec<LootClick>,
    pub teleports: Vec<LootApproach>,
    scan_frames: usize,
    scan_observations: Vec<ScanObservation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    labels_state: Option<LabelsState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    movement_allowed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ended_at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    labels_on_sent: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    labels_off_sent: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    configured_labels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unavailable_potion_kinds: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bounded_reason: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    restore_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    overlay_hud_reacquired_at: Option<f64>,
}
impl LootScanReport {
    fn new(reason: &str) -> Self {
        Self {
            status: "inspection_pending".into(),
            reason: reason.into(),
            kill_confirmed: false,
            pickups_confirmed: 0,
            clicks: vec![],
            teleports: vec![],
            scan_frames: 0,
            scan_observations: vec![],
            labels_state: None,
            movement_allowed: None,
            ended_at: None,
            labels_on_sent: None,
            labels_off_sent: None,
            configured_labels: None,
            unavailable_potion_kinds: None,
            bounded_reason: None,
            error: None,
            restore_error: None,
            overlay_hud_reacquired_at: None,
        }
    }
}
enum Failure<E> {
    Abort(&'static str),
    Error(E),
}
impl<E: Display> Failure<E> {
    fn message(&self) -> String {
        match self {
            Self::Abort(reason) => (*reason).into(),
            Self::Error(error) => error.to_string(),
        }
    }
}
pub struct ConditionalLootController {
    policy: Arc<CharacterSurvivalPolicy>,
    movement_key: String,
    ignored: BTreeSet<String>,
    limits: LootLimits,
    labels_state: LabelsState,
    prepared: bool,
    overlay_hud_reacquisition_pending: bool,
    resource_exit_required: bool,
    recovery_attempted: bool,
    records: Vec<LootScanReport>,
    combat_ids: VecDeque<Value>,
    last_hud_failure: Option<LootFrameEvidence<HudFailure>>,
    last_landing_observation: Option<LootFrameEvidence<LandingObservation>>,
}
impl ConditionalLootController {
    pub fn new(
        policy: Arc<CharacterSurvivalPolicy>,
        movement_key: String,
        ignored: impl IntoIterator<Item = String>,
        limits: LootLimits,
    ) -> Self {
        Self {
            policy,
            movement_key,
            ignored: ignored.into_iter().map(|label| casefold(&label)).collect(),
            limits,
            labels_state: LabelsState::Unknown,
            prepared: false,
            overlay_hud_reacquisition_pending: false,
            resource_exit_required: false,
            recovery_attempted: false,
            records: vec![],
            combat_ids: VecDeque::new(),
            last_hud_failure: None,
            last_landing_observation: None,
        }
    }
    pub fn movement_allowed(&self) -> bool {
        self.prepared
            && self.labels_state == LabelsState::Off
            && !self.overlay_hud_reacquisition_pending
    }
    pub fn labels_state(&self) -> LabelsState {
        self.labels_state
    }
    pub fn resource_exit_required(&self) -> bool {
        self.resource_exit_required
    }
    pub fn records(&self) -> &[LootScanReport] {
        &self.records
    }
    pub fn last_hud_failure(&self) -> Option<&LootFrameEvidence<HudFailure>> {
        self.last_hud_failure.as_ref()
    }
    pub fn last_landing_observation(&self) -> Option<&LootFrameEvidence<LandingObservation>> {
        self.last_landing_observation.as_ref()
    }
    pub fn take_hud_failure(&mut self) -> Option<LootFrameEvidence<HudFailure>> {
        self.last_hud_failure.take()
    }
    pub fn take_landing_observation(&mut self) -> Option<LootFrameEvidence<LandingObservation>> {
        self.last_landing_observation.take()
    }
    pub fn prepare<E: ConditionalLootEnvironment>(
        &mut self,
        env: &mut E,
        fresh_room: bool,
        labels_on: Option<bool>,
    ) -> Result<Value, E::Error> {
        if fresh_room && labels_on == Some(true) {
            return Err(ConfigError::value("fresh room defaults to labels OFF").into());
        }
        self.read(env, None, None, true).map_err(external)?;
        if self.labels_state == LabelsState::Off {
            self.overlay_hud_reacquisition_pending = false;
        }
        if self.prepared {
            if self.labels_state != LabelsState::Off {
                return Err(external(Failure::Abort("item_labels_state_unknown")));
            }
            return Ok(
                serde_json::json!({"status":"off_preserved","alt_sent":false,"labels_state":"off","basis":"program_owned_toggle_state"}),
            );
        }
        if !fresh_room && labels_on.is_none() {
            return Err(external(Failure::Abort("item_labels_state_unknown")));
        }
        self.labels_state = if labels_on == Some(true) {
            LabelsState::On
        } else {
            LabelsState::Off
        };
        let toggled = if labels_on == Some(true) {
            self.toggle(env, LabelsState::Off, None, None, true, None)
                .map_err(external)?
        } else {
            false
        };
        self.prepared = true;
        Ok(
            serde_json::json!({"status":if toggled {"off_requested"}else{"off_known"},"alt_sent":toggled,"labels_state":"off","basis":if fresh_room {"fresh_room_default_off_toggle_mode"}else{"supervised_starting_toggle_state"}}),
        )
    }
}
fn external<E: From<super::error::ControlError>>(error: Failure<E>) -> E {
    match error {
        Failure::Abort(reason) => super::error::ControlError::Runtime(reason.into()).into(),
        Failure::Error(error) => error,
    }
}
fn potion_label(label: &str) -> Option<&'static str> {
    match label {
        "full rejuvenation potion" | "full_rejuvenation_potion" => Some("full_rejuvenation_potion"),
        "rejuvenation potion" | "rejuvenation_potion" => Some("rejuvenation_potion"),
        _ => None,
    }
}

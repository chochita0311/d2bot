use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Difficulty {
    Normal,
    Nightmare,
    Hell,
}
impl Difficulty {
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Nightmare => "nightmare",
            Self::Hell => "hell",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Destination {
    Town,
    Arcane,
    Canyon,
}
impl Destination {
    pub fn name(self) -> &'static str {
        match self {
            Self::Town => "town",
            Self::Arcane => "arcane",
            Self::Canyon => "canyon",
        }
    }
    pub fn area(self) -> &'static str {
        match self {
            Self::Town => "area_act2",
            Self::Arcane => "area_arcane",
            Self::Canyon => "area_canyon",
        }
    }
    pub fn waypoint(self) -> &'static str {
        match self {
            Self::Town => "world_act2_waypoint",
            Self::Arcane => "world_arcane_waypoint",
            Self::Canyon => "world_canyon_waypoint",
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct TownOptions {
    pub repeat_count: u8,
    pub difficulty: Difficulty,
    pub destination: Destination,
    pub arrival_only: bool,
    pub reset_act2_start: bool,
    pub inspect_arcane_map: bool,
}
impl Default for TownOptions {
    fn default() -> Self {
        Self {
            repeat_count: 1,
            difficulty: Difficulty::Hell,
            destination: Destination::Town,
            arrival_only: false,
            reset_act2_start: false,
            inspect_arcane_map: false,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TownPhase {
    Character,
    AfterExit,
    Difficulty,
    Act1Spawn,
    Act2Spawn,
    Act2Moving,
    Act1Waypoint,
    Act1Moving,
    WaitAct1Panel,
    NpcRecovery,
    Act2List,
    DestinationArrival,
    ReturnPanel,
    ReturnList,
    Act1Return,
    ExitMenu,
    Done,
}
impl TownPhase {
    pub fn name(self) -> &'static str {
        match self {
            Self::Character => "character",
            Self::AfterExit => "after_exit",
            Self::Difficulty => "difficulty",
            Self::Act1Spawn => "act1_spawn",
            Self::Act2Spawn => "act2_spawn",
            Self::Act2Moving => "act2_moving",
            Self::Act1Waypoint => "act1_waypoint",
            Self::Act1Moving => "act1_moving",
            Self::WaitAct1Panel => "wait_act1_panel",
            Self::NpcRecovery => "npc_recovery",
            Self::Act2List => "act2_list",
            Self::DestinationArrival => "destination_arrival",
            Self::ReturnPanel => "return_panel",
            Self::ReturnList => "return_list",
            Self::Act1Return => "act1_return",
            Self::ExitMenu => "exit_menu",
            Self::Done => "done",
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResetStage {
    LeaveInitialHell,
    Nightmare,
    LeaveNightmare,
    RestoredHell,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectionStatus {
    Hidden,
    RestoreForReacquisition,
    OnReacquiring,
    OnNativeReacquired,
    RestoreRequested,
    Act1OnObserved,
    RestoreRequestedAfterError,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapInspection {
    pub status: InspectionStatus,
    pub requested_at: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restore_requested_at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restored_at: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TownState {
    pub phase: TownPhase,
    pub repeat_count: u8,
    pub difficulty: Difficulty,
    pub destination: Destination,
    pub arrival_only: bool,
    pub completed: u64,
    pub started: Option<f64>,
    pub deadline: Option<f64>,
    pub ready_at: f64,
    pub probes: usize,
    pub waypoint_attempts: usize,
    pub act1_waypoint_point: Option<[f64; 2]>,
    pub waypoint_detour_used: bool,
    pub waypoint_detour_until: f64,
    pub return_attempts: usize,
    pub map_toggled: bool,
    pub action_number: u64,
    pub holding: bool,
    pub move_until: f64,
    pub npc_recoveries: usize,
    pub town_layout: Option<String>,
    pub reset_act2_start: bool,
    pub difficulty_reset_used: bool,
    pub reset_stage: Option<ResetStage>,
    pub preparation_rooms: u64,
    pub inspect_arcane_map: bool,
    pub arcane_map_inspection: Option<MapInspection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera_settling: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera_deadline: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    pub camera_reading: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub town_current: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palace_approach_used: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub act2_probes: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approach_until: Option<f64>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}
impl TownState {
    pub(super) fn new(options: TownOptions) -> Self {
        Self {
            phase: TownPhase::Character,
            repeat_count: options.repeat_count,
            difficulty: options.difficulty,
            destination: options.destination,
            arrival_only: options.arrival_only,
            completed: 0,
            started: None,
            deadline: None,
            ready_at: 0.0,
            probes: 0,
            waypoint_attempts: 0,
            act1_waypoint_point: None,
            waypoint_detour_used: false,
            waypoint_detour_until: 0.0,
            return_attempts: 0,
            map_toggled: false,
            action_number: 0,
            holding: false,
            move_until: 0.0,
            npc_recoveries: 0,
            town_layout: None,
            reset_act2_start: options.reset_act2_start,
            difficulty_reset_used: false,
            reset_stage: None,
            preparation_rooms: 0,
            inspect_arcane_map: options.inspect_arcane_map,
            arcane_map_inspection: None,
            camera_settling: None,
            camera_deadline: None,
            camera_reading: None,
            town_current: None,
            palace_approach_used: None,
            act2_probes: None,
            approach_until: None,
            extra: serde_json::Map::new(),
        }
    }
}
fn present_value<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<serde_json::Value>, D::Error> {
    serde_json::Value::deserialize(deserializer).map(Some)
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TownPoint {
    pub x: f64,
    pub y: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width_scale: Option<f64>,
}
impl TownPoint {
    pub fn point(&self) -> [f64; 2] {
        [self.x, self.y]
    }
    pub fn is_native_hub(&self) -> bool {
        self.evidence.as_deref() == Some("current_native_hub")
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TownActionKind {
    Click,
    Key,
    Hold,
    Release,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TownAction {
    pub kind: TownActionKind,
    pub reason: String,
    pub phase: TownPhase,
    pub number: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub point: Option<[i64; 2]>,
}

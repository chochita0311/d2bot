use crate::{
    common::{
        buffing::BeltVisibility,
        config::BotConfig,
        encounters::Appearance,
        error::{ControlError, ObservationError},
        field_control::{DesktopPoint, FieldInputBackend},
        frame::{BgrFrame, Size},
        survival::CharacterSurvivalPolicy,
        survival_vision::{HudReading, ResourceReading},
        time::MonotonicClock,
        workers::CooperativeStop,
    },
    runs::{
        RunError,
        summoner::{
            appearance_vision::SummonerAppearance, encounter_execution::EncounterEnvironment,
            scene_vision::HostileTarget,
        },
    },
};
use serde_json::{Value, json};
use std::{
    error::Error,
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};
#[derive(Clone)]
pub(crate) struct World(
    pub(crate) Arc<Mutex<WorldState>>,
    pub(crate) CooperativeStop,
);
pub(crate) struct WorldState {
    pub(crate) case: Value,
    pub(crate) now: f64,
    pub(crate) index: i64,
    pub(crate) pointer: Option<DesktopPoint>,
    pub(crate) events: Vec<Value>,
    pub(crate) emits: Vec<String>,
    pub(crate) probe_pending: bool,
    pub(crate) probe_requested_at: Option<f64>,
}
impl World {
    pub(crate) fn new(case: &Value) -> Self {
        Self(
            Arc::new(Mutex::new(WorldState {
                case: case.clone(),
                now: 10.0,
                index: -1,
                pointer: None,
                events: vec![],
                emits: vec![],
                probe_pending: false,
                probe_requested_at: None,
            })),
            CooperativeStop::default(),
        )
    }
    pub(crate) fn state(&self) -> MutexGuard<'_, WorldState> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub(crate) fn index(&self) -> i64 {
        self.state().index
    }
    pub(crate) fn sample(&self) -> Value {
        let state = self.state();
        state.case["frames"]
            .as_array()
            .and_then(|frames| {
                frames.get((state.index.max(0) as usize).min(frames.len().saturating_sub(1)))
            })
            .cloned()
            .unwrap_or(json!({}))
    }
    pub(crate) fn event(&self, event: Value) {
        self.state().events.push(event);
    }
    pub(crate) fn advance(&self, seconds: f64) {
        let mut state = self.state();
        state.now += seconds;
        if state.now >= state.case["stop_at"].as_f64().unwrap_or(1e9) {
            self.1.set();
        }
    }
    pub(crate) fn delay(&self, name: &str) {
        self.advance(self.sample()[name].as_f64().unwrap_or(0.0));
    }
    pub(crate) fn focused(&self) -> Result<bool, ControlError> {
        let now = self.now();
        self.event(json!(["focus", now]));
        Ok(now < self.state().case["focus_lost_at"].as_f64().unwrap_or(1e9))
    }
    pub(crate) fn appearance(&self, state: &str) -> Option<SummonerAppearance> {
        self.event(json!([state, self.index()]));
        self.delay(&format!("{state}_delay"));
        let sample = self.sample();
        sample[state]
            .as_bool()
            .unwrap_or(state == "alive")
            .then(|| SummonerAppearance {
                r#box: [790, 540, 810, 580],
                ground: point(&sample[format!("{state}_ground")], [800.0, 580.0]),
                score: 1.0,
                appearance: if state == "alive" {
                    Appearance::Alive
                } else {
                    Appearance::Corpse
                },
                asset_id: state.into(),
            })
    }
}
pub(crate) fn point(value: &Value, fallback: [f64; 2]) -> [f64; 2] {
    value
        .as_array()
        .filter(|v| v.len() == 2)
        .map(|v| {
            [
                v[0].as_f64().unwrap_or(fallback[0]),
                v[1].as_f64().unwrap_or(fallback[1]),
            ]
        })
        .unwrap_or(fallback)
}
impl MonotonicClock for World {
    fn now(&self) -> f64 {
        self.state().now
    }
}
impl FieldInputBackend for World {
    fn key_down(&self, key: &str) -> Result<(), ControlError> {
        self.event(json!(["down", key, self.now()]));
        Ok(())
    }
    fn key_up(&self, key: &str) -> Result<(), ControlError> {
        self.event(json!(["up", key, self.now()]));
        Ok(())
    }
    fn press(&self, key: &str) -> Result<(), ControlError> {
        self.event(json!(["press", key, self.now()]));
        let delay = self.state().case["press_delay"].as_f64().unwrap_or(0.0);
        self.advance(delay);
        Ok(())
    }
    fn move_pointer(&self, point: DesktopPoint) -> Result<(), ControlError> {
        self.event(json!(["move", point, self.now()]));
        let delay = {
            let mut state = self.state();
            state.pointer = Some(point);
            state.case["move_delay"].as_f64().unwrap_or(0.0)
        };
        self.advance(delay);
        Ok(())
    }
    fn supports_pointer_confirmation(&self) -> bool {
        true
    }
    fn pointer_matches(&self, point: DesktopPoint) -> Result<bool, ControlError> {
        let state = self.state();
        Ok(state.case["pointer_drift"] != true && state.pointer == Some(point))
    }
}
impl EncounterEnvironment for World {
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError> {
        let (index, now) = {
            let mut s = self.state();
            s.index += 1;
            (s.index, s.now)
        };
        self.event(json!(["capture", index, now]));
        self.advance(self.sample()["capture_delay"].as_f64().unwrap_or(0.1));
        Ok(Arc::new(
            BgrFrame::from_vec(
                Size::new(1, 1).map_err(ObservationError::from)?,
                vec![index as u8, (index >> 8) as u8, (index >> 16) as u8],
            )
            .map_err(ObservationError::from)?,
        ))
    }
    fn hud(&mut self, _: &Arc<BgrFrame>) -> Result<HudReading, RunError> {
        self.event(json!(["hud", self.index()]));
        self.delay("hud_delay");
        let sample = self.sample();
        let resource = |name: &str| match sample.get(name).unwrap_or(&json!(90)).as_u64() {
            Some(value) => ResourceReading::from_counts(value, 100, "read", None),
            None => ResourceReading::default(),
        };
        Ok(HudReading {
            layout_id: "test".into(),
            revision: "1".into(),
            life: resource("life"),
            mana: resource("mana"),
            belt_status: "read".into(),
            slots: vec![],
            belt_visibility: match sample["belt"].as_str().unwrap_or("closed") {
                "closed" => BeltVisibility::Closed,
                "expanded" => BeltVisibility::Expanded,
                _ => BeltVisibility::Unknown,
            },
            mercenary_status: sample["merc"].as_str().map(str::to_owned),
        })
    }
    fn wait(&self, seconds: f64) -> Result<(), ControlError> {
        self.advance(seconds);
        Ok(())
    }
    fn frame_to_desktop(&self, point: [i32; 2]) -> Result<[i64; 2], ControlError> {
        Ok([i64::from(point[0]) - 1922, i64::from(point[1])])
    }
    fn emit(&mut self, message: &str) {
        self.state().emits.push(message.into());
    }
    fn loot_movement_allowed(&self) -> Option<bool> {
        let state = self.state();
        (state.case["loot"] == true).then(|| state.case["labels_off"].as_bool().unwrap_or(true))
    }
    fn recover_resources(
        &mut self,
        _: &Arc<BgrFrame>,
        _: &HudReading,
        acquired: f64,
    ) -> Result<bool, RunError> {
        self.event(json!(["recovery", self.index(), acquired]));
        Ok(self.sample()["recover"].as_bool().unwrap_or(false))
    }
    fn has_probe(&self) -> bool {
        self.state().case["probe"] == true
    }
    fn probe_pending(&self) -> bool {
        self.state().probe_pending
    }
    fn encounter_map_visible(&mut self, _: &Arc<BgrFrame>) -> Result<Option<bool>, RunError> {
        let state = self.state();
        if state.case["map"] != true {
            return Ok(None);
        }
        let toggles = state
            .events
            .iter()
            .filter(|event| event[0] == "press" && event[1] == "tab")
            .count();
        Ok(Some(state.case["map_stuck_on"] == true || toggles % 2 == 0))
    }
    fn encounter_panels_open(&mut self, _: &Arc<BgrFrame>) -> Result<bool, RunError> {
        Ok(self.sample()["panel"] == true)
    }
    fn restore_map(&mut self) -> Result<Value, RunError> {
        self.event(json!(["restore_map"]));
        if self.state().case["map_restore_error"] == true {
            Err(RunError::Runtime("map restore failed".into()))
        } else {
            Ok(json!({"status":"on_observed"}))
        }
    }
    fn reset_probe(&mut self) {
        if self.has_probe() {
            self.event(json!(["probe_reset"]));
            self.state().probe_pending = false;
        }
    }
    fn actor_proposals(
        &mut self,
        _: &Arc<BgrFrame>,
        player: [i32; 2],
    ) -> Result<Vec<[i32; 2]>, RunError> {
        self.event(json!(["proposals", self.index(), player]));
        Ok(self.sample()["proposals"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|v| point(v, [0.0; 2]).map(|x| x as i32))
                    .collect()
            })
            .unwrap_or_default())
    }
    fn probe(
        &mut self,
        _: &Arc<BgrFrame>,
        player: [i32; 2],
        acquired: f64,
        points: Option<&[[i32; 2]]>,
    ) -> Result<Option<HostileTarget>, RunError> {
        self.event(json!(["probe", self.index(), player, acquired, points]));
        self.state().probe_pending = true;
        self.state().probe_requested_at = Some(self.now());
        Ok((self.sample()["hostile"] == true).then(|| HostileTarget {
            kind: "hostile".into(),
            evidence_box: [700, 55, 800, 90],
            title_score: None,
        }))
    }
    fn inspect_loot(
        &mut self,
        started: f64,
        dispatches: u64,
        buff: Option<f64>,
    ) -> Result<Option<Value>, RunError> {
        if self.state().case["loot"] != true {
            return Ok(None);
        }
        let kind = if self.state().case["kind"] == "continuous" {
            "navigation"
        } else if self.state().case["kind"] == "paused" {
            "paused"
        } else {
            "encounter"
        };
        let reason = if kind == "paused" {
            "paused_defense_ended_scan"
        } else {
            "combat_ended_scan"
        };
        self.event(json!(["loot", {"buff_expires_at":buff,"reason":reason,"combat_id":[kind,started,dispatches]}]));
        let mut fallback = json!({"status":"inspection_complete","bounded_reason":"two_frames_without_eligible_candidate","movement_allowed":true,"clicks":[]});
        if kind == "navigation" {
            fallback["teleports"] = json!([]);
            fallback["labels_state"] = json!("off");
        }
        Ok(Some(
            self.state()
                .case
                .get("inspection")
                .cloned()
                .unwrap_or(fallback),
        ))
    }
}
pub(crate) fn config() -> Result<(Value, CharacterSurvivalPolicy), Box<dyn Error>> {
    let mut config = BotConfig::load(Path::new(env!("CARGO_MANIFEST_DIR")).join("config"))?;
    config.apply_character_selection(Some("flash"))?;
    let value = config.active_character_profile().ok_or("profile")?["survival"].clone();
    Ok((value.clone(), CharacterSurvivalPolicy::from_value(&value)?))
}
pub(crate) fn frame_index(frame: &BgrFrame) -> u64 {
    let p = frame.pixels();
    u64::from(p[0]) | (u64::from(p[1]) << 8) | (u64::from(p[2]) << 16)
}
pub(crate) fn compare(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert!(
            (a.as_f64().unwrap_or(f64::NAN) - b.as_f64().unwrap_or(f64::NAN)).abs() < 1e-8,
            "{path}: {a} != {b}"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}: {a:?} != {b:?}");
            for (key, a) in a {
                compare(a, &b[key], &format!("{path}.{key}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}

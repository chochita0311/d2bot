use d2bot::{
    common::{
        buffing::{BeltVisibility, PauseSchedule},
        conditional_loot::{
            ConditionalLootController, ConditionalLootEnvironment, LootGround, LootLabelHit,
            LootLimits,
        },
        config::{BotConfig, ConfigError},
        error::ControlError,
        field_preparation::*,
        frame::{BgrFrame, Size},
        survival::CharacterSurvivalPolicy,
        survival_vision::{HudReading, ResourceReading, SlotReading},
    },
    runs::{RunError, summoner::supervised_renewal::*},
};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap, error::Error, path::Path, rc::Rc, sync::Arc};
#[path = "rust-field-preparation/renewal.rs"]
mod renewal;
mod support;
type TestResult = Result<(), Box<dyn Error>>;
#[test]
fn streamed_buff_traces_release_each_frame_and_preserve_dispatch_contract() -> TestResult {
    let (_, policy) = config()?;
    for before in [false, true] {
        for extra in [
            json!({}),
            json!({"focus_lost_at":10.3}),
            json!({"frames":[{"capture_delay":0.3}]}),
        ] {
            let mut case = json!({"kind":"dispatch","order":["w","a","a","w"],"trace":true,"trace_before":before});
            case.as_object_mut()
                .ok_or("case")?
                .extend(extra.as_object().ok_or("extra")?.clone());
            let buffered = execute(&case, &policy)?;
            case["stream_trace"] = json!(true);
            compare(&execute(&case, &policy)?, &buffered, "streamed buff");
        }
    }
    let failed = execute(
        &json!({"kind":"dispatch","order":["w","a","w"],"trace_before":true,"stream_trace":true,"trace_sink_error":true}),
        &policy,
    )?;
    assert!(
        failed["error"]["message"]
            .as_str()
            .is_some_and(|v| v.contains("trace consumer unavailable"))
    );
    assert!(
        !failed["events"]
            .as_array()
            .ok_or("events")?
            .iter()
            .any(|e| e[0] == "press")
    );
    Ok(())
}
fn compare(a: &Value, b: &Value, path: &str) {
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
fn config() -> Result<(Value, CharacterSurvivalPolicy), Box<dyn Error>> {
    let mut config = BotConfig::load(Path::new(env!("CARGO_MANIFEST_DIR")).join("config"))?;
    config.apply_character_selection(Some("flash"))?;
    let raw = config.active_character_profile().ok_or("profile")?["survival"].clone();
    Ok((raw.clone(), CharacterSurvivalPolicy::from_value(&raw)?))
}
fn config_error(error: ConfigError) -> Value {
    json!({"kind":error.kind(),"message":error.message()})
}
fn run_error(error: RunError) -> Value {
    match error {
        RunError::Control(ControlError::Configuration(error)) => config_error(error),
        error => json!({"kind":"RuntimeError","message":error.to_string()}),
    }
}
#[derive(Clone)]
struct Env(Rc<RefCell<State>>);
struct State {
    case: Value,
    now: f64,
    index: i64,
    events: Vec<Value>,
    stopped: bool,
    scan_calls: usize,
    pointer: Option<[i64; 2]>,
}
impl Env {
    fn index(&self) -> i64 {
        self.0.borrow().index
    }
    fn new(case: &Value) -> Self {
        Self(Rc::new(RefCell::new(State {
            case: case.clone(),
            now: case["now"].as_f64().unwrap_or(10.0),
            index: -1,
            events: vec![],
            stopped: false,
            scan_calls: 0,
            pointer: None,
        })))
    }
    fn sample(&self) -> Value {
        let state = self.0.borrow();
        let frames = state.case["frames"].as_array();
        frames
            .and_then(|frames| {
                frames.get((state.index.max(0) as usize).min(frames.len().saturating_sub(1)))
            })
            .cloned()
            .unwrap_or(json!({}))
    }
    fn event(&self, event: Value) {
        self.0.borrow_mut().events.push(event);
    }
    fn delay(&self, name: &str) {
        let amount = self.sample()[name].as_f64().unwrap_or(0.0);
        self.0.borrow_mut().now += amount;
    }
}
impl PreparationEnvironment for Env {
    type Error = RunError;
    fn clock(&self) -> f64 {
        self.0.borrow().now
    }
    fn wall_clock(&self) -> f64 {
        1000.0 + self.clock()
    }
    fn stopped(&self) -> bool {
        self.0.borrow().stopped
    }
    fn focused(&self) -> Result<bool, RunError> {
        let now = self.clock();
        self.event(json!(["focus", now]));
        Ok(now
            < self.0.borrow().case["focus_lost_at"]
                .as_f64()
                .unwrap_or(1e9))
    }
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError> {
        let (index, now) = {
            let mut s = self.0.borrow_mut();
            s.index += 1;
            (s.index, s.now)
        };
        self.event(json!(["capture", index, now]));
        self.delay("capture_delay");
        if self.0.borrow().case["kind"] == "loot" {
            let mut frame = BgrFrame::zeros(
                Size::new(1922, 1140).map_err(|e| ControlError::Runtime(e.to_string()))?,
            )
            .map_err(|e| ControlError::Runtime(e.to_string()))?;
            frame.pixels_mut()[0] = index as u8;
            return Ok(Arc::new(frame));
        }
        Ok(Arc::new(
            BgrFrame::from_vec(
                Size::new(1, 1).map_err(|e| ControlError::Runtime(e.to_string()))?,
                vec![index as u8, 0, 0],
            )
            .map_err(|e| ControlError::Runtime(e.to_string()))?,
        ))
    }
    fn hud(&mut self, _: &Arc<BgrFrame>) -> Result<HudReading, RunError> {
        let index = self.0.borrow().index;
        self.event(json!(["hud", index]));
        self.delay("hud_delay");
        let sample = self.sample();
        let resource = |key: &str| -> ResourceReading {
            if let Some(lower_bound) = sample[format!("{key}_lower_bound")].as_f64() {
                return ResourceReading::with_lower_bound(lower_bound, "orb", None)
                    .unwrap_or_default();
            }
            match sample.get(key).unwrap_or(&json!(90)).as_u64() {
                Some(value) => ResourceReading::from_counts(
                    value,
                    sample[format!("{key}_maximum")].as_u64().unwrap_or(100),
                    "read",
                    None,
                ),
                None => ResourceReading::default(),
            }
        };
        Ok(HudReading {
            layout_id: "test".into(),
            revision: "1".into(),
            life: resource("life"),
            mana: resource("mana"),
            belt_status: sample["belt_status"].as_str().unwrap_or("read").into(),
            slots: slots(&sample["slots"]),
            belt_visibility: match sample["belt"].as_str().unwrap_or("closed") {
                "closed" => BeltVisibility::Closed,
                "expanded" => BeltVisibility::Expanded,
                _ => BeltVisibility::Unknown,
            },
            mercenary_status: sample["merc"].as_str().map(str::to_owned),
        })
    }
    fn press(&mut self, key: &str) -> Result<(), RunError> {
        self.event(json!(["press", key, self.clock()]));
        let press_count = self
            .0
            .borrow()
            .events
            .iter()
            .filter(|event| event[0] == "press")
            .count() as u64;
        if self.0.borrow().case["fail_key"] == key
            || self.0.borrow().case["fail_press_number"].as_u64() == Some(press_count)
        {
            return Err(RunError::Runtime("backend request failed".into()));
        }
        let delay = self.0.borrow().case["press_delay"].as_f64().unwrap_or(0.0);
        self.0.borrow_mut().now += delay;
        {
            let mut state = self.0.borrow_mut();
            if state.case["stop_on_key"] == key {
                state.stopped = true;
            }
            if state.case["focus_after_key"] == key {
                state.case["focus_lost_at"] = json!(state.now);
            }
        }
        Ok(())
    }
    fn wait(&mut self, seconds: f64) -> Result<(), RunError> {
        self.event(json!(["wait", seconds]));
        let mut s = self.0.borrow_mut();
        s.now += s.case["wait_advance"].as_f64().unwrap_or(seconds);
        s.stopped = s.now >= s.case["stop_at"].as_f64().unwrap_or(1e9);
        Ok(())
    }
}
struct Checks(Env);
impl PreparationChecks<Env> for Checks {
    fn observe_frame(&mut self, env: &mut Env, _: &Arc<BgrFrame>, at: f64) -> Result<(), RunError> {
        self.0.event(json!(["observe", env.index(), at]));
        self.0.delay("observe_delay");
        Ok(())
    }
    fn check_frame(
        &mut self,
        env: &mut Env,
        _: &Arc<BgrFrame>,
        at: f64,
        _: &HudReading,
    ) -> Result<Option<String>, RunError> {
        self.0.event(json!(["check", env.index(), at]));
        self.0.delay("check_delay");
        Ok(self.0.sample()["blocked"].as_str().map(str::to_owned))
    }
    fn before_step(&mut self, env: &mut Env) -> Result<Option<String>, RunError> {
        self.0.event(json!(["before", env.clock()]));
        let mut state = self.0.0.borrow_mut();
        state.now += state.case["before_delay"].as_f64().unwrap_or(0.0);
        Ok(state.case["before_blocked"].as_str().map(str::to_owned))
    }
}
impl PreparationScreen<Env> for Checks {
    fn panels_open(&mut self, _: &Arc<BgrFrame>) -> Result<bool, RunError> {
        self.0.event(json!(["panels", self.0.index()]));
        self.0.delay("panels_delay");
        Ok(self.0.sample()["panels"] == true)
    }
    fn automap(&mut self, _: &Arc<BgrFrame>) -> Result<AutomapSample, RunError> {
        self.0
            .event(json!(["automap", self.0.index(), "arcane_sanctuary"]));
        self.0.delay("map_delay");
        let sample = self.0.sample();
        Ok(AutomapSample {
            visible: sample["visible"] == true,
            obscured: sample["obscured"] == true,
            evidence: json!({"sample":self.0.0.borrow().index}),
        })
    }
}
fn execute(case: &Value, policy: &CharacterSurvivalPolicy) -> Result<Value, Box<dyn Error>> {
    let kind = case["kind"].as_str().ok_or("kind")?;
    if kind == "automatic" {
        return automatic_case(case, policy);
    }
    if kind == "site" {
        return site_case(case, policy);
    }
    if kind == "recovery" {
        return recovery_case(case, policy);
    }
    if kind == "loot" {
        return loot_case(case, policy);
    }
    if ["runtime", "permit", "travel", "confirm"].contains(&kind) {
        let now = case["now"].as_f64().ok_or("now")?;
        let outcome = match kind {
            "runtime" => validate_runtime_permit(&case["proof"], now),
            "permit" => validate_permit(&case["proof"], "flash", policy, now, case["buff"] == true),
            "travel" => validate_travel_permit(&case["proof"], policy, now),
            _ => confirm_initial_probe(
                &case["proof"],
                "flash",
                case["last_cast"].as_f64().unwrap_or(0.0),
                now,
            ),
        };
        return Ok(outcome.err().map(config_error).unwrap_or(Value::Null));
    }
    let mut env = Env::new(case);
    let mut checks = Checks(env.clone());
    let mut traces = vec![];
    let mut streamed = vec![];
    let mut released: Vec<std::sync::Weak<BgrFrame>> = vec![];
    let mut store_trace = |trace: BuffTrace| -> Result<(), ControlError> {
        assert!(released.iter().all(|weak| weak.upgrade().is_none()));
        released.push(Arc::downgrade(&trace.frame));
        streamed.push(json!({"frame":trace.frame.pixels()[0],"metadata":trace.metadata}));
        if case["trace_sink_error"] == true {
            return Err(ControlError::Runtime("trace consumer unavailable".into()));
        }
        Ok(())
    };
    let outcome: Result<Value, RunError> = match kind {
        "wait" => {
            let retry: Vec<&str> = case["retry"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            wait_readable_resources(
                &mut env,
                policy,
                case["deadline"].as_f64().unwrap_or(11.0),
                &mut checks,
                &retry,
            )
            .map(|value| json!(value))
        }
        "dispatch" => {
            let order: Vec<String> = serde_json::from_value(case["order"].clone())?;
            let pauses: BTreeMap<String, f64> =
                serde_json::from_value(case.get("pauses").cloned().unwrap_or(json!({})))?;
            let pauses = PauseSchedule::new(pauses, 0.4)?;
            dispatch_buffs(
                &mut env,
                policy,
                &order,
                &pauses,
                &mut checks,
                BuffDispatchOptions {
                    trace_sink: if case["stream_trace"] == true {
                        Some(&mut store_trace)
                    } else {
                        None
                    },
                    trace_frames: if case["trace"] == true && case["stream_trace"] != true {
                        Some(&mut traces)
                    } else {
                        None
                    },
                    observation_wait: case["observation_wait"].as_f64().unwrap_or(0.75),
                    sequence_timeout: case["sequence_timeout"].as_f64().unwrap_or(8.0),
                    trace_before_dispatch: case["trace_before"] == true,
                },
            )
            .map(|value| json!(value))
        }
        "map" => ensure_automap_on(
            &mut env,
            policy,
            &mut checks,
            case.get("proof"),
            case["allow_toggle"] != false,
            case["timeout"].as_f64().unwrap_or(0.4),
        )
        .map(|value| json!(value)),
        "labels" => enable_fresh_room_item_labels(&mut env, policy, &mut checks),
        "weapon" => {
            let observed = env.clone();
            ensure_weapon_set(
                &mut env,
                case["expected"].as_u64().unwrap_or(1) as u8,
                case["timeout"].as_f64().unwrap_or(0.1),
                &mut |_frame| {
                    observed.event(json!(["weapon", observed.index()]));
                    observed.delay("weapon_delay");
                    Ok(observed.sample()["weapon"]
                        .as_u64()
                        .map(|number| number as u8))
                },
            )
        }
        _ => return Err("kind".into()),
    };
    let (result, error) = match outcome {
        Ok(value) => (value, Value::Null),
        Err(error) => (Value::Null, run_error(error)),
    };
    assert!(released.iter().all(|weak| weak.upgrade().is_none()));
    let mut traces: Vec<_> = traces
        .into_iter()
        .map(|trace| json!({"frame":trace.frame.pixels()[0],"metadata":trace.metadata}))
        .collect();
    traces.extend(streamed);
    Ok(
        json!({"result":result,"error":error,"events":env.0.borrow().events,"now":env.clock(),"trace":traces}),
    )
}

impl ConditionalLootEnvironment for Env {
    type Candidate = String;
    fn candidate_label(candidate: &String) -> &str {
        candidate
    }
    fn build_candidates(&mut self) -> Result<Vec<String>, RunError> {
        self.event(json!(["build"]));
        let state = self.0.borrow();
        if state.case["build_error"] == true {
            return Err(RunError::Runtime("candidate build failed".into()));
        }
        Ok(serde_json::from_value(
            state
                .case
                .get("candidates")
                .cloned()
                .unwrap_or(json!(["key of hate"])),
        )
        .map_err(|error| ControlError::Runtime(error.to_string()))?)
    }
    fn scan_frame(
        &mut self,
        _: &Arc<BgrFrame>,
        allowed: &[String],
    ) -> Result<Option<LootLabelHit>, RunError> {
        self.event(json!(["scan", self.index(), allowed]));
        self.delay("scan_delay");
        if self.sample()["scan_error"] == true {
            return Err(RunError::Runtime("label scan failed".into()));
        }
        let value = {
            let mut state = self.0.borrow_mut();
            let index = state.scan_calls;
            state.scan_calls += 1;
            state.case["hits"]
                .get(index)
                .or_else(|| state.case.get("repeat_hit"))
                .cloned()
                .unwrap_or(Value::Null)
        };
        if value.is_null() {
            return Ok(None);
        }
        Ok(Some(LootLabelHit {
            label: value["label"].as_str().unwrap_or("key of hate").into(),
            top_left: serde_json::from_value(
                value.get("top_left").cloned().unwrap_or(json!([700, 400])),
            )
            .map_err(|error| ControlError::Runtime(error.to_string()))?,
            width: value["width"].as_u64().unwrap_or(100) as usize,
            height: value["height"].as_u64().unwrap_or(20) as usize,
        }))
    }
    fn panels_open(&mut self, frame: &Arc<BgrFrame>) -> Result<bool, RunError> {
        Checks(self.clone()).panels_open(frame)
    }
    fn frame_to_desktop(&mut self, point: [i64; 2]) -> Result<[i64; 2], RunError> {
        self.event(json!(["map", point, self.clock()]));
        let state = self.0.borrow();
        if state.case["mapper_error"] == true {
            return Err(RunError::Runtime("geometry changed".into()));
        }
        let origin: [i64; 2] = serde_json::from_value(
            state
                .case
                .get("origin")
                .cloned()
                .unwrap_or(json!([-1922, 0])),
        )
        .map_err(|error| ControlError::Runtime(error.to_string()))?;
        Ok([point[0] + origin[0], point[1] + origin[1]])
    }
    fn supports_click_confirmation(&self) -> bool {
        self.0.borrow().case["supports_click"] != false
    }
    fn move_pointer(&mut self, point: [i64; 2]) -> Result<(), RunError> {
        self.event(json!(["move", point, self.clock()]));
        let mut state = self.0.borrow_mut();
        state.pointer = Some(point);
        state.now += state.case["move_delay"].as_f64().unwrap_or(0.0);
        Ok(())
    }
    fn pointer_matches(&mut self, point: [i64; 2]) -> Result<bool, RunError> {
        self.event(json!(["pointer", point, self.clock()]));
        let mut state = self.0.borrow_mut();
        state.now += state.case["pointer_delay"].as_f64().unwrap_or(0.0);
        Ok(state.case["pointer_drift"] != true && state.pointer == Some(point))
    }
    fn click(&mut self) -> Result<(), RunError> {
        self.event(json!(["click", self.clock()]));
        let mut state = self.0.borrow_mut();
        if state.case["click_error"] == true {
            return Err(RunError::Runtime("click failed".into()));
        }
        state.now += state.case["click_delay"].as_f64().unwrap_or(0.0);
        Ok(())
    }
    fn buff_deadline(&mut self) -> Result<Option<f64>, RunError> {
        self.event(json!(["deadline"]));
        Ok(self
            .0
            .borrow()
            .case
            .get("deadline")
            .filter(|value| !value.is_null())
            .map(|value| value.as_f64().unwrap_or(f64::NAN)))
    }
    fn potion_count(&mut self) -> Result<Option<Value>, RunError> {
        self.event(json!(["count"]));
        Ok(self.0.borrow().case.get("potion_count").cloned())
    }
    fn has_recovery(&self) -> bool {
        self.0.borrow().case.get("recover").is_some()
    }
    fn recover_resources(
        &mut self,
        _: &Arc<BgrFrame>,
        _: &HudReading,
        at: f64,
    ) -> Result<bool, RunError> {
        self.event(json!(["recovery", self.index(), at]));
        let mut state = self.0.borrow_mut();
        state.now += state.case["recovery_delay"].as_f64().unwrap_or(0.0);
        Ok(state.case["recover"] == true)
    }
    fn ground_available(&mut self) -> Result<bool, RunError> {
        self.event(json!(["ground_available"]));
        Ok(self.0.borrow().case["ground_observer"] != false)
    }
    fn observe_ground(&mut self, frame: &Arc<BgrFrame>, at: f64) -> Result<LootGround, RunError> {
        if self.0.borrow().case["camera"] == true {
            let _ = self.hud(frame)?;
            let focused = self.focused()?;
            self.event(json!(["prepare_ground", self.index(), at, focused]));
            self.delay("prepare_ground_delay");
        }
        self.event(json!(["loot_ground", self.index(), at]));
        self.delay("ground_delay");
        let sample = self.sample();
        Ok(LootGround {
            point: serde_json::from_value(
                sample.get("ground").cloned().unwrap_or(json!([750, 450])),
            )
            .map_err(|error| ControlError::Runtime(error.to_string()))?,
            reason: sample["ground_reason"].as_str().map(str::to_owned),
        })
    }
}
fn loot_case(case: &Value, policy: &CharacterSurvivalPolicy) -> Result<Value, Box<dyn Error>> {
    let limits = match LootLimits::new(
        case["scan_seconds"].as_f64().unwrap_or(3.0),
        case["scan_frames"].as_u64().unwrap_or(12) as usize,
        case["clicks"].as_u64().unwrap_or(2) as usize,
        case["reference"].as_f64().unwrap_or(0.32),
    ) {
        Ok(limits) => limits,
        Err(error) => return Ok(config_error(error)),
    };
    let ignored: Vec<String> =
        serde_json::from_value(case.get("ignored").cloned().unwrap_or(json!([])))?;
    let mut controller =
        ConditionalLootController::new(Arc::new(policy.clone()), "f2".into(), ignored, limits);
    let mut env = Env::new(case);
    let mut rows = vec![];
    let default = vec![
        json!({"method":"prepare","fresh_room":true}),
        json!({"method":"scan"}),
    ];
    let operations = case["operations"].as_array().unwrap_or(&default);
    for operation in operations {
        let outcome = if operation["method"] == "prepare" {
            controller.prepare(
                &mut env,
                operation["fresh_room"] != false,
                operation["labels_on"].as_bool(),
            )
        } else {
            controller
                .after_combat(
                    &mut env,
                    operation["buff"].as_f64(),
                    operation["reason"].as_str().unwrap_or("combat_ended_scan"),
                    operation.get("combat_id").cloned(),
                )
                .map(|value| json!(value))
        };
        let (result, error) = match outcome {
            Ok(value) => (value, Value::Null),
            Err(error) => (Value::Null, run_error(error)),
        };
        rows.push(json!({"result":result,"error":error}));
    }
    let hud_failure = controller
        .last_hud_failure()
        .map(|evidence| json!({"frame":evidence.frame.pixels()[0],"metadata":evidence.metadata}));
    let landing = controller
        .last_landing_observation()
        .map(|evidence| json!({"frame":evidence.frame.pixels()[0],"metadata":evidence.metadata}));
    Ok(
        json!({"rows":rows,"records":controller.records(),"labels_state":controller.labels_state(),"movement_allowed":controller.movement_allowed(),"resource_exit_required":controller.resource_exit_required(),"hud_failure":hud_failure,"landing":landing,"events":env.0.borrow().events,"now":env.clock()}),
    )
}

#[test]
fn conditional_loot_preserves_off_on_reconfirmation_teleport_click_and_no_pickup_claim()
-> TestResult {
    let base =
        json!({"kind":"loot","hits":[{},{},{"top_left":[850,400]},{"top_left":[850,400]},null]});
    let mut cases = vec![
        json!({"kind":"loot"}),
        base.clone(),
        json!({"kind":"loot","operations":[{"method":"prepare"},{"method":"prepare"},{"method":"scan","combat_id":"one"},{"method":"scan","combat_id":"one"}]}),
    ];
    for (key, value) in [
        ("supports_click", json!(false)),
        ("ground_observer", json!(false)),
        ("pointer_drift", json!(true)),
        ("mapper_error", json!(true)),
        ("click_error", json!(true)),
        ("build_error", json!(true)),
        ("camera", json!(true)),
        ("clicks", json!(0)),
        ("scan_frames", json!(1)),
        ("deadline", json!(10.1)),
        ("deadline", json!("nan")),
        ("deadline", json!(true)),
        ("move_delay", json!(0.25)),
        ("pointer_delay", json!(0.25)),
        ("fail_press_number", json!(2)),
        ("fail_press_number", json!(3)),
        ("fail_press_number", json!(4)),
        ("stop_on_key", json!("f2")),
        ("focus_after_key", json!("alt")),
        ("press_delay", json!(0.25)),
    ] {
        let mut case = base.clone();
        case[key] = value;
        cases.push(case);
    }
    for hit in [
        Value::Null,
        json!({"top_left":[1300,400]}),
        json!({"label":"rune"}),
        json!({"top_left":[718,400]}),
        json!({"top_left":[719,400]}),
    ] {
        for index in [1, 2, 3] {
            let mut case = base.clone();
            case["hits"][index] = hit.clone();
            cases.push(case);
        }
    }
    for sample in [
        json!({"ground":null,"ground_reason":"pose_lost"}),
        json!({"ground":[1400,900]}),
        json!({"scan_delay":0.25}),
        json!({"ground_delay":0.25}),
        json!({"life":null}),
        json!({"panels":true}),
    ] {
        let mut case = base.clone();
        let mut frames = vec![json!({}); 7];
        frames[4] = sample;
        case["frames"] = json!(frames);
        cases.push(case);
    }
    for operations in [
        json!([{"method":"prepare","fresh_room":false}]),
        json!([{"method":"prepare","fresh_room":true,"labels_on":true}]),
        json!([{"method":"prepare","fresh_room":false,"labels_on":true},{"method":"scan"}]),
        json!([{"method":"scan"}]),
    ] {
        let mut case = base.clone();
        case["operations"] = operations;
        cases.push(case);
    }
    exercise(cases)
}

#[test]
fn conditional_loot_keeps_resource_priority_unknown_hud_deadlines_and_current_potion_count()
-> TestResult {
    let mut cases = vec![];
    for sample in [
        json!({"life":null}),
        json!({"life":null,"mana":1}),
        json!({"capture_delay":0.25}),
        json!({"panels":true}),
        json!({"merc":"unverified"}),
    ] {
        for index in [0, 2, 4] {
            // ON 복원의 초상 가림은 별도 Rust 회귀에서 새 계약을 검증한다.
            if sample["merc"] == "unverified" && index == 4 {
                continue;
            }
            let mut frames = vec![json!({}); 6];
            frames[index] = sample.clone();
            cases.push(json!({"kind":"loot","frames":frames}));
        }
    }
    for recovered in [true, false] {
        for index in [2, 4] {
            let mut frames = vec![json!({}); 6];
            frames[index] = json!({"life":1,"mana":null});
            cases.push(json!({"kind":"loot","frames":frames,"recover":recovered}));
        }
    }
    cases.push(json!({"kind":"loot","frames":[{"life":null}]}));
    for labels in [
        json!(["gold"]),
        json!(["rejuvenation potion"]),
        json!(["full rejuvenation potion"]),
        json!(["key of hate"]),
        json!(["KEY OF HATE"]),
    ] {
        for count in [
            json!({"count":6,"observed_at":10,"confirmed":true}),
            json!({"count":7,"observed_at":10,"confirmed":true}),
            json!({"count":6,"observed_at":9.85,"confirmed":true}),
            json!({"count":6.0,"observed_at":10,"confirmed":true}),
            json!({"count":6,"observed_at":10,"confirmed":false}),
            Value::Null,
        ] {
            cases.push(json!({"kind":"loot","candidates":labels,"potion_count":count}));
        }
    }
    cases.push(json!({"kind":"loot","candidates":["KEY OF HATE"],"ignored":["key of hate"]}));
    for (key, value) in [
        ("scan_seconds", json!(0)),
        ("scan_seconds", json!(3.1)),
        ("scan_frames", json!(0)),
        ("scan_frames", json!(31)),
        ("clicks", json!(5)),
        ("reference", json!(0)),
        ("reference", json!(1.5)),
    ] {
        let mut case = json!({"kind":"loot"});
        case[key] = value;
        cases.push(case);
    }
    exercise(cases)
}

#[test]
fn owned_label_cleanup_rechecks_full_hud_before_allowing_movement() -> TestResult {
    let (_, policy) = config()?;
    let unknown = json!({"merc":"unverified"});
    let recovered = loot_case(
        &json!({"kind":"loot","frames":[{}, {},unknown,unknown,{}]}),
        &policy,
    )?;
    let report = &recovered["records"][0];
    assert_eq!(report["status"], "inspection_interrupted");
    assert_eq!(
        report["bounded_reason"],
        "mercenary_reacquired_after_labels_off"
    );
    assert!(report["overlay_hud_reacquired_at"].as_f64().is_some());
    assert_eq!(recovered["labels_state"], "off");
    assert_eq!(recovered["movement_allowed"], true);
    assert_eq!(report["pickups_confirmed"], 0);
    assert!(report["clicks"].as_array().is_some_and(Vec::is_empty));
    assert!(report["teleports"].as_array().is_some_and(Vec::is_empty));
    let events = recovered["events"].as_array().ok_or("events")?;
    let presses: Vec<_> = events
        .iter()
        .filter(|e| e[0] == "press")
        .map(|e| e[1].clone())
        .collect();
    assert_eq!(presses, vec![json!("alt"), json!("alt")]);
    let off = events.iter().rposition(|e| e[0] == "press").ok_or("off")?;
    let fresh = events
        .iter()
        .position(|e| e[0] == "capture" && e[1] == 4)
        .ok_or("fresh full HUD")?;
    assert!(off < fresh);
    let after_scan = loot_case(
        &json!({"kind":"loot","frames":[{}, {},{}, {},unknown,{}]}),
        &policy,
    )?;
    assert_eq!(after_scan["records"][0]["status"], "inspection_complete");
    assert!(after_scan["records"][0]["overlay_hud_reacquired_at"].is_number());
    assert_eq!(after_scan["movement_allowed"], true);
    let failed_off = loot_case(
        &json!({"kind":"loot","frames":[{}, {},unknown],"fail_press_number":2}),
        &policy,
    )?;
    assert_eq!(failed_off["labels_state"], "unknown");
    assert_eq!(failed_off["movement_allowed"], false);
    assert_eq!(
        failed_off["records"][0]["restore_error"],
        "backend request failed"
    );
    let permanent = loot_case(&json!({"kind":"loot","frames":[{}, {},unknown]}), &policy)?;
    assert_eq!(permanent["labels_state"], "off");
    assert_eq!(permanent["movement_allowed"], false);
    assert_eq!(
        permanent["records"][0]["restore_error"],
        "mercenary_unverified"
    );
    assert!(permanent["records"][0]["overlay_hud_reacquired_at"].is_null());
    for guard in [
        json!({"life":1}),
        json!({"panels":true}),
        json!({"capture_delay":0.25}),
        json!({"belt":"expanded"}),
    ] {
        let blocked = loot_case(
            &json!({"kind":"loot","frames":[{}, {},unknown,unknown,guard]}),
            &policy,
        )?;
        assert_eq!(blocked["movement_allowed"], false, "{blocked}");
        assert!(
            blocked["records"][0]["restore_error"].is_string(),
            "{blocked}"
        );
        assert!(blocked["records"][0]["overlay_hud_reacquired_at"].is_null());
    }
    for key in ["stop_at", "focus_lost_at"] {
        let mut case = json!({"kind":"loot","frames":[{}, {},unknown,unknown,{}]});
        case[key] = json!(10.075);
        let blocked = loot_case(&case, &policy)?;
        assert_eq!(blocked["movement_allowed"], false, "{blocked}");
        assert!(blocked["records"][0]["overlay_hud_reacquired_at"].is_null());
    }
    for guard in [
        json!({"merc":"unverified","life":1}),
        json!({"merc":"unverified","panels":true}),
        json!({"merc":"unverified","capture_delay":0.25}),
    ] {
        let blocked = loot_case(
            &json!({"kind":"loot","frames":[{}, {},unknown,guard]}),
            &policy,
        )?;
        assert_eq!(blocked["movement_allowed"], false);
        let presses: Vec<_> = blocked["events"]
            .as_array()
            .ok_or("events")?
            .iter()
            .filter(|e| e[0] == "press")
            .collect();
        assert_eq!(
            presses.len(),
            1,
            "cleanup must not bypass other guards: {blocked}"
        );
    }
    Ok(())
}

#[test]
fn conditional_loot_keeps_fixed_hud_deadline_single_recovery_and_combat_id_eviction() -> TestResult
{
    let base =
        json!({"kind":"loot","hits":[{},{},{"top_left":[850,400]},{"top_left":[850,400]},null]});
    let mut cases = vec![];
    for advance in [0.729, 0.73] {
        cases.push(json!({"kind":"loot","frames":[{"life":null,"capture_delay":0.01},{"capture_delay":0.01}],"wait_advance":advance,"operations":[{"method":"prepare"}]}));
    }
    for (key, value) in [
        ("stop_at", json!(10.075)),
        ("focus_lost_at", json!(10.075)),
        ("deadline", json!(10.075)),
    ] {
        let mut case = base.clone();
        case[key] = value;
        cases.push(case);
    }
    for frames in [
        json!([{}, {}, {"life":1},{"life":1},{}]),
        json!([{}, {}, {}, {}, {"life":1},{"life":1}]),
        json!([{}, {}, {"life":null}]),
        json!([{}, {}, {}, {}, {"life":null}]),
    ] {
        cases.push(json!({"kind":"loot","frames":frames,"recover":true}));
    }
    for point in [json!([-51, 400]), json!([1900, 400]), json!([700, 1140])] {
        let mut case = base.clone();
        case["hits"][0]["top_left"] = point;
        cases.push(case);
    }
    let mut operations = vec![json!({"method":"prepare"})];
    operations.extend((0..33).map(|id| json!({"method":"scan","combat_id":id})));
    operations.push(json!({"method":"scan","combat_id":0}));
    cases.push(json!({"kind":"loot","operations":operations}));
    cases.push(json!({"kind":"loot","operations":[
        {"method":"prepare"},
        {"method":"scan","combat_id":1}, {"method":"scan","combat_id":1.0}, {"method":"scan","combat_id":true},
        {"method":"scan","combat_id":["room",2,{"dispatch":0}]}, {"method":"scan","combat_id":["room",2.0,{"dispatch":false}]},
        {"method":"scan","combat_id":9007199254740993u64}, {"method":"scan","combat_id":9007199254740992.0},
        {"method":"scan","combat_id":-0.0}, {"method":"scan","combat_id":false}
    ]}));
    exercise(cases)
}

#[test]
fn loot_diagnostic_handoff_releases_pixels_without_erasing_report() -> TestResult {
    let (_, policy) = config()?;
    let policy = Arc::new(policy);
    for (case, movement) in [
        (
            json!({"kind":"loot","frames":[{}, {}, {"life":null}]}),
            false,
        ),
        (
            json!({"kind":"loot","frames":[{}, {}, {"merc":"unverified"}]}),
            false,
        ),
        (
            json!({"kind":"loot","frames":[{}, {}, {"merc":"unverified"},{"merc":"unverified"},{}]}),
            true,
        ),
    ] {
        let mut env = Env::new(&case);
        let mut controller = ConditionalLootController::new(
            Arc::clone(&policy),
            "f2".into(),
            vec![],
            LootLimits::default(),
        );
        controller.prepare(&mut env, true, None)?;
        let _ = controller.after_combat(&mut env, None, "combat_ended_scan", None)?;
        let weak = controller
            .last_hud_failure()
            .map(|evidence| Arc::downgrade(&evidence.frame))
            .ok_or("missing HUD evidence")?;
        let evidence = controller.take_hud_failure();
        assert!(weak.upgrade().is_some());
        drop(evidence);
        assert!(weak.upgrade().is_none());
        assert_eq!(controller.records().len(), 1);
        assert_eq!(controller.movement_allowed(), movement);
    }
    Ok(())
}

fn slots(raw: &Value) -> Vec<SlotReading> {
    raw.as_array()
        .into_iter()
        .flatten()
        .map(|slot| SlotReading {
            row: slot["row"].as_u64().unwrap_or(0) as u8,
            column: slot["column"].as_u64().unwrap_or(0) as u8,
            kind: slot["kind"].as_str().map(str::to_owned),
            distance: slot["distance"].as_f64().unwrap_or(0.0),
        })
        .collect()
}
impl d2bot::common::emergency_recovery::RecoveryEnvironment for Env {
    fn closed_belt_slots(
        &mut self,
        _: &Arc<BgrFrame>,
        _: &HudReading,
    ) -> Result<Vec<SlotReading>, RunError> {
        self.event(json!(["closed_slots", self.index()]));
        self.delay("closed_delay");
        Ok(slots(&self.sample()["closed_slots"]))
    }
}
fn recovery_case(case: &Value, policy: &CharacterSurvivalPolicy) -> Result<Value, Box<dyn Error>> {
    let mut env = Env::new(case);
    let mut recovery = d2bot::common::emergency_recovery::EmergencyResourceRecovery::default();
    let mut rows = vec![];
    let default = vec![json!({})];
    let operations = case["operations"].as_array().unwrap_or(&default);
    for operation in operations {
        let acquired = env.clock();
        let frame = env.capture()?;
        let reading = env.hud(&frame)?;
        env.0.borrow_mut().now += operation["delay"].as_f64().unwrap_or(0.0);
        let (result, error) = match recovery.recover(&mut env, policy, &frame, &reading, acquired) {
            Ok(value) => (json!(value), Value::Null),
            Err(error) => (Value::Null, run_error(error)),
        };
        rows.push(json!({"result":result,"error":error}));
    }
    let evidence: Vec<_> = recovery
        .evidence()
        .iter()
        .map(|evidence| json!({"filename":evidence.filename,"frame":evidence.frame.pixels()[0]}))
        .collect();
    Ok(
        json!({"rows":rows,"records":recovery.records(),"evidence":evidence,"events":env.0.borrow().events,"now":env.clock()}),
    )
}

#[test]
fn emergency_recovery_compares_single_request_new_ratios_counts_and_finally_closure() -> TestResult
{
    let (_, policy) = config()?;
    let columns: Vec<_> = policy
        .belt_columns()
        .iter()
        .map(|column| json!({"row":4,"column":column.column(),"kind":column.kind(),"distance":0}))
        .collect();
    let all:Vec<_>=(1..=4).flat_map(|row|policy.belt_columns().iter().map(move|column|json!({"row":row,"column":column.column(),"kind":if row==1 && column.column()==1 {"empty"}else{column.kind()},"distance":0}))).collect();
    let base = json!({"kind":"recovery","frames":[{"mana":5,"closed_slots":columns},{},{"belt":"expanded","slots":all},{}]});
    let mut cases = vec![base.clone()];
    for (index, key, value) in [
        (0, "life", json!(1)),
        (0, "life", Value::Null),
        (0, "mana", Value::Null),
        (0, "mana", json!(10)),
        (0, "belt", json!("unknown")),
        (0, "capture_delay", json!(0.25)),
        (0, "capture_delay", json!(-0.01)),
        (0, "closed_delay", json!(0.25)),
        (0, "closed_slots", json!([])),
        (1, "mana", json!(5)),
        (1, "life", Value::Null),
        (1, "capture_delay", json!(0.25)),
        (1, "capture_delay", json!(1)),
        (1, "capture_delay", json!(3)),
        (2, "belt_status", json!("slot_unverified")),
        (2, "life", json!(1)),
        (2, "slots", json!([])),
        (2, "merc", json!("unverified")),
        (3, "belt", json!("expanded")),
        (3, "mana", json!(5)),
        (3, "merc", json!("unverified")),
        (2, "capture_delay", json!(0.75)),
    ] {
        let mut case = base.clone();
        case["frames"][index][key] = value;
        cases.push(case);
    }
    for extra in [
        json!({"stop_on_key":"1"}),
        json!({"focus_after_key":"1"}),
        json!({"stop_on_key":"`"}),
        json!({"focus_after_key":"`"}),
        json!({"fail_press_number":1}),
        json!({"fail_press_number":2}),
        json!({"fail_press_number":3}),
        json!({"press_delay":0.3}),
        json!({"focus_lost_at":10}),
        json!({"stop_at":10.1}),
        json!({"operations":[{"delay":0.25}]}),
        json!({"operations":[{"delay":-0.01}]}),
    ] {
        let mut case = base.clone();
        for (key, value) in extra.as_object().ok_or("object")? {
            case[key] = value.clone();
        }
        cases.push(case);
    }
    let mut empty = base.clone();
    for slot in empty["frames"][2]["slots"].as_array_mut().ok_or("slots")? {
        if slot["kind"] == policy.potion_kind() {
            slot["kind"] = json!("empty");
        }
    }
    cases.push(empty);
    let mut wrong = base.clone();
    for slot in wrong["frames"][0]["closed_slots"]
        .as_array_mut()
        .ok_or("slots")?
    {
        slot["kind"] = json!("town_portal_scroll");
    }
    cases.push(wrong);
    let mut partial = base.clone();
    partial["frames"][0]["life"] = json!(5);
    partial["frames"][0]["mana"] = Value::Null;
    cases.push(partial);
    let mut repeated = base.clone();
    repeated["frames"] = json!([{}]);
    repeated["operations"] = json!(vec![json!({}); 13]);
    cases.push(repeated);
    let mut boosted = base.clone();
    for sample in boosted["frames"].as_array_mut().ok_or("frames")? {
        sample["life_maximum"] = json!(5000);
        sample["mana_maximum"] = json!(5000);
        sample["life"] = json!(4500);
        sample["mana"] = json!(4500);
    }
    boosted["frames"][0]["mana"] = json!(250);
    cases.push(boosted);
    exercise(cases)
}

impl BuffSiteEnvironment for Env {
    fn prepare_ground(
        &mut self,
        _: &Arc<BgrFrame>,
        at: f64,
        _: &HudReading,
    ) -> Result<(), RunError> {
        if self.0.borrow().case["camera"] == true {
            let focused = self.focused()?;
            self.event(json!(["prepare_ground", self.index(), at, focused]));
            self.delay("prepare_ground_delay");
        }
        Ok(())
    }
    fn ground(&mut self, _: &Arc<BgrFrame>, at: f64) -> Result<Option<[i32; 2]>, RunError> {
        self.event(json!(["ground", self.index(), at]));
        self.delay("ground_delay");
        Ok(serde_json::from_value(
            self.sample()
                .get("ground")
                .cloned()
                .unwrap_or(json!([960, 580])),
        )
        .map_err(|error| ControlError::Runtime(error.to_string()))?)
    }
    fn hazard_grounds(
        &mut self,
        _: &Arc<BgrFrame>,
        player: [i32; 2],
        at: f64,
        processed: f64,
    ) -> Result<Vec<Option<[f64; 2]>>, RunError> {
        self.event(json!(["hazards", self.index(), player, at, processed]));
        self.delay("hazard_delay");
        Ok(
            serde_json::from_value(self.sample().get("hazards").cloned().unwrap_or(json!([])))
                .map_err(|error| ControlError::Runtime(error.to_string()))?,
        )
    }
}
fn site_case(case: &Value, policy: &CharacterSurvivalPolicy) -> Result<Value, Box<dyn Error>> {
    let mut env = Env::new(case);
    let mut guard = BuffSiteGuard::new(case.get("proof").cloned());
    let mut rows = vec![];
    for operation in case["operations"].as_array().ok_or("operations")? {
        {
            let mut state = env.0.borrow_mut();
            state.now = operation["now"].as_f64().unwrap_or(state.now);
            state.stopped = operation["stop"].as_bool().unwrap_or(state.stopped);
        }
        let outcome: Result<Value, RunError> = if operation["method"] == "prepare" {
            guard
                .prepare(&mut env, policy)
                .map_err(|error| match error {
                    GuardPreparationError::NearbyHostile => {
                        RunError::Runtime("buff_nearby_hostile".into())
                    }
                    GuardPreparationError::Other(error) => error,
                })
        } else {
            let supplied = if operation["supplied"] == true {
                let at = env.clock();
                let frame = env.capture()?;
                let reading = env.hud(&frame)?;
                env.0.borrow_mut().now += operation["delay"].as_f64().unwrap_or(0.0);
                Some((frame, at, reading))
            } else {
                None
            };
            let supplied = supplied
                .as_ref()
                .map(|(frame, at, reading)| BuffSafetyFrame {
                    frame,
                    acquired: if operation["missing_at"] == true {
                        None
                    } else {
                        Some(*at)
                    },
                    reading: if operation["missing_reading"] == true {
                        None
                    } else {
                        Some(reading)
                    },
                });
            if operation["method"] == "wait" {
                guard.wait_check(&mut env, policy, supplied)
            } else {
                guard.check(&mut env, policy, supplied)
            }
            .map(|value| json!(value))
        };
        let (result, error) = match outcome {
            Ok(value) => (value, Value::Null),
            Err(error) => (Value::Null, run_error(error)),
        };
        rows.push(json!({"result":result,"error":error,"baseline":guard.baseline(),"samples":guard.samples(),"observations":guard.observations(),"now":env.clock()}));
    }
    Ok(json!({"rows":rows,"events":env.0.borrow().events}))
}

#[test]
fn buff_site_keeps_current_frame_identity_interval_bounds_and_nearby_veto() -> TestResult {
    let mut cases = vec![];
    for sample in [
        json!({}),
        json!({"life":null}),
        json!({"mana":0,"life":null}),
        json!({"ground":null}),
        json!({"hazards":[null]}),
        json!({"hazards":[[1260,580]]}),
        json!({"hazards":[[1261,580]]}),
        json!({"hazards":[[960,760]]}),
        json!({"hazards":[[960,761]]}),
        json!({"hazard_delay":0.25}),
        json!({"ground_delay":0.25}),
        json!({"capture_delay":0.25}),
        json!({"capture_delay":-0.01}),
        json!({"belt":"expanded"}),
    ] {
        for supplied in [false, true] {
            cases.push(
                json!({"kind":"site","frames":[sample],"operations":[{"supplied":supplied}]}),
            );
        }
    }
    for operation in [
        json!({"supplied":true,"missing_at":true}),
        json!({"supplied":true,"missing_reading":true}),
        json!({"supplied":true,"delay":0.25}),
        json!({"supplied":true,"delay":-0.01}),
        json!({"stop":true}),
    ] {
        cases.push(json!({"kind":"site","operations":[operation]}));
    }
    cases.push(json!({"kind":"site","focus_lost_at":10,"operations":[{}]}));
    cases.push(json!({"kind":"site","camera":true,"operations":[{"supplied":true}]}));
    for (source, at, ground) in [
        ("supervised_visual", json!(1010), json!([960, 580])),
        ("supervised_visual", json!(995), json!([960, 580])),
        ("supervised_visual", json!(994.999), json!([960, 580])),
        ("runtime_screen", json!(1010), json!([960, 580])),
        ("supervised_visual", json!(true), json!([960, 580])),
        ("supervised_visual", json!(1010.1), json!([960, 580])),
        ("supervised_visual", json!(1010), json!([24, 55])),
        ("supervised_visual", json!(1010), json!([1900, 900])),
        ("supervised_visual", json!(1010), json!([960.0, 580.0])),
        ("supervised_visual", json!(1010), json!([true, 580])),
    ] {
        cases.push(json!({"kind":"site","frames":[{"ground":null}],"proof":{"source":source,"observed_at":at,"player_ground":ground},"operations":[{}]}));
    }
    for frames in [
        json!([{}]),
        json!([{"life":null},{}]),
        json!([{"ground":null},{}]),
        json!([{"hazards":[null]}]),
        json!([{"life":null}]),
    ] {
        cases.push(json!({"kind":"site","frames":frames,"operations":[{"method":"prepare"}]}));
        cases.push(json!({"kind":"site","frames":frames,"operations":[{"method":"wait"}]}));
    }
    let mut frames = vec![json!({}); 51];
    frames.extend([
        json!({"life":84}),
        json!({"life_lower_bound":0.9}),
        json!({"life":94,"life_maximum":105}),
    ]);
    cases.push(json!({"kind":"site","frames":frames,"operations":[{"method":"prepare"},{"supplied":true},{"now":11.5},{"now":12.5}]}));
    cases.push(json!({"kind":"site","operations":vec![json!({});40]}));
    cases.push(json!({"kind":"site","frames":[{"life_lower_bound":0.9}],"operations":[{"method":"prepare"},{}]}));
    exercise(cases)
}

struct AutoEnv {
    env: Env,
    operation: Value,
    attempts: usize,
    observations: usize,
    policy: CharacterSurvivalPolicy,
}
impl AutomaticBuffEnvironment for AutoEnv {
    type Error = RunError;
    type Guard = usize;
    fn clock(&self) -> f64 {
        self.env.clock()
    }
    fn stopped(&self) -> bool {
        self.env.stopped()
    }
    fn create_guard(&mut self) -> Result<usize, RunError> {
        let index = self.attempts;
        self.attempts += 1;
        self.env.event(json!(["create", index]));
        Ok(index)
    }
    fn prepare(&mut self, guard: &mut usize) -> Result<Value, GuardPreparationError<RunError>> {
        self.env.event(json!(["prepare", guard]));
        self.env.0.borrow_mut().now += self.operation["prepare_delay"].as_f64().unwrap_or(0.0);
        match self.operation["prepare_errors"]
            .get(*guard)
            .and_then(Value::as_str)
        {
            Some("buff_nearby_hostile") => Err(GuardPreparationError::NearbyHostile),
            Some(error) => Err(GuardPreparationError::Other(RunError::Runtime(
                error.into(),
            ))),
            None => Ok(json!({"guard":guard})),
        }
    }
    fn clear_hostiles(&mut self) -> Result<(), RunError> {
        self.env.event(json!(["clear", self.clock()]));
        self.env.0.borrow_mut().now += self.operation["clear_delay"].as_f64().unwrap_or(0.0);
        Ok(())
    }
    fn observe(&mut self) -> Result<Value, RunError> {
        let index = self.observations;
        self.observations += 1;
        self.env.event(json!(["observation", index, self.clock()]));
        self.env.0.borrow_mut().now += self.operation["observe_delay"].as_f64().unwrap_or(0.0);
        Ok(json!({"sample":index}))
    }
    fn dispatch(&mut self, guard: &mut usize) -> Result<BuffDispatchResult, RunError> {
        let at = self.clock();
        self.env.event(json!(["dispatch", guard, at]));
        if self.operation["dispatch_error"] == true {
            return Err(RunError::Runtime(
                "backend request outcome unverified".into(),
            ));
        }
        self.env.0.borrow_mut().now += self.operation["dispatch_delay"].as_f64().unwrap_or(0.0);
        let events = if let Some(events) = self.operation["dispatches"].as_array() {
            events
                .iter()
                .map(|event| BuffDispatch {
                    key: event["key"].as_str().unwrap_or("").into(),
                    cast_request_at: event["cast_request_at"].as_f64().unwrap_or(0.0),
                    cast_request_monotonic: match event["cast_request_monotonic"].as_str() {
                        Some("nan") => f64::NAN,
                        Some("inf") => f64::INFINITY,
                        _ => event["cast_request_monotonic"].as_f64().unwrap_or(f64::NAN),
                    },
                })
                .collect()
        } else {
            self.policy
                .buffs()
                .iter()
                .map(|rule| BuffDispatch {
                    key: rule.key().into(),
                    cast_request_at: 1000.0 + at,
                    cast_request_monotonic: at,
                })
                .collect()
        };
        Ok(BuffDispatchResult {
            status: self.operation["status"]
                .as_str()
                .unwrap_or("dispatched_unconfirmed")
                .into(),
            equipment_dispatch_balanced: self.operation["balanced"] != false,
            buff_dispatches: events,
            effect_verified: false,
            held_after: vec![],
            errors: vec![],
            trace_phase: "after_dispatch",
        })
    }
}
fn automatic_case(case: &Value, policy: &CharacterSurvivalPolicy) -> Result<Value, Box<dyn Error>> {
    let mut env = AutoEnv {
        env: Env::new(case),
        operation: Value::Null,
        attempts: 0,
        observations: 0,
        policy: policy.clone(),
    };
    let mut driver = AutomaticBuffDriver::default();
    let mut rows = vec![];
    let order: Vec<String> =
        serde_json::from_value(case.get("order").cloned().unwrap_or(json!(["w", "s", "w"])))?;
    for operation in case["operations"].as_array().ok_or("operations")? {
        env.operation = operation.clone();
        env.attempts = 0;
        let now = operation["now"].as_f64().unwrap_or(env.clock());
        {
            let mut state = env.env.0.borrow_mut();
            state.now = now;
            state.stopped = operation["stop"] == true;
        }
        let (result, error) = match driver.execute(&mut env, &order, policy) {
            Ok(value) => (value, Value::Null),
            Err(error) => (Value::Null, run_error(error)),
        };
        rows.push(json!({"result":result,"error":error,"deadlines":driver.deadlines(),"preparation_at":driver.preparation_at(),"due":driver.due(env.clock()),"now":env.clock()}));
    }
    Ok(json!({"rows":rows,"events":env.env.0.borrow().events}))
}

#[test]
fn partial_or_failed_weapon_dispatch_preserves_evidence_and_blocks_new_sequence() -> TestResult {
    let (_, policy) = config()?;
    let order = vec!["w".into(), "s".into(), "w".into()];
    for operation in [
        json!({"status":"buff_resources_unstable","balanced":false,"dispatches":[
            {"key":"w","cast_request_at":1009,"cast_request_monotonic":9},
            {"key":"a","cast_request_at":1009.1,"cast_request_monotonic":9.1},
            {"key":"a","cast_request_at":1009.2,"cast_request_monotonic":9.2},
            {"key":"s","cast_request_at":1009.3,"cast_request_monotonic":9.3}]}),
        json!({"dispatch_error":true}),
    ] {
        let mut env = AutoEnv {
            env: Env::new(&json!({})),
            operation: Value::Null,
            attempts: 0,
            observations: 0,
            policy: policy.clone(),
        };
        let mut driver = AutomaticBuffDriver::default();
        driver.execute(&mut env, &order, &policy)?;
        let deadlines = driver.deadlines().to_vec();
        env.operation = operation;
        assert!(driver.execute(&mut env, &order, &policy).is_err());
        assert!(driver.weapon_state_unverified());
        assert_eq!(driver.deadlines(), deadlines);
        if env.operation["dispatch_error"] == true {
            assert!(driver.failed_dispatch().is_none());
        } else {
            let failed = driver
                .failed_dispatch()
                .ok_or("partial dispatch evidence")?;
            assert_eq!(failed.status, "buff_resources_unstable");
            assert!(!failed.equipment_dispatch_balanced);
            assert_eq!(
                failed
                    .buff_dispatches
                    .iter()
                    .map(|r| r.key.as_str())
                    .collect::<Vec<_>>(),
                ["w", "a", "a", "s"]
            );
        }
        let before = env.env.0.borrow().events.clone();
        env.operation = json!({});
        let error = driver
            .execute(&mut env, &order, &policy)
            .err()
            .ok_or("re-entry blocked")?;
        assert_eq!(error.to_string(), "buff_weapon_state_unverified");
        assert_eq!(
            env.env.0.borrow().events,
            before,
            "no guard, observation, W or dispatch after unknown equipment"
        );
        assert_eq!(driver.deadlines(), deadlines);
    }
    // 무기 균형이 확인된 실패는 원래 안전 준비 재시도를 유지한다.
    let mut env = AutoEnv {
        env: Env::new(&json!({})),
        operation: json!({"status":"buff_resources_unstable"}),
        attempts: 0,
        observations: 0,
        policy: policy.clone(),
    };
    let mut driver = AutomaticBuffDriver::default();
    assert!(driver.execute(&mut env, &order, &policy).is_err());
    assert!(!driver.weapon_state_unverified());
    env.operation = json!({});
    driver.execute(&mut env, &order, &policy)?;
    assert!(driver.failed_dispatch().is_none());
    Ok(())
}

#[test]
fn automatic_buff_uses_last_actual_request_and_keeps_old_deadline_on_failure() -> TestResult {
    let (_, policy) = config()?;
    let mut cases = vec![];
    for op in [
        json!({}),
        json!({"prepare_errors":["buff_nearby_hostile"]}),
        json!({"prepare_errors":["buff_nearby_hostile","buff_nearby_hostile"]}),
        json!({"prepare_errors":["buff_nearby_hostile","buff_nearby_hostile","buff_nearby_hostile"]}),
        json!({"prepare_errors":["buff_resources_unstable"]}),
        json!({"status":"interrupted"}),
        json!({"balanced":false}),
        json!({"stop":true}),
        json!({"dispatches":[]}),
        json!({"dispatch_delay":200}),
        json!({"prepare_delay":1,"observe_delay":0.2,"dispatch_delay":2}),
    ] {
        cases.push(json!({"kind":"automatic","operations":[{},op]}));
    }
    for at in [
        json!(-1),
        json!(100),
        json!(true),
        json!("nan"),
        json!("inf"),
    ] {
        let events: Vec<_> = policy
            .buffs()
            .iter()
            .map(
                |rule| json!({"key":rule.key(),"cast_request_at":1010,"cast_request_monotonic":at}),
            )
            .collect();
        cases.push(json!({"kind":"automatic","operations":[{"dispatches":events}]}));
    }
    let mut events: Vec<Value> = policy
        .buffs()
        .iter()
        .map(|rule| json!({"key":rule.key(),"cast_request_at":1001,"cast_request_monotonic":1}))
        .collect();
    events.extend(
        policy.buffs().iter().map(
            |rule| json!({"key":rule.key(),"cast_request_at":1009,"cast_request_monotonic":9}),
        ),
    );
    cases.push(json!({"kind":"automatic","operations":[{"dispatches":events},{"now":148,"prepare_delay":1,"dispatch_delay":1},{"status":"equipment_dispatch_unbalanced"}]}));
    for order in [json!([]), json!(["w"]), json!(["W", "s", "w"])] {
        cases.push(json!({"kind":"automatic","order":order,"operations":[{}]}));
    }
    exercise(cases)
}

#[test]
fn recovery_evidence_can_be_drained_without_retaining_source_pixels() -> TestResult {
    use d2bot::common::emergency_recovery::EmergencyResourceRecovery;
    let (_, policy) = config()?;
    let all:Vec<_>=(1..=4).flat_map(|row|policy.belt_columns().iter().map(move|column|json!({"row":row,"column":column.column(),"kind":column.kind(),"distance":0}))).collect();
    let case = json!({"frames":[{"mana":5,"closed_slots":[{"row":4,"column":1,"kind":"purple_potion","distance":0}]},{},{"belt":"expanded","slots":all},{}]});
    let mut env = Env::new(&case);
    let acquired = env.clock();
    let frame = env.capture()?;
    let reading = env.hud(&frame)?;
    let weak = Arc::downgrade(&frame);
    let mut recovery = EmergencyResourceRecovery::default();
    assert!(recovery.recover(&mut env, &policy, &frame, &reading, acquired)?);
    drop(frame);
    assert!(weak.upgrade().is_some());
    let evidence = recovery.take_evidence();
    assert_eq!(evidence.len(), 3);
    assert!(recovery.evidence().is_empty());
    drop(evidence);
    assert!(weak.upgrade().is_none());
    assert_eq!(recovery.records().len(), 1);
    Ok(())
}

#[test]
fn cooperative_room_cleanup_does_not_cancel_parent_session() -> TestResult {
    use d2bot::common::workers::{Cancellation, CooperativeStop};
    use std::time::Duration;
    let parent = Cancellation::default();
    let room = CooperativeStop::new(Some(parent.clone()));
    assert!(!room.wait(Duration::ZERO)?);
    room.set();
    assert!(room.is_set());
    assert!(!parent.is_cancelled());
    assert!(room.wait(Duration::from_secs(1))?);
    let room = CooperativeStop::new(Some(parent.clone()));
    parent.cancel();
    assert!(room.is_set());
    assert!(room.wait(Duration::from_secs(1))?);
    let parent = Cancellation::default();
    let room = CooperativeStop::new(Some(parent.clone()));
    room.request_stop();
    assert!(parent.is_cancelled());
    Ok(())
}
fn exercise(cases: Vec<Value>) -> TestResult {
    let (raw, policy) = config()?;
    let expected = support::legacy_expectation(
        "field-preparation-oracle",
        &json!({"policy":raw,"cases":cases}),
    )?;
    for (index, case) in cases.iter().enumerate() {
        let selected = case
            .get("policy")
            .map(CharacterSurvivalPolicy::from_value)
            .transpose()?;
        let actual = execute(case, selected.as_ref().unwrap_or(&policy))?;
        compare(&actual, &expected[index], &format!("case{index} {case}"));
    }
    Ok(())
}

#[test]
fn multiple_timed_rules_share_keys_without_merging_their_deadlines() -> TestResult {
    let (mut raw, _) = config()?;
    raw["buffs"] = json!([
        {"name":"battle_orders","key":"s","duration_seconds":150,"renew_before_seconds":10},
        {"name":"short_shield","key":"f1","duration_seconds":50,"renew_before_seconds":5},
        {"name":"same_key","key":"s","duration_seconds":80,"renew_before_seconds":3}
    ]);
    let events = json!([
        {"key":"s","cast_request_at":1001,"cast_request_monotonic":1},
        {"key":"f1","cast_request_at":1004,"cast_request_monotonic":4},
        {"key":"s","cast_request_at":1009,"cast_request_monotonic":9}
    ]);
    let mut cases = vec![
        json!({"kind":"automatic","policy":raw,"operations":[{"dispatches":events},{"now":44,"prepare_delay":1}]}),
        json!({"kind":"automatic","policy":raw,"operations":[{"dispatches":[{"key":"s","cast_request_at":1009,"cast_request_monotonic":9}]}]}),
    ];
    raw["buffs"] = json!([]);
    cases.push(json!({"kind":"automatic","policy":raw,"operations":[{}]}));
    exercise(cases)
}
#[test]
fn permits_preserve_types_expiry_priority_and_scope() -> TestResult {
    let (_, policy) = config()?;
    let casts: serde_json::Map<String, Value> = policy
        .buffs()
        .iter()
        .map(|rule| (rule.name().to_owned(), json!(99)))
        .collect();
    let runtime = json!({"source":"runtime_screen","area":"arcane_sanctuary","panels_closed":true,"belt_closed":true,"nearby_candidates":0,"safety_scope":"current_nearby_candidates_and_stable_ratios","observed_at":99.9});
    let supervised = json!({"source":"supervised_visual","character_id":"flash","field":true,"monsters_clear":true,"belt_closed":true,"buff_effect_verified":true,"equipment_restored":true,"observed_at":99,"potions_remaining":4,"buff_casts":casts});
    let mut cases = vec![];
    for (kind, proof) in [
        ("runtime", runtime.clone()),
        ("permit", supervised.clone()),
        ("travel", supervised.clone()),
    ] {
        cases.push(json!({"kind":kind,"proof":proof,"now":100}));
        for at in [
            json!(100.1),
            json!(100),
            json!(99.5),
            json!(85),
            json!(84.999),
            json!(-1),
            json!(true),
            json!("99"),
            Value::Null,
        ] {
            let mut proof = proof.clone();
            proof["observed_at"] = at;
            cases.push(json!({"kind":kind,"proof":proof,"now":100}));
        }
    }
    for field in [
        "source",
        "area",
        "panels_closed",
        "belt_closed",
        "nearby_candidates",
        "safety_scope",
    ] {
        for value in [json!(false), json!(0.0), json!(1), Value::Null] {
            let mut proof = runtime.clone();
            proof[field] = value;
            cases.push(json!({"kind":"runtime","proof":proof,"now":100}));
        }
    }
    for field in [
        "character_id",
        "field",
        "monsters_clear",
        "belt_closed",
        "buff_effect_verified",
        "equipment_restored",
        "potions_remaining",
        "buff_casts",
    ] {
        for value in [
            json!(false),
            json!(0),
            json!(4.0),
            json!(100),
            json!({}),
            Value::Null,
        ] {
            let mut proof = supervised.clone();
            proof[field] = value;
            for buff in [false, true] {
                cases.push(json!({"kind":"permit","proof":proof,"now":100,"buff":buff}));
            }
        }
    }
    for value in [
        json!(false),
        json!(true),
        json!(0),
        json!("false"),
        Value::Null,
    ] {
        let mut proof = supervised.clone();
        proof["monsters_clear"] = value;
        cases.push(json!({"kind":"travel","proof":proof,"now":100}));
    }
    cases.push(json!({"kind":"travel","proof":runtime,"now":100}));
    for at in [
        json!(100.1),
        json!(100),
        json!(99),
        json!(85),
        json!(84.999),
        json!(true),
        json!("99"),
        Value::Null,
    ] {
        for field in [
            "effect_verified",
            "equipment_restored",
            "sequence_effects_verified",
        ] {
            for value in [true, false] {
                let mut proof = json!({"source":"supervised_visual","character_id":"flash","effect_verified":true,"equipment_restored":true,"sequence_effects_verified":true,"observed_at":at});
                proof[field] = json!(value);
                cases.push(json!({"kind":"confirm","proof":proof,"now":100,"last_cast":80}));
            }
        }
    }
    exercise(cases)
}
#[test]
fn resource_and_dispatch_compare_actual_event_order_and_traces() -> TestResult {
    let mut cases = vec![
        json!({"kind":"wait"}),
        json!({"kind":"wait","frames":[{"life":null},{"mana":null},{}]}),
        json!({"kind":"wait","frames":[{"life":null,"mana":0}]}),
        json!({"kind":"wait","frames":[{"capture_delay":0.25}]}),
        json!({"kind":"wait","frames":[{"capture_delay":0.25},{}],"retry":["screen_stale"]}),
        json!({"kind":"wait","frames":[{"blocked":"buff_ground_unverified"},{}],"retry":["buff_ground_unverified"]}),
        json!({"kind":"wait","frames":[{"life":null}],"stop_at":10.1}),
        json!({"kind":"wait","focus_lost_at":10}),
        json!({"kind":"wait","frames":[{"observe_delay":0.25}]}),
        json!({"kind":"wait","deadline":10}),
    ];
    let base = json!({"kind":"dispatch","order":["w","a","f1","w"],"pauses":{"a":0.1}});
    for trace in [false, true] {
        for before in [false, true] {
            let mut case = base.clone();
            case["trace"] = json!(trace);
            case["trace_before"] = json!(before);
            cases.push(case.clone());
            for (key, value) in [
                ("stop_at", json!(10.1)),
                ("focus_lost_at", json!(10.4)),
                ("before_delay", json!(8)),
                ("before_blocked", json!("buff_nearby_hostile")),
                ("fail_key", json!("a")),
                ("sequence_timeout", json!(0.4)),
            ] {
                let mut row = case.clone();
                row[key] = value;
                cases.push(row);
            }
            for frames in [
                json!([{"life":null},{}]),
                json!([{"blocked":"buff_ground_unverified"},{}]),
                json!([{}, {"capture_delay":0.25},{}]),
                json!([{"merc":"unverified"}]),
                json!([{"life":1,"mana":null}]),
                json!([{"belt":"expanded"}]),
                json!([{"check_delay":0.25}]),
            ] {
                let mut row = case.clone();
                row["frames"] = frames;
                cases.push(row);
            }
        }
    }
    for order in [
        json!([]),
        json!(["w"]),
        json!(["a"]),
        json!(["left-click"]),
        json!(["w", "right-click"]),
        json!(vec!["a"; 17]),
    ] {
        let mut case = base.clone();
        case["order"] = order;
        case["trace"] = json!(true);
        cases.push(case);
    }
    exercise(cases)
}
#[test]
fn screen_preparation_requires_positive_current_evidence_and_single_toggles() -> TestResult {
    let mut cases = vec![];
    for frames in [
        json!([{"visible":true}]),
        json!([{}, {}, {}, {}, {}, {}, {}, {}, {}, {"visible":true}]),
        json!([{}]),
        json!([{"obscured":true}]),
        json!([{"capture_delay":0.25},{"visible":true}]),
        json!([{"life":null}]),
        json!([{"panels":true}]),
        json!([{"life":0,"panels":true}]),
        json!([{"map_delay":0.25}, {"visible":true}]),
    ] {
        for allow in [false, true] {
            cases.push(json!({"kind":"map","frames":frames,"allow_toggle":allow}));
        }
    }
    for at in [
        json!(1010),
        json!(995),
        json!(994.999),
        json!(1010.1),
        json!(true),
        Value::Null,
    ] {
        cases.push(json!({"kind":"map","proof":{"source":"supervised_visual","area":"arcane_sanctuary","automap_on":true,"observed_at":at}}));
    }
    for timeout in [0.099, 0.1, 3.001] {
        cases.push(json!({"kind":"map","timeout":timeout,"frames":[{"visible":true}]}));
    }
    for kind in ["map", "labels"] {
        for extra in [
            json!({"focus_lost_at":10}),
            json!({"stop_at":10.01}),
            json!({"frames":[{"panels":true}]}),
            json!({"frames":[{"life":null}]}),
            json!({"frames":[{"panels_delay":0.25}]}),
            json!({"fail_key":"alt"}),
            json!({}),
        ] {
            let mut row = extra;
            row["kind"] = json!(kind);
            cases.push(row);
        }
    }
    for frames in [
        json!([{"weapon":1}]),
        json!([{"weapon":2},{"weapon":1}]),
        json!([{}, {"weapon":1}]),
        json!([{"weapon":2},{}]),
        json!([{"weapon":1,"weapon_delay":0.25}, {"weapon":1}]),
        json!([{}]),
    ] {
        cases.push(json!({"kind":"weapon","frames":frames}));
    }
    for extra in [
        json!({"expected":3}),
        json!({"focus_lost_at":10}),
        json!({"focus_lost_at":10.01,"frames":[{}, {"weapon":1}]}),
        json!({"stop_at":10.01}),
        json!({"fail_key":"w","frames":[{"weapon":2}]}),
    ] {
        let mut row = extra;
        row["kind"] = json!("weapon");
        cases.push(row);
    }
    exercise(cases)
}

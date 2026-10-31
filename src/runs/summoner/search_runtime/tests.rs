use super::*;
use crate::{
    common::{
        buffing::BeltVisibility,
        config::BotConfig,
        field_control::{Enemy, FieldLimits, NavigationEvidence},
        field_runtime::{FieldRuntimeOptions, create_field_control_with_backend},
        frame::Size,
        scene_motion::MotionReason,
        survival::{BeltContents, BeltReading, Location, SafetyObservation},
        survival_vision::ResourceReading,
        time::MonotonicClock,
    },
    runs::summoner::{
        navigation::{ArcanePathObserver, PathSettings},
        search::SearchLimits,
    },
    test_field_world::compare,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    error::Error,
    path::Path,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
type TestResult = Result<(), Box<dyn Error>>;
struct Clock(AtomicU64);
impl Clock {
    fn set(&self, now: f64) {
        self.0.store(now.to_bits(), Ordering::Release);
    }
    fn add(&self, seconds: f64) {
        self.set(self.now() + seconds);
    }
}
impl MonotonicClock for Clock {
    fn now(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Acquire))
    }
}
struct Backend {
    native: bool,
    events: Mutex<Vec<Value>>,
}
impl Backend {
    fn record(&self, event: Value) -> Result<(), ControlError> {
        self.events
            .lock()
            .map_err(|_| ControlError::Runtime("test events poisoned".into()))?
            .push(event);
        Ok(())
    }
}
impl FieldInputBackend for Backend {
    fn is_dry_run(&self) -> bool {
        !self.native
    }
    fn key_down(&self, key: &str) -> Result<(), ControlError> {
        self.record(json!(["down", key]))
    }
    fn key_up(&self, key: &str) -> Result<(), ControlError> {
        self.record(json!(["up", key]))
    }
    fn press(&self, key: &str) -> Result<(), ControlError> {
        self.record(json!(["press", key]))
    }
    fn move_pointer(&self, p: [i64; 2]) -> Result<(), ControlError> {
        self.record(json!(["move", p[0], p[1]]))
    }
}
#[derive(Clone)]
struct Model {
    op: Value,
    current: PhysicalTarget,
    serial: u64,
    enemy: Enemy,
    terminal: Option<Terminal>,
    belt: BeltVisibility,
    life: u64,
    mana: u64,
    motion: Option<[f64; 2]>,
}
struct Capture {
    model: Arc<Mutex<Model>>,
    frame: Arc<BgrFrame>,
    target: PhysicalTarget,
    closed: Arc<AtomicBool>,
}
impl SearchCapture for Capture {
    fn current_target(&self) -> Result<PhysicalTarget, RunError> {
        Ok(self
            .model
            .lock()
            .map_err(|_| RunError::Runtime("test model poisoned".into()))?
            .current)
    }
    fn target(&self) -> PhysicalTarget {
        self.target
    }
    fn grab(&mut self) -> Result<CapturedFrame, RunError> {
        let mut model = self
            .model
            .lock()
            .map_err(|_| RunError::Runtime("test model poisoned".into()))?;
        if model.op["move_grab"] == true {
            model.current.left = 301;
        }
        drop(model);
        Ok(CapturedFrame {
            frame: self.frame.clone(),
            target: self.target,
            wall_captured_at: Some(999999.0),
            geometry: None,
        })
    }
    fn client_box(&self) -> Result<Option<PixelRect>, RunError> {
        Ok(None)
    }
    fn close(&mut self) -> Result<(), RunError> {
        self.closed.store(true, Ordering::Release);
        Ok(())
    }
}
struct Perception {
    model: Arc<Mutex<Model>>,
    clock: Arc<Clock>,
    focused: Arc<AtomicBool>,
}
impl SearchPerception for Perception {
    fn next_stamp(&mut self) -> Result<ObservationStamp, RunError> {
        let mut model = self
            .model
            .lock()
            .map_err(|_| RunError::Runtime("test model poisoned".into()))?;
        model.serial += 1;
        Ok(ObservationStamp::new(
            "flash",
            "room",
            model.serial,
            self.clock.now(),
        )?)
    }
    fn scene(
        &mut self,
        _frame: &Arc<BgrFrame>,
        stamp: &ObservationStamp,
    ) -> Result<ArcaneScene, RunError> {
        let model = self
            .model
            .lock()
            .map_err(|_| RunError::Runtime("test model poisoned".into()))?
            .clone();
        let stamp = if model.op["wrong_stamp"] == true {
            ObservationStamp::new(
                stamp.character_id(),
                "other",
                stamp.sequence(),
                stamp.observed_at(),
            )?
        } else {
            stamp.clone()
        };
        let field = FieldObservation::new(
            SafetyObservation::new(
                stamp,
                None,
                None,
                Location::Field,
                Some(model.enemy == Enemy::Clear),
            )?,
            Some(true),
            model.enemy,
            NavigationEvidence {
                travel_aim: (model.op["held_aim"] == true).then_some([500, 300]),
                ..Default::default()
            },
        )?;
        let hover = if model.op["hover"].is_null() {
            None
        } else {
            let [l, t, r, b]: [usize; 4] =
                serde_json::from_value(model.op["hover"]["bounds"].clone())?;
            Some(SceneHover {
                kind: serde_json::from_value(model.op["hover"]["kind"].clone())?,
                bounds: PixelRect::new(l, t, r, b)
                    .map_err(crate::common::error::ObservationError::from)?,
            })
        };
        Ok(ArcaneScene::new(
            field,
            model.terminal,
            false,
            hover,
            None,
            serde_json::from_value(model.op["ground"].clone())?,
        )?)
    }
    fn hud(&mut self, _frame: &Arc<BgrFrame>) -> Result<HudReading, RunError> {
        let mut model = self
            .model
            .lock()
            .map_err(|_| RunError::Runtime("test model poisoned".into()))?;
        if model.op["move_hud"] == true {
            model.current.top += 1;
        }
        let model = model.clone();
        self.clock.add(model.op["age"].as_f64().unwrap_or(0.0));
        if let Some(focused) = model.op["hud_focus"].as_bool() {
            self.focused.store(focused, Ordering::Release);
        }
        Ok(HudReading {
            layout_id: "test".into(),
            revision: "1".into(),
            life: ResourceReading::from_counts(model.life, 100, "test", None),
            mana: ResourceReading::from_counts(model.mana, 100, "test", None),
            belt_status: "unknown".into(),
            belt_visibility: model.belt,
            slots: vec![],
            mercenary_status: None,
        })
    }
    fn motion(
        &mut self,
        _frame: &Arc<BgrFrame>,
        _view: &PathView,
    ) -> Result<CameraMotion, RunError> {
        let displacement = self
            .model
            .lock()
            .map_err(|_| RunError::Runtime("test model poisoned".into()))?
            .motion;
        Ok(CameraMotion {
            displacement,
            inliers: 20,
            reason: MotionReason::BackgroundTranslation,
            matches: 20,
            inlier_ratio: None,
            linear_error: None,
        })
    }
}
fn error_message(error: &RunError) -> String {
    match error {
        RunError::Control(ControlError::Configuration(e)) => e.message().into(),
        _ => error.to_string(),
    }
}
fn execute(case: &Value) -> Result<Value, Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config = BotConfig::load(root.join("config"))?;
    let clock = Arc::new(Clock(AtomicU64::new(1.0f64.to_bits())));
    let focused = Arc::new(AtomicBool::new(true));
    let backend = Arc::new(Backend {
        native: case["native"] == true,
        events: Mutex::new(vec![]),
    });
    let mut control = create_field_control_with_backend(
        &config,
        "flash",
        "room",
        FieldLimits::new(0.5, 1000.0, 3.0, 2.0, 1.0)?,
        clock.clone(),
        backend.clone(),
    )?;
    let mut contents: BeltContents = BTreeMap::new();
    for column in 1..=4 {
        contents.insert(
            column,
            BeltReading {
                kind: Some(
                    if column == 4 {
                        "town_portal_scroll"
                    } else {
                        "purple_potion"
                    }
                    .into(),
                ),
                count: 4,
            },
        );
    }
    control.state_mut().observe_belt(
        &ObservationStamp::new("flash", "room", 2, 1.0)?,
        Some(&contents),
    );
    control.state_mut().confirm_buff(
        &ObservationStamp::new("flash", "room", 3, 1.0)?,
        "battle_orders",
        Location::Field,
        Some(true),
    );
    let next_clock = clock.clone();
    let next_sequence = Arc::new(AtomicU64::new(100));
    let next_focus = focused.clone();
    let field = FieldControlRuntime::new(
        control,
        None,
        FieldRuntimeOptions {
            clock: clock.clone(),
            focused: Arc::new(move || Ok(next_focus.load(Ordering::Acquire))),
            interval: Duration::from_millis(2),
            next_stamp: Box::new(move || {
                Ok(ObservationStamp::new(
                    "flash",
                    "room",
                    next_sequence.fetch_add(1, Ordering::AcqRel),
                    next_clock.now(),
                )?)
            }),
        },
    )?;
    let observer = ArcanePathObserver::new(
        PathSettings::new(4, 0.5, 20.0, 32.0, 40.0, 8.0, 8)?,
        None,
        None,
        None,
    )?;
    let limits = SearchLimits {
        screen_age: 0.5,
        landing_seconds: 2.0,
        wing_seconds: 20.0,
        return_seconds: 20.0,
        total_seconds: 100.0,
        minimum_movement: 4.0,
        breadcrumb_radius: 8.0,
        maximum_casts: 100,
        maximum_hover_attempts: 2,
        settled_frames: 2,
        settled_motion_pixels: 2.0,
    };
    let search = ArcaneSearch::new(observer, limits, 0.32, 1.0)?;
    let target = PhysicalTarget::new(300, 200, 200, 200)?;
    let closed = Arc::new(AtomicBool::new(false));
    let model = Arc::new(Mutex::new(Model {
        op: Value::Null,
        current: target,
        serial: 3,
        enemy: Enemy::Clear,
        terminal: None,
        belt: BeltVisibility::Closed,
        life: 100,
        mana: 100,
        motion: Some([0.0; 2]),
    }));
    let size = Size::new(200, 200)?;
    let frame = Arc::new(BgrFrame::from_vec(size, [111, 109, 109].repeat(40000))?);
    let capture = Capture {
        model: model.clone(),
        frame,
        target,
        closed: closed.clone(),
    };
    let perception = Perception {
        model: model.clone(),
        clock: clock.clone(),
        focused: focused.clone(),
    };
    let mut runtime = ArcaneSearchRuntime::new(
        search,
        field,
        capture,
        perception,
        SearchRuntimeOptions {
            viewport: PixelRect::full(size),
            anchor: [100.0, 100.0],
        },
    )?;
    if case["running"] == true {
        runtime.start()?;
    }
    let handle = runtime.field.handle();
    let mut rows = vec![];
    for op in case["ops"].as_array().ok_or("ops")? {
        clock.set(
            op["now"]
                .as_f64()
                .unwrap_or(clock.now() + op["dt"].as_f64().unwrap_or(0.0)),
        );
        {
            let mut model = model.lock().map_err(|_| "test model poisoned")?;
            model.op = op.clone();
            if let Some(enemy) = op["enemy"].as_str() {
                model.enemy = match enemy {
                    "clear" => Enemy::Clear,
                    "near" => Enemy::Near,
                    "far" => Enemy::Far,
                    _ => Enemy::Unknown,
                };
            }
            if op.get("terminal").is_some() {
                model.terminal = serde_json::from_value(op["terminal"].clone())?;
            }
            if let Some(serial) = op["serial"].as_u64() {
                model.serial = serial;
            }
            if op.get("motion").is_some() {
                model.motion = serde_json::from_value(op["motion"].clone())?;
            }
            if let Some(belt) = op["belt"].as_str() {
                model.belt = match belt {
                    "closed" => BeltVisibility::Closed,
                    "expanded" => BeltVisibility::Expanded,
                    _ => BeltVisibility::Unknown,
                };
            }
            if let Some(life) = op["life"].as_u64() {
                model.life = life;
            }
            if let Some(mana) = op["mana"].as_u64() {
                model.mana = mana;
            }
        }
        if let Some(focus) = op["focus"].as_bool() {
            focused.store(focus, Ordering::Release);
        }
        if let Some(pause) = op["pause"].as_bool() {
            handle.pause(pause)?;
        }
        let result = if op["op"] == "close" {
            runtime.close().map(|_| Value::Null)
        } else {
            runtime
                .step()
                .and_then(|action| Ok(serde_json::to_value(action)?))
        };
        let mut row = match result {
            Ok(value) => json!({"result":value}),
            Err(error) => json!({"error":error_message(&error)}),
        };
        let status = handle.status()?;
        row["search"] = json!({"phase":runtime.search.phase(),"wing":runtime.search.wing(),"generation":runtime.search.expected_generation(),"view":runtime.search.last_view().map(|v|json!({"sequence":v.context.sequence,"generation":v.context.generation,"captured_at":v.context.captured_at,"anchor":v.context.anchor}))});
        row["field"] = json!({"action":status.last.action,"reason":status.last.reason,"stopped":status.stopped,"paused":status.paused,"held":status.inputs.held});
        row["closed"] = json!(closed.load(Ordering::Acquire));
        row["events"] = json!(*backend.events.lock().map_err(|_| "test events poisoned")?);
        rows.push(row);
    }
    runtime.close()?;
    Ok(json!(rows))
}
#[test]
fn retained_search_runtime_matches_source_same_frame_shared_control_and_capture_guards()
-> TestResult {
    let cases = vec![
        json!({"ops":[{},{"dt":0.05},{"dt":0.1},{"op":"close"},{}]}),
        json!({"ops":[{"enemy":"unknown"},{"dt":0.1,"enemy":"clear","belt":"unknown"}]}),
        json!({"ops":[{"enemy":"near"},{"dt":0.1,"terminal":"summoner_present"}]}),
        json!({"ops":[{"life":1}]}),
        json!({"ops":[{"mana":1}]}),
        json!({"ops":[{"age":0.6}]}),
        json!({"ops":[{"move_grab":true}]}),
        json!({"ops":[{"move_hud":true}]}),
        json!({"ops":[{"wrong_stamp":true}]}),
        json!({"ops":[{"held_aim":true}]}),
        json!({"ops":[{"ground":[-1,0]}]}),
        json!({"ops":[{"hover":{"kind":"chest","bounds":[120,60,160,100]}},{"dt":0.05}]}),
        json!({"ops":[{"ground":[120,100]}]}),
        json!({"ops":[{},{"dt":0.1,"serial":2}]}),
        json!({"ops":[{"pause":true},{"dt":0.1,"pause":false}]}),
        json!({"ops":[{"focus":false}]}),
        json!({"ops":[{"hud_focus":false}]}),
        json!({"native":true,"ops":[{}]}),
        json!({"native":true,"running":true,"ops":[{}]}),
        json!({"native":true,"running":true,"ops":[{"ground":[120,100]}]}),
    ];
    let expected = crate::test_support::legacy_expectation("search-runtime-oracle", &json!(cases))?;
    for (index, case) in cases.iter().enumerate() {
        compare(
            &execute(case)?,
            &expected[index],
            &format!("search runtime {index}"),
        );
    }
    Ok(())
}

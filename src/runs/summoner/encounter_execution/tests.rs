use super::*;
use crate::{
    common::{field_control::InputSessionOptions, frame::Size},
    test_field_world::{World, compare, config, frame_index, point},
};
use std::{error::Error, path::Path};
type TestResult = Result<(), Box<dyn Error>>;
#[test]
fn native_approach_uses_current_goal_within_reviewed_wide_floor() -> TestResult {
    use crate::{
        common::vision_ops::codec,
        runs::summoner::{
            camera_ground::{CalibratedCameraGroundObserver, CameraLimits},
            navigation_execution::load_arcane_step_planner,
        },
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let owner = root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
    let (ground, path) = load_arcane_step_planner(root, true, "other")?;
    let shared = Arc::new(path);
    let camera = CameraGroundHandle::new(CalibratedCameraGroundObserver::new(
        ground,
        |_| Ok(false),
        CameraLimits::default(),
    )?);
    let mut vision = NativeEncounterVision::new(
        camera,
        shared.clone(),
        Arc::new(ArcaneObjectiveObserver::load(&owner)?),
        Arc::new(SummonerAppearanceObserver::load(
            owner.join("summoner-encounter.json"),
        )?),
        CooperativeStop::default(),
    )?;
    let frame = codec::load_color(owner.join("references/summoner/platform-alive-front-lit.png"))?;
    let player = [962.0, 580.0];
    let goal = [1449.5, 570.0];
    let view = vision.path.observe(
        frame.view(),
        PathContext {
            sequence: 0,
            generation: 0,
            captured_at: 0.0,
            frame_size: frame.size(),
            viewport: PixelRect::new(120, 180, 1720, 890)?,
            anchor: player,
        },
    )?;
    let choose = ChooseOptions {
        destination: Some([goal[0] - player[0], goal[1] - player[1]]),
        ..Default::default()
    };
    let distance = |choice: PathChoice| -> Result<f64, Box<dyn Error>> {
        let aim = choice.aim.ok_or("current floor")?;
        Ok((f64::from(aim[0]) - goal[0]).hypot(f64::from(aim[1]) - goal[1]))
    };
    assert!(distance(shared.choose(&view, Wing::North, choose)?)? > 100.0);
    assert!(distance(vision.path.choose(&view, Wing::North, choose)?)? <= 100.0);
    assert_eq!(
        shared.teleport().and_then(|g| g.preferred_distance),
        Some(240.0)
    );
    vision.finish(Duration::from_secs(1))?;
    Ok(())
}
struct Vision(World);
impl EncounterVision for Vision {
    type Path = Result<i64, RunError>;
    fn prepares_ground(&self) -> bool {
        self.0.state().case["camera"] == true
    }
    fn prepare(
        &mut self,
        _: &Arc<BgrFrame>,
        acquired: f64,
        _: &HudReading,
        focus: bool,
    ) -> Result<(), RunError> {
        self.0
            .event(json!(["prepare", self.0.index(), acquired, focus]));
        Ok(())
    }
    fn observe(
        &mut self,
        _: &Arc<BgrFrame>,
        sequence: u64,
        acquired: f64,
        may_approach: bool,
        corpse: bool,
    ) -> Result<EncounterFrame<Self::Path>, RunError> {
        let w = &self.0;
        w.event(json!(["ground", w.index(), acquired]));
        w.delay("ground_delay");
        let sample = w.sample();
        let player = if sample.get("player") == Some(&Value::Null) {
            None
        } else {
            Some(point(&sample["player"], [750.0, 580.0]).map(|x| x as i32))
        };
        w.event(json!(["platform", w.index()]));
        w.delay("platform_delay");
        let platform = (sample.get("platform") != Some(&Value::Null)).then(|| ObjectiveReading {
            kind: "summoner_platform".into(),
            journal_box: None,
            journal_score: None,
            rune_boxes: vec![[650, 400, 850, 500]],
            evidence_kind: "runes".into(),
            glyph_scores: vec![],
            ground_point: Some(point(&sample["platform_ground"], [800.0, 580.0])),
            slot_indices: vec![],
            layout_residual: None,
        });
        let target = w.appearance("alive");
        let corpse = if corpse { w.appearance("corpse") } else { None };
        let path = player
            .filter(|_| platform.is_some() && may_approach)
            .map(|player| {
                w.event(json!(["path", w.index(), sequence, acquired, player]));
                w.delay("path_delay");
                if sample["path_error"] == true {
                    Err(RunError::Runtime("path observation failed".into()))
                } else {
                    Ok(w.index())
                }
            });
        Ok(EncounterFrame {
            player: GroundReading {
                point: player,
                reason: "test".into(),
                score: None,
                calibration: None,
            },
            platform,
            target,
            corpse,
            path,
        })
    }
    fn choose(&mut self, path: Self::Path, destination: [f64; 2]) -> Result<PathChoice, RunError> {
        let index = path?;
        self.0.event(json!(["choose", index, "north", destination]));
        let sample = self.0.sample();
        Ok(PathChoice {
            aim: if sample.get("aim") == Some(&Value::Null) {
                None
            } else {
                Some(point(&sample["aim"], [1300.0, 580.0]).map(|x| x as i32))
            },
            reason: "test".into(),
            path: vec![],
        })
    }
    fn finish(&mut self, _: Duration) -> Result<(), RunError> {
        if self.0.state().case["cleanup_error"] == true {
            Err(RunError::Runtime("vision shutdown timed out".into()))
        } else {
            Ok(())
        }
    }
}
fn exercise(case: &Value, policy: &CharacterSurvivalPolicy) -> Result<Value, Box<dyn Error>> {
    let mut w = World::new(case);
    if case["stop_at"].as_f64().is_some_and(|x| x <= 10.0) {
        w.1.set();
    }
    let focused = w.clone();
    let mut inputs = FieldInputSession::new(
        Arc::new(w.clone()),
        "f2",
        "f4",
        InputSessionOptions {
            clock: Arc::new(w.clone()),
            stop: w.1.clone(),
            focused: Arc::new(move || focused.focused()),
            repeat_seconds: Some(0.32),
            aim_settle_seconds: 0.0,
        },
    )?;
    let mut vision = Vision(w.clone());
    let mut life = (case["life"] == true)
        .then(|| SummonerLifeTracker::new("flash", "room"))
        .transpose()?;
    let mut evidence = vec![];
    let result = execute(
        &mut inputs,
        &mut w,
        &mut vision,
        EncounterRequest {
            policy,
            buff_expires_at: case["buff"].as_f64().unwrap_or(100.0),
            reference_seconds: 0.32,
        },
        life.as_mut(),
        &mut evidence,
        |_, _| Ok(()),
    );
    assert!(!inputs.workers_retained());
    assert!(inputs.inputs().snapshot()?.held.is_empty());
    let (result, error) = match result {
        Ok(result) => (serde_json::to_value(result)?, Value::Null),
        Err(error) => (Value::Null, json!(error.to_string())),
    };
    let state = w.state();
    Ok(
        json!({"result":result,"error":error,"events":state.events,"emits":state.emits,"now":state.now,"evidence":evidence.iter().map(|e|json!({"file":e.filename,"frame":frame_index(&e.frame)})).collect::<Vec<_>>()}),
    )
}
#[test]
fn current_frame_encounter_matches_original_execution() -> TestResult {
    let (raw, policy) = config()?;
    let cases = json!([
        {}, {"camera":true}, {"buff":10.0}, {"buff":10.05}, {"stop_at":10.0}, {"focus_lost_at":10.0}, {"focus_lost_at":10.21},
        {"frames":[{"life":5}]}, {"frames":[{"merc":"unverified"}]}, {"frames":[{"life":null}]}, {"frames":[{"belt":"expanded"}]},
        {"probe":true,"frames":[{"life":5,"recover":true},{}]},
        {"frames":[{"player":null}]}, {"frames":[{"capture_delay":0.25}]}, {"frames":[{"ground_delay":0.16}]},
        {"frames":[{"alive":false}]}, {"probe":true,"frames":[{"alive":false}]}, {"probe":true,"frames":[{"alive":false,"hostile":true}]},
        {"probe":true,"frames":[{}, {"platform":null,"alive":false,"hostile":true,"proposals":[[800,550]]}]},
        {"probe":true,"frames":[{}, {"platform":null,"alive":false}]},
        {"frames":[{"alive_ground":[1300,580]}]}, {"frames":[{"alive_ground":[1300,580],"aim":null}]},
        {"frames":[{"alive_ground":[1300,580],"aim":[1199,580]}]}, {"frames":[{"alive_ground":[1300,580],"aim":[1200,580]}]},
        {"pointer_drift":true,"frames":[{"alive_ground":[1300,580]}]}, {"move_delay":0.11,"frames":[{"alive_ground":[1300,580]}]},
        {"frames":[{"path_error":true}]}, {"frames":[{"path_error":true,"alive_ground":[1300,580]}]},
        {"life":true,"frames":[{}, {"alive":false,"corpse":true}, {"alive":false,"corpse":true}]},
        {"life":true,"frames":[{"alive":false,"corpse":true},{"alive":false,"corpse":false}]},
        {"life":true,"frames":[{"alive":false,"corpse":true},{"alive":false,"corpse":true}]},
        {"life":true,"frames":[{"alive_ground":[1300,580]}, {"alive":false,"corpse":true}, {"alive":false,"corpse":true}]},
        {"loot":true}, {"loot":true,"labels_off":false},
        {"life":true,"loot":true,"frames":[{}, {"alive":false,"corpse":true}, {"alive":false,"corpse":true}]},
        {"life":true,"loot":true,"inspection":{"status":"inspection_complete","bounded_reason":"two_frames_without_eligible_candidate","movement_allowed":true,"clicks":[1]},"frames":[{"alive":false,"corpse":true},{"alive":false,"corpse":true}]},
        {"life":true,"loot":true,"inspection":{"status":"inspection_complete","bounded_reason":"two_frames_without_eligible_candidate","movement_allowed":true,"teleports":[1]},"frames":[{"alive":false,"corpse":true},{"alive":false,"corpse":true}]}
    ]);
    let expected = crate::test_support::legacy_expectation(
        "encounter-execution-oracle",
        &json!({"policy":raw,"cases":cases}),
    )?;
    for (i, case) in cases.as_array().ok_or("cases")?.iter().enumerate() {
        let mut actual = exercise(case, &policy)?;
        // 새 진단 한 장만 제외하고 입력·시계·판정과 기존 증거를 그대로 비교한다.
        let extra = actual
            .get_mut("result")
            .filter(|r| r.is_object())
            .and_then(|r| r.get_mut("evidence"))
            .and_then(Value::as_array_mut)
            .map(|items| {
                let before = items.len();
                items.retain(|item| item["phase"] != "unconfirmed");
                before - items.len()
            })
            .unwrap_or(0);
        if extra > 0 {
            let count = actual["result"]["evidence_frames"]
                .as_u64()
                .ok_or("evidence count")?;
            actual["result"]["evidence_frames"] = json!(count - extra as u64);
            actual["evidence"]
                .as_array_mut()
                .ok_or("evidence")?
                .retain(|item| {
                    !item["file"]
                        .as_str()
                        .is_some_and(|file| file.starts_with("encounter-unconfirmed-"))
                });
        }
        if let Some(rows) = actual["result"]
            .get_mut("observations")
            .and_then(Value::as_array_mut)
        {
            for row in rows {
                if let Some(fields) = row.as_object_mut() {
                    fields.remove("platform_hostile_proposals");
                }
            }
        }
        if let (Some(events), Some(old)) = (
            actual["events"].as_array_mut(),
            expected[i]["events"].as_array(),
        ) {
            for (event, before) in events.iter_mut().zip(old) {
                // 현재 발판 호버 지점만 새 계약이다. 그 외 입력·결과·시각은 비교한다.
                if event[0] == "probe" && before[4].is_null() && event[4].is_array() {
                    event[4] = Value::Null;
                }
            }
        }
        compare(&actual, &expected[i], &format!("case[{i}]"));
    }
    Ok(())
}
#[test]
fn encounter_hides_map_reobserves_corpse_and_restores_before_loot() -> TestResult {
    let (_, policy) = config()?;
    let base = json!({"map":true,"probe":true,"life":true,"loot":true,
        "frames":[{"alive":false},{"alive":false,"corpse":true},{"alive":false,"corpse":true}]});
    let result = exercise(&base, &policy)?;
    assert!(result["error"].is_null(), "{result}");
    assert_eq!(result["result"]["status"], "kill_confirmed");
    assert_eq!(result["result"]["kill_confirmed"], true);
    assert_eq!(result["result"]["map_inspection"]["status"], "restored");
    assert_eq!(
        result["result"]["observations"][0]["action"],
        "summoner_map_hide"
    );
    let events = result["events"].as_array().ok_or("events")?;
    assert_eq!(
        events
            .iter()
            .filter(|event| event[0] == "press" && event[1] == "tab")
            .count(),
        2
    );
    let restoration = events
        .iter()
        .position(|event| event[0] == "restore_map")
        .ok_or("restore event")?;
    let inspection = events
        .iter()
        .position(|event| event[0] == "loot")
        .ok_or("loot event")?;
    assert!(restoration < inspection);
    let mut case = base.clone();
    case["map_restore_error"] = json!(true);
    assert_eq!(exercise(&case, &policy)?["error"], "map restore failed");
    let mut case = base.clone();
    case["frames"] = json!([{"alive":false}]);
    let result = exercise(&case, &policy)?;
    assert_ne!(result["result"]["kill_confirmed"], true);
    assert!(result["result"]["loot_inspection"].is_null());
    let mut case = base.clone();
    case["stop_at"] = json!(10.25);
    let result = exercise(&case, &policy)?;
    assert!(
        result["error"]
            .as_str()
            .is_some_and(|e| e.contains("restoration interrupted"))
    );
    assert_eq!(
        result["events"]
            .as_array()
            .ok_or("events")?
            .iter()
            .filter(|event| event[0] == "press" && event[1] == "tab")
            .count(),
        1
    );
    Ok(())
}

#[test]
fn current_platform_probe_covers_lower_body_without_unverified_attacks() -> TestResult {
    assert_eq!(
        platform_probe_points([1450.0, 570.0], [1450, 570])?,
        vec![[1450, 658], [1426, 658], [1474, 658], [1450, 634]]
    );
    assert!(platform_probe_points([f64::NAN, f64::INFINITY], [962, 580])?.is_empty());
    assert!(platform_probe_points([1899.0, 899.0], [1890, 890])?.is_empty());
    assert_eq!(
        platform_probe_points([1082.0, 432.0], [962, 580])?.first(),
        Some(&[1082, 608])
    );
    let (_, policy) = config()?;
    let result = exercise(&json!({"probe":true,"frames":[{"alive":false}]}), &policy)?;
    assert_eq!(
        result["result"]["observations"][0]["platform_hostile_proposals"],
        json!([[800, 668], [776, 668], [824, 668], [800, 644]])
    );
    assert_eq!(result["result"]["nova_dispatches"], 0);
    assert_eq!(result["result"]["approach_requests"], 0);
    Ok(())
}
#[test]
fn unknown_platform_approaches_before_probing_without_attacking() -> TestResult {
    let (_, policy) = config()?;
    let result = exercise(
        &json!({"probe":true,"frames":[{
            "alive":false,"player":[962,580],"platform_ground":[1161.5,714],"aim":[1162,714]
        }]}),
        &policy,
    )?;
    assert!(result["error"].is_null(), "{result}");
    assert_eq!(
        result["result"]["observations"][0]["action"],
        "approach_requested"
    );
    assert_eq!(result["result"]["approach_requests"], 2);
    assert_eq!(result["result"]["nova_dispatches"], 0);
    Ok(())
}
#[test]
fn unconfirmed_encounter_keeps_one_distinct_existing_frame_within_cap() -> TestResult {
    let (_, policy) = config()?;
    let result = exercise(&json!({"probe":true,"frames":[{"alive":false}]}), &policy)?;
    let evidence = result["result"]["evidence"].as_array().ok_or("evidence")?;
    let extra: Vec<_> = evidence
        .iter()
        .filter(|e| e["phase"] == "unconfirmed")
        .collect();
    assert_eq!(extra.len(), 1);
    assert!(evidence.len() <= 24);
    assert!(
        evidence
            .iter()
            .filter(|e| e["phase"] != "unconfirmed")
            .all(|e| e["sequence"] != extra[0]["sequence"])
    );
    assert_eq!(result["result"]["nova_dispatches"], 0);
    Ok(())
}
#[test]
fn encounter_cleanup_releases_input_and_preserves_primary_error() -> TestResult {
    let (_, policy) = config()?;
    let result = exercise(
        &json!({"cleanup_error":true,"frames":[{"alive_ground":[1300,580],"path_error":true}]}),
        &policy,
    )?;
    assert!(result["error"].as_str().is_some_and(
        |e| e.contains("path observation failed") && e.contains("vision shutdown timed out")
    ));
    let result = exercise(&json!({"cleanup_error":true}), &policy)?;
    assert!(
        result["events"]
            .as_array()
            .is_some_and(|e| e.iter().any(|v| v[0] == "up" && v[1] == "f4"))
    );
    assert_eq!(result["error"], "vision shutdown timed out");
    Ok(())
}

#[test]
fn camera_lease_recovers_after_reentry_and_unwind() -> TestResult {
    use crate::runs::summoner::camera_ground::{
        CalibratedCameraGroundObserver, CameraLimits, CameraOnlyGround,
    };
    let camera = CameraGroundHandle::new(CalibratedCameraGroundObserver::new(
        Box::new(CameraOnlyGround),
        |_| Ok(false),
        CameraLimits::default(),
    )?);
    camera.with(|_| {
        // 중첩 판독은 잠금을 기다리며 멈추지 않고 소유권 충돌을 보고한다.
        assert!(camera.with(|_| Ok(())).is_err());
        Ok(())
    })?;
    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = camera.with::<()>(|_| panic!("camera fixture panic"));
    }));
    assert!(unwind.is_err());
    camera.with(|_| Ok(()))?;
    Ok(())
}

#[test]
fn native_parallel_observation_joins_and_releases_current_frame() -> TestResult {
    use crate::runs::summoner::{
        camera_ground::{CalibratedCameraGroundObserver, CameraLimits},
        navigation_execution::load_arcane_step_planner,
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let owner = root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
    let (ground, path) = load_arcane_step_planner(root, false, "other")?;
    let camera = CameraGroundHandle::new(CalibratedCameraGroundObserver::new(
        ground,
        |_| Ok(false),
        CameraLimits::default(),
    )?);
    let mut vision = NativeEncounterVision::new(
        camera.clone(),
        Arc::new(path),
        Arc::new(ArcaneObjectiveObserver::load(&owner)?),
        Arc::new(SummonerAppearanceObserver::load(
            owner.join("summoner-encounter.json"),
        )?),
        CooperativeStop::default(),
    )?;
    let frame = Arc::new(BgrFrame::zeros(Size::new(1922, 1140)?)?);
    let weak = Arc::downgrade(&frame);
    let observed = vision.observe(&frame, 0, 10.0, true, true)?;
    assert!(observed.player.point.is_none());
    assert!(observed.platform.is_none());
    assert!(observed.target.is_none());
    assert!(observed.corpse.is_none());
    assert!(observed.path.is_none());
    drop(observed);
    vision.finish(Duration::from_secs(2))?;
    camera.with(|_| Ok(()))?;
    drop(frame);
    assert!(weak.upgrade().is_none());
    Ok(())
}

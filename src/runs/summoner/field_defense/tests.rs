use super::*;
use crate::{
    common::{
        encounters::{Appearance, CandidateCategory, CandidateIdentity, SceneContext},
        error::ObservationError,
        field_control::InputSessionOptions,
        frame::{PixelRect, Size},
        survival::ObservationStamp,
        time::MonotonicClock,
    },
    test_field_world::{World, compare, config, frame_index, point},
};
use std::error::Error;
type TestResult = Result<(), Box<dyn Error>>;
impl PausedEnvironment for World {
    fn encounter_recheck(&mut self, _: &Arc<BgrFrame>, _: f64) -> Result<bool, RunError> {
        if self.state().case["recheck"] != true {
            return Ok(false);
        }
        self.delay("recheck_delay");
        Ok(self.sample()["corpse"] == true)
    }
    fn has_renewal(&self) -> bool {
        self.state().case["renew"] == true
    }
    fn renew_buffs(
        &mut self,
        _: &mut Value,
        _: &mut Vec<EncounterEvidence>,
    ) -> Result<f64, RunError> {
        self.event(json!(["renew", self.now()]));
        let delay = self.state().case["renew_delay"].as_f64().unwrap_or(0.1);
        self.advance(delay);
        if self.state().case["renew_error"] == true {
            return Err(RunError::Runtime("buff renewal failed".into()));
        }
        let configured = self.state().case["renewed"].as_f64();
        Ok(configured.unwrap_or(self.now() + 20.0))
    }
}
struct Vision(World);
impl DefenseVision for Vision {
    fn prepares_ground(&self) -> bool {
        self.0.state().case["camera"] == true
    }
    fn prepare(
        &mut self,
        _: &Arc<BgrFrame>,
        acquired: f64,
        _: &HudReading,
    ) -> Result<(), RunError> {
        self.0
            .event(json!(["prepare", self.0.index(), acquired, true]));
        Ok(())
    }
    fn ground(&mut self, _: &Arc<BgrFrame>, acquired: f64) -> Result<GroundReading, RunError> {
        self.0.event(json!(["ground", self.0.index(), acquired]));
        self.0.delay("ground_delay");
        let s = self.0.sample();
        Ok(GroundReading {
            point: if s.get("player") == Some(&Value::Null) {
                None
            } else {
                Some(point(&s["player"], [750.0, 580.0]).map(|v| v as i32))
            },
            reason: "test".into(),
            score: None,
            calibration: None,
        })
    }
    fn hazards(
        &mut self,
        _: &Arc<BgrFrame>,
        player: [i32; 2],
        acquired: f64,
    ) -> Result<Vec<VisualCandidate>, RunError> {
        self.0.event(json!([
            "hazards",
            self.0.index(),
            player,
            acquired,
            self.0.now(),
            null
        ]));
        self.0.delay("hazard_delay");
        if self.0.sample()["near"] != true {
            return Ok(vec![]);
        }
        let context = Arc::new(SceneContext::new(
            ObservationStamp::new("flash", "room", self.0.index() as u64, acquired)?,
            "arcane",
            self.0.index() as u64,
            Size::new(1922, 1140).map_err(ObservationError::from)?,
            PixelRect::new(24, 55, 1900, 900).map_err(ObservationError::from)?,
        )?);
        Ok(vec![VisualCandidate::new(
            context,
            CandidateIdentity::new(
                "hostile",
                "hostile",
                CandidateCategory::Monster,
                Appearance::Alive,
            )?,
            1.0,
            PixelRect::new(700, 500, 810, 610).map_err(ObservationError::from)?,
            Some([800.0, 580.0]),
        )?])
    }
}
fn exercise(case: &Value, policy: &CharacterSurvivalPolicy) -> Result<Value, Box<dyn Error>> {
    let is_pause = case["operation"] == "pause";
    let mut case = case.clone();
    case["kind"] = json!(if is_pause { "paused" } else { "clear" });
    let mut w = World::new(&case);
    if case["stop_at"].as_f64().is_some_and(|at| at <= 10.0) {
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
            repeat_seconds: if is_pause { None } else { Some(0.32) },
            aim_settle_seconds: 0.0,
        },
    )?;
    let mut vision = Vision(w.clone());
    let mut report = case.get("report").cloned().unwrap_or(json!({}));
    let mut evidence = vec![];
    let result = if is_pause {
        paused(
            &mut inputs,
            &mut w,
            if case["vision"] == false || case["backend"] == false {
                None
            } else {
                Some(&mut vision)
            },
            PausedRequest {
                policy,
                buff_expires_at: case["buff"].as_f64(),
                engage_monsters: case["engage"].as_bool().unwrap_or(true),
            },
            &mut report,
            &mut evidence,
            |_, _| Ok(()),
        )
        .map(|_| Value::Null)
    } else {
        clear(&mut inputs, &mut w, policy, &mut evidence, |_, _| Ok(())).map(|report| json!(report))
    };
    assert!(!inputs.workers_retained());
    assert!(inputs.inputs().snapshot()?.held.is_empty());
    let (result, error) = match result {
        Ok(value) => (value, Value::Null),
        Err(error) => (Value::Null, json!(error.to_string())),
    };
    let state = w.state();
    Ok(
        json!({"result":result,"error":error,"progress":report,"events":state.events,"emits":state.emits,"now":state.now,"evidence":evidence.iter().map(|e|json!({"file":e.filename,"frame":frame_index(&e.frame)})).collect::<Vec<_>>()}),
    )
}
#[test]
fn paused_corpse_candidate_returns_to_verification_before_idle_sweep() -> TestResult {
    let (_, policy) = config()?;
    let case = json!({"operation":"pause","buff":100.0,"probe":true,"recheck":true,"stop_at":11.0,"frames":[{"corpse":true}]});
    let result = exercise(&case, &policy)?;
    assert!(result["error"].is_null(), "{result}");
    assert_eq!(result["progress"]["encounter_recheck_requested"], true);
    assert_eq!(
        result["progress"]["paused_survival_guard"]["status"],
        "encounter_recheck"
    );
    assert!(
        !result["events"]
            .as_array()
            .ok_or("events")?
            .iter()
            .any(|event| event[0] == "probe")
    );
    let mut stale = case.clone();
    stale["frames"][0]["recheck_delay"] = json!(0.3);
    assert_ne!(
        exercise(&stale, &policy)?["progress"]["encounter_recheck_requested"],
        true
    );
    let mut low = case;
    low["frames"][0]["life"] = json!(5);
    assert_ne!(
        exercise(&low, &policy)?["progress"]["encounter_recheck_requested"],
        true
    );
    Ok(())
}

#[test]
fn paused_and_buff_site_defense_match_original() -> TestResult {
    let (raw, policy) = config()?;
    let mut cases: Value = serde_json::from_str(
        r#"[
      {"operation":"clear"},{"operation":"clear","stop_at":10},{"operation":"clear","focus_lost_at":10},{"operation":"clear","focus_lost_at":10.5},
      {"operation":"clear","frames":[{"life":5}]},{"operation":"clear","frames":[{"merc":"unverified"}]},{"operation":"clear","frames":[{"life":null}]},{"operation":"clear","frames":[{"belt":"expanded"},{},{"capture_delay":0.25},{}]},
      {"operation":"pause"},{"operation":"pause","stop_at":10},{"operation":"pause","focus_lost_at":10},{"operation":"pause","focus_lost_at":10.5},
      {"operation":"pause","frames":[{"life":5}]},{"operation":"pause","frames":[{"merc":"unverified"}]},{"operation":"pause","stop_at":20,"frames":[{"life":null}]},
      {"operation":"pause","buff":100,"frames":[{"near":true}]},{"operation":"pause","buff":100,"camera":true,"frames":[{"near":true}]},{"operation":"pause","buff":100,"engage":false,"frames":[{"near":true}]},
      {"operation":"pause","buff":100,"vision":false,"frames":[{"near":true}]},{"operation":"pause","buff":100,"backend":false,"frames":[{"near":true}]},
      {"operation":"pause","buff":10,"frames":[{"near":true}]},{"operation":"pause","buff":100,"frames":[{"near":true,"hazard_delay":0.16}]},{"operation":"pause","buff":100,"frames":[{"near":true,"life":5,"recover":true},{}]},
      {"operation":"pause","buff":100,"stop_at":20,"frames":[{"near":true}]},{"operation":"pause","buff":100,"frames":[{"near":true},{"near":true},{"near":true},{}],"loot":true},
      {"operation":"pause","buff":100,"probe":true,"frames":[{"hostile":true}]},{"operation":"pause","buff":100,"probe":true,"frames":[{"player":null}]},
      {"operation":"pause","buff":10,"renew":true},{"operation":"pause","buff":10,"renew":true,"renew_error":true},{"operation":"pause","buff":10,"renew":true,"engage":false},
      {"operation":"pause","buff":100,"frames":[{"capture_delay":0.25,"life":5}]},{"operation":"pause","buff":100,"frames":[{"life":null},{"near":true}]}
    ]"#,
    )?;
    for case in cases.as_array_mut().ok_or("cases")? {
        if case["operation"] == "pause" && case.get("stop_at").is_none() {
            case["stop_at"] = json!(10.6);
        }
    }
    let expected = crate::test_support::legacy_expectation(
        "continuous-execution-oracle",
        &json!({"policy":raw,"cases":cases}),
    )?;
    for (i, case) in cases.as_array().ok_or("cases")?.iter().enumerate() {
        compare(
            &exercise(case, &policy)?,
            &expected[i],
            &format!("defense case[{i}]"),
        );
    }
    Ok(())
}

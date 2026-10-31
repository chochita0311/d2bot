use super::*;
use crate::{
    common::{
        encounters::{Appearance, CandidateCategory, CandidateIdentity, SceneContext},
        error::{ControlError, ObservationError},
        frame::{PixelRect, Size},
        survival::ObservationStamp,
        time::MonotonicClock,
    },
    test_field_world::{World, compare, config, frame_index, point},
};
use std::error::Error;
type TestResult = Result<(), Box<dyn Error>>;
impl LegacyEnvironment for World {
    fn wall_clock(&self) -> f64 {
        1010.0
    }
}
impl StepEnvironment for World {
    fn clock(&self) -> f64 {
        self.now()
    }
    fn stopped(&self) -> bool {
        self.1.is_set()
    }
    fn focused(&self) -> Result<bool, ControlError> {
        World::focused(self)
    }
    fn wait(&self, s: f64) -> Result<(), ControlError> {
        self.advance(s);
        Ok(())
    }
}
struct Vision(World);
impl LegacyVision for Vision {
    type View = i64;
    fn has_hazards(&self) -> bool {
        self.0.state().case["hazards"] != false
    }
    fn has_hover(&self) -> bool {
        self.0.state().case["hover"] != false
    }
    fn hub(&mut self, _: &Arc<BgrFrame>) -> Result<Option<[f64; 2]>, RunError> {
        if self.0.state().case["has_hub"] != true {
            return Ok(None);
        }
        self.0.event(json!(["hub", self.0.index()]));
        Ok(self.0.sample()["hub"]
            .as_array()
            .map(|_| point(&self.0.sample()["hub"], [0.0; 2])))
    }
    fn ground(&mut self, _: &Arc<BgrFrame>, at: f64) -> Result<GroundReading, RunError> {
        self.0.event(json!(["ground", self.0.index(), at]));
        self.0.delay("ground_delay");
        let s = self.0.sample();
        Ok(GroundReading {
            point: if s.get("player") == Some(&Value::Null) {
                None
            } else {
                Some(point(&s["player"], [750.0, 580.0]).map(|v| v as i32))
            },
            reason: s["ground_reason"].as_str().unwrap_or("test").into(),
            score: None,
            calibration: None,
        })
    }
    fn plan(
        &mut self,
        f: &Arc<BgrFrame>,
        r: LegacyPlanRequest<'_>,
    ) -> Result<CurrentPlan<i64>, RunError> {
        self.0.event(json!([
            "plan",
            self.0.index(),
            r.acquired,
            r.hub,
            r.position,
            r.visited
        ]));
        let ground = self.ground(f, r.acquired)?;
        self.0.delay("plan_delay");
        let s = self.0.sample();
        if let Some(error) = s["plan_error"].as_str() {
            return Err(RunError::Runtime(error.into()));
        }
        if ground.point.is_none() {
            return Err(RunError::Runtime(ground.reason));
        }
        if !(0.0..0.25).contains(&(self.0.now() - r.acquired)) {
            return Err(RunError::Runtime("planned step frame is stale".into()));
        }
        Ok(CurrentPlan {
            ground,
            view: self.0.index(),
            choice: PathChoice {
                aim: if s.get("aim") == Some(&Value::Null) {
                    None
                } else {
                    Some(point(&s["aim"], [1300.0, 300.0]).map(|v| v as i32))
                },
                reason: s["choice_reason"].as_str().unwrap_or("test").into(),
                path: vec![],
            },
        })
    }
    fn hazards(
        &mut self,
        _: &Arc<BgrFrame>,
        p: [i32; 2],
        at: f64,
        now: f64,
        aim: Option<[i32; 2]>,
    ) -> Result<Vec<VisualCandidate>, RunError> {
        self.0
            .event(json!(["hazards", self.0.index(), p, at, now, aim]));
        self.0.delay("hazard_delay");
        if self.0.sample()["near"] != true {
            return Ok(vec![]);
        }
        let context = Arc::new(SceneContext::new(
            ObservationStamp::new("flash", "room", self.0.index() as u64, at)?,
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
    fn hover(&mut self, _: &Arc<BgrFrame>, aim: [i32; 2]) -> Result<Vec<HoverCandidate>, RunError> {
        self.0.event(json!(["hover", self.0.index(), aim]));
        self.0.delay("hover_delay");
        Ok(if self.0.sample()["hover"] == true {
            vec![HoverCandidate {
                kind: "chest".into(),
                evidence_box: [1000, 400, 1050, 420],
                score: 1.0,
            }]
        } else {
            vec![]
        })
    }
    fn landing(
        &mut self,
        _: &mut impl LandingEnvironment,
        r: LegacyLandingRequest<'_, i64>,
    ) -> Result<LandingResult, RunError> {
        self.0.event(json!([
            "landing",
            self.0.index(),
            r.reference_seconds,
            r.expires_at
        ]));
        self.0.delay("landing_delay");
        Ok(LandingResult {
            status: match self.0.sample()["landing_status"].as_str() {
                Some("arrival_unverified") => "arrival_unverified",
                Some("resource_low") => "resource_low",
                _ => "arrival_observed",
            },
            displacement: point(&self.0.sample()["displacement"], [12.5, -8.0]),
            observations: vec![],
        })
    }
}
fn exercise(case: &Value, policy: &CharacterSurvivalPolicy) -> Result<Value, Box<dyn Error>> {
    let mut w = World::new(case);
    let backend = w.clone();
    let guards = w.clone();
    let mut v = Vision(w.clone());
    let proof=case.get("proof").cloned().unwrap_or(json!({"source":"runtime_screen","area":"arcane_sanctuary","panels_closed":true,"belt_closed":true,"nearby_candidates":0,"safety_scope":"current_nearby_candidates_and_stable_ratios","observed_at":1009.9,"current_wing":"north","wing_phase":"outbound"}));
    let mut progress = case
        .get("progress")
        .cloned()
        .unwrap_or(json!({"steps":[],"position":[0.0,0.0],"breadcrumbs":[[0.0,0.0]]}));
    let mut evidence = vec![];
    let result = run_north_segment(
        &backend,
        &guards,
        &mut w,
        &mut v,
        LegacyRequest {
            policy,
            proof: &proof,
            casts: case["casts"].as_u64().unwrap_or(1) as usize,
            reference_seconds: case["reference"].as_f64().unwrap_or(0.32),
            buff_expires_at: case["buff"].as_f64(),
            movement_key: "f2",
        },
        &mut progress,
        &mut evidence,
    );
    let (result, error) = match result {
        Ok(v) => (v, Value::Null),
        Err(RunError::Control(ControlError::Configuration(e))) => (Value::Null, json!(e.message())),
        Err(e) => (Value::Null, json!(e.to_string())),
    };
    let s = w.state();
    Ok(
        json!({"result":result,"error":error,"progress":progress,"events":s.events,"emits":s.emits,"now":s.now,"evidence":evidence.iter().map(|e|json!({"file":e.filename,"frame":frame_index(&e.frame)})).collect::<Vec<_>>()}),
    )
}
#[test]
fn legacy_segments_match_original() -> TestResult {
    let (raw, policy) = config()?;
    let mut cases: Value = serde_json::from_str(
        r#"[
 {},{"casts":0},{"casts":13},{"casts":3},{"has_hub":true,"frames":[{"hub":[750,580]}]},
 {"hazards":false,"hover":false},{"buff":10},{"buff":10.03},{"focus_lost_at":10},{"stop_at":10.01},
 {"frames":[{"life":5}]},{"frames":[{"merc":"unverified"}]},{"frames":[{"life":null},{}]},
 {"frames":[{"player":null,"ground_reason":"ground_unverified"},{}]},
 {"frames":[{"player":null,"ground_reason":"current_ground_missing"}]},
 {"frames":[{"plan_error":"player_floor_unobserved"},{}]},
 {"frames":[{"plan_error":"no_observed_progress"}]},{"frames":[{"plan_error":"invalid_path"}]},
 {"frames":[{"plan_delay":0.11},{}]},{"frames":[{"plan_delay":0.11}]},
 {"frames":[{"near":true,"plan_delay":0.3}]},{"frames":[{}, {"hover":true}]},
 {"frames":[{}, {"life":5}]},{"frames":[{}, {"life":null},{},{}]},
 {"frames":[{}, {"capture_delay":0.25},{},{}]},
 {"move_delay":0.3,"hover":false},{"pointer_drift":true},
 {"frames":[{"landing_status":"arrival_unverified"}]},
 {"progress":{"steps":[{"old":true}],"position":[2,3],"breadcrumbs":[[0,0],[2,3]]},"casts":2}
 ]"#,
    )?;
    for case in cases.as_array_mut().ok_or("cases")? {
        case["operation"] = json!("legacy");
        if !case["frames"].is_array() {
            case["frames"] = json!([{}]);
        }
        for f in case["frames"].as_array_mut().ok_or("frames")? {
            if f.get("capture_delay").is_none() {
                f["capture_delay"] = json!(0.01)
            }
        }
    }
    let expected = crate::test_support::legacy_expectation(
        "continuous-execution-oracle",
        &json!({"policy":raw,"cases":cases}),
    )?;
    for (i, c) in cases.as_array().ok_or("cases")?.iter().enumerate() {
        compare(
            &exercise(c, &policy)?,
            &expected[i],
            &format!("legacy case[{i}]"),
        );
    }
    Ok(())
}

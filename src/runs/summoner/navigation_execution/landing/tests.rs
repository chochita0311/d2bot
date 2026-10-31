use super::*;
use crate::common::{
    frame::{GrayFrame, Size},
    scene_motion::MotionReason,
};
use serde_json::{Value, json};
use std::{
    error::Error,
    sync::{Arc, Mutex},
};
struct State {
    now: f64,
    captures: usize,
    case: Value,
}
struct Env {
    state: Arc<Mutex<State>>,
    frame: Arc<BgrFrame>,
}
impl StepEnvironment for Env {
    fn clock(&self) -> f64 {
        self.state.lock().unwrap_or_else(|p| p.into_inner()).now
    }
    fn stopped(&self) -> bool {
        let s = self.state.lock().unwrap_or_else(|p| p.into_inner());
        s.captures >= s.case["stop_at"].as_u64().unwrap_or(100) as usize
    }
    fn focused(&self) -> Result<bool, crate::common::error::ControlError> {
        let s = self.state.lock().unwrap_or_else(|p| p.into_inner());
        Ok(s.captures < s.case["focus_lost_at"].as_u64().unwrap_or(100) as usize)
    }
    fn wait(&self, _: f64) -> Result<(), crate::common::error::ControlError> {
        let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
        s.now += s.case["wait"].as_f64().unwrap_or(0.05);
        Ok(())
    }
}
impl LandingEnvironment for Env {
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError> {
        self.state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .captures += 1;
        Ok(self.frame.clone())
    }
    fn hud(&mut self, _: &Arc<BgrFrame>) -> Result<HudReading, RunError> {
        let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
        s.now += s.case["processing"].as_f64().unwrap_or(0.0);
        let unknown = s.case["unknown"] == true
            || s.case["unknown_at"]
                .as_array()
                .is_some_and(|a| a.contains(&json!(s.captures)));
        let life = if unknown {
            ResourceReading::default()
        } else {
            ResourceReading::from_counts(
                s.case["life"].as_u64().unwrap_or(100),
                100,
                "unverified",
                None,
            )
        };
        let mana = if s.case.get("mana") == Some(&Value::Null) {
            ResourceReading::default()
        } else {
            ResourceReading::from_counts(
                s.case["mana"].as_u64().unwrap_or(100),
                100,
                "unverified",
                None,
            )
        };
        Ok(HudReading {
            layout_id: String::new(),
            revision: String::new(),
            life,
            mana,
            belt_status: "read".into(),
            slots: vec![],
            belt_visibility: match s.case["belt"].as_str().unwrap_or("closed") {
                "closed" => BeltVisibility::Closed,
                "expanded" => BeltVisibility::Expanded,
                _ => BeltVisibility::Unknown,
            },
            mercenary_status: s.case["mercenary"].as_str().map(Into::into),
        })
    }
}
struct Ground(Arc<Mutex<State>>);
impl GroundObserver for Ground {
    fn observe(
        &mut self,
        _: &Arc<BgrFrame>,
        _: Option<f64>,
    ) -> Result<GroundReading, ObservationError> {
        let s = self.0.lock().unwrap_or_else(|p| p.into_inner());
        let point = if s.case["no_ground"] == true
            || s.case["ground_at"]
                .as_u64()
                .is_some_and(|n| n as usize != s.captures)
        {
            None
        } else {
            Some([960, 578])
        };
        Ok(GroundReading {
            point,
            reason: "test".into(),
            score: None,
            calibration: None,
        })
    }
}
struct Motion {
    case: Value,
    hints: Vec<Option<[f64; 2]>>,
}
impl LandingMotion for Motion {
    fn observe(
        &mut self,
        _: &Arc<BgrFrame>,
        _: &PathView,
        hint: Option<[f64; 2]>,
    ) -> Result<CameraMotion, RunError> {
        self.hints.push(hint);
        let count = self.hints.len();
        let mut delta = match count {
            1 => None,
            2 => Some(if self.case["delta"].is_null() {
                [80.0, -40.0]
            } else {
                serde_json::from_value(self.case["delta"].clone())
                    .map_err(ObservationError::from)?
            }),
            _ => Some([0.0; 2]),
        };
        if self.case["motion_missing_at"]
            .as_array()
            .is_some_and(|a| a.contains(&json!(count)))
        {
            delta = None;
        }
        Ok(CameraMotion {
            displacement: delta,
            inliers: 20,
            reason: MotionReason::BackgroundTranslation,
            matches: 0,
            inlier_ratio: None,
            linear_error: None,
        })
    }
}
fn compare(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert!(
            (a.as_f64().unwrap_or(f64::NAN) - b.as_f64().unwrap_or(f64::NAN)).abs() < 1e-9,
            "{path}: {a} != {b}"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (k, a) in a {
                compare(a, &b[k], &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
#[test]
fn landing_requires_current_resources_motion_alignment_and_settled_ground()
-> Result<(), Box<dyn Error>> {
    let cases = vec![
        json!({}),
        json!({"delta":[0,0]}),
        json!({"delta":[-80,40]}),
        json!({"no_ground":true}),
        json!({"processing":0.26}),
        json!({"expires":100.34}),
        json!({"ground_at":8}),
        json!({"unknown_at":[2,3]}),
        json!({"unknown":true}),
        json!({"motion_missing_at":[3,4]}),
        json!({"life":49,"mana":null,"belt":"expanded"}),
        json!({"life":50,"mana":10}),
        json!({"mercenary":"unverified"}),
        json!({"focus_lost_at":3}),
        json!({"stop_at":3}),
        json!({"delta":[400,-200]}),
        json!({"delta":[10,-5]}),
        json!({"processing":-0.1}),
        json!({"wait":0}),
        json!({"reference":0}),
        json!({"reference":1.5}),
        json!({"reference":"nan"}),
    ];
    let raw: Value = serde_json::from_slice(&std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("config/characters/characters.json"),
    )?)?;
    let policy_raw = &raw["characters"]["flash"]["survival"];
    let policy = CharacterSurvivalPolicy::from_value(policy_raw)?;
    let expected = crate::test_support::legacy_expectation(
        "navigation-execution-oracle",
        &json!({"kind":"landing","cases":cases,"policy":policy_raw}),
    )?;
    let frame = Arc::new(BgrFrame::zeros(Size::new(1922, 1140)?)?);
    let (_, path) = load_arcane_step_planner(env!("CARGO_MANIFEST_DIR"), false, "sorceress")?;
    let view = PathView::from_grid(
        PathContext {
            sequence: 1,
            generation: 0,
            captured_at: 100.0,
            frame_size: frame.size(),
            viewport: PixelRect::new(600, 358, 1320, 798)?,
            anchor: [960.0, 578.0],
        },
        8,
        GrayFrame::zeros(Size::new(90, 55)?)?,
    )?;
    let choice = PathChoice {
        aim: Some([1060, 528]),
        reason: "test".into(),
        path: vec![],
    };
    for (i, case) in cases.iter().enumerate() {
        let state = Arc::new(Mutex::new(State {
            now: 100.0,
            captures: 0,
            case: case.clone(),
        }));
        let mut env = Env {
            state: state.clone(),
            frame: frame.clone(),
        };
        let mut ground = Ground(state.clone());
        let mut motion = Motion {
            case: case.clone(),
            hints: vec![],
        };
        let reference = if case["reference"] == "nan" {
            f64::NAN
        } else {
            case["reference"].as_f64().unwrap_or(0.32)
        };
        let result = observe_with_motion(
            &mut env,
            &mut ground,
            &path,
            LandingRequest {
                frame: &frame,
                view: &view,
                choice: &choice,
                policy: &policy,
                reference_seconds: reference,
                expires_at: case["expires"].as_f64(),
            },
            &mut motion,
        );
        let (outcome, error) = match result {
            Ok(value) => (Some(value), None),
            Err(error) => (None, Some(error.to_string())),
        };
        let actual = json!({"outcome":outcome,"error":error,"captures":state.lock().unwrap_or_else(|p|p.into_inner()).captures,"motion_hints":motion.hints});
        compare(&actual, &expected[i], &format!("landing{i}"));
    }
    Ok(())
}

use d2bot::{
    common::{
        error::{ControlError, ObservationError},
        field_control::{DesktopPoint, FieldInputBackend},
        frame::{BgrFrame, Size},
        time::MonotonicClock,
        vision_ops::codec,
    },
    runs::summoner::{
        camera_ground::GroundObserver,
        navigation::ChooseOptions,
        navigation_execution::{
            DispatchDiagnostics, DispatchOptions, PathMotionMasks, PlanOptions, PointerPreparation,
            StepEnvironment, StepRequest, dispatch_planned_step, load_arcane_step_planner,
            plan_arcane_step,
        },
        scene_vision::GroundReading,
    },
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    error::Error,
    path::Path,
    sync::{Arc, Mutex},
};
mod support;
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
fn number(value: &Value) -> f64 {
    match value.as_str() {
        Some("nan") => f64::NAN,
        Some("inf") => f64::INFINITY,
        Some("-inf") => f64::NEG_INFINITY,
        _ => value.as_f64().unwrap_or(f64::NAN),
    }
}
#[derive(Default)]
struct ScriptState {
    operation: Value,
    counts: BTreeMap<String, usize>,
    trace: Vec<Value>,
    pointer: Option<DesktopPoint>,
}
#[derive(Default)]
struct Script(Mutex<ScriptState>);
impl Script {
    fn state(&self) -> std::sync::MutexGuard<'_, ScriptState> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }
    fn next(&self, name: &str, fallback: Value) -> Value {
        let mut state = self.state();
        let index = state.counts.get(name).copied().unwrap_or(0);
        let value = state.operation[name]
            .as_array()
            .and_then(|a| a.get(index).or_else(|| a.last()))
            .cloned()
            .unwrap_or(fallback);
        state.counts.insert(name.into(), index + 1);
        let traced = if name == "clock" {
            json!(number(&value))
        } else {
            value.clone()
        };
        state.trace.push(json!([name, traced]));
        value
    }
}
struct FakeBackend(Arc<Script>);
impl FieldInputBackend for FakeBackend {
    fn key_down(&self, _: &str) -> Result<(), ControlError> {
        Err(ControlError::Runtime("unexpected held key".into()))
    }
    fn key_up(&self, _: &str) -> Result<(), ControlError> {
        Err(ControlError::Runtime("unexpected held release".into()))
    }
    fn press(&self, key: &str) -> Result<(), ControlError> {
        let mut s = self.0.state();
        s.trace.push(json!(["press", key]));
        if s.operation["failure"] == "press" {
            Err(ControlError::Backend("press failed".into()))
        } else {
            Ok(())
        }
    }
    fn move_pointer(&self, aim: DesktopPoint) -> Result<(), ControlError> {
        let mut s = self.0.state();
        s.trace.push(json!(["move", aim[0], aim[1]]));
        if s.operation["failure"] == "move" {
            return Err(ControlError::Backend("move failed".into()));
        }
        s.pointer = Some(aim);
        Ok(())
    }
    fn pointer_matches(&self, aim: DesktopPoint) -> Result<bool, ControlError> {
        let mut s = self.0.state();
        let matches = s.pointer == Some(aim);
        s.trace.push(json!(["pointer_matches", aim, matches]));
        Ok(matches)
    }
}
impl StepEnvironment for Script {
    fn clock(&self) -> f64 {
        number(&self.next("clock", json!(100.2)))
    }
    fn stopped(&self) -> bool {
        self.next("stop", json!(false)) == true
    }
    fn focused(&self) -> Result<bool, ControlError> {
        Ok(self.next("focus", json!(true)) == true)
    }
    fn wait(&self, seconds: f64) -> Result<(), ControlError> {
        self.state().trace.push(json!(["wait", seconds]));
        Ok(())
    }
}
#[test]
fn dispatch_matches_guard_order_diagnostics_preparation_and_errors() -> Result<(), Box<dyn Error>> {
    let cases = vec![
        json!({"operations":[{"at":100,"clock":[100.2]}]}),
        json!({"operations":[{"at":100,"clock":[100.1,100.25]}]}),
        json!({"operations":[{"at":100,"clock":[100.1],"stop":[false,true]}]}),
        json!({"operations":[{"at":100,"clock":[100.1],"focus":[true,false]}]}),
        json!({"operations":[{"at":100,"clock":[99.9]}]}),
        json!({"operations":[{"at":"nan"}]}),
        json!({"operations":[{"at":100,"expires":"nan"}]}),
        json!({"operations":[{"at":100,"clock":["nan"]}]}),
        json!({"operations":[{"at":100,"clock":[100.1,100.2],"verify":[false]}]}),
        json!({"operations":[{"at":111.9,"clock":[111.9,111.95,112.0],"expires":112,"verify":[true]}]}),
        json!({"operations":[{"at":100,"clock":[100.1,100.2,100.25],"verify":[true]}]}),
        json!({"prepared":true,"operations":[{"at":100,"clock":[100.2]},{"at":100.22,"clock":[100.24]},{"at":100.3,"clock":[100.51]}]}),
        json!({"prepared":true,"pointer":[110,100],"preparation":{"aim":[100,100],"requested_at":100},"operations":[{"at":100.1}]}),
        json!({"prepared":true,"pointer":[100,100],"preparation":{"aim":[100,100],"requested_at":100},"operations":[{"at":100.1,"aim":[120,100]}]}),
        json!({"prepared":true,"pointer":[100,100],"preparation":{"aim":[100,100],"requested_at":100},"operations":[{"at":100.1,"verify":[false]}]}),
        json!({"prepared":true,"pointer":[100,100],"preparation":{"aim":[100,100],"requested_at":100},"operations":[{"at":100.3,"clock":[100.4,100.55],"verify":[true]}]}),
        json!({"prepared":true,"preparation":{"aim":[100,100],"requested_at":100},"operations":[{"at":100.1,"stop":[true]}]}),
        json!({"prepared":true,"preparation":{"aim":[100,100],"requested_at":100},"operations":[{"at":100.1,"focus":[false]}]}),
        json!({"prepared":true,"preparation":{"aim":[100,100],"requested_at":100},"operations":[{"at":101}]}),
        json!({"prepared":true,"operations":[{"at":100,"failure":"move"}]}),
        json!({"operations":[{"at":100,"failure":"press"}]}),
        json!({"prepared":true,"pointer":[100,100],"preparation":{"aim":[100,100],"requested_at":100},"operations":[{"at":100.05},{"at":100.04}]}),
        json!({"operations":[{"at":100,"expires":100.2}]}),
        json!({"prepared":true,"operations":[{"at":100,"clock":[100.1,100.25]}]}),
    ];
    let expected = support::legacy_expectation(
        "navigation-execution-oracle",
        &json!({"kind":"dispatch","cases":cases}),
    )?;
    for (g, case) in cases.iter().enumerate() {
        let script = Arc::new(Script::default());
        script.state().pointer = serde_json::from_value(case["pointer"].clone())?;
        let backend = FakeBackend(script.clone());
        let mut preparation: Option<PointerPreparation> = if case["preparation"].is_null() {
            None
        } else {
            Some(PointerPreparation {
                aim: serde_json::from_value(case["preparation"]["aim"].clone())?,
                requested_at: number(&case["preparation"]["requested_at"]),
            })
        };
        let mut diagnostics = DispatchDiagnostics::default();
        for (i, op) in case["operations"]
            .as_array()
            .ok_or("operations")?
            .iter()
            .enumerate()
        {
            {
                let mut s = script.state();
                s.operation = op.clone();
                s.counts.clear();
                s.trace.clear();
            }
            let mut verify = || Ok(script.next("verify", json!(true)) == true);
            let result = dispatch_planned_step(
                &backend,
                script.as_ref(),
                StepRequest {
                    aim: if op["aim"].is_null() {
                        [100, 100]
                    } else {
                        serde_json::from_value(op["aim"].clone())?
                    },
                    key: "f2",
                    captured_at: number(&op["at"]),
                    expires_at: (!op["expires"].is_null()).then(|| number(&op["expires"])),
                },
                DispatchOptions {
                    verify_aim: op.get("verify").map(|_| {
                        &mut verify as &mut dyn FnMut() -> Result<bool, d2bot::runs::RunError>
                    }),
                    preparation: if case["prepared"] == true {
                        Some(&mut preparation)
                    } else {
                        None
                    },
                },
                &mut diagnostics,
            );
            let (outcome, error) = match result {
                Ok(value) => (Some(value), None),
                Err(error) => (None, Some(error.to_string())),
            };
            let actual = json!({"outcome":outcome,"error":error,"diagnostics":diagnostics,"preparation":preparation,"trace":script.state().trace});
            compare(&actual, &expected[g][i], &format!("dispatch{g}.{i}"));
        }
    }
    Ok(())
}
struct Clock(f64);
impl MonotonicClock for Clock {
    fn now(&self) -> f64 {
        self.0
    }
}
struct Ground {
    reading: GroundReading,
    calls: Arc<Mutex<Vec<Option<f64>>>>,
}
impl GroundObserver for Ground {
    fn observe(
        &mut self,
        _: &Arc<BgrFrame>,
        at: Option<f64>,
    ) -> Result<GroundReading, ObservationError> {
        self.calls
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(at);
        Ok(self.reading.clone())
    }
}
#[test]
fn planner_preserves_current_ground_native_viewport_hub_boundary_and_stale_refusal()
-> Result<(), Box<dyn Error>> {
    let path = "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/navigation/return-portal-corner-short-goal.png";
    let cases = vec![
        json!({"path":path}),
        json!({"path":path,"wing":"south","automap":true,"provided":true}),
        json!({"path":path,"frontier":true}),
        json!({"path":path,"walking":true,"hub":[1182,580]}),
        json!({"path":path,"walking":true,"hub":[1183,580]}),
        json!({"path":path,"now":100.25}),
        json!({"path":path,"now":99.9}),
        json!({}),
        json!({"allow_no_aim":true}),
        json!({"ground":null}),
        json!({"loader_ground":true}),
        json!({"loader_ground":true,"class":"barbarian"}),
    ];
    let expected = support::legacy_expectation(
        "navigation-execution-oracle",
        &json!({"kind":"plan","cases":cases}),
    )?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (i, case) in cases.iter().enumerate() {
        let (mut loaded, mut path) = load_arcane_step_planner(
            root,
            case["automap"] == true,
            case["class"].as_str().unwrap_or("sorceress"),
        )?;
        if case["walking"] == true {
            path.set_teleport(None)?;
        }
        let frame = Arc::new(if let Some(path) = case["path"].as_str() {
            codec::load_color(root.join(path))?
        } else {
            BgrFrame::zeros(Size::new(1922, 1140)?)?
        });
        let point = if let Some(point) = case.get("ground") {
            serde_json::from_value(point.clone())?
        } else {
            Some([962, 580])
        };
        let reading = GroundReading {
            point,
            reason: if point.is_some() {
                "test_current_ground"
            } else {
                "ground_unverified"
            }
            .into(),
            score: None,
            calibration: None,
        };
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut ground = Ground {
            reading: reading.clone(),
            calls: calls.clone(),
        };
        let result = plan_arcane_step(
            &frame,
            serde_json::from_value(case.get("wing").cloned().unwrap_or(json!("north")))?,
            if case["loader_ground"] == true {
                loaded.as_mut()
            } else {
                &mut ground
            },
            &path,
            100.0,
            PlanOptions {
                hub_point: serde_json::from_value(case["hub"].clone())?,
                choose: ChooseOptions {
                    prefer_frontier: case["frontier"] == true,
                    ..Default::default()
                },
                allow_no_aim: case["allow_no_aim"] == true,
                ground_reading: (case["provided"] == true).then_some(&reading),
            },
            &Clock(case["now"].as_f64().unwrap_or(100.1)),
        );
        let (outcome, error) = match result {
            Ok(plan) => {
                let floor = plan.view.floor().to_owned()?;
                let masks = PathMotionMasks::new(&plan.view)?;
                let motion = masks.view();
                assert_eq!(
                    motion.floor.pixels(),
                    floor.pixels().iter().map(|v| *v != 0).collect::<Vec<_>>()
                );
                assert_eq!(motion.viewport, plan.view.context.viewport);
                assert_eq!(motion.captured_at, 100.0);
                let viewport = plan.view.context.viewport;
                (
                    Some(
                        json!({"choice":plan.choice,"ground":plan.ground,"viewport":[viewport.left(),viewport.top(),viewport.right(),viewport.bottom()],"anchor":plan.view.context.anchor,"size":[floor.size().width(),floor.size().height()],"floor":format!("{:x}",Sha256::digest(floor.pixels()))}),
                    ),
                    None,
                )
            }
            Err(error) => (None, Some(error.to_string())),
        };
        let actual = json!({"outcome":outcome,"error":error,"calls":*calls.lock().unwrap_or_else(|p|p.into_inner()),"original_settings":path.settings()});
        compare(&actual, &expected[i], &format!("plan{i}"));
    }
    Ok(())
}

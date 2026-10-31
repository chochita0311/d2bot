use d2bot::common::{
    error::ControlError,
    field_control::{DesktopPoint, FieldInputBackend},
    frame::{BgrFrame, BgrView, PixelRect, Size},
    hostile_probe::{NearbyHostileProbe, nearby_actor_proposals},
    time::MonotonicClock,
    vision_ops::codec,
};
use serde_json::{Value, json};
use std::{
    error::Error,
    path::Path,
    sync::{Arc, Mutex},
};
mod support;
#[test]
fn narrower_probe_keeps_current_confirmation_and_rejects_outside_candidates()
-> Result<(), Box<dyn Error>> {
    use d2bot::common::hostile_probe::nearby_actor_proposals_with_radius;
    let frame = image(
        &json!({"rectangles":[[1150,570,1190,590,[0,255,0]],[1215,570,1255,590,[0,255,0]]]}),
    )?;
    let wide = nearby_actor_proposals(frame.view(), Some([960.0, 580.0]))?;
    let narrow =
        nearby_actor_proposals_with_radius(frame.view(), Some([960.0, 580.0]), [240.0, 144.0])?;
    assert_eq!(wide.len(), 2);
    assert_eq!(narrow.len(), 1);
    assert!(
        nearby_actor_proposals_with_radius(frame.view(), Some([960.0, 580.0]), [f64::NAN, 144.0])
            .is_err()
    );
    let shared = Arc::new(Shared(Mutex::new(State {
        now: 10.0,
        op: Value::Null,
        clock_index: 0,
        pointer: None,
        events: vec![],
    })));
    let mut probe = NearbyHostileProbe::new(
        |_: BgrView<'_>| -> Result<Option<u8>, ControlError> { Ok(Some(1)) },
        Arc::new(Backend(shared.clone())),
        [0, 0],
        shared.clone(),
    )
    .with_radius([240.0, 144.0])?;
    assert_eq!(probe.radius(), [240.0, 144.0]);
    assert!(
        probe
            .observe::<u8, ControlError>(
                frame.view(),
                Some([960.0, 580.0]),
                10.0,
                Some(&[[1201, 500]])
            )?
            .is_none()
    );
    assert!(!probe.has_pending());
    shared.state().now = 10.6;
    assert!(
        probe
            .observe::<u8, ControlError>(
                frame.view(),
                Some([960.0, 580.0]),
                10.6,
                Some(&[[1200, 500]])
            )?
            .is_none()
    );
    assert!(probe.has_pending());
    shared.state().now = 10.7;
    assert!(
        probe
            .observe::<u8, ControlError>(
                frame.view(),
                Some([959.0, 580.0]),
                10.7,
                Some(&[[1200, 500]])
            )?
            .is_none()
    );
    assert!(!probe.has_pending());
    Ok(())
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
fn image(case: &Value) -> Result<BgrFrame, Box<dyn Error>> {
    if let Some(path) = case["path"].as_str() {
        return Ok(codec::load_color(
            Path::new(env!("CARGO_MANIFEST_DIR")).join(path),
        )?);
    }
    let [w, h] = if case["size"].is_null() {
        [1922, 1140]
    } else {
        serde_json::from_value(case["size"].clone())?
    };
    let mut frame = BgrFrame::zeros(Size::new(w, h)?)?;
    for rect in case["rectangles"].as_array().into_iter().flatten() {
        let l = rect[0].as_u64().ok_or("left")? as usize;
        let t = rect[1].as_u64().ok_or("top")? as usize;
        let r = rect[2].as_u64().ok_or("right")? as usize;
        let b = rect[3].as_u64().ok_or("bottom")? as usize;
        let color: [u8; 3] = serde_json::from_value(rect[4].clone())?;
        PixelRect::new(l, t, r, b)?.checked_in(frame.size())?;
        for y in t..b {
            for x in l..r {
                frame.pixels_mut()[(y * w + x) * 3..(y * w + x + 1) * 3].copy_from_slice(&color);
            }
        }
    }
    Ok(frame)
}
struct State {
    now: f64,
    op: Value,
    clock_index: usize,
    pointer: Option<DesktopPoint>,
    events: Vec<Value>,
}
struct Shared(Mutex<State>);
impl Shared {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }
}
impl MonotonicClock for Shared {
    fn now(&self) -> f64 {
        let mut s = self.state();
        if let Some(values) = s.op["clock"].as_array().filter(|a| !a.is_empty()) {
            s.now = values[s.clock_index.min(values.len() - 1)]
                .as_f64()
                .unwrap_or(f64::NAN);
            s.clock_index += 1;
        }
        let now = s.now;
        s.events.push(json!(["clock", now]));
        now
    }
}
struct Backend(Arc<Shared>);
impl FieldInputBackend for Backend {
    fn key_down(&self, _: &str) -> Result<(), ControlError> {
        Err(ControlError::Runtime("probe attempted key down".into()))
    }
    fn key_up(&self, _: &str) -> Result<(), ControlError> {
        Err(ControlError::Runtime("probe attempted key up".into()))
    }
    fn press(&self, _: &str) -> Result<(), ControlError> {
        Err(ControlError::Runtime("probe attempted key press".into()))
    }
    fn move_pointer(&self, point: DesktopPoint) -> Result<(), ControlError> {
        let mut s = self.0.state();
        s.events.push(json!(["move", point[0], point[1]]));
        if s.op["move_error"] == true {
            return Err(ControlError::Backend("move failed".into()));
        }
        s.pointer = Some(point);
        s.now += s.op["move_delay"].as_f64().unwrap_or(0.0);
        Ok(())
    }
    fn pointer_matches(&self, point: DesktopPoint) -> Result<bool, ControlError> {
        let mut s = self.0.state();
        let matches = s.op["pointer_matches"]
            .as_bool()
            .unwrap_or(s.pointer == Some(point));
        s.events.push(json!(["pointer_matches", point, matches]));
        Ok(matches)
    }
}
fn exercise(cases: Vec<Value>) -> Result<(), Box<dyn Error>> {
    let expected = support::legacy_expectation("hostile-probe-oracle", &json!(cases))?;
    for (i, case) in cases.iter().enumerate() {
        let frame = image(case)?;
        let actual = if case["kind"] == "proposals" {
            json!(nearby_actor_proposals(
                frame.view(),
                serde_json::from_value(case["player"].clone())?
            )?)
        } else {
            let shared = Arc::new(Shared(Mutex::new(State {
                now: 10.0,
                op: Value::Null,
                clock_index: 0,
                pointer: None,
                events: vec![],
            })));
            let observed = shared.clone();
            let observer = move |_view: BgrView<'_>| -> Result<Option<String>, ControlError> {
                let mut s = observed.state();
                s.events.push(json!(["observe"]));
                s.now += s.op["observer_delay"].as_f64().unwrap_or(0.0);
                if s.op["observer_error"] == true {
                    return Err(ControlError::Backend("observer failed".into()));
                }
                Ok((s.op["target"] != false).then(|| "current hostile".into()))
            };
            let origin: DesktopPoint = if case["origin"].is_null() {
                [0, 0]
            } else {
                serde_json::from_value(case["origin"].clone())?
            };
            let mut probe = NearbyHostileProbe::new(
                observer,
                Arc::new(Backend(shared.clone())),
                origin,
                shared.clone(),
            );
            if !case["scale"].is_null() {
                let scale: [f64; 2] = serde_json::from_value(case["scale"].clone())?;
                probe = probe.with_mapper(move |point| {
                    Ok(std::array::from_fn(|i| {
                        (origin[i] as f64 + f64::from(point[i]) * scale[i]).round_ties_even() as i64
                    }))
                });
            }
            let mut rows = vec![];
            for op in case["operations"].as_array().ok_or("operations")? {
                {
                    let mut s = shared.state();
                    s.now = op["now"].as_f64().unwrap_or(10.0);
                    s.op = op.clone();
                    s.clock_index = 0;
                    s.events.clear();
                }
                let outcome = if op["reset"] == true {
                    probe.reset();
                    Ok(None)
                } else {
                    let points: Option<Vec<[i32; 2]>> =
                        serde_json::from_value(op["points"].clone())?;
                    let player = if let Some(value) = op.get("player") {
                        serde_json::from_value(value.clone())?
                    } else {
                        Some([962.0, 580.0])
                    };
                    let acquired = op["at"]
                        .as_f64()
                        .unwrap_or(op["now"].as_f64().unwrap_or(10.0));
                    probe.observe(frame.view(), player, acquired, points.as_deref())
                };
                let (outcome, error) = match outcome {
                    Ok(value) => (value, None),
                    Err(error) => (None, Some(error.to_string())),
                };
                rows.push(json!({"outcome":outcome,"error":error,"state":probe.snapshot(),"events":shared.state().events}));
            }
            json!(rows)
        };
        compare(&actual, &expected[i], &format!("case{i}"));
    }
    Ok(())
}
#[test]
fn current_color_candidates_preserve_filters_rounding_order_and_eight_point_cap()
-> Result<(), Box<dyn Error>> {
    let green = json!([0, 220, 0]);
    let mut rectangles = vec![];
    for y in [470, 580, 690] {
        for x in [782, 902, 1022, 1142] {
            rectangles.push(json!([x - 15, y - 4, x + 16, y + 5, green]));
        }
    }
    let mut cases = vec![
        json!({"kind":"proposals","player":null}),
        json!({"kind":"proposals","player":[962,580]}),
        json!({"kind":"proposals","player":[962,580],"rectangles":rectangles}),
        json!({"kind":"proposals","player":[962,580],"rectangles":[[950,570,975,590,green],[1102,595,1142,605,green],[1430,595,1470,605,green]]}),
    ];
    for (w, h) in [
        (19, 5),
        (20, 4),
        (20, 5),
        (180, 5),
        (181, 5),
        (20, 100),
        (20, 101),
    ] {
        cases.push(json!({"kind":"proposals","player":[962,580],"rectangles":[[1050,550,1050+w,550+h,green]]}));
    }
    for color in [
        json!([0, 79, 0]),
        json!([0, 80, 0]),
        json!([0, 220, 220]),
        json!([220, 0, 0]),
        json!([100, 180, 100]),
    ] {
        cases.push(
            json!({"kind":"proposals","player":[962,580],"rectangles":[[1100,570,1140,580,color]]}),
        );
    }
    cases.push(json!({"kind":"proposals","size":[200,140],"player":[100,70],"rectangles":[[150,60,180,70,green]]}));
    cases.push(json!({"kind":"proposals","player":[962,580],"path":"assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/navigation/return-portal-corner-short-goal.png"}));
    exercise(cases)
}
#[test]
fn probe_requires_current_near_panel_pointer_settle_and_original_deadline()
-> Result<(), Box<dyn Error>> {
    let mut cases = vec![
        json!({"kind":"probe","origin":[100,200],"operations":[{"now":10},{"now":10.1},{"now":10.2,"target":false},{"now":10.3,"player":null},{"now":10.4},{"now":10.41},{"now":10.6}]}),
        json!({"kind":"probe","operations":[{"now":10,"points":[[1122,520]]},{"now":10.1,"points":[[1122,520]],"target":false},{"now":10.51,"points":[[1122,520]]},{"now":10.61,"points":[[1122,520]]},{"now":10.62,"points":[[1122,520]]},{"now":10.72,"points":[]}]}),
        json!({"kind":"probe","size":[200,140],"origin":[-200,100],"scale":[0.5,2],"operations":[{"now":10,"player":[100,70],"points":[[70,60]]},{"now":10.06,"player":[100,70]}]}),
        json!({"kind":"probe","operations":[{"now":10,"move_error":true},{"now":10.1},{"now":10.2,"observer_error":true},{"reset":true},{"now":10.3}]}),
    ];
    for change in [
        json!({"target":false}),
        json!({"pointer_matches":false}),
        json!({"player":[500,580]}),
        json!({"points":[]}),
        json!({"points":[[1240,520]]}),
        json!({"points":[[1450,520]]}),
        json!({"observer_delay":0.3}),
        json!({"at":9.8}),
        json!({"at":10.2}),
    ] {
        let mut changed = json!({"now":10.1,"points":[[1154,429]]});
        for (k, v) in change.as_object().ok_or("change")? {
            changed[k] = v.clone();
        }
        cases.push(json!({"kind":"probe","origin":[100,200],"operations":[{"now":10,"points":[[1154,429]]},changed]}));
    }
    for at in [10.049, 10.05, 10.5, 10.501] {
        cases.push(json!({"kind":"probe","operations":[{"now":10,"points":[[1122,520]]},{"now":at,"points":[[1122,520]]}]}));
    }
    let ops: Vec<_> = (0..14)
        .map(|i| json!({"now":10.0+i as f64*0.1,"target":false}))
        .collect();
    cases.push(json!({"kind":"probe","operations":ops}));
    cases.push(json!({"kind":"probe","operations":[{"now":10,"clock":[10.0,10.25]}]}));
    cases.push(json!({"kind":"probe","operations":[{"now":10,"move_delay":0.08},{"now":10.1},{"now":10.13}]}));
    exercise(cases)
}

use d2bot::{
    common::frame::{GrayFrame, PixelRect, Size},
    runs::summoner::{
        navigation::{
            ArcanePathObserver, PathChoice, PathContext, PathSettings, PathView, TeleportGeometry,
            Wing,
        },
        steering::{ArcaneRouteFlow, ContinuousSteering, RouteFlowOptions},
    },
};
use serde_json::{Value, json};
use std::error::Error;
mod support;
fn compare(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert!(
            (a.as_f64().unwrap_or(f64::NAN) - b.as_f64().unwrap_or(f64::NAN)).abs() <= 1e-9,
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
fn exercise(groups: Vec<Value>) -> Result<(), Box<dyn Error>> {
    let expected = support::legacy_expectation("steering-oracle", &json!(groups))?;
    for (g, group) in groups.iter().enumerate() {
        let path = ArcanePathObserver::new(
            PathSettings::new(8, 0.5, 96.0, 120.0, 35.0, 90.0, 12)?,
            None,
            Some(TeleportGeometry::new(
                600.0,
                32.0,
                group["preferred"].as_f64(),
            )?),
            None,
        )?;
        let mut flow = ArcaneRouteFlow::new(serde_json::from_value(group["detour"].clone())?);
        let mut steering =
            ContinuousSteering::new(0.32, 32.0, serde_json::from_value(group["aim"].clone())?)?;
        for (i, op) in group["operations"]
            .as_array()
            .ok_or("operations")?
            .iter()
            .enumerate()
        {
            let hex = expected[g][i]["floor"].as_str().ok_or("floor")?;
            let pixels = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
                .collect::<Result<Vec<_>, _>>()?;
            let view = PathView::from_grid(
                PathContext {
                    sequence: 1,
                    generation: 0,
                    captured_at: op["at"].as_f64().ok_or("at")?,
                    frame_size: Size::new(1600, 800)?,
                    viewport: PixelRect::new(0, 0, 1600, 800)?,
                    anchor: [800.0, 400.0],
                },
                8,
                GrayFrame::from_vec(Size::new(200, 100)?, pixels)?,
            )?;
            let choice = PathChoice {
                aim: serde_json::from_value(op["aim"].clone())?,
                reason: op["reason"].as_str().unwrap_or("floor").into(),
                path: Vec::new(),
            };
            let now = op["now"].as_f64().unwrap_or(op["at"].as_f64().ok_or("at")?);
            let (outcome, state) = if group["kind"] == "steering" {
                (
                    serde_json::to_value(steering.update(&view, &choice, now)?)?,
                    serde_json::to_value(&steering)?,
                )
            } else {
                let visited: Vec<[f64; 2]> = if op["visited"].is_null() {
                    Vec::new()
                } else {
                    serde_json::from_value(op["visited"].clone())?
                };
                let excluded: Vec<[f64; 4]> = if op["excluded"].is_null() {
                    Vec::new()
                } else {
                    serde_json::from_value(op["excluded"].clone())?
                };
                (
                    serde_json::to_value(flow.choose(
                        &path,
                        &view,
                        if op["wing"].is_null() {
                            Wing::North
                        } else {
                            serde_json::from_value(op["wing"].clone())?
                        },
                        &choice,
                        now,
                        RouteFlowOptions {
                            position: if op["position"].is_null() {
                                [0.0, 0.0]
                            } else {
                                serde_json::from_value(op["position"].clone())?
                            },
                            visited: &visited,
                            excluded: &excluded,
                        },
                    )?)?,
                    serde_json::to_value(&flow)?,
                )
            };
            compare(
                &outcome,
                &expected[g][i]["outcome"],
                &format!("group{g}.op{i}.outcome"),
            );
            compare(
                &state,
                &expected[g][i]["state"],
                &format!("group{g}.op{i}.state"),
            );
        }
    }
    Ok(())
}
#[test]
fn continuous_steering_preserves_new_frame_turn_and_opposite_recheck_rules()
-> Result<(), Box<dyn Error>> {
    exercise(vec![
        json!({"kind":"steering","operations":[{"at":1.0,"aim":[1200,200]},{"at":1.08,"aim":[400,200],"closed":true}]}),
        json!({"kind":"steering","operations":[{"at":1.0,"aim":[1200,200]},{"at":1.08,"aim":[400,200]},{"at":1.16,"aim":[1200,200]}]}),
        json!({"kind":"steering","operations":[{"at":1.0,"aim":[1200,200]},{"at":1.08,"aim":[400,600],"closed":true},{"at":1.08,"now":1.1,"aim":[400,600],"closed":true},{"at":1.16,"aim":[400,600],"closed":true}]}),
        json!({"kind":"steering","operations":[{"at":1.0,"aim":[1200,200]},{"at":1.08,"aim":[400,200]},{"at":1.16,"aim":[420,210]},{"at":1.16,"aim":[1200,200]},{"at":1.02,"now":1.16,"aim":[1200,200]},{"at":1.48,"aim":[420,210]}]}),
        json!({"kind":"steering","aim":[1200,200],"operations":[{"at":0.0},{"at":1.0,"now":1.5,"aim":[1200,200]},{"at":1.1,"now":1.6,"aim":[1200,200]}]}),
    ])
}
#[test]
fn route_flow_keeps_current_detours_and_observed_corner_probes() -> Result<(), Box<dyn Error>> {
    exercise(vec![
        json!({"kind":"flow","detour":[-1,-0.5],"operations":[{"at":1.0,"aim":[400,200]},{"at":1.08,"aim":[1200,600]}]}),
        json!({"kind":"flow","detour":[-1,-0.5],"operations":[{"at":1.0,"aim":[1200,200],"corner":true},{"at":1.0,"now":1.08,"aim":[1200,200],"corner":true},{"at":1.16,"aim":[1200,200],"corner":true}]}),
        json!({"kind":"flow","detour":[-400,-200],"operations":[{"at":1.0,"aim":[1200,200]},{"at":1.16,"aim":[1200,200]},{"at":1.2,"now":1.45,"aim":[1200,200]}]}),
        json!({"kind":"flow","operations":[{"at":1.0,"aim":[1394,353]},{"at":1.16,"now":1.1,"aim":[1394,353]}]}),
        json!({"kind":"flow","preferred":600,"operations":[{"at":1.0,"aim":[300,650]},{"at":1.16,"aim":[1200,650]}]}),
        json!({"kind":"flow","detour":[-400,-200],"operations":[{"at":1.0,"aim":[1200,600],"blank":true}]}),
        json!({"kind":"flow","preferred":600,"detour":[-400,-200],"operations":[{"at":1.0,"aim":[1250,600],"lines":[[800,400,300,650,120],[800,400,1300,650,120]]}]}),
        json!({"kind":"flow","preferred":600,"operations":[{"at":1.0,"aim":[300,650],"lines":[[800,400,300,650,120],[800,400,300,150,120]]}]}),
        json!({"kind":"flow","preferred":600,"detour":[-1,-0.5],"operations":[{"at":1.0,"aim":[1250,600],"lines":[[800,400,650,475,100],[800,400,1300,650,100]]}]}),
    ])
}

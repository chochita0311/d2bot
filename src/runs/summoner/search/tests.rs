use super::*;
mod runner;
use crate::{
    common::{
        frame::{GrayFrame, PixelRect, Size},
        scene_motion::MotionReason,
    },
    test_field_world::compare,
};
use serde_json::{Value, json};
use std::error::Error;
type TestResult = Result<(), Box<dyn Error>>;
fn observer() -> Result<ArcanePathObserver, RunError> {
    Ok(ArcanePathObserver::new(
        super::super::navigation::PathSettings::new(4, 0.5, 20.0, 32.0, 40.0, 8.0, 8)?,
        None,
        None,
        None,
    )?)
}
fn limits() -> Value {
    json!({"screen_age":0.5,"landing_seconds":2.0,"wing_seconds":20.0,"return_seconds":20.0,"total_seconds":100.0,"minimum_movement":4.0,"breadcrumb_radius":8.0,"maximum_casts":100,"maximum_hover_attempts":2,"settled_frames":2,"settled_motion_pixels":2.0})
}
fn snapshot(search: &ArcaneSearch) -> Value {
    json!({"phase":search.phase,"wing":search.wing(),"position":search.position,"breadcrumbs":search.breadcrumbs,"return_points":search.return_points,"checked_wings":search.checked_wings,"expected_generation":search.expected_generation,"pending":search.pending,"cursor":search.cursor,"landing":search.landing,"hover_attempts":search.hover_attempts,"cast_count":search.cast_count,"serial":search.serial,"landing_stable":search.landing_stable,"review_reason":search.review_reason,"interval":search.interval,"stage_at":search.stage_at,"last_now":search.last_now,"last_view":search.last_view.as_ref().map(|v|json!({"sequence":v.context.sequence,"generation":v.context.generation,"captured_at":v.context.captured_at,"frame_size":[v.context.frame_size.width(),v.context.frame_size.height()],"viewport":[v.context.viewport.left(),v.context.viewport.top(),v.context.viewport.right(),v.context.viewport.bottom()]}))})
}
fn exercise(case: &Value) -> Result<Value, Box<dyn Error>> {
    let mut policy = limits();
    if let Some(values) = case["limits"].as_object() {
        policy
            .as_object_mut()
            .ok_or("limits")?
            .extend(values.clone());
    }
    let search = ArcaneSearch::new(
        observer()?,
        serde_json::from_value(policy)?,
        case["reference"].as_f64().unwrap_or(0.32),
        0.0,
    );
    let mut search = match search {
        Ok(v) => v,
        Err(e) => return Ok(json!({"error":{"kind":e.kind(),"message":e.message()}})),
    };
    let mut now = 0.0;
    let mut sequence = 0;
    let mut last = None;
    let mut rows = vec![];
    for op in case["ops"].as_array().ok_or("ops")? {
        now = op["now"]
            .as_f64()
            .unwrap_or(now + op["dt"].as_f64().unwrap_or(0.0));
        let result: Value = match op["op"].as_str().unwrap_or("step") {
            "step" => {
                sequence = op["sequence"].as_u64().unwrap_or(sequence + 1);
                let [w, h]: [usize; 2] = if op["size"].is_null() {
                    [200, 200]
                } else {
                    serde_json::from_value(op["size"].clone())?
                };
                let size = Size::new(w, h)?;
                let [l, t, r, b]: [usize; 4] = if op["viewport"].is_null() {
                    [0, 0, w, h]
                } else {
                    serde_json::from_value(op["viewport"].clone())?
                };
                let mask = GrayFrame::from_vec(size, vec![u8::from(op["empty"] != true); w * h])?;
                let view = search.observer.from_mask(
                    mask.view(),
                    super::super::navigation::PathContext {
                        sequence,
                        generation: op["generation"]
                            .as_u64()
                            .unwrap_or(search.expected_generation.unwrap_or(0)),
                        captured_at: op["captured"].as_f64().unwrap_or(now),
                        frame_size: size,
                        viewport: PixelRect::new(l, t, r, b)?,
                        anchor: [100.0, 100.0],
                    },
                )?;
                let observation = SearchObservation {
                    view,
                    motion: CameraMotion {
                        displacement: if op.get("motion").is_some() {
                            serde_json::from_value(op["motion"].clone())?
                        } else {
                            Some([0.0; 2])
                        },
                        inliers: 20,
                        reason: MotionReason::BackgroundTranslation,
                        matches: 20,
                        inlier_ratio: None,
                        linear_error: None,
                    },
                    movement_allowed: op["allowed"].as_bool().unwrap_or(true),
                    terminal: serde_json::from_value(op["terminal"].clone())?,
                    hub_ready: op["hub"].as_bool().unwrap_or(false),
                    hover: serde_json::from_value(op["hover"].clone())?,
                };
                let action = search.step(
                    &observation,
                    now,
                    op["interrupted"] == true,
                    op["focused"].as_bool().unwrap_or(true),
                )?;
                last = Some(action.clone());
                serde_json::to_value(action)?
            }
            "tick" => {
                let action = search.tick(
                    now,
                    op["interrupted"] == true,
                    op["focused"].as_bool().unwrap_or(true),
                )?;
                last = Some(action.clone());
                serde_json::to_value(action)?
            }
            "ack" => {
                let mut request = if op["pending"] == true {
                    search.pending.clone()
                } else {
                    last.clone()
                }
                .ok_or("last request")?;
                if op["wrong"] == true {
                    request.request_id = request.request_id.map(|id| id + 1);
                }
                json!(search.acknowledge(&request, now, op["success"].as_bool().unwrap_or(true))?)
            }
            "defer" => json!(search.defer(last.as_ref().ok_or("last request")?)),
            _ => {
                let action = search.fail(op["reason"].as_str().ok_or("reason")?);
                last = Some(action.clone());
                serde_json::to_value(action)?
            }
        };
        rows.push(json!({"result":result,"state":snapshot(&search)}));
    }
    Ok(json!(rows))
}
fn cast(ops: &mut Vec<Value>, at: &mut f64) {
    ops.extend([json!({"now":*at}), json!({"op":"ack"})]);
    *at += 0.05;
    ops.extend([json!({"now":*at}), json!({"op":"ack"})]);
}
#[test]
fn retained_search_state_matches_source_guards_requests_landing_and_wing_transitions() -> TestResult
{
    let mut groups = vec![];
    let base = vec![
        json!({}),
        json!({"op":"ack"}),
        json!({"now":0.05}),
        json!({"op":"ack"}),
    ];
    for suffix in [
        vec![
            json!({"now":0.15,"generation":0}),
            json!({"motion":[20,-10]}),
            json!({"dt":0.3}),
            json!({"dt":0.05}),
        ],
        vec![
            json!({"now":0.2,"motion":null}),
            json!({"now":0.6,"motion":null}),
            json!({"now":1.5,"motion":null}),
            json!({"now":2.05}),
            json!({"dt":0.1}),
        ],
        vec![
            json!({"dt":0.4,"motion":[20,-10]}),
            json!({"dt":0.05}),
            json!({"dt":0.05,"motion":[4,-2]}),
            json!({"dt":0.05}),
            json!({"dt":0.05}),
        ],
        vec![
            json!({"dt":0.1,"generation":9}),
            json!({"dt":0.1,"terminal":"summoner_present"}),
        ],
    ] {
        let mut ops = base.clone();
        ops.extend(suffix);
        groups.push(json!({"ops":ops}));
    }
    let hover = json!({"kind":"chest","bounds":[120,60,160,100]});
    groups.extend([
        json!({"ops":[{"hover":hover},{"op":"ack"},{"hover":hover},{"hover":hover},{"hover":hover}]}),
        json!({"ops":[{"terminal":"summoner_present"},{"op":"tick","interrupted":true}]}),
        json!({"ops":[{"allowed":false},{},{},{"op":"ack","pending":true},{"dt":0.1,"allowed":false},{"dt":0.1}]}),
        json!({"ops":[{},{"op":"ack"},{"dt":0.05,"motion":null},{"op":"ack"},{"dt":0.05},{"op":"ack"},{"dt":0.1,"motion":[20,-10]},{"dt":0.3,"motion":null},{"dt":0.05},{"dt":0.05}]}),
        json!({"ops":[{},{"op":"tick","now":0.5}]}),
        json!({"ops":[{},{"op":"ack","now":0.5}]}),
        json!({"ops":[{},{"op":"ack","success":false},{"op":"fail","reason":"later"}]}),
        json!({"ops":[{},{"op":"ack","wrong":true},{"op":"defer"},{"dt":0.1}]}),
        json!({"ops":[{"now":1,"captured":1.01},{"captured":0.49},{"captured":0.5},{"op":"defer"},{"sequence":1},{"sequence":5,"captured":0.49}]}),
        json!({"ops":[{},{"op":"defer"},{"dt":0.1,"size":[220,200]}]}),
        json!({"limits":{"total_seconds":1},"ops":[{"op":"tick","now":1}]}),
        json!({"ops":[{"op":"tick","now":20}]}),
        json!({"ops":[{"terminal":"nonboss_end"},{"op":"tick","now":20}]}),
        json!({"ops":[{"now":1},{"op":"defer"},{"op":"tick","now":0.9}]}),
        json!({"ops":[{"empty":true}]}),
        json!({"ops":[{"terminal":"nonboss_end","hub":true},{"terminal":"nonboss_end","hub":true},{"terminal":"nonboss_end","hub":true},{"terminal":"nonboss_end","hub":true}]}),
        json!({"ops":[{"focused":false}]}),json!({"ops":[{"interrupted":true}]}),
    ]);
    let mut ops = vec![];
    let mut at = 0.0;
    for _ in 0..4 {
        cast(&mut ops, &mut at);
        at += 0.4;
        ops.push(json!({"now":at,"motion":[20,-10],"terminal":"nonboss_end"}));
        for _ in 0..2 {
            at += 0.05;
            ops.push(json!({"now":at,"terminal":"nonboss_end"}));
        }
        ops.push(json!({"op":"ack"}));
        at += 0.05;
        ops.extend([json!({"now":at}), json!({"op":"ack"})]);
        at += 0.4;
        ops.push(json!({"now":at,"motion":[-20,10]}));
        for _ in 0..2 {
            at += 0.05;
            ops.push(json!({"now":at,"hub":true}));
        }
        at += 0.05;
    }
    groups.push(json!({"ops":ops}));
    let mut ops = base;
    ops.extend([
        json!({"dt":0.4,"motion":[20,-10]}),
        json!({"dt":0.05}),
        json!({"dt":0.05}),
    ]);
    groups.push(json!({"limits":{"maximum_casts":1},"ops":ops}));
    for key in [
        "screen_age",
        "landing_seconds",
        "wing_seconds",
        "return_seconds",
        "total_seconds",
        "minimum_movement",
        "breadcrumb_radius",
        "settled_motion_pixels",
    ] {
        let mut values = serde_json::Map::new();
        values.insert(key.into(), json!(0));
        groups.push(json!({"limits":values,"ops":[]}));
    }
    groups.extend([
        json!({"reference":0,"ops":[]}),
        json!({"reference":2,"ops":[]}),
        json!({"limits":{"settled_frames":1},"ops":[]}),
        json!({"limits":{"maximum_casts":0},"ops":[]}),
        json!({"limits":{"maximum_hover_attempts":0},"ops":[]}),
    ]);
    let expected = crate::test_support::legacy_expectation("search-oracle", &json!(groups))?;
    for (index, case) in groups.iter().enumerate() {
        compare(
            &exercise(case)?,
            &expected[index],
            &format!("search case {index}"),
        );
    }
    Ok(())
}

use super::*;

#[test]
fn full_place_memory_does_not_reuse_a_chart_coordinate_system()
-> Result<(), Box<dyn std::error::Error>> {
    let mut graph = ArcaneRouteGraph::new(2, 4, 120.0, 300.0)?;
    graph.observe(None, [962.0, 580.0], None, 0.0)?;
    for step in 1..4 {
        graph.observe(
            None,
            [962.0, 580.0],
            Some([200.0, 0.0]),
            f64::from(step) * 0.1,
        )?;
    }
    assert_eq!(graph.places().len(), 4);
    for (at, expected_chart) in [(1.0, 1), (2.0, 2), (3.0, 3)] {
        graph.observe(None, [962.0, 580.0], None, at)?;
        graph.observe(None, [962.0, 580.0], None, at + 0.7)?;
        assert_eq!(graph.summary()["chart"], expected_chart);
        assert_eq!(graph.places().len(), 4);
        assert!(graph.current_place.is_none());
    }
    // 과거 chart 재대응 뒤 새 chart도 이미 사용한 번호로 돌아가지 않는다.
    graph.chart = 0;
    graph.observe(None, [962.0, 580.0], None, 4.0)?;
    graph.observe(None, [962.0, 580.0], None, 4.7)?;
    assert_eq!(graph.chart, 4);
    graph.next_chart = usize::MAX;
    let position = graph.position;
    graph.observe(None, [962.0, 580.0], None, 5.0)?;
    assert!(graph.observe(None, [962.0, 580.0], None, 5.7).is_err());
    assert_eq!(graph.chart, 4);
    assert_eq!(graph.position, position);
    assert_eq!(graph.next_chart, usize::MAX);
    assert_eq!(graph.places().len(), 4);
    Ok(())
}
use crate::common::{
    frame::{GrayFrame, PixelRect, Size},
    scene_motion::SceneFeatures,
};
use crate::runs::summoner::navigation::{PathContext, PathSettings, TeleportGeometry};
use std::error::Error;
fn compare(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert!(
            (a.as_f64().unwrap_or(f64::NAN) - b.as_f64().unwrap_or(f64::NAN)).abs() < 1e-5,
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
            for (key, a) in a {
                compare(a, &b[key], &format!("{path}.{key}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
fn size(value: &Value) -> Result<Size, Box<dyn Error>> {
    let [w, h]: [usize; 2] = serde_json::from_value(value.clone())?;
    Ok(Size::new(w, h)?)
}
fn counters(value: &Value) -> Result<BTreeMap<ExitKey, usize>, Box<dyn Error>> {
    let pairs: Vec<(ExitKey, usize)> = serde_json::from_value(value.clone())?;
    Ok(pairs.into_iter().collect())
}
fn apply_manual_changes(
    graph: &mut ArcaneRouteGraph,
    changes: &Value,
) -> Result<(), Box<dyn Error>> {
    for (key, value) in changes.as_object().ok_or("manual changes")? {
        match key.as_str() {
            "entry" => graph.entry = serde_json::from_value(value.clone())?,
            "exit_intent" => graph.exit_intent = serde_json::from_value(value.clone())?,
            "pending_exit" => graph.pending_exit = serde_json::from_value(value.clone())?,
            "pending_retreat" => graph.pending_retreat = serde_json::from_value(value.clone())?,
            "traversed" => graph.traversed = counters(value)?,
            "failed_exits" => graph.failed_exits = counters(value)?,
            "escape_retreats" => graph.escape_retreats = serde_json::from_value(value.clone())?,
            "places" => graph.places = serde_json::from_value(value.clone())?,
            "place_charts" => graph.place_charts = serde_json::from_value(value.clone())?,
            _ => return Err(format!("unexpected manual field {key}").into()),
        }
    }
    Ok(())
}
#[test]
fn original_graph_cases_compare_state_and_choices_after_each_operation()
-> Result<(), Box<dyn Error>> {
    let trace = crate::test_support::legacy_expectation("route-graph-oracle", &Value::Null)?;
    let mut graphs = BTreeMap::new();
    let events = trace["events"].as_array().ok_or("events")?;
    for (index, event) in events.iter().enumerate() {
        let id = event["id"].as_u64().ok_or("graph id")?;
        if event["kind"] == "new" {
            let options = &event["options"];
            graphs.insert(
                id,
                ArcaneRouteGraph::new(
                    options["maximum_references"].as_u64().unwrap_or(12) as usize,
                    options["maximum_places"].as_u64().unwrap_or(96) as usize,
                    options["place_radius"].as_f64().unwrap_or(120.0),
                    options["reference_spacing"].as_f64().unwrap_or(300.0),
                )?,
            );
            continue;
        }
        let graph = graphs.get_mut(&id).ok_or("unknown graph id")?;
        apply_manual_changes(graph, &event["patch"])?;
        let actual = match event["kind"].as_str().ok_or("event kind")? {
            "observe" => {
                let raw = &event["features"];
                let features = if raw.is_null() {
                    None
                } else {
                    Some(Arc::new(SceneFeatures::from_native(
                        raw["sequence"].as_u64().ok_or("sequence")?,
                        raw["captured_at"].as_f64().ok_or("captured_at")?,
                        size(&raw["frame_size"])?,
                        serde_json::from_value(raw["points"].clone())?,
                        serde_json::from_value(raw["descriptors"].clone())?,
                    )?))
                };
                json!(graph.observe(
                    features.as_ref(),
                    serde_json::from_value(event["anchor"].clone())?,
                    serde_json::from_value(event["displacement"].clone())?,
                    event["at"].as_f64().ok_or("at")?
                )?)
            }
            "fail" => {
                graph.fail_landing(
                    serde_json::from_value(event["aim"].clone())?,
                    serde_json::from_value(event["anchor"].clone())?,
                )?;
                Value::Null
            }
            "choose" => {
                let path = ArcanePathObserver::new(
                    PathSettings::from_value(&event["settings"])?,
                    None,
                    Some(TeleportGeometry::from_value(&event["teleport"])?),
                    None,
                )?;
                let raw = &event["view"];
                let hex = raw["floor"].as_str().ok_or("floor")?;
                let pixels = (0..hex.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
                    .collect::<Result<Vec<_>, _>>()?;
                let [l, t, r, b]: [usize; 4] = serde_json::from_value(raw["viewport"].clone())?;
                let view = PathView::from_grid(
                    PathContext {
                        sequence: raw["sequence"].as_u64().ok_or("view sequence")?,
                        generation: raw["generation"].as_u64().ok_or("generation")?,
                        captured_at: raw["captured_at"].as_f64().ok_or("view time")?,
                        frame_size: size(&raw["frame_size"])?,
                        viewport: PixelRect::new(l, t, r, b)?,
                        anchor: serde_json::from_value(raw["anchor"].clone())?,
                    },
                    raw["cell_pixels"].as_u64().ok_or("cell")? as usize,
                    GrayFrame::from_vec(size(&raw["size"])?, pixels)?,
                )?;
                let excluded: Vec<[f64; 4]> = serde_json::from_value(event["excluded"].clone())?;
                let landings = event["landings"]
                    .as_array()
                    .ok_or("landings")?
                    .iter()
                    .map(|raw| -> Result<_, Box<dyn Error>> {
                        Ok(TerrainLanding {
                            kind: raw["kind"].as_str().ok_or("landing kind")?.into(),
                            point: serde_json::from_value(raw["point"].clone())?,
                            score: raw["score"].as_f64().ok_or("landing score")?,
                            evidence_box: [0; 4],
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                serde_json::to_value(graph.choose(
                    &path,
                    &view,
                    serde_json::from_value(event["wing"].clone())?,
                    event["now"].as_f64().ok_or("now")?,
                    GraphChooseOptions {
                        excluded: &excluded,
                        landings: &landings,
                        hub: serde_json::from_value(event["hub"].clone())?,
                        prefer_goal_exit: event["prefer_goal_exit"] == true,
                    },
                )?)?
            }
            kind => return Err(format!("unknown operation {kind}").into()),
        };
        compare(&actual, &event["outcome"], &format!("event{index}.outcome"));
        compare(
            &graph.summary(),
            &event["summary"],
            &format!("event{index}.summary"),
        );
    }
    assert!(trace["cases"].as_u64().ok_or("cases")? >= 25);
    eprintln!(
        "original graph cases={}, graphs={}, operations={}",
        trace["cases"],
        trace["graphs"],
        events.len()
    );
    Ok(())
}
#[test]
fn graph_features_are_validated_and_reference_ownership_is_bounded() -> Result<(), Box<dyn Error>> {
    let size = Size::new(1922, 1140)?;
    assert!(SceneFeatures::from_native(1, f64::NAN, size, vec![], None).is_err());
    assert!(SceneFeatures::from_native(1, 1.0, size, vec![[0.0, 0.0]], Some(vec![])).is_err());
    assert!(SceneFeatures::from_native(1, 1.0, size, vec![[f32::INFINITY, 0.0]], None).is_err());
    let mut graph = ArcaneRouteGraph::new(2, 4, 120.0, 300.0)?;
    let mut weak = Vec::new();
    for sequence in 1..=12 {
        let features = Arc::new(SceneFeatures::from_native(
            sequence,
            sequence as f64,
            size,
            vec![[1.0, 2.0]; 24],
            Some(vec![[sequence as u8; 32]; 24]),
        )?);
        weak.push(Arc::downgrade(&features));
        graph.observe(
            Some(&features),
            [960.0, 580.0],
            Some([400.0, -200.0]),
            sequence as f64,
        )?;
    }
    assert!(weak[..10].iter().all(|item| item.upgrade().is_none()));
    assert!(weak[10..].iter().all(|item| item.upgrade().is_some()));
    assert_eq!(graph.references.len(), 2);
    assert_eq!(graph.places.len(), 4);
    assert_eq!(graph.current_place, None);
    assert_eq!(graph.summary()["feature_bytes"], 24 * 40 * 2);
    drop(graph);
    assert!(weak.iter().all(|item| item.upgrade().is_none()));
    Ok(())
}

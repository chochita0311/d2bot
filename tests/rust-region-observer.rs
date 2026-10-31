use d2bot::common::{
    encounter_vision::{RegionObserver, ScanOptions, TerrainOptions},
    encounters::SceneContext,
    frame::{PixelRect, Size},
    survival::ObservationStamp,
    vision_ops::codec,
};
use serde_json::{Value, json};
use std::{error::Error, path::Path, sync::Arc};
mod support;
fn rect(raw: &Value) -> Result<PixelRect, Box<dyn Error>> {
    let [l, t, r, b]: [usize; 4] = serde_json::from_value(raw.clone())?;
    Ok(PixelRect::new(l, t, r, b)?)
}
#[test]
fn region_replay_preserves_candidates_parallel_order_and_current_terrain()
-> Result<(), Box<dyn Error>> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
    let manifest: Value = serde_json::from_slice(&std::fs::read(directory.join("manifest.json"))?)?;
    let observer = RegionObserver::load(directory.join("manifest.json"))?;
    let mut cases = Vec::new();
    for id in [
        "alive",
        "alive-followup",
        "corpses",
        "corpses-followup",
        "corpse-ground",
        "shrine",
        "stairs",
    ] {
        for workers in [1, 2] {
            cases.push(json!({"id":id,"workers":workers}));
        }
    }
    cases.push(
        json!({"id":"terrain","terrain":[[1300,340,1360,385],[550,500,610,560],[920,395,985,550]]}),
    );
    cases.push(json!({"id":"terrain","now":12,"terrain":[[1300,340,1360,385]]}));
    let expected =
        support::legacy_expectation("observer-oracle", &json!({"kind":"region","cases":cases}))?;
    for (index, case) in cases.iter().enumerate() {
        let reference = manifest["cases"]
            .as_array()
            .ok_or("cases")?
            .iter()
            .find(|r| r["id"] == case["id"])
            .ok_or("reference")?;
        let frame = codec::load_color(directory.join(reference["path"].as_str().ok_or("path")?))?;
        let context = Arc::new(SceneContext::new(
            ObservationStamp::new("character", "room", 10, 10.0)?,
            observer.region_id(),
            0,
            Size::new(1922, 1140)?,
            rect(&reference["viewport"])?,
        )?);
        let now = case["now"].as_f64().unwrap_or(10.0);
        let options = ScanOptions {
            now,
            max_age: 1.0,
            limit_per_template: 3,
            workers: case["workers"].as_u64().unwrap_or(1) as usize,
        };
        let hits = observer.scan(frame.view(), Arc::clone(&context), options)?;
        let wanted = expected[index]["hits"].as_array().ok_or("hits")?;
        assert_eq!(hits.len(), wanted.len(), "case {index}");
        for (hit, expected) in hits.iter().zip(wanted) {
            let actual = serde_json::to_value(hit)?;
            for key in ["asset_id", "kind", "category", "appearance", "box"] {
                assert_eq!(actual[key], expected[key], "case {index} {key}");
            }
            assert!(
                (hit.score() - expected["score"].as_f64().ok_or("score")?).abs() <= 2e-5,
                "case {index}"
            );
            assert_eq!(
                hit.ground(),
                serde_json::from_value::<Option<[f64; 2]>>(expected["ground"].clone())?,
                "case {index}"
            );
            assert_eq!(hit.context().as_ref(), context.as_ref());
            for key in ["region_id", "movement_generation", "frame_size", "viewport"] {
                assert_eq!(
                    actual["context"][key], expected["context"][key],
                    "case {index} context {key}"
                );
            }
        }
        if let Some(boxes) = case["terrain"].as_array() {
            for (i, bounds) in boxes.iter().enumerate() {
                let bounds = rect(bounds)?;
                let actual = observer.terrain(
                    frame.view(),
                    &context,
                    bounds,
                    TerrainOptions {
                        now,
                        max_age: 1.0,
                        minimum_floor: 0.75,
                        maximum_void: 0.15,
                    },
                )?;
                let wanted = &expected[index]["terrain"][i];
                assert_eq!(
                    actual.state,
                    wanted["state"].as_str().ok_or("state")?,
                    "terrain case {index} box {i}"
                );
                let pixels = (bounds.width() * bounds.height()) as f64;
                for (key, ratio) in [
                    ("floor_ratio", actual.floor_ratio),
                    ("void_ratio", actual.void_ratio),
                ] {
                    let expected = wanted[key].as_f64().ok_or("ratio")?;
                    assert_eq!(
                        (ratio * pixels).round_ties_even(),
                        (expected * pixels).round_ties_even(),
                        "terrain case {index} box {i} {key}: {ratio} != {expected}"
                    );
                }
            }
        }
    }
    Ok(())
}

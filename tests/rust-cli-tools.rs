#[path = "support/directory.rs"]
mod directory;
mod support;
use d2bot::{
    common::{
        config::BotConfig,
        frame::{BgrFrame, Size},
        vision_ops::codec,
    },
    tools::{arcane_navigation_replay, field_replay, survival_observe},
};
use serde_json::{Value, json};
use std::{error::Error, ffi::OsString, path::Path};
type TestResult = Result<(), Box<dyn Error>>;
fn assert_values(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert!(
            (a.as_f64().unwrap_or(f64::NAN) - b.as_f64().unwrap_or(f64::NAN)).abs() <= 1e-6,
            "{path}: {a} / {b}"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                assert_values(a, b, &format!("{path}/{i}"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}: {a:?} / {b:?}");
            for (k, b) in b {
                assert_values(&actual[k], b, &format!("{path}/{k}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
fn scenario() -> Value {
    let mut events = vec![
        json!({"at":0,"sequence":0,"type":"belt","contents":{"1":["purple_potion",4],"2":["purple_potion",4],"3":["purple_potion",4],"4":["town_portal_scroll",4]}}),
        json!({"at":0,"sequence":1,"type":"buff","name":"battle_orders","location":"field","monsters_clear":true}),
    ];
    for (sequence, at, enemy, loot) in [
        (2, 1.0, "clear", false),
        (3, 1.1, "near", false),
        (4, 1.2, "clear", false),
        (5, 1.3, "clear", true),
    ] {
        events.push(json!({"at":at,"sequence":sequence,"type":"observation","field":{"safety":{"life_ratio":1,"mana_ratio":1,"location":"field","monsters_clear":enemy=="clear"},"belt_closed":true,"enemy":enemy,"travel_aim":[600,400],"reachable":true,"loot_checked":loot}}));
    }
    events.push(json!({"at":2,"type":"tick"}));
    json!({"evidence":"synthetic","limits":{"screen_age":0.5,"belt_age":1000,"combat_seconds":3,"arrival_seconds":2,"potion_confirmation_seconds":1},"events":events})
}
#[test]
fn synthetic_tool_preserves_source_trace_and_rejects_wrong_evidence() -> TestResult {
    let config = BotConfig::load("config")?;
    let base = scenario();
    let mut cases = vec![base.clone()];
    for key in ["evidence", "events", "limits"] {
        let mut bad = base.clone();
        bad[key] = Value::Null;
        cases.push(bad);
    }
    let mut bad = base.clone();
    bad["events"][3]["at"] = json!(0);
    cases.push(bad);
    let mut bad = base.clone();
    bad["events"][3]["field"]["belt_closed"] = json!("true");
    cases.push(bad);
    let mut bad = base.clone();
    bad["events"][3]["field"]["safety"]["life_ratio"] = json!(true);
    cases.push(bad);
    let mut bad = base.clone();
    bad["events"][3]["field"]["extra"] = json!(1);
    cases.push(bad);
    let request = cases
        .iter()
        .map(|c| json!({"op":"field","scenario":c}))
        .collect::<Vec<_>>();
    let expected = support::legacy_expectation("tools-oracle", &json!(request))?;
    for (case, expected) in cases.iter().zip(expected.as_array().ok_or("oracle array")?) {
        match field_replay::replay(&config, "flash", "demo", case) {
            Ok(actual) => {
                assert_eq!(expected["ok"], true);
                assert_values(&actual, &expected["value"], "replay");
            }
            Err(error) => assert_eq!(expected["ok"], false, "{error}: {expected}"),
        }
    }
    let directory = directory::TestDirectory::new()?;
    let path = directory.path().join("scenario.json");
    std::fs::write(&path, serde_json::to_vec(&base)?)?;
    let mut output = Vec::new();
    assert_eq!(
        field_replay::run(
            [
                OsString::from("--character=flash"),
                "--room=demo".into(),
                "--scenario".into(),
                path.into_os_string()
            ],
            &mut output
        )?,
        0
    );
    assert_values(
        &serde_json::from_slice(&output)?,
        &expected[0]["value"],
        "cli",
    );
    Ok(())
}
fn trim_timing(value: &mut Value) {
    if let Some(reports) = value.get_mut("reports").and_then(Value::as_array_mut) {
        for report in reports {
            if let Some(map) = report.as_object_mut() {
                for key in [
                    "capture_started_at",
                    "acquired_at",
                    "processed_at",
                    "capture_seconds",
                    "processing_seconds",
                ] {
                    map.remove(key);
                }
            }
        }
    }
    if let Some(results) = value.get_mut("results").and_then(Value::as_array_mut) {
        for result in results {
            if let Some(map) = result.as_object_mut() {
                map.remove("vision_seconds");
            }
        }
    }
}
#[test]
fn offline_observation_reports_source_policy_and_parser_errors() -> TestResult {
    let directory = directory::TestDirectory::new()?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let assets = root.join("assets/ui/survival");
    let manifest: Value = serde_json::from_slice(&std::fs::read(assets.join("manifest.json"))?)?;
    let reference = &manifest["references"][0];
    let [w, h] = [
        manifest["frame_size"][0].as_u64().ok_or("width")? as usize,
        manifest["frame_size"][1].as_u64().ok_or("height")? as usize,
    ];
    let crop = codec::load_color(assets.join(reference["path"].as_str().ok_or("reference path")?))?;
    let x = reference["canvas_box"][0].as_u64().ok_or("x")? as usize;
    let y = reference["canvas_box"][1].as_u64().ok_or("y")? as usize;
    let mut frame = BgrFrame::zeros(Size::new(w, h)?)?;
    for row in 0..crop.size().height() {
        let start = ((y + row) * w + x) * 3;
        let src = row * crop.size().row_bytes();
        frame.pixels_mut()[start..start + crop.size().row_bytes()]
            .copy_from_slice(&crop.pixels()[src..src + crop.size().row_bytes()]);
    }
    let path = directory.path().join("hud.png");
    codec::save_png(&path, &frame)?;
    let output_path = directory.path().join("report.json");
    let valid = vec![
        "--image".to_owned(),
        path.to_string_lossy().into_owned(),
        "--character=flash".into(),
        "--room=demo".into(),
        "--age-limit=60".into(),
    ];
    let mut cases = [
        valid.clone(),
        vec!["--frames=6".into()],
        vec!["--image=x".into(), "--live".into()],
        vec!["--image=x".into(), "--character=flash".into()],
        vec![
            "--image=x".into(),
            "--character=flash".into(),
            "--room=r".into(),
            "--age-limit=NaN".into(),
        ],
    ];
    cases[0].extend([
        "--output".into(),
        output_path.to_string_lossy().into_owned(),
    ]);
    let requests = cases
        .iter()
        .map(|a| json!({"op":"survival","arguments":a}))
        .collect::<Vec<_>>();
    let expected = support::legacy_expectation("tools-oracle", &json!(requests))?;
    for (case, expected) in cases.iter().zip(expected.as_array().ok_or("oracle list")?) {
        let mut output = Vec::new();
        match survival_observe::run(case.iter().map(OsString::from), &mut output) {
            Ok(code) => {
                assert_eq!(expected["ok"], true);
                assert_eq!(code, 0);
                let mut actual: Value = serde_json::from_slice(&output)?;
                let mut wanted = expected["value"].clone();
                for report in actual["reports"].as_array().ok_or("reports")? {
                    assert!(
                        report["processing_seconds"]
                            .as_f64()
                            .is_some_and(|n| n >= 0.0)
                    );
                    assert!(report["processed_at"].as_f64() >= report["acquired_at"].as_f64());
                }
                assert_values(
                    &serde_json::from_slice(&std::fs::read(&output_path)?)?,
                    &actual,
                    "saved report",
                );
                trim_timing(&mut actual);
                trim_timing(&mut wanted);
                assert_values(&actual, &wanted, "observation");
            }
            Err(error) => assert_eq!(expected["ok"], false, "{error}"),
        }
    }
    Ok(())
}
#[test]
fn navigation_tool_preserves_crops_choices_resets_and_overlay_pixels() -> TestResult {
    let directory = directory::TestDirectory::new()?;
    let path = directory.path();
    let source = BgrFrame::from_vec(Size::new(200, 200)?, [111, 109, 109].repeat(200 * 200))?;
    codec::save_png(path.join("source.png"), &source)?;
    let calibration = json!({"mode":"input_free_calibration","frame_size":[200,200],"anchor":[100,100],"wing":"north","path":{"cell_pixels":4,"floor_fraction":0.5,"seed_radius":20,"maximum_step":48,"turn_degrees":40,"revisit_radius":16,"minimum_component_cells":8},"motion":{"minimum_matches":12,"minimum_inlier_ratio":0.7,"maximum_error":2},"terrain":{"floor_bgr":[[111,109,109]],"void_bgr":[[0,0,0]],"floor_lab_distance":18,"void_lab_distance":16}});
    let cases=(0..3).map(|n|json!({"id":n,"path":"source.png","viewport":[0,0,200,200],"provenance":{"episode":if n==2 {"two"}else{"one"},"seconds_from_record_start":if n==1 {0.1}else{0.0}}})).collect::<Vec<_>>();
    let manifest = path.join("manifest.json");
    std::fs::write(
        &manifest,
        serde_json::to_vec(&json!({"revision":"synthetic","frame_size":[200,200],"cases":cases}))?,
    )?;
    let rust_overlays = path.join("rust-overlays");
    let python_overlays = path.join("python-overlays");
    let mut actual =
        arcane_navigation_replay::replay(&manifest, &calibration, Some(&rust_overlays))?;
    let expected = support::legacy_expectation(
        "tools-oracle",
        &json!([{"op":"navigation","manifest":manifest,"calibration":calibration,"overlays":python_overlays,"rust_overlays":rust_overlays}]),
    )?;
    assert_eq!(expected[0]["ok"], true);
    assert_eq!(expected[0]["overlays_equal"], json!([true, true, true]));
    let mut wanted = expected[0]["value"].clone();
    trim_timing(&mut actual);
    trim_timing(&mut wanted);
    assert_values(&actual, &wanted, "navigation");
    assert!(
        arcane_navigation_replay::replay(&manifest, &calibration, Some(&rust_overlays)).is_err()
    );
    let mut bad = calibration;
    bad["mode"] = json!("live");
    assert!(arcane_navigation_replay::replay(&manifest, &bad, None).is_err());
    Ok(())
}

#[path = "support/directory.rs"]
mod directory;
mod support;
use d2bot::{
    actions::town_loop::TownOptions,
    common::{
        frame::{BgrFrame, Size},
        vision_ops::codec,
    },
    tools::town_loop_vision,
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, error::Error, path::Path};
type TestResult = Result<(), Box<dyn Error>>;
fn copy_bank(source: &Path, target: &Path) -> TestResult {
    std::fs::create_dir_all(target)?;
    let bytes = std::fs::read(source.join("manifest.json"))?;
    let manifest: Value = serde_json::from_slice(&bytes)?;
    std::fs::write(target.join("manifest.json"), bytes)?;
    let mut paths = BTreeSet::new();
    for definition in manifest["templates"]
        .as_object()
        .ok_or("templates")?
        .values()
    {
        paths.insert(definition["file"].as_str().ok_or("file")?.to_owned());
        if let Some(variants) = definition["variants"].as_array() {
            for v in variants {
                paths.insert(v.as_str().ok_or("variant")?.to_owned());
            }
        }
    }
    for path in paths {
        let parent = target
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .ok_or("assets owner")?;
        let source_path = source.join(&path).canonicalize()?;
        let relative = source_path.strip_prefix(
            source
                .parent()
                .and_then(Path::parent)
                .and_then(Path::parent)
                .ok_or("source assets")?
                .canonicalize()?,
        )?;
        let destination = parent.join(relative);
        std::fs::create_dir_all(destination.parent().ok_or("parent")?)?;
        // 정규화된 assets 상대 경로만 사용해 형제 자산도 fixture 소유자 안에 복사한다.
        if !destination.starts_with(parent) {
            return Err("copy escaped fixture assets".into());
        }
        std::fs::copy(source_path, destination)?;
    }
    Ok(())
}
#[test]
fn baseline_and_reviewed_scene_calibration_preserve_pixels_and_hash_ownership() -> TestResult {
    let directory = directory::TestDirectory::new()?;
    let plain_path = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(directory.path().file_name().ok_or("fixture name")?);
    let path = plain_path.as_path();
    let evidence = path.join("evidence");
    let frames = evidence.join("frames/raw");
    std::fs::create_dir_all(&frames)?;
    let size = Size::new(1267, 753)?;
    let frame = BgrFrame::from_vec(
        size,
        (0..size.byte_len()?)
            .map(|n| ((n * 37 + n / 11) % 256) as u8)
            .collect(),
    )?;
    for filename in [
        "01-character-select.png",
        "02-difficulty.png",
        "03-act1-spawn.png",
        "04-act1-map.png",
        "09-act2-town.png",
        "07-waypoint-act1.png",
        "08-waypoint-act2.png",
        "05-act1-waypoint-world.png",
        "11-save-exit-menu.png",
    ] {
        codec::save_png(frames.join(filename), &frame)?;
    }
    let rust = path.join("rust-baseline");
    let python = path.join("python-baseline");
    town_loop_vision::calibrate(&evidence, &rust)?;
    let result = support::legacy_expectation(
        "tools-oracle",
        &json!([{"op":"town-calibrate","evidence":evidence,"directory":python,"rust_directory":rust}]),
    )?;
    assert_eq!(
        result[0],
        json!({"ok":true,"metadata_equal":true,"pixels_equal":true,"hashes_valid":true,"templates":13})
    );
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = root.join(town_loop_vision::DIRECTORY);
    let rust = path.join("rust-scene/assets/ui/town-loop/remastered-ko-window");
    let python = path.join("python-scene/assets/ui/town-loop/remastered-ko-window");
    copy_bank(&source, &rust)?;
    copy_bank(&source, &python)?;
    let manifest: Value = serde_json::from_slice(&std::fs::read(source.join("manifest.json"))?)?;
    let template = codec::load_gray(
        source.join(
            manifest["templates"]["play"]["file"]
                .as_str()
                .ok_or("play")?,
        ),
    )?;
    let mut frame = BgrFrame::zeros(size)?;
    for y in 0..template.size().height() {
        for x in 0..template.size().width() {
            let n = ((665 + y) * 1267 + 472 + x) * 3;
            frame.pixels_mut()[n..n + 3].fill(template.pixels()[y * template.size().width() + x]);
        }
    }
    let frame_path = path.join("character.png");
    codec::save_png(&frame_path, &frame)?;
    town_loop_vision::calibrate_scene(root, &frame_path, "character", &rust)?;
    let result = support::legacy_expectation(
        "tools-oracle",
        &json!([{"op":"town-scene","frame":frame_path,"scene":"character","directory":python,"rust_directory":rust}]),
    )?;
    assert_eq!(result[0]["ok"], true, "{}", result[0]);
    assert_eq!(result[0]["metadata_equal"], true, "{}", result[0]);
    assert_eq!(result[0]["pixels_equal"], true);
    assert_eq!(result[0]["hashes_valid"], true);
    assert!(town_loop_vision::calibrate_scene(root, &frame_path, "bad", &rust).is_err());
    Ok(())
}
#[test]
fn file_bridge_keeps_state_across_calls_and_uses_source_clock_basis() -> TestResult {
    let directory = directory::TestDirectory::new()?;
    let path = directory.path();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let frame = path.join("blank.png");
    codec::save_png(&frame, &BgrFrame::zeros(Size::new(1267, 753)?)?)?;
    let rust_state = path.join("rust-state.json");
    let python_state = path.join("python-state.json");
    for now in [1_234_000.0, 1_234_000.1] {
        let actual =
            town_loop_vision::step_bridge(root, &frame, &rust_state, TownOptions::default(), now)?;
        let expected = support::legacy_expectation(
            "tools-oracle",
            &json!([{"op":"town-step","now":now,"arguments":["--frame",frame,"--state",python_state]}]),
        )?;
        assert_eq!(expected[0]["ok"], true);
        let mut actual_normalized = actual.clone();
        let mut expected_normalized = expected[0]["value"].clone();
        normalize_numbers(&mut actual_normalized);
        normalize_numbers(&mut expected_normalized);
        assert_eq!(actual_normalized, expected_normalized);
        let saved: Value = serde_json::from_slice(&std::fs::read(&rust_state)?)?;
        assert_eq!(saved, actual["state"]);
        // 원본 도구가 기록한 직전 상태를 복원한다. Rust 결과를 정답으로 쓰지 않는다.
        std::fs::write(
            &python_state,
            serde_json::to_vec(&expected[0]["value"]["state"])?,
        )?;
    }
    Ok(())
}
fn normalize_numbers(value: &mut Value) {
    match value {
        Value::Number(n) => {
            if let Some(number) = n.as_f64() {
                *value = json!(number);
            }
        }
        Value::Array(a) => {
            for v in a {
                normalize_numbers(v);
            }
        }
        Value::Object(o) => {
            for v in o.values_mut() {
                normalize_numbers(v);
            }
        }
        _ => {}
    }
}

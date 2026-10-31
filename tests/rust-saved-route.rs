use d2bot::{
    common::{
        frame::{BgrFrame, Size},
        vision_ops::codec,
    },
    runs::summoner::continuation::restore_route_flow,
};
use serde_json::{Value, json};
use std::error::Error;
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
struct SavedFixture {
    root: std::path::PathBuf,
    path: std::path::PathBuf,
    files: Vec<std::path::PathBuf>,
}
impl SavedFixture {
    fn new() -> Result<Self, Box<dyn Error>> {
        let root = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
        std::fs::create_dir_all(&root)?;
        let root = root.canonicalize()?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let path = root.join(format!("rust-saved-route-{}-{stamp}", std::process::id()));
        std::fs::create_dir(&path)?;
        Ok(Self {
            root,
            path,
            files: vec![],
        })
    }
    fn write(&mut self, name: &str, bytes: &[u8]) -> Result<std::path::PathBuf, Box<dyn Error>> {
        let path = self.path.join(name);
        if path.parent() != Some(self.path.as_path()) {
            return Err("fixture file outside its owner".into());
        }
        if !self.files.contains(&path) {
            self.files.push(path.clone());
        }
        std::fs::write(&path, bytes)?;
        Ok(path)
    }
}
impl Drop for SavedFixture {
    fn drop(&mut self) {
        // oracle 종료 뒤 이 검사가 만든 정확한 파일과 빈 소유 폴더만 제거한다.
        if self.path.parent() != Some(self.root.as_path())
            || !self
                .path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("rust-saved-route-"))
        {
            return;
        }
        for path in self.files.iter().rev() {
            if path.parent() == Some(self.path.as_path())
                && let Err(error) = std::fs::remove_file(path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                eprintln!("saved route fixture cleanup failed: {error}");
            }
        }
        if let Err(error) = std::fs::remove_dir(&self.path) {
            eprintln!("saved route fixture directory cleanup failed: {error}");
        }
    }
}
#[test]
fn saved_route_restores_only_detour_after_current_native_correspondence()
-> Result<(), Box<dyn Error>> {
    let mut owner = SavedFixture::new()?;
    let size = Size::new(1922, 1140)?;
    let mut random = 12u32;
    let frame = BgrFrame::from_vec(
        size,
        (0..1922 * 1140 * 3)
            .map(|_| {
                random = random.wrapping_mul(1664525).wrapping_add(1013904223);
                ((random >> 24) % 200) as u8
            })
            .collect(),
    )?;
    let original = owner.write("continuous-0001-after.png", &codec::encode_png(&frame)?)?;
    let mut shifted = BgrFrame::zeros(size)?;
    for y in 42..1140 {
        for x in 0..1922 - 86 {
            shifted.pixels_mut()[(y * 1922 + x) * 3..(y * 1922 + x + 1) * 3].copy_from_slice(
                &frame.pixels()[((y - 42) * 1922 + x + 86) * 3..((y - 42) * 1922 + x + 87) * 3],
            );
        }
    }
    let shifted_file = owner.write("pending.png", &codec::encode_png(&shifted)?)?;
    let blank = owner.write("blank.png", &codec::encode_png(&BgrFrame::zeros(size)?)?)?;
    let small = owner.write(
        "small.png",
        &codec::encode_png(&BgrFrame::zeros(Size::new(20, 20)?)?)?,
    )?;
    let base = json!({"continuous":true,"wing":"north","return_from":"south","navigation":{"route_detour":[-400,-200],"position":[99999,99999],"continuous_frames":[{"ground":{"point":[960,580]}}]}});
    let mut variants = vec![
        (base.clone(), original.clone(), "north", "south", 600.0),
        (base.clone(), shifted_file.clone(), "north", "south", 600.0),
        (base.clone(), shifted_file.clone(), "north", "south", 50.0),
        (base.clone(), blank, "north", "south", 600.0),
        (base.clone(), small, "north", "south", 600.0),
        (base.clone(), original.clone(), "east", "south", 600.0),
    ];
    for detour in [
        Value::Null,
        json!([0, 0]),
        json!([true, 1]),
        json!([1]),
        json!("north"),
    ] {
        let mut p = base.clone();
        p["navigation"]["route_detour"] = detour;
        variants.push((p, original.clone(), "north", "south", 600.0));
    }
    let mut missing = base.clone();
    missing["navigation"]
        .as_object_mut()
        .ok_or("navigation")?
        .remove("route_detour");
    variants.push((missing, original.clone(), "north", "south", 600.0));
    for value in [
        json!("../pending.png"),
        json!("sub/pending.png"),
        json!("missing.png"),
        json!(true),
        json!(false),
    ] {
        let mut p = base.clone();
        p["navigation"]["final_frame"] = value;
        variants.push((p, original.clone(), "north", "south", 600.0));
    }
    let mut overridden = base.clone();
    overridden["navigation"]["wing"] = json!("north");
    overridden["navigation"]["return_from"] = json!("north");
    variants.push((
        overridden.clone(),
        original.clone(),
        "north",
        "north",
        600.0,
    ));
    variants.push((overridden, original.clone(), "north", "south", 600.0));
    for change in [
        json!({"continuous":false}),
        json!({"continuous":1}),
        json!({"navigation":{"route_detour":null,"continuous_frames":[]}}),
    ] {
        let mut p = base.clone();
        for (k, v) in change.as_object().ok_or("change")? {
            p[k] = v.clone();
        }
        variants.push((p, original.clone(), "north", "south", 600.0));
    }
    let mut cases = vec![];
    for (i, (report, frame, wing, from, maximum)) in variants.iter().enumerate() {
        let path = owner.write(&format!("report-{i}.json"), &serde_json::to_vec(report)?)?;
        cases.push(json!({"kind":"saved","report":path,"frame":frame,"wing":wing,"return_from":from,"maximum":maximum}));
    }
    // 원본의 보고서와 참조/현재 이미지까지 비교 입력에 포함한다.
    for case in &mut cases {
        let report = case["report"].clone();
        case["paths"] = json!([
            report,
            original,
            "tests/fixtures/rust-port/motion-baseline.json"
        ]);
    }
    let expected = support::legacy_expectation("continuation-oracle", &json!(cases))?;
    for (i, case) in cases.iter().enumerate() {
        let frame = codec::load_color(case["frame"].as_str().ok_or("frame")?)?;
        let result = restore_route_flow(
            case["report"].as_str().ok_or("report")?,
            &frame,
            serde_json::from_value(case["wing"].clone())?,
            serde_json::from_value(case["return_from"].clone())?,
            case["maximum"].as_f64().ok_or("maximum")?,
        );
        let (outcome, error) = match result {
            Ok(value) => (Some(value), None),
            Err(error) => (None, Some(error.to_string())),
        };
        compare(
            &json!({"outcome":outcome,"error":error}),
            &expected[i],
            &format!("saved{i}"),
        );
    }
    assert!(!expected[0]["outcome"].is_null());
    assert!(expected[0]["outcome"].get("position").is_none());
    assert!(
        expected[1]["outcome"]["motion"]["inliers"]
            .as_u64()
            .ok_or("inliers")?
            >= 24
    );
    Ok(())
}

use d2bot::{
    common::{
        buffing::BeltVisibility,
        error::ObservationError,
        frame::{BgrFrame, GrayFrame, Image, PixelRect, Size},
        survival_vision::{HudReading, ResourceReading},
        vision_ops::{codec, color, resize},
    },
    runs::summoner::{
        camera_ground::{
            CalibratedCameraGroundObserver, CameraLimits, CameraViewportProof, GroundObserver,
            ReviewedCameraLayout,
        },
        scene_vision::{
            ArcaneHoverObserver, ArcaneHubObserver, ArcaneTargetObserver, FlashGroundObserver,
            GroundOptions, GroundReading, GroundTracking, HubStabilityTracker,
        },
        terrain_patterns::ArcaneTerrainPatterns,
    },
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
mod support;
fn image_arg<const C: usize>(image: &Image<u8, C>) -> Value {
    json!({"width":image.size().width(),"height":image.size().height(),"pixels":image.pixels().iter().map(|v|format!("{v:02x}")).collect::<String>()})
}
fn compare(actual: &Value, expected: &Value, path: &str) -> Result<(), Box<dyn Error>> {
    match (actual, expected) {
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}[{i}]"))?;
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (key, a) in a {
                compare(
                    a,
                    b.get(key).ok_or("missing key")?,
                    &format!("{path}.{key}"),
                )?;
            }
        }
        (Value::Number(a), Value::Number(b)) if path.ends_with("score") => assert!(
            (a.as_f64().ok_or("score")? - b.as_f64().ok_or("score")?).abs() <= 2e-5,
            "{path}: {a} != {b}"
        ),
        (Value::Number(a), Value::Number(b)) => assert_eq!(a.as_f64(), b.as_f64(), "{path}"),
        _ => assert_eq!(actual, expected, "{path}"),
    }
    Ok(())
}
#[test]
fn explicit_half_scale_keeps_odd_size_and_pixels() -> Result<(), Box<dyn Error>> {
    let mut cases = Vec::new();
    let mut gray = Vec::new();
    let mut color = Vec::new();
    let mut random = 73_u32;
    for (w, h) in [(32, 40), (125, 78), (1876, 845), (7, 11), (2, 3)] {
        let size = Size::new(w, h)?;
        let mut next = || {
            random = random.wrapping_mul(1664525).wrapping_add(1013904223);
            (random >> 24) as u8
        };
        let g = GrayFrame::from_vec(size, (0..w * h).map(|_| next()).collect())?;
        let c = BgrFrame::from_vec(size, (0..w * h * 3).map(|_| next()).collect())?;
        cases.push(json!({"frame":image_arg(&g),"channels":1}));
        cases.push(json!({"frame":image_arg(&c),"channels":3}));
        gray.push(g);
        color.push(c);
    }
    let expected =
        support::legacy_expectation("observer-oracle", &json!({"kind":"half","cases":cases}))?;
    for (i, (g, c)) in gray.iter().zip(&color).enumerate() {
        let check =
            |actual_size: Size, pixels: &[u8], index: usize| -> Result<(), Box<dyn Error>> {
                for mode in 0..2 {
                    assert_eq!(
                        json!([actual_size.width(), actual_size.height()]),
                        expected[index][mode]["size"]
                    );
                    assert_eq!(
                        format!("{:x}", Sha256::digest(pixels)),
                        expected[index][mode]["sha256"].as_str().ok_or("hash")?,
                        "case {index} mode {mode}"
                    );
                }
                Ok(())
            };
        let g = resize::half(g.view())?;
        check(g.size(), g.pixels(), i * 2)?;
        let c = resize::half(c.view())?;
        check(c.size(), c.pixels(), i * 2 + 1)?;
    }
    Ok(())
}
#[test]
fn terrain_candidates_translate_and_do_not_approve_blank_floor() -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let owner = root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
    let path = "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/navigation/stair-cap-two-descents.png";
    let cases = json!([{"frame":{"path":path}},{"frame":{"path":path,"shift":[120,-60]}},{"frame":{"path":path,"clear":[[0,0,1922,1140]]}}]);
    let expected =
        support::legacy_expectation("observer-oracle", &json!({"kind":"terrain","cases":cases}))?;
    let observer = ArcaneTerrainPatterns::load(owner.join("navigation-patterns.json"))?;
    let native = codec::load_color(root.join(path))?;
    let mut shifted = BgrFrame::zeros(native.size())?;
    for y in 60..1140 {
        let source = native.view().row(y).ok_or("row")?;
        shifted.pixels_mut()[((y - 60) * 1922 + 120) * 3..((y - 60) * 1922 + 1922) * 3]
            .copy_from_slice(&source[..(1922 - 120) * 3]);
    }
    let blank = BgrFrame::zeros(native.size())?;
    for (i, frame) in [&native, &shifted, &blank].into_iter().enumerate() {
        compare(
            &serde_json::to_value(observer.observe(frame.view())?)?,
            &expected[i],
            &format!("terrain {i}"),
        )?;
    }
    assert_eq!(expected[2], json!([]));
    assert!(
        observer
            .observe(BgrFrame::zeros(Size::new(1267, 753)?)?.view())
            .is_err()
    );
    Ok(())
}
fn fill(frame: &mut BgrFrame, [l, t, r, b]: [usize; 4], color: [u8; 3]) {
    let w = frame.size().width();
    for y in t..b {
        for x in l..r {
            frame.pixels_mut()[(y * w + x) * 3..(y * w + x + 1) * 3].copy_from_slice(&color);
        }
    }
}
#[test]
fn target_needs_one_red_panel_and_current_bright_title() -> Result<(), Box<dyn Error>> {
    let owner = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
    let scenarios = ["blank", "red", "ghoul", "two_panels"];
    let cases: Vec<_> = scenarios
        .iter()
        .map(|name| json!({"frame":{"target_scene":name}}))
        .collect();
    let expected =
        support::legacy_expectation("observer-oracle", &json!({"kind":"target","cases":cases}))?;
    let observer = ArcaneTargetObserver::load(&owner)?;
    let image = codec::load_color(owner.join("templates/encounters/ghoul-name.png"))?;
    let title = color::gray(image.view().crop(PixelRect::new(27, 9, 138, 35)?)?)?;
    for (i, scenario) in scenarios.into_iter().enumerate() {
        let mut frame = BgrFrame::zeros(Size::new(1922, 1140)?)?;
        if scenario != "blank" {
            fill(&mut frame, [880, 76, 1050, 116], [0, 0, 67]);
        }
        if matches!(scenario, "ghoul" | "two_panels") {
            for y in 0..26 {
                for (x, v) in title.view().row(y).ok_or("title row")?.iter().enumerate() {
                    frame.pixels_mut()
                        [((y + 83) * 1922 + x + 906) * 3..((y + 83) * 1922 + x + 907) * 3]
                        .fill(*v);
                }
            }
        }
        if scenario == "two_panels" {
            fill(&mut frame, [1100, 76, 1230, 116], [0, 0, 67]);
        }
        compare(
            &serde_json::to_value(observer.observe(frame.view())?)?,
            &expected[i],
            &format!("target {i}"),
        )?;
    }
    assert_eq!(expected[2]["kind"], "ghoul-lord");
    assert!(expected[3].is_null());
    Ok(())
}
#[test]
fn hover_preserves_kind_order_boxes_and_rejects_npcs() -> Result<(), Box<dyn Error>> {
    let mut random = 31_u32;
    let template = BgrFrame::from_vec(
        Size::new(49, 24)?,
        (0..49 * 24 * 3)
            .map(|_| {
                random = random.wrapping_mul(1664525).wrapping_add(1013904223);
                (random >> 24) as u8
            })
            .collect(),
    )?;
    let mut frame = BgrFrame::zeros(Size::new(1922, 1140)?)?;
    for y in 0..24 {
        frame.pixels_mut()[((y + 400) * 1922 + 1000) * 3..((y + 400) * 1922 + 1049) * 3]
            .copy_from_slice(template.view().row(y).ok_or("row")?);
    }
    let blank = BgrFrame::zeros(frame.size())?;
    let expected = support::legacy_expectation(
        "observer-oracle",
        &json!({"kind":"hover","templates":[{"kind":"shrine","frame":image_arg(&template)},{"kind":"chest","frame":image_arg(&template)}],"cases":[{"frame":image_arg(&frame),"aim":[1030,480]},{"frame":image_arg(&blank),"aim":[1030,480]}]}),
    )?;
    let observer = ArcaneHoverObserver::new(vec![
        ("shrine".into(), template.clone()),
        ("chest".into(), template.clone()),
    ])?;
    for (i, frame) in [&frame, &blank].into_iter().enumerate() {
        compare(
            &serde_json::to_value(observer.observe(frame.view(), [1030, 480])?)?,
            &expected[i],
            &format!("hover {i}"),
        )?;
    }
    assert_eq!(
        expected[0][0]["evidence_box"],
        json!([1000, 400, 1049, 424])
    );
    assert!(ArcaneHoverObserver::new(vec![("npc".into(), template)]).is_err());
    assert!(observer.observe(frame.view(), [1900, 480]).is_err());
    Ok(())
}

fn marker() -> Result<BgrFrame, Box<dyn Error>> {
    let mut image = BgrFrame::zeros(Size::new(32, 20)?)?;
    for y in 4_i32..17 {
        for x in 3_i32..28 {
            if ((x - 3) * 12 - (y - 4) * 24).abs() <= 24
                || ((27 - x) * 12 - (y - 4) * 24).abs() <= 24
            {
                let i = (y as usize * 32 + x as usize) * 3;
                image.pixels_mut()[i..i + 3].copy_from_slice(&[220, 180, 20]);
            }
        }
    }
    Ok(image)
}
fn ground_frame(
    case: &Value,
    marker: &BgrFrame,
    body: Option<&BgrFrame>,
) -> Result<BgrFrame, Box<dyn Error>> {
    let mut frame = BgrFrame::zeros(Size::new(1922, 1140)?)?;
    for item in case["points"].as_array().ok_or("points")? {
        let [x, y]: [usize; 2] = serde_json::from_value(item["point"].clone())?;
        if item["marker"].as_bool().unwrap_or(true) {
            for row in 0..20 {
                frame.pixels_mut()
                    [((y - 10 + row) * 1922 + x - 16) * 3..((y - 10 + row) * 1922 + x + 16) * 3]
                    .copy_from_slice(marker.view().row(row).ok_or("marker row")?);
            }
        }
        if item["shield"].as_bool().unwrap_or(false) {
            fill(
                &mut frame,
                [x - 13, y - 163, x + 14, y - 136],
                [35, 100, 125],
            );
        }
        if item["body"].as_bool().unwrap_or(false) {
            let body = body.ok_or("body required")?;
            for row in 0..80 {
                frame.pixels_mut()
                    [((y - 120 + row) * 1922 + x - 24) * 3..((y - 120 + row) * 1922 + x + 24) * 3]
                    .copy_from_slice(body.view().row(row).ok_or("body row")?);
            }
        }
        if item["shape"].as_bool().unwrap_or(false) {
            fill(&mut frame, [x - 6, y - 120, x + 7, y - 40], [55, 75, 110]);
        }
        if item["fog"].as_bool().unwrap_or(false) {
            fill(&mut frame, [x - 48, y - 120, x + 48, y - 40], [55, 75, 110]);
        }
        if item["tail"].as_bool().unwrap_or(false) {
            fill(
                &mut frame,
                [x + 13, y - 150, x + 33, y - 149],
                [35, 100, 125],
            );
        }
    }
    if case["layout_bad"].as_bool().unwrap_or(false) {
        return Ok(frame
            .view()
            .crop(PixelRect::new(0, 0, 1600, 900)?)?
            .to_owned()?);
    }
    Ok(frame)
}
#[test]
fn ground_marker_identity_ambiguity_overflow_and_strong_age_match_source()
-> Result<(), Box<dyn Error>> {
    let marker = marker()?;
    let tracking =
        json!({"maximum_seconds":0.6,"maximum_gap_seconds":0.25,"maximum_displacement":8});
    let mut cases = vec![
        json!({"at":1,"points":[{"point":[960,580]}]}),
        json!({"at":2,"points":[{"point":[960,580],"shield":true}]}),
        json!({"at":2.2,"points":[{"point":[963,580]}]}),
        json!({"at":2.4,"points":[{"point":[964,582]}]}),
        json!({"at":2.59,"points":[{"point":[965,581]}]}),
        json!({"at":2.61,"points":[{"point":[965,581]}]}),
        json!({"at":3,"points":[{"point":[960,580],"shield":true,"tail":true}]}),
        json!({"at":2.9,"points":[{"point":[960,580]}]}),
        json!({"at":3.2,"points":[{"point":[960,580]}]}),
        json!({"at":4,"points":[{"point":[900,580],"shield":true},{"point":[1030,580],"shield":true}]}),
        json!({"at":5,"points":[],"layout_bad":true}),
    ];
    let overflow: Vec<_> = (0..33)
        .map(|i| json!({"point":[816+(i%6)*48,490+(i/6)*38]}))
        .collect();
    cases.push(json!({"at":6,"points":overflow}));
    let expected = support::legacy_expectation(
        "observer-oracle",
        &json!({"kind":"ground","marker":image_arg(&marker),"tracking":tracking,"cases":cases}),
    )?;
    let mut options = GroundOptions::new(PixelRect::new(800, 480, 1120, 700)?, [16, 10]);
    options.tracking = Some(serde_json::from_value::<GroundTracking>(tracking)?);
    let mut observer = FlashGroundObserver::new(marker.clone(), options)?;
    for (i, case) in cases.iter().enumerate() {
        let frame = ground_frame(case, &marker, None)?;
        compare(
            &serde_json::to_value(observer.observe(frame.view(), case["at"].as_f64())?)?,
            &expected[i],
            &format!("ground {i}"),
        )?;
    }
    assert_eq!(expected[4]["reason"], "flash_marker_tracked");
    assert_eq!(expected[5]["reason"], "ground_unverified");
    assert_eq!(expected[7]["reason"], "ground_time_reversed");
    assert_eq!(expected[9]["reason"], "ground_ambiguous");
    assert_eq!(expected[11]["reason"], "ground_search_overflow");
    let frame = ground_frame(&cases[1], &marker, None)?;
    assert!(observer.observe(frame.view(), Some(f64::NAN)).is_err());
    Ok(())
}

#[test]
fn current_body_and_shield_support_ground_without_old_coordinate_reuse()
-> Result<(), Box<dyn Error>> {
    let marker = marker()?;
    let mut random = 71_u32;
    let body = BgrFrame::from_vec(
        Size::new(48, 80)?,
        (0..48 * 80 * 3)
            .map(|_| {
                random = random.wrapping_mul(1664525).wrapping_add(1013904223);
                ((random >> 24) % 100) as u8
            })
            .collect(),
    )?;
    let cases = json!([
        {"points":[{"point":[960,580],"body":true}]},
        {"points":[{"point":[997,610],"body":true}]},
        {"points":[{"point":[997,610]}]},
        {"points":[{"point":[960,580],"marker":false,"body":true,"shield":true}]},
        {"points":[{"point":[997,610],"marker":false,"body":true,"shield":true}]},
        {"points":[{"point":[960,580],"marker":false,"body":true}]},
        {"points":[{"point":[960,580],"marker":false,"shield":true,"shape":true}]},
        {"points":[{"point":[960,580],"marker":false,"shield":true,"fog":true}]},
        {"points":[]}
    ]);
    let expected = support::legacy_expectation(
        "observer-oracle",
        &json!({"kind":"ground","marker":image_arg(&marker),"body":image_arg(&body),"cases":cases}),
    )?;
    let mut options = GroundOptions::new(PixelRect::new(800, 480, 1120, 700)?, [16, 10]);
    options.body_templates.push(body.clone());
    options.body_box = Some([-24, -120, 24, -40]);
    let mut observer = FlashGroundObserver::new(marker.clone(), options)?;
    for (i, case) in cases.as_array().ok_or("cases")?.iter().enumerate() {
        let frame = ground_frame(case, &marker, Some(&body))?;
        compare(
            &serde_json::to_value(observer.observe(frame.view(), None)?)?,
            &expected[i],
            &format!("body ground {i}"),
        )?;
    }
    assert_eq!(expected[0]["reason"], "flash_marker_and_body");
    assert_eq!(expected[3]["reason"], "flash_body_and_es");
    assert_eq!(expected[6]["reason"], "flash_es_and_body_shape");
    assert!(expected[7]["point"].is_null());
    Ok(())
}

struct FakeGround(Arc<Mutex<(GroundReading, u64)>>);
impl GroundObserver for FakeGround {
    fn observe(
        &mut self,
        _: &Arc<BgrFrame>,
        _: Option<f64>,
    ) -> Result<GroundReading, ObservationError> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| ObservationError::invalid("fake ground poisoned"))?;
        state.1 += 1;
        Ok(state.0.clone())
    }
}
fn camera_operations(operations: Vec<Value>, persistent: bool) -> Result<Value, Box<dyn Error>> {
    let expected = support::legacy_expectation(
        "observer-oracle",
        &json!({"kind":"camera","limits":{"persist_reviewed_layout":persistent},"operations":operations}),
    )?;
    let state = Arc::new(Mutex::new((
        GroundReading::unknown("ground_unverified"),
        0_u64,
    )));
    let panel = Arc::new(AtomicBool::new(false));
    let panel_reader = Arc::clone(&panel);
    let mut observer = CalibratedCameraGroundObserver::new(
        Box::new(FakeGround(Arc::clone(&state))),
        move |_| Ok(panel_reader.load(Ordering::Acquire)),
        CameraLimits {
            persist_reviewed_layout: persistent,
            ..CameraLimits::default()
        },
    )?;
    let native = Arc::new(BgrFrame::zeros(Size::new(1922, 1140)?)?);
    let clone = Arc::new(native.as_ref().clone());
    let small = Arc::new(BgrFrame::zeros(Size::new(1600, 900)?)?);
    let frame = |name: Option<&str>| match name {
        Some("clone") => &clone,
        Some("small") => &small,
        _ => &native,
    };
    let ratio = |raw: Option<&Value>| -> Result<ResourceReading, Box<dyn Error>> {
        if raw.is_some_and(Value::is_null) {
            Ok(ResourceReading::unknown("hud_unverified", None))
        } else {
            Ok(ResourceReading::with_lower_bound(
                raw.and_then(Value::as_f64).unwrap_or(1.0),
                "test_bound",
                None,
            )?)
        }
    };
    for (i, op) in operations.iter().enumerate() {
        panel.store(op["panel"].as_bool().unwrap_or(false), Ordering::Release);
        let current = frame(op["frame"].as_str());
        let hud = HudReading {
            layout_id: "test".into(),
            revision: "test".into(),
            life: ratio(op.get("life"))?,
            mana: ratio(op.get("mana"))?,
            belt_status: "unverified".into(),
            slots: Vec::new(),
            belt_visibility: match op["belt"].as_str() {
                Some("unknown") => BeltVisibility::Unknown,
                Some("expanded") => BeltVisibility::Expanded,
                _ => BeltVisibility::Closed,
            },
            mercenary_status: None,
        };
        let mut outcome = Value::Null;
        let result: Result<(), ObservationError> = match op["action"].as_str().ok_or("action")? {
            "confirm" => {
                let proof: CameraViewportProof = serde_json::from_value(op["proof"].clone())?;
                let layout = op
                    .get("layout")
                    .map(|v| serde_json::from_value::<ReviewedCameraLayout>(v.clone()))
                    .transpose()?;
                observer.confirm_closed_viewport(
                    &proof,
                    op["now"].as_f64().ok_or("now")?,
                    layout.as_ref(),
                )
            }
            "runtime_confirm" => {
                let layout = serde_json::from_value(op["layout"].clone())?;
                observer.confirm_runtime_viewport(
                    current,
                    &hud,
                    op["focused"].as_bool().unwrap_or(true),
                    op["at"].as_f64().ok_or("at")?,
                    op["now"].as_f64().ok_or("now")?,
                    &layout,
                )
            }
            "observe" => {
                let raw = op
                    .get("reading")
                    .cloned()
                    .unwrap_or_else(|| json!({"point":null,"reason":"ground_unverified"}));
                let reading = GroundReading {
                    point: serde_json::from_value(raw["point"].clone())?,
                    reason: raw["reason"].as_str().ok_or("reason")?.into(),
                    score: raw["score"].as_f64(),
                    calibration: None,
                };
                state.lock().map_err(|_| "state poisoned")?.0 = reading;
                let at = op["at"].as_f64().ok_or("at")?;
                let prepare = if op["prepare"].as_bool().unwrap_or(true) {
                    let prepared = frame(op["prepare_frame"].as_str().or(op["frame"].as_str()));
                    observer.prepare_frame(
                        prepared,
                        op["prepare_at"].as_f64().unwrap_or(at),
                        &hud,
                        op["focused"].as_bool().unwrap_or(true),
                    )
                } else {
                    Ok(())
                };
                match prepare.and_then(|()| observer.observe(current, at)) {
                    Ok(reading) => {
                        outcome = serde_json::to_value(reading)?;
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            }
            _ => return Err("camera operation".into()),
        };
        if let Err(e) = result {
            outcome = json!({"error":e.to_string()});
        }
        let calls = state.lock().map_err(|_| "state poisoned")?.1;
        let actual = json!({"outcome":outcome,"status":observer.diagnostics(),"calls":calls});
        compare(&actual, &expected[i], &format!("camera operation {i}"))?;
    }
    Ok(expected)
}
fn confirmed_camera() -> Value {
    json!({"action":"confirm","proof":{"source":"supervised_visual","panels_closed":true,"observed_at":10},"now":10})
}
fn camera_body(at: f64, point: [i32; 2]) -> Value {
    json!({"action":"observe","at":at,"reading":{"point":point,"reason":"flash_body_and_es","score":0.95}})
}
fn calibrated_operations() -> Vec<Value> {
    vec![
        confirmed_camera(),
        camera_body(10.0, [962, 578]),
        camera_body(10.1, [960, 580]),
        camera_body(10.2, [964, 576]),
    ]
}
fn reviewed_layout() -> Value {
    json!({"schema":1,"frame_size":[1922,1140],"body_observations":[{"point":[962,578]},{"point":[962,578]},{"point":[962,578]}]})
}
#[test]
fn camera_requires_current_identity_three_samples_and_fixed_expiry() -> Result<(), Box<dyn Error>> {
    let mut operations = calibrated_operations();
    operations.extend([
        json!({"action":"observe","at":10.3}),
        json!({"action":"observe","at":11.9,"reading":{"point":[962,578],"reason":"flash_marker_tracked"}}),
        json!({"action":"observe","at":12.2}),json!({"action":"observe","at":12.21}),
    ]);
    let result = camera_operations(operations, false)?;
    assert_eq!(result[4]["outcome"]["reason"], "camera_ground_projection");
    assert!(result[7]["outcome"]["point"].is_null());
    for extra in [
        json!({"action":"observe","at":10.3,"prepare":false,"frame":"clone"}),
        json!({"action":"observe","at":10.3,"prepare_at":10.2}),
        json!({"action":"observe","at":10.1}),
    ] {
        let mut operations = calibrated_operations();
        operations.push(extra);
        camera_operations(operations, false)?;
    }
    let mut operations = vec![confirmed_camera()];
    for at in [10.0, 10.1, 10.2] {
        operations.push(json!({"action":"observe","at":at,"reading":{"point":[962,578],"reason":"flash_marker_tracked"}}));
    }
    let result = camera_operations(operations, false)?;
    assert!(result[3]["status"]["center"].is_null());
    Ok(())
}
#[test]
fn camera_context_failures_and_actor_displacement_clear_projection() -> Result<(), Box<dyn Error>> {
    for extra in [
        json!({"panel":true}),
        json!({"focused":false}),
        json!({"frame":"small"}),
        json!({"mana":null}),
        json!({"belt":"unknown"}),
    ] {
        let mut operations = calibrated_operations();
        let mut op = json!({"action":"observe","at":10.3});
        op.as_object_mut()
            .ok_or("op")?
            .extend(extra.as_object().ok_or("extra")?.clone());
        operations.push(op);
        let result = camera_operations(operations, false)?;
        assert!(result[4]["status"]["center"].is_null());
    }
    let mut operations = calibrated_operations();
    operations.push(camera_body(10.4, [1000, 578]));
    operations.extend([
        camera_body(10.6, [962, 578]),
        camera_body(10.7, [962, 578]),
        camera_body(10.8, [962, 578]),
        json!({"action":"observe","at":10.9}),
    ]);
    let result = camera_operations(operations, false)?;
    assert_eq!(result[4]["outcome"]["reason"], "camera_actor_displaced");
    assert_eq!(result[8]["outcome"]["reason"], "camera_ground_projection");
    Ok(())
}
#[test]
fn reviewed_camera_persistence_keeps_only_current_valid_viewport() -> Result<(), Box<dyn Error>> {
    let mut confirmation = confirmed_camera();
    confirmation["layout"] = reviewed_layout();
    confirmation["proof"]["camera_layout_verified"] = json!(true);
    for persistent in [false, true] {
        let result = camera_operations(
            vec![
                confirmation.clone(),
                json!({"action":"observe","at":10}),
                json!({"action":"observe","at":12.01}),
            ],
            persistent,
        )?;
        assert_eq!(result[2]["outcome"]["point"].is_array(), persistent);
    }
    for extra in [
        json!({"mana":null}),
        json!({"belt":"unknown"}),
        json!({"panel":true}),
        json!({"focused":false}),
        json!({"frame":"small"}),
    ] {
        let mut op = json!({"action":"observe","at":10});
        op.as_object_mut()
            .ok_or("op")?
            .extend(extra.as_object().ok_or("extra")?.clone());
        camera_operations(
            vec![
                confirmation.clone(),
                op,
                json!({"action":"observe","at":10.1}),
            ],
            true,
        )?;
    }
    let mut operations = vec![
        confirmation.clone(),
        json!({"action":"observe","at":10,"reading":{"point":[926,514],"reason":"flash_es_and_body_shape"}}),
        camera_body(10.1, [926, 514]),
        camera_body(10.2, [962, 578]),
        camera_body(10.3, [962, 578]),
        camera_body(10.4, [962, 578]),
        json!({"action":"observe","at":30}),
        json!({"action":"observe","at":30.1,"panel":true}),
    ];
    let result = camera_operations(std::mem::take(&mut operations), true)?;
    assert_eq!(result[1]["outcome"]["point"], json!([962, 578]));
    assert_eq!(result[2]["outcome"]["reason"], "camera_actor_displaced");
    assert_eq!(result[6]["outcome"]["reason"], "camera_ground_projection");
    assert_eq!(result[7]["status"]["authorized"], false);
    let runtime = json!({"action":"runtime_confirm","at":10,"now":10.1,"layout":reviewed_layout()});
    camera_operations(
        vec![runtime, json!({"action":"observe","at":10,"prepare":false})],
        true,
    )?;
    for (key, value) in [
        ("source", json!("guess")),
        ("panels_closed", json!(false)),
        ("observed_at", json!(-10)),
    ] {
        let mut invalid = confirmed_camera();
        invalid["proof"][key] = value;
        camera_operations(vec![invalid], false)?;
    }
    Ok(())
}

fn hub_cases(cases: Vec<Value>, track: bool) -> Result<Value, Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let expected = support::legacy_expectation(
        "observer-oracle",
        &json!({"kind":"hub","track":track,"cases":cases}),
    )?;
    let mut observer = ArcaneHubObserver::load(
        root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140/hub-patterns.json"),
    )?;
    let mut tracker = HubStabilityTracker::new(2.0)?;
    for (i, case) in cases.iter().enumerate() {
        let mut frame =
            codec::load_color(root.join(case["frame"]["path"].as_str().ok_or("path")?))?;
        if let Some(rects) = case["frame"]["clear"].as_array() {
            for r in rects {
                let region: [usize; 4] = serde_json::from_value(r.clone())?;
                fill(&mut frame, region, [0, 0, 0]);
            }
        }
        let frame = Arc::new(frame);
        let point = observer.observe(&frame)?;
        let evidence = observer.last_evidence().cloned();
        let tracked = if track {
            let motion = case.get("motion").cloned().unwrap_or_else(|| json!([0, 0]));
            Some(tracker.update(
                &observer,
                &frame,
                point,
                evidence.clone(),
                case["at"].as_f64().ok_or("at")?,
                serde_json::from_value(motion)?,
            )?)
        } else {
            None
        };
        let player = case
            .get("player")
            .cloned()
            .unwrap_or_else(|| json!([962, 578]));
        let near = observer.current_pair_near(&frame, serde_json::from_value(player)?, 100.0)?;
        let actual = json!({"point":point,"evidence":evidence,"pairs":observer.pair_points(),"near":near,"tracked":tracked});
        compare(&actual, &expected[i], &format!("hub {i}"))?;
        assert!(
            observer
                .current_pair_near(&Arc::new(frame.as_ref().clone()), [962, 578], 100.0)
                .is_err()
        );
    }
    Ok(expected)
}
#[test]
fn hub_requires_distinct_current_ornaments_and_both_blue_patches() -> Result<(), Box<dyn Error>> {
    let base = "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references";
    let mut cases = Vec::new();
    for name in [
        "hub/main.png",
        "hub/clear-floor.png",
        "hub/distant-effects.png",
        "navigation/stairs-hostile-effects-no-waypoint.png",
        "hub/actor-occluded.png",
        "hub/return-single-ornament.png",
        "hub/return-map-overlay-on.png",
        "hub/return-map-overlay-off.png",
        "hub/merged-brazier-town-finish.png",
        "hub/merged-dim-brazier.png",
        "hub/southwest-approach-floor.png",
        "hub/effects.png",
        "hub/left-floor-return.png",
        "hub/merged-pair-one-ornament.png",
        "hub/return-player-merc-single-ornament.png",
        "hub/return-player-merc-two-ornaments.png",
    ] {
        cases.push(json!({"frame":{"path":format!("{base}/{name}")}}));
    }
    for region in [
        [800, 492, 884, 535],
        [800, 427, 870, 472],
        [884, 421, 937, 489],
        [983, 421, 1036, 489],
    ] {
        cases.push(
            json!({"frame":{"path":format!("{base}/hub/left-floor-return.png"),"clear":[region]}}),
        );
    }
    let result = hub_cases(cases, false)?;
    assert_eq!(result[12]["point"], json!([961, 503]));
    assert!(result[13]["point"].is_null());
    assert_eq!(result[13]["near"], json!([961, 503]));
    assert!(result[14]["point"].is_null());
    assert_eq!(result[14]["near"], json!([966, 502]));
    assert_eq!(result[15]["point"], json!([961, 503]));
    Ok(())
}
#[test]
fn hub_static_tracking_does_not_renew_full_evidence_age() -> Result<(), Box<dyn Error>> {
    let base = "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/hub";
    let full = format!("{base}/left-floor-return.png");
    let partial = format!("{base}/merged-pair-one-ornament.png");
    let result = hub_cases(
        vec![
            json!({"frame":{"path":partial},"at":0}),
            json!({"frame":{"path":full},"at":1}),
            json!({"frame":{"path":partial},"at":1.4}),
            json!({"frame":{"path":partial},"at":2.9}),
            json!({"frame":{"path":partial},"at":3.01}),
        ],
        true,
    )?;
    assert_eq!(result[2]["tracked"][0], json!([961, 503]));
    assert_eq!(result[3]["tracked"][1]["reference_observed_at"], 1);
    assert!(result[4]["tracked"][0].is_null());
    for extra in [
        json!({"motion":[2,0]}),
        json!({"motion":null}),
        json!({"frame":{"path":partial,"clear":[[790,420,880,480]]}}),
    ] {
        let mut current = json!({"frame":{"path":partial},"at":1.4});
        current
            .as_object_mut()
            .ok_or("current")?
            .extend(extra.as_object().ok_or("extra")?.clone());
        hub_cases(vec![json!({"frame":{"path":full},"at":1}), current], true)?;
    }
    Ok(())
}

#[test]
fn town_receives_only_current_full_native_hub() -> Result<(), Box<dyn Error>> {
    use d2bot::actions::town_loop::TownSight;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/hub");
    let mut vision = d2bot::runs::summoner::town_vision(root)?;
    let frame = Arc::new(codec::load_color(base.join("left-floor-return.png"))?);
    vision.set_frame(frame);
    vision.normalize()?;
    let point = vision
        .find("world_arcane_waypoint")?
        .ok_or("native waypoint")?;
    assert_eq!(point.evidence.as_deref(), Some("current_native_hub"));
    assert_eq!(point.x, 961.0 * 1267.0 / 1922.0);
    assert_eq!(point.y, 503.0 * 753.0 / 1140.0);
    vision.set_frame(Arc::new(codec::load_color(
        base.join("merged-pair-one-ornament.png"),
    )?));
    vision.normalize()?;
    let current = vision.find("world_arcane_waypoint")?;
    assert_ne!(
        current.as_ref().and_then(|p| p.evidence.as_deref()),
        Some("current_native_hub")
    );
    Ok(())
}

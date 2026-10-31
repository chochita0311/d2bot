use d2bot::common::{
    automap_vision::AutomapObserver,
    buffing_vision::{WeaponSetObserver, resource_increase_candidate},
    frame::{BgrFrame, GrayFrame, Image, Size},
    label_occlusion::{label_occlusion_mask, visible_template_peak},
    survival_vision::{HudObserverOptions, SurvivalHudObserver},
    vision_ops::{
        codec,
        resize::{self, Interpolation},
    },
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{error::Error, path::Path};
mod support;

fn image_arg<const C: usize>(image: &Image<u8, C>) -> Value {
    json!({"width":image.size().width(),"height":image.size().height(),"pixels":image.pixels().iter().map(|v| format!("{v:02x}")).collect::<String>()})
}
#[test]
fn loot_labels_keep_obscured_mercenary_unknown_while_resources_remain_current()
-> Result<(), Box<dyn Error>> {
    let owner = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/ui/survival");
    let observer = SurvivalHudObserver::load(
        &owner,
        HudObserverOptions {
            allow_high_orb_fallback: true,
            allow_partial_orb_fallback: true,
            observe_mercenary: true,
        },
    )?;
    let frame = codec::load_color(owner.join("references/mercenary-face-loot-label-overlay.png"))?;
    let reading = observer.observe(frame.view())?;
    assert_eq!(reading.life.ratio(), Some(1.0));
    assert_eq!(reading.mana.ratio(), Some(1.0));
    assert_eq!(reading.mercenary_status.as_deref(), Some("unverified"));
    Ok(())
}
#[test]
fn automap_current_parts_variants_missing_title_and_layout_match_source()
-> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/navigation";
    let mut cases = Vec::new();
    for file in [
        "area-title-stone-underlay.png",
        "area-title-light-floor.png",
        "area-title-marker-overlap.png",
    ] {
        for area in ["arcane_sanctuary", "canyon_of_the_magi"] {
            cases.push(json!({"frame":{"path":format!("{base}/{file}")},"area":area}));
        }
        for (a, b) in [(1178, 1200), (1200, 1223), (1223, 1246), (1178, 1246)] {
            cases.push(json!({"frame":{"path":format!("{base}/{file}"),"resize":[1267,753],"clear":[[a,70,b,85]]},"area":"arcane_sanctuary"}));
        }
    }
    for file in [
        "assets/regions/rogue-encampment/remastered-ko-window/references/map-reviewed-window.png",
        "assets/regions/lut-gholein/remastered-ko-window/references/main-reviewed.png",
    ] {
        cases.push(json!({"frame":{"path":file},"area":"arcane_sanctuary"}));
    }
    let expected =
        support::legacy_expectation("observer-oracle", &json!({"kind":"automap","cases":cases}))?;
    let mut observer = AutomapObserver::load(root.join("assets/ui/automap"))?;
    for (index, case) in cases.iter().enumerate() {
        let mut frame =
            codec::load_color(root.join(case["frame"]["path"].as_str().ok_or("path")?))?;
        if case["frame"].get("resize").is_some() {
            frame = resize::resize(frame.view(), Size::new(1267, 753)?, Interpolation::Area)?;
        }
        if let Some(clear) = case["frame"]["clear"].as_array() {
            for p in clear {
                let [a, t, b, d]: [usize; 4] = serde_json::from_value(p.clone())?;
                for y in t..d {
                    frame.pixels_mut()[(y * 1267 + a) * 3..(y * 1267 + b) * 3].fill(0);
                }
            }
        }
        let visible = observer.visible(frame.view(), case["area"].as_str().ok_or("area")?)?;
        assert_eq!(
            Some(visible),
            expected[index]["visible"].as_bool(),
            "case {index}"
        );
        assert_eq!(
            Some(observer.last_obscured()),
            expected[index]["obscured"].as_bool(),
            "case {index}"
        );
        let actual = serde_json::to_value(observer.last_evidence())?;
        let wanted = &expected[index]["evidence"];
        if actual.is_null() || wanted.is_null() {
            assert_eq!(&actual, wanted, "case {index}");
            continue;
        }
        for key in ["area", "title_at", "template", "encoding"] {
            assert_eq!(actual[key], wanted[key], "case {index} {key}");
        }
        assert!(
            (actual["score"].as_f64().ok_or("score")? - wanted["score"].as_f64().ok_or("score")?)
                .abs()
                <= 2e-5,
            "case {index}"
        );
        match (
            actual["part_scores"].as_array(),
            wanted["part_scores"].as_array(),
        ) {
            (Some(a), Some(b)) => {
                assert_eq!(a.len(), b.len());
                for (a, b) in a.iter().zip(b) {
                    assert!((a.as_f64().ok_or("part")? - b.as_f64().ok_or("part")?).abs() <= 2e-5);
                }
            }
            (None, None) => (),
            _ => return Err("part score evidence differs".into()),
        }
    }
    assert!(!observer.visible(
        BgrFrame::zeros(Size::new(900, 753)?)?.view(),
        "unknown_area"
    )?);
    assert!(observer.last_evidence().is_none());
    assert!(!observer.last_obscured());
    assert!(
        observer
            .visible(
                BgrFrame::zeros(Size::new(1267, 753)?)?.view(),
                "unknown_area"
            )
            .is_err()
    );
    Ok(())
}

#[test]
fn label_rows_merging_gold_and_empty_frames_match_source_pixels() -> Result<(), Box<dyn Error>> {
    let mut frame = BgrFrame::zeros(Size::new(800, 1140)?)?;
    // 성긴 글자 기둥, 세로 확장으로 붙는 두 행, 금색 글자를 함께 검사한다.
    for (top, left, color) in [
        (150, 40, [240, 240, 240]),
        (164, 40, [240, 240, 240]),
        (400, 100, [40, 140, 210]),
        (58, 300, [240, 240, 240]),
    ] {
        for x in left..left + 150 {
            for y in top..top + 12 {
                if (x - left) % 7 < 2 || (y - top) % 9 == 0 {
                    frame.pixels_mut()[(y * 800 + x) * 3..(y * 800 + x + 1) * 3]
                        .copy_from_slice(&color);
                }
            }
        }
    }
    let blank = BgrFrame::zeros(frame.size())?;
    let fixtures = [(&frame, true), (&frame, false), (&blank, true)];
    let cases: Vec<_> = fixtures
        .iter()
        .map(|(frame, gold)| json!({"frame":image_arg(frame),"include_gold":gold}))
        .collect();
    let expected = support::legacy_expectation(
        "observer-oracle",
        &json!({"kind":"label_mask","cases":cases}),
    )?;
    for (index, (frame, gold)) in fixtures.into_iter().enumerate() {
        let mask = label_occlusion_mask(frame.view(), gold)?;
        assert_eq!(
            format!("{:x}", Sha256::digest(mask.pixels())),
            expected[index]["sha256"].as_str().ok_or("hash")?,
            "case {index}"
        );
        assert_eq!(
            mask.pixels().iter().filter(|v| **v != 0).count() as u64,
            expected[index]["nonzero"].as_u64().ok_or("count")?
        );
    }
    assert!(expected[0]["nonzero"].as_u64() > expected[1]["nonzero"].as_u64());
    assert_eq!(expected[2]["nonzero"], 0);
    Ok(())
}

#[test]
fn visible_texture_requires_current_coverage_and_variance() -> Result<(), Box<dyn Error>> {
    let mut random = 37_u32;
    let mut next = || {
        random = random.wrapping_mul(1664525).wrapping_add(1013904223);
        (random >> 24) as u8
    };
    let template = GrayFrame::from_vec(Size::new(50, 40)?, (0..2000).map(|_| next()).collect())?;
    let mut frame = GrayFrame::from_vec(Size::new(60, 48)?, (0..2880).map(|_| next()).collect())?;
    for y in 0..40 {
        frame.pixels_mut()[(y + 4) * 60 + 6..(y + 4) * 60 + 56]
            .copy_from_slice(template.view().row(y).ok_or("row")?);
    }
    let mut visible = GrayFrame::from_vec(frame.size(), vec![1; 2880])?;
    for y in 4..20 {
        frame.pixels_mut()[y * 60 + 6..y * 60 + 56].fill(0);
        visible.pixels_mut()[y * 60 + 6..y * 60 + 56].fill(0);
    }
    let mut less = visible.clone();
    less.pixels_mut()[20 * 60 + 6..20 * 60 + 56].fill(0);
    let zeros = GrayFrame::zeros(frame.size())?;
    let all = GrayFrame::from_vec(frame.size(), vec![1; 2880])?;
    let other = GrayFrame::from_vec(template.size(), (0..2000).map(|_| next()).collect())?;
    let fixtures = [
        (&frame, &template, &visible),
        (&frame, &template, &less),
        (&frame, &other, &all),
        (&zeros, &template, &all),
        (&frame, &template, &zeros),
    ];
    let cases:Vec<_> = fixtures.iter().map(|(f,t,v)| json!({"gray":image_arg(f),"template":image_arg(t),"visible":image_arg(v),"minimum_visible":0.6})).collect();
    let expected = support::legacy_expectation(
        "observer-oracle",
        &json!({"kind":"visible_peak","cases":cases}),
    )?;
    for (i, (frame, template, visible)) in fixtures.into_iter().enumerate() {
        let actual = visible_template_peak(frame.view(), template.view(), visible.view(), 0.6)?;
        assert_eq!(
            serde_json::to_value(actual.location)?,
            expected[i]["location"],
            "case {i}"
        );
        assert!(
            (actual.score - expected[i]["score"].as_f64().ok_or("score")?).abs() <= 2e-5,
            "case {i}: {} != {}",
            actual.score,
            expected[i]["score"]
        );
        assert!(
            (actual.visible_fraction
                - expected[i]["visible_fraction"].as_f64().ok_or("fraction")?)
            .abs()
                <= 1e-6,
            "case {i}"
        );
    }
    assert!(expected[0]["score"].as_f64().ok_or("score")? > 0.99);
    assert!(expected[1]["score"].as_f64().ok_or("score")? < 0.92);
    assert!(visible_template_peak(frame.view(), template.view(), zeros.view(), f64::NAN).is_err());
    Ok(())
}

fn paste(
    frame: &mut BgrFrame,
    image: &BgrFrame,
    [l, t, r, b]: [usize; 4],
) -> Result<(), Box<dyn Error>> {
    assert_eq!(
        [r - l, b - t],
        [image.size().width(), image.size().height()]
    );
    let w = frame.size().width();
    for y in t..b {
        frame.pixels_mut()[(y * w + l) * 3..(y * w + r) * 3]
            .copy_from_slice(image.view().row(y - t).ok_or("paste row")?);
    }
    Ok(())
}
fn compare_observation(a: &Value, b: &Value, path: &str) -> Result<(), Box<dyn Error>> {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (key, value) in a {
                compare_observation(
                    value,
                    b.get(key).ok_or("missing expected key")?,
                    &format!("{path}.{key}"),
                )?;
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare_observation(a, b, &format!("{path}[{i}]"))?;
            }
        }
        (Value::Number(a), Value::Number(b)) if a.is_f64() || b.is_f64() => {
            let a = a.as_f64().ok_or("number")?;
            let b = b.as_f64().ok_or("number")?;
            let tolerance = if path.ends_with("distance") {
                1e-7
            } else {
                2e-5
            };
            assert!((a - b).abs() <= tolerance, "{path}: {a} != {b}");
        }
        _ => assert_eq!(a, b, "{path}"),
    }
    Ok(())
}
#[test]
fn hud_numbers_belt_current_unknown_orb_and_portrait_match_source() -> Result<(), Box<dyn Error>> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/ui/survival");
    let manifest: Value = serde_json::from_slice(&std::fs::read(directory.join("manifest.json"))?)?;
    let mut cases = Vec::new();
    for group in [
        "references",
        "negative_references",
        "reduced_window_references",
    ] {
        for (index, _) in manifest[group]
            .as_array()
            .ok_or("references")?
            .iter()
            .enumerate()
        {
            cases.push(json!({"frame":{"group":group,"reference":index}}));
        }
    }
    let closed = manifest["references"]
        .as_array()
        .ok_or("refs")?
        .iter()
        .position(|v| v["path"] == "references/closed.png")
        .ok_or("closed")?;
    for entry in manifest["mercenary_portrait"]["references"]
        .as_array()
        .ok_or("portrait references")?
    {
        let region = entry["box"].clone();
        cases.push(json!({"frame":{"group":"references","reference":closed},"options":{"observe_mercenary":true},"paste":[{"box":region,"path":entry["path"]}]}));
    }
    let portrait_path = manifest["mercenary_portrait"]["label_occlusion_reference"]["path"]
        .as_str()
        .ok_or("portrait path")?;
    for fill in [
        json!([]),
        json!([{"box":[39,79,93,133],"color":[0,0,0]}]),
        json!([{"box":[39,79,93,104],"color":[0,0,0]}]),
        json!([{"box":[39,79,93,133],"color":[255,255,255]}]),
    ] {
        cases.push(json!({"frame":{"path":format!("assets/ui/survival/{portrait_path}")},"options":{"observe_mercenary":true},"fill":fill}));
    }
    let bounds = json!({"allow_high_orb_fallback":true,"allow_partial_orb_fallback":true});
    cases.push(
        json!({"frame":{"group":"reduced_window_references","reference":0},"options":bounds}),
    );
    for key in ["life", "mana"] {
        for color in [[0, 0, 0], [30, 150, 30], [90, 90, 90]] {
            let fill: Vec<_> = ["high_orb_bound", "partial_orb_bound"]
                .into_iter()
                .map(|bound| json!({"box":manifest[bound]["resources"][key]["box"],"color":color}))
                .collect();
            cases.push(json!({"frame":{"group":"reduced_window_references","reference":0},"options":bounds,"fill":fill}));
        }
    }
    for region in [
        json!([1088, 946, 1148, 1003]),
        manifest["belt"]["closed_anchor_box"].clone(),
        manifest["resources"]["life"]["label_search_box"].clone(),
        manifest["resources"]["mana"]["label_search_box"].clone(),
    ] {
        cases.push(json!({"frame":{"group":"references","reference":closed},"fill":[{"box":region,"color":[255,255,255]}]}));
    }
    cases.push(json!({"frame":image_arg(&BgrFrame::zeros(Size::new(1267,753)?)?),"options":{"observe_mercenary":true}}));
    let expected =
        support::legacy_expectation("observer-oracle", &json!({"kind":"hud","cases":cases}))?;
    let mut readings = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let raw = &case["frame"];
        let mut frame = if let Some(i) = raw["reference"].as_u64() {
            let entry = &manifest[raw["group"].as_str().ok_or("group")?][i as usize];
            let mut frame = BgrFrame::zeros(Size::new(1922, 1140)?)?;
            let image = codec::load_color(directory.join(entry["path"].as_str().ok_or("path")?))?;
            paste(
                &mut frame,
                &image,
                serde_json::from_value(entry["canvas_box"].clone())?,
            )?;
            frame
        } else if let Some(path) = raw["path"].as_str() {
            codec::load_color(Path::new(env!("CARGO_MANIFEST_DIR")).join(path))?
        } else {
            BgrFrame::zeros(Size::new(1267, 753)?)?
        };
        if let Some(changes) = case["fill"].as_array() {
            for c in changes {
                let [l, t, r, b]: [usize; 4] = serde_json::from_value(c["box"].clone())?;
                let color: [u8; 3] = serde_json::from_value(c["color"].clone())?;
                let w = frame.size().width();
                for y in t..b {
                    for x in l..r {
                        frame.pixels_mut()[(y * w + x) * 3..(y * w + x + 1) * 3]
                            .copy_from_slice(&color);
                    }
                }
            }
        }
        if let Some(changes) = case["paste"].as_array() {
            for c in changes {
                paste(
                    &mut frame,
                    &codec::load_color(directory.join(c["path"].as_str().ok_or("paste path")?))?,
                    serde_json::from_value(c["box"].clone())?,
                )?;
            }
        }
        let option = &case["options"];
        let observer = SurvivalHudObserver::load(
            &directory,
            HudObserverOptions {
                allow_high_orb_fallback: option["allow_high_orb_fallback"]
                    .as_bool()
                    .unwrap_or(false),
                allow_partial_orb_fallback: option["allow_partial_orb_fallback"]
                    .as_bool()
                    .unwrap_or(false),
                observe_mercenary: option["observe_mercenary"].as_bool().unwrap_or(false),
            },
        )?;
        let reading = observer.observe(frame.view())?;
        let slots = observer.closed_belt_slots(frame.view(), &reading)?;
        compare_observation(
            &json!({"reading":reading,"closed_slots":slots}),
            &expected[index],
            &format!("HUD case {index}"),
        )?;
        readings.push(reading);
    }
    let locate = |path: &str| {
        manifest["references"]
            .as_array()
            .and_then(|rows| rows.iter().position(|r| r["path"] == path))
            .ok_or("resource reference")
    };
    assert!(resource_increase_candidate(
        &readings[locate("references/before-orders.png")?],
        &readings[locate("references/after-orders.png")?]
    ));
    assert!(!resource_increase_candidate(
        &readings[closed],
        &readings[closed]
    ));
    Ok(())
}

#[test]
fn weapon_set_requires_both_current_regions() -> Result<(), Box<dyn Error>> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/ui/weapon-set");
    let manifest: Value = serde_json::from_slice(&std::fs::read(directory.join("manifest.json"))?)?;
    let cases = json!([{"set":1},{"set":2},{"set":1,"clear":[manifest["regions"][0]]},{"set":2,"clear":[manifest["regions"][1]]},{"set":null}]);
    let expected =
        support::legacy_expectation("observer-oracle", &json!({"kind":"weapon","cases":cases}))?;
    let observer = WeaponSetObserver::load(&directory)?;
    for (index, case) in cases.as_array().ok_or("cases")?.iter().enumerate() {
        let mut frame = BgrFrame::zeros(Size::new(1922, 1140)?)?;
        if let Some(number) = case["set"].as_u64() {
            let template = codec::load_color(directory.join(format!("active-{number}.png")))?;
            let mut start = 0;
            for region in manifest["regions"].as_array().ok_or("regions")? {
                let [l, t, r, b]: [usize; 4] = serde_json::from_value(region.clone())?;
                for y in 0..b - t {
                    let row = template.view().row(y).ok_or("row")?;
                    frame.pixels_mut()[((t + y) * 1922 + l) * 3..((t + y) * 1922 + r) * 3]
                        .copy_from_slice(&row[start * 3..(start + r - l) * 3]);
                }
                start += r - l;
            }
        }
        if let Some(rects) = case["clear"].as_array() {
            for r in rects {
                let [l, t, r, b]: [usize; 4] = serde_json::from_value(r.clone())?;
                for y in t..b {
                    frame.pixels_mut()[(y * 1922 + l) * 3..(y * 1922 + r) * 3].fill(0);
                }
            }
        }
        assert_eq!(
            serde_json::to_value(observer.observe(frame.view())?)?,
            expected[index],
            "weapon case {index}"
        );
    }
    assert!(
        observer
            .observe(BgrFrame::zeros(Size::new(1267, 753)?)?.view())?
            .is_none()
    );
    Ok(())
}

#[test]
fn invalid_reviewed_manifests_fail_before_images_or_input() -> Result<(), Box<dyn Error>> {
    struct Fixture {
        root: std::path::PathBuf,
        path: std::path::PathBuf,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            if self.path.parent() == Some(self.root.as_path())
                && self
                    .path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("rust-observer-manifest-"))
                && let Err(e) = std::fs::remove_dir_all(&self.path)
            {
                eprintln!("observer fixture cleanup failed: {e}");
            }
        }
    }
    let root = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    std::fs::create_dir_all(&root)?;
    let root = root.canonicalize()?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let path = root.join(format!(
        "rust-observer-manifest-{}-{stamp}",
        std::process::id()
    ));
    std::fs::create_dir(&path)?;
    let fixture = Fixture { root, path };
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source: Value = serde_json::from_slice(&std::fs::read(
        repo.join("assets/ui/survival/manifest.json"),
    )?)?;
    for (pointer, value) in [
        ("/schema_version", json!(true)),
        ("/frame_size/0", json!(true)),
        ("/layout_id", json!(" ")),
        ("/matching/label_score", json!(0)),
        ("/matching/number_max_gap", json!(0)),
        ("/resources", json!({})),
    ] {
        let mut bad = source.clone();
        *bad.pointer_mut(pointer).ok_or("manifest pointer")? = value;
        std::fs::write(
            fixture.path.join("manifest.json"),
            serde_json::to_vec(&bad)?,
        )?;
        assert!(
            SurvivalHudObserver::load(&fixture.path, HudObserverOptions::default()).is_err(),
            "{pointer}"
        );
    }
    let mut bad = source;
    bad["resources"]["life"]["template"] = json!("../outside.png");
    std::fs::write(
        fixture.path.join("manifest.json"),
        serde_json::to_vec(&bad)?,
    )?;
    let error = match SurvivalHudObserver::load(&fixture.path, HudObserverOptions::default()) {
        Ok(_) => return Err("traversal accepted".into()),
        Err(e) => e,
    };
    assert!(error.to_string().contains("within its owner"), "{error}");
    let terrain: Value = serde_json::from_slice(&std::fs::read(repo.join(
        "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/navigation-patterns.json",
    ))?)?;
    for invalid in ["../outside.png", "templates/unreviewed/part.png"] {
        let mut bad = terrain.clone();
        bad["patterns"][0]["path"] = json!(invalid);
        let path = fixture.path.join("navigation-patterns.json");
        std::fs::write(&path, serde_json::to_vec(&bad)?)?;
        let error =
            match d2bot::runs::summoner::terrain_patterns::ArcaneTerrainPatterns::load(&path) {
                Ok(_) => return Err("terrain traversal accepted".into()),
                Err(e) => e,
            };
        assert!(error.to_string().contains("outside"), "{error}");
    }
    Ok(())
}

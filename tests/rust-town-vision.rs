use d2bot::{
    actions::town_loop::{BluePair, TownSight, TownVision},
    common::{
        frame::{BgrFrame, Size},
        vision_ops::{
            codec,
            resize::{self, Interpolation},
        },
    },
};
use serde_json::{Value, json};
use std::{error::Error, sync::Arc};
mod support;
#[test]
fn png_menu_font_title_parts_and_wrong_regions_match_source() -> Result<(), Box<dyn Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let names = [
        "area_arcane",
        "area_canyon",
        "waypoint_header",
        "world_arcane_waypoint",
        "area_act1",
        "area_act2",
    ];
    let base = "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/navigation";
    let mut cases = Vec::new();
    for filename in [
        "area-title-stone-underlay.png",
        "area-title-light-floor.png",
        "area-title-marker-overlap.png",
    ] {
        let path = format!("{base}/{filename}");
        cases.push(json!({"frame":{"path":path},"finds":names}));
        for (a, b) in [(1178, 1200), (1200, 1223), (1223, 1246), (1178, 1246)] {
            cases.push(json!({"frame":{"path":path},"resize":[1267,753],"clear":[[a,70,b,85]],"finds":["area_arcane","area_canyon"]}));
        }
    }
    cases.push(json!({"frame":{"path":"assets/regions/rogue-encampment/remastered-ko-window/references/map-reviewed-window.png"},"finds":names}));
    cases.push(json!({"frame":{"path":"assets/regions/lut-gholein/remastered-ko-window/references/main-reviewed.png"},"finds":names}));
    let expected = support::legacy_expectation(
        "action-oracle",
        &json!({"kind":"town_vision","cases":cases}),
    )?;
    let mut vision = TownVision::load(root)?;
    for (index, case) in cases.iter().enumerate() {
        let mut frame =
            codec::load_color(root.join(case["frame"]["path"].as_str().ok_or("path")?))?;
        if case.get("resize").is_some() {
            frame = resize::resize(frame.view(), Size::new(1267, 753)?, Interpolation::Area)?;
        }
        if let Some(clear) = case["clear"].as_array() {
            for p in clear {
                let bounds: [usize; 4] = serde_json::from_value(p.clone())?;
                for y in bounds[1]..bounds[3] {
                    frame.pixels_mut()[(y * 1267 + bounds[0]) * 3..(y * 1267 + bounds[2]) * 3]
                        .fill(0);
                }
            }
        }
        vision.set_frame(Arc::new(frame));
        vision.normalize()?;
        for name in case["finds"].as_array().ok_or("finds")? {
            let name = name.as_str().ok_or("name")?;
            let actual = serde_json::to_value(vision.find(name)?)?;
            compare_match(
                &actual,
                &expected[index][name],
                &format!("case {index} {name}"),
            );
        }
    }
    Ok(())
}
fn compare_match(actual: &Value, expected: &Value, label: &str) {
    if actual.is_null() || expected.is_null() {
        assert_eq!(actual, expected, "{label}");
        return;
    }
    assert_eq!(
        actual.as_object().map(|m| m.len()),
        expected.as_object().map(|m| m.len()),
        "{label}"
    );
    for key in ["x", "y", "width_scale"] {
        if actual.get(key).is_some() || expected.get(key).is_some() {
            assert_eq!(
                actual[key].as_f64(),
                expected[key].as_f64(),
                "{label} {key}"
            );
        }
    }
    for key in ["template", "evidence"] {
        assert_eq!(actual.get(key), expected.get(key), "{label} {key}");
    }
    if let (Some(a), Some(b)) = (actual["score"].as_f64(), expected["score"].as_f64()) {
        assert!((a - b).abs() <= 2e-5, "{label} score {a} != {b}");
    }
}
#[test]
fn reduced_label_and_missing_letters_are_distinguished() -> Result<(), Box<dyn Error>> {
    let title =
        codec::load_color("assets/ui/waypoint/references/waypoint-header-reduced-frame.png")?;
    let mut frame = BgrFrame::zeros(Size::new(1267, 753)?)?;
    for y in 0..title.size().height() {
        frame.pixels_mut()
            [((y + 110) * 1267 + 190) * 3..((y + 110) * 1267 + 190 + title.size().width()) * 3]
            .copy_from_slice(title.view().row(y).ok_or("row")?);
    }
    let bgr = frame
        .pixels()
        .iter()
        .map(|n| format!("{n:02x}"))
        .collect::<String>();
    let frame_arg = json!({"width":1267,"height":753,"bgr":bgr});
    let cases = json!([{"frame":frame_arg,"finds":["waypoint_header"]},{"frame":frame_arg,"clear":[[232,120,322,144]],"finds":["waypoint_header"]}]);
    let expected = support::legacy_expectation(
        "action-oracle",
        &json!({"kind":"town_vision","cases":cases}),
    )?;
    let mut vision = TownVision::load(env!("CARGO_MANIFEST_DIR"))?;
    vision.set_frame(Arc::new(frame.clone()));
    vision.normalize()?;
    compare_match(
        &serde_json::to_value(vision.find("waypoint_header")?)?,
        &expected[0]["waypoint_header"],
        "reduced font",
    );
    for y in 120..144 {
        frame.pixels_mut()[(y * 1267 + 232) * 3..(y * 1267 + 322) * 3].fill(0);
    }
    vision.set_frame(Arc::new(frame));
    vision.normalize()?;
    assert!(vision.find("waypoint_header")?.is_none());
    assert!(expected[1]["waypoint_header"].is_null());
    Ok(())
}
#[test]
fn blue_pair_rejects_flat_shapes_and_multiple_pairs_and_native_hook_is_current()
-> Result<(), Box<dyn Error>> {
    let definition: BluePair = serde_json::from_value(
        json!({"search":[0,0,500,200],"hsv_lower":[95,55,120],"hsv_upper":[135,255,255],"minimum_area":200,"width":[6,55],"height":[25,100],"separation":[115,155],"vertical_difference":20,"click_offset":[0,25],"maximum_aspect_ratio":1.35}),
    )?;
    let mut frame = BgrFrame::zeros(Size::new(500, 200)?)?;
    for (index, rects) in [
        vec![[30, 40, 68, 54], [160, 40, 198, 54]],
        vec![[30, 80, 66, 121], [160, 80, 196, 121]],
        vec![[310, 80, 346, 121], [440, 80, 476, 121]],
    ]
    .into_iter()
    .enumerate()
    {
        for [x0, y0, x1, y1] in rects {
            for y in y0..y1 {
                for x in x0..x1 {
                    frame.pixels_mut()[(y * 500 + x) * 3..(y * 500 + x) * 3 + 3]
                        .copy_from_slice(&[255, 100, 0]);
                }
            }
        }
        let actual = TownVision::find_blue_pair(frame.view(), &definition)?;
        assert_eq!(actual.is_some(), index == 1);
        let bgr = frame
            .pixels()
            .iter()
            .map(|n| format!("{n:02x}"))
            .collect::<String>();
        let expected = support::legacy_expectation(
            "action-oracle",
            &json!({"kind":"town_blue_pair","frame":{"width":500,"height":200,"bgr":bgr},"definition":definition}),
        )?;
        compare_match(&serde_json::to_value(actual)?, &expected, "blue pair");
    }
    let mut vision = TownVision::load(env!("CARGO_MANIFEST_DIR"))?;
    let mut count = 0;
    vision.set_native_waypoint_observer(move |_| {
        count += 1;
        Ok((count == 1).then_some([1046.0, 575.0]))
    });
    vision.set_frame(Arc::new(BgrFrame::zeros(Size::new(1922, 1140)?)?));
    vision.normalize()?;
    let point = vision.find("world_arcane_waypoint")?.ok_or("native hook")?;
    assert!(point.is_native_hub());
    assert_eq!(point.x, 1046.0 * 1267.0 / 1922.0);
    vision.set_frame(Arc::new(BgrFrame::zeros(Size::new(1922, 1140)?)?));
    vision.normalize()?;
    assert!(vision.find("world_arcane_waypoint")?.is_none());
    Ok(())
}

#[test]
fn camera_unknown_reseeds_and_requires_three_new_stable_frames() -> Result<(), Box<dyn Error>> {
    let before = "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/navigation/north-step-before.png";
    let after = "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/navigation/north-step-after.png";
    let cases = [
        (before, 0.0),
        (after, 0.1),
        (after, 0.2),
        (after, 0.3),
        (after, 0.4),
        (after, 0.5),
    ]
    .into_iter()
    .map(|(path, now)| json!({"frame":{"path":path},"now":now}))
    .collect::<Vec<_>>();
    let expected = support::legacy_expectation(
        "action-oracle",
        &json!({"kind":"town_camera","cases":cases}),
    )?;
    let mut vision = TownVision::load(env!("CARGO_MANIFEST_DIR"))?;
    for (i, case) in cases.iter().enumerate() {
        vision.set_frame(Arc::new(codec::load_color(
            case["frame"]["path"].as_str().ok_or("path")?,
        )?));
        let stable = vision.camera_stable(case["now"].as_f64().ok_or("time")?)?;
        assert_eq!(stable, expected[i]["stable"].as_bool().ok_or("stable")?);
        let reading = vision.camera_reading();
        for key in ["reason", "inliers"] {
            assert_eq!(reading[key], expected[i]["reading"][key]);
        }
        if reading["displacement"].is_null() {
            assert_eq!(
                reading["displacement"],
                expected[i]["reading"]["displacement"]
            );
        } else {
            for axis in 0..2 {
                let a = reading["displacement"][axis]
                    .as_f64()
                    .ok_or("displacement")?;
                let b = expected[i]["reading"]["displacement"][axis]
                    .as_f64()
                    .ok_or("expected displacement")?;
                assert!((a - b).abs() <= 0.1);
            }
        }
    }
    for size in [(799, 450), (1267, 449), (900, 753)] {
        vision.set_frame(Arc::new(BgrFrame::zeros(Size::new(size.0, size.1)?)?));
        assert!(vision.normalize().is_err());
    }
    Ok(())
}

use d2bot::{
    common::{
        frame::{BgrFrame, Size},
        vision_ops::codec,
    },
    runs::summoner::{
        appearance_vision::SummonerAppearanceObserver, objective_vision::ArcaneObjectiveObserver,
    },
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, error::Error, path::Path};
mod support;
fn compare(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert!(
            (a.as_f64().unwrap_or(f64::NAN) - b.as_f64().unwrap_or(f64::NAN)).abs() <= 2e-5,
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
#[test]
fn right_band_lighting_keeps_current_platform_color_and_unique_body_guards()
-> Result<(), Box<dyn Error>> {
    let owner = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
    let observer = SummonerAppearanceObserver::load(owner.join("summoner-encounter.json"))?;
    let objective = ArcaneObjectiveObserver::load(&owner)?;
    let source = codec::load_color(
        owner.join("references/summoner/platform-corpse-right-band-lighting.png"),
    )?;
    let platform = objective.observe(source.view())?;
    let corpse = observer
        .observe_corpse(source.view(), platform.as_ref())?
        .ok_or("current corpse")?;
    assert_eq!(corpse.asset_id, "summoner-robe-corpse-lower-band-right");
    assert!((0.85..0.86).contains(&corpse.score));
    assert!(observer.observe_corpse(source.view(), None)?.is_none());
    let alive = codec::load_color(
        owner.join("references/summoner/platform-alive-right-band-negative.png"),
    )?;
    assert!(
        observer
            .observe_corpse(alive.view(), objective.observe(alive.view())?.as_ref())?
            .is_none()
    );
    let mut erased = source.clone();
    for y in 625..710 {
        erased.pixels_mut()[(y * 1922 + 975) * 3..(y * 1922 + 1105) * 3].fill(0);
    }
    assert!(
        observer
            .observe_corpse(erased.view(), objective.observe(erased.view())?.as_ref())?
            .is_none()
    );
    let mut gray = source.clone();
    for y in 625..710 {
        for x in 975..1105 {
            let p = &mut gray.pixels_mut()[(y * 1922 + x) * 3..(y * 1922 + x) * 3 + 3];
            let value = ((u16::from(p[0]) + u16::from(p[1]) + u16::from(p[2])) / 3) as u8;
            p.fill(value);
        }
    }
    assert!(
        observer
            .observe_corpse(gray.view(), objective.observe(gray.view())?.as_ref())?
            .is_none()
    );
    let mut duplicate = source.clone();
    for y in 0..14 {
        duplicate.pixels_mut()[((650 + y) * 1922 + 1200) * 3..((650 + y) * 1922 + 1232) * 3]
            .copy_from_slice(
                &source.pixels()[((677 + y) * 1922 + 1050) * 3..((677 + y) * 1922 + 1082) * 3],
            );
    }
    assert!(
        observer
            .observe_corpse(duplicate.view(), platform.as_ref())?
            .is_none()
    );
    Ok(())
}

#[test]
fn reviewed_alive_corpse_parts_require_current_platform_and_unique_native_body()
-> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let directory = root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
    let observer = SummonerAppearanceObserver::load(directory.join("summoner-encounter.json"))?;
    let objective = ArcaneObjectiveObserver::load(&directory)?;
    let data: Value =
        serde_json::from_slice(&std::fs::read(directory.join("summoner-encounter.json"))?)?;
    let mut paths = BTreeSet::new();
    paths.insert(
        data["source"]["reference"]
            .as_str()
            .ok_or("reference")?
            .to_owned(),
    );
    for part in data["parts"].as_array().ok_or("parts")? {
        if let Some(path) = part["source"]["reference"].as_str() {
            paths.insert(path.to_owned());
        }
    }
    for name in [
        "platform-corpse-face-down-lighting-before.png",
        "platform-corpse-face-down-lighting-after-approach.png",
        "platform-corpse-face-down-lighting.png",
        "platform-corpse-looped-hem-bone-cover.png",
        "platform-corpse-looped-hem-bone-cover-followup.png",
    ] {
        paths.insert(format!(
            "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/summoner/{name}"
        ));
    }
    let mut cases: Vec<Value> = paths
        .iter()
        .map(|path| json!({"frame":{"path":path}}))
        .collect();
    for (name, opposite) in [
        ("platform-alive-cast-unrecognized.png", "corpse"),
        ("platform-alive-rear-unrecognized.png", "corpse"),
        ("platform-corpse-merc-overlap-unrecognized.png", "alive"),
    ] {
        cases.push(json!({"frame":{"path":format!("assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/summoner/{name}")},"opposite":opposite}));
    }
    let path = "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/summoner/platform-corpse-face-down.png";
    cases.push(json!({"frame":{"path":path},"platform":false}));
    cases.push(json!({"frame":{"path":path,"clear":[[535,675,625,745]]}}));
    cases.push(json!({"frame":{"path":path,"copy":[{"box":[548,681,615,729],"at":[748,561]}]}}));
    cases.push(json!({"frame":{"path":path,"clear":[[0,0,1922,1140]]}}));
    let expected = support::legacy_expectation(
        "observer-oracle",
        &json!({"kind":"appearance","cases":cases}),
    )?;
    for (index, case) in cases.iter().enumerate() {
        let raw = &case["frame"];
        let mut frame = codec::load_color(root.join(raw["path"].as_str().ok_or("path")?))?;
        if let Some(boxes) = raw["clear"].as_array() {
            for bounds in boxes {
                let [l, t, r, b]: [usize; 4] = serde_json::from_value(bounds.clone())?;
                for y in t..b {
                    frame.pixels_mut()[(y * 1922 + l) * 3..(y * 1922 + r) * 3].fill(0);
                }
            }
        }
        if let Some(copies) = raw["copy"].as_array() {
            for item in copies {
                let [l, t, r, b]: [usize; 4] = serde_json::from_value(item["box"].clone())?;
                let [x, y]: [usize; 2] = serde_json::from_value(item["at"].clone())?;
                let source = frame.clone();
                for row in 0..b - t {
                    frame.pixels_mut()
                        [((y + row) * 1922 + x) * 3..((y + row) * 1922 + x + r - l) * 3]
                        .copy_from_slice(
                            &source.pixels()
                                [((t + row) * 1922 + l) * 3..((t + row) * 1922 + r) * 3],
                        );
                }
            }
        }
        let platform = if case["platform"] == false {
            None
        } else {
            objective.observe(frame.view())?
        };
        let actual = json!({"platform":platform,"alive":observer.observe(frame.view(),platform.as_ref())?,"corpse":observer.observe_corpse(frame.view(),platform.as_ref())?});
        if let Some(opposite) = case["opposite"].as_str() {
            assert!(
                actual[opposite].is_null(),
                "false {opposite} in {}",
                raw["path"]
            );
        }
        compare(&actual, &expected[index], &format!("case{index}"));
    }
    assert!(
        observer
            .observe(BgrFrame::zeros(Size::new(100, 100)?)?.view(), None)
            .is_err()
    );
    Ok(())
}

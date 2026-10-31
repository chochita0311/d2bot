use d2bot::{
    common::{
        frame::{BgrFrame, Size},
        vision_ops::codec,
    },
    runs::summoner::{
        canyon_finish::{CanyonApproachObserver, RedPortalObserver, journal_landing},
        journal_vision::HorazonJournalObserver,
        navigation::PathContext,
        navigation_execution::load_arcane_step_planner,
    },
};
use serde_json::{Value, json};
use std::{error::Error, path::Path, sync::Arc};
mod support;
type TestResult = Result<(), Box<dyn Error>>;
fn compare(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(a.as_f64(), b.as_f64(), "{path}"),
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
fn image(raw: &Value) -> Result<BgrFrame, Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut frame = codec::load_color(root.join(raw["path"].as_str().ok_or("path")?))?;
    for rect in raw["clear"].as_array().into_iter().flatten() {
        let [l, t, r, b]: [usize; 4] = serde_json::from_value(rect.clone())?;
        for y in t..b {
            for x in l..r {
                frame.pixels_mut()[(y * 1922 + x) * 3..(y * 1922 + x + 1) * 3].fill(0);
            }
        }
    }
    if raw["shift"].is_array() {
        let old = frame;
        frame = BgrFrame::zeros(Size::new(1922, 1140)?)?;
        for y in 0..1139 {
            for x in 0..1921 {
                frame.pixels_mut()[((y + 1) * 1922 + x + 1) * 3..((y + 1) * 1922 + x + 2) * 3]
                    .copy_from_slice(&old.pixels()[(y * 1922 + x) * 3..(y * 1922 + x + 1) * 3]);
            }
        }
    }
    if let Some(boxes) = raw["keep"].as_array() {
        let old = frame;
        frame = BgrFrame::zeros(Size::new(1922, 1140)?)?;
        for rect in boxes {
            let [l, t, r, b]: [usize; 4] = serde_json::from_value(rect.clone())?;
            for y in t..b {
                frame.pixels_mut()[(y * 1922 + l) * 3..(y * 1922 + r) * 3]
                    .copy_from_slice(&old.pixels()[(y * 1922 + l) * 3..(y * 1922 + r) * 3]);
            }
        }
    }
    for copy in raw["copy"].as_array().into_iter().flatten() {
        let [l, t, r, b]: [usize; 4] = serde_json::from_value(copy["box"].clone())?;
        let [x, y]: [usize; 2] = serde_json::from_value(copy["at"].clone())?;
        let old = frame.clone();
        for row in 0..b - t {
            frame.pixels_mut()[((y + row) * 1922 + x) * 3..((y + row) * 1922 + x + r - l) * 3]
                .copy_from_slice(
                    &old.pixels()[((t + row) * 1922 + l) * 3..((t + row) * 1922 + r) * 3],
                );
        }
    }
    Ok(frame)
}
#[test]
fn current_portal_canyon_steps_and_journal_landing_match_original() -> TestResult {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let owner = root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
    let portal = RedPortalObserver::load(&owner)?;
    let stairs = CanyonApproachObserver::load(root)?;
    let mut cases = vec![];
    for name in [
        "platform-corpse-north.png",
        "journal-red-portal.png",
        "journal-portal-hover.png",
    ] {
        cases.push(json!({"frame":{"path":format!("assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/summoner/{name}")}}));
    }
    for name in [
        "portal-arrival.png",
        "waypoint-arrival.png",
        "portal-arrival-dark-steps.png",
        "waypoint-center-occupied.png",
        "waypoint-center-occupied-day.png",
    ] {
        cases.push(json!({"frame":{"path":format!("assets/regions/canyon-of-the-magi/remastered-ko-1922x1140/references/{name}")}}));
    }
    let hover = cases[2]["frame"]["path"].clone();
    let dark = cases[5]["frame"]["path"].clone();
    for path in [&hover, &dark] {
        cases.push(json!({"frame":{"path":path,"shift":[1,1]}}));
    }
    for (l, r) in [(925, 988), (988, 1052), (1052, 1115), (925, 1115)] {
        cases.push(json!({"frame":{"path":hover,"clear":[[l,497,r,523]]}}));
    }
    cases.push(json!({"frame":{"path":hover,"keep":[[925,497,1115,523]]}}));
    cases.push(json!({"frame":{"path":hover,"copy":[{"box":[800,350,1200,750],"at":[1200,250]}]}}));
    for (l, r) in [(951, 1010), (1010, 1070), (951, 1070)] {
        cases.push(json!({"frame":{"path":dark,"clear":[[l,129,r,180]]}}));
    }
    for pose in [
        "back-robe",
        "front-open",
        "hit-turn",
        "lower-robe",
        "raised-arm",
    ] {
        cases.push(json!({"frame":{"path":format!("assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/summoner/platform-alive-{pose}.png")},"landing":true}));
    }
    let expected =
        support::legacy_expectation("observer-oracle", &json!({"kind":"canyon","cases":cases}))?;
    let (mut ground, path) = load_arcane_step_planner(root, false, "sorceress")?;
    let journal_observer = HorazonJournalObserver::load(owner.join("journal-exit.json"))?;
    for (i, c) in cases.iter().enumerate() {
        let frame = Arc::new(image(&c["frame"])?);
        let mut row =
            json!({"stairs":stairs.observe(frame.view())?,"portal":portal.observe(frame.view())?});
        if c["landing"] == true {
            let player = ground.observe(&frame, Some(1.0))?.point.ok_or("player")?;
            let journal = journal_observer.observe(frame.view())?.ok_or("journal")?;
            let view = path.observe(
                frame.view(),
                PathContext {
                    sequence: 1,
                    generation: 0,
                    captured_at: 1.0,
                    frame_size: frame.size(),
                    viewport: d2bot::common::frame::PixelRect::new(120, 180, 1720, 890)?,
                    anchor: player.map(f64::from),
                },
            )?;
            row["player"] = json!(player);
            row["journal"] = json!(journal);
            row["landing"] = json!(journal_landing(
                &path,
                &view,
                journal,
                player.map(f64::from)
            )?);
            let teleport = path.teleport().ok_or("teleport")?;
            row["teleport"] = json!({"maximum_distance":teleport.maximum_distance,"clearance_pixels":teleport.clearance_pixels,"preferred_distance":teleport.preferred_distance});
        }
        compare(&row, &expected[i], &format!("canyon image {i}"));
    }
    Ok(())
}

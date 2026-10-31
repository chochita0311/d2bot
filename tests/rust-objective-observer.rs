use d2bot::{
    common::{
        frame::{BgrFrame, Size},
        vision_ops::codec,
    },
    runs::summoner::{
        journal_vision::HorazonJournalObserver, objective_vision::ArcaneObjectiveObserver,
    },
};
use serde_json::{Value, json};
use std::{error::Error, path::Path};
mod support;
fn compare(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert!(
            (a.as_f64().unwrap_or(f64::NAN) - b.as_f64().unwrap_or(f64::NAN)).abs() <= 2e-5,
            "{a} != {b}"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                compare(a, b);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (k, a) in a {
                compare(a, &b[k]);
            }
        }
        _ => assert_eq!(a, b),
    }
}
#[test]
fn native_journal_runes_slots_and_ground_match_current_reference_evidence()
-> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let directory = root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
    let observer = ArcaneObjectiveObserver::load(&directory)?;
    let journal = HorazonJournalObserver::load(directory.join("journal-exit.json"))?;
    let mut cases = Vec::new();
    for name in [
        "platform-journal.png",
        "platform-labels-combat.png",
        "platform-slot-variation-a.png",
        "platform-slot-variation-b.png",
        "platform-glyph-unverified.png",
        "platform-corpse-wide-side.png",
        "platform-corpse-front-hem.png",
        "platform-corpse-front-hem-followup.png",
    ] {
        let path = format!(
            "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/summoner/{name}"
        );
        cases.push(json!({"frame":{"path":path}}));
    }
    let front = cases[6]["frame"]["path"].clone();
    cases.push(json!({"frame":{"path":front,"shift":[1,1]}}));
    cases.push(json!({"frame":{"path":front,"keep":[[971,258,1021,309]]}}));
    let wide = cases[5]["frame"]["path"].clone();
    cases.push(json!({"frame":{"path":wide,"clear":[[900,400,965,460]]}}));
    cases.push(json!({"frame":{"path":wide,"keep":[[913,405,946,442]]}}));
    cases.push(json!({"frame":{"path":wide,"clear":[[0,0,1922,1140]]}}));
    let expected = support::legacy_expectation(
        "observer-oracle",
        &json!({"kind":"objective","cases":cases}),
    )?;
    for (index, case) in cases.iter().enumerate() {
        let raw = &case["frame"];
        let original = codec::load_color(root.join(raw["path"].as_str().ok_or("path")?))?;
        let mut frame = original.clone();
        if raw["shift"].is_array() {
            frame = BgrFrame::zeros(Size::new(1922, 1140)?)?;
            for y in 0..1139 {
                let begin = ((y + 1) * 1922 + 1) * 3;
                frame.pixels_mut()[begin..begin + 1921 * 3]
                    .copy_from_slice(&original.view().row(y).ok_or("row")?[..1921 * 3]);
            }
        }
        if let Some(boxes) = raw["clear"].as_array() {
            for bounds in boxes {
                let [l, t, r, b]: [usize; 4] = serde_json::from_value(bounds.clone())?;
                for y in t..b {
                    frame.pixels_mut()[(y * 1922 + l) * 3..(y * 1922 + r) * 3].fill(0);
                }
            }
        }
        if let Some(boxes) = raw["keep"].as_array() {
            let mut retained = BgrFrame::zeros(Size::new(1922, 1140)?)?;
            for bounds in boxes {
                let [l, t, r, b]: [usize; 4] = serde_json::from_value(bounds.clone())?;
                for y in t..b {
                    let range = (y * 1922 + l) * 3..(y * 1922 + r) * 3;
                    retained.pixels_mut()[range.clone()].copy_from_slice(&frame.pixels()[range]);
                }
            }
            frame = retained;
        }
        let actual = json!({"reading":observer.observe(frame.view())?,"matches":journal.matches(frame.view())?,"point":journal.observe(frame.view())?});
        eprintln!("objective case {index}");
        compare(&actual, &expected[index]);
    }
    assert!(
        observer
            .observe(BgrFrame::zeros(Size::new(100, 100)?)?.view())
            .is_err()
    );
    Ok(())
}

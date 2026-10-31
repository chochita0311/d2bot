use d2bot::{
    common::{
        frame::{BgrFrame, Size},
        vision_ops::codec,
    },
    runs::summoner::scene_vision::ArcaneHazardObserver,
};
use serde_json::{Value, json};
use std::{error::Error, path::Path};
mod support;

fn compare(actual: &Value, expected: &Value) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => {
            assert!(
                (a.as_f64().unwrap_or(f64::NAN) - b.as_f64().unwrap_or(f64::NAN)).abs() <= 2e-5,
                "{a} != {b}"
            );
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                compare(a, b);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (key, a) in a {
                compare(a, &b[key]);
            }
        }
        _ => assert_eq!(actual, expected),
    }
}

#[test]
fn current_hazard_preserves_roi_order_duplicates_sequence_and_age() -> Result<(), Box<dyn Error>> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
    let manifest: Value = serde_json::from_slice(&std::fs::read(directory.join("manifest.json"))?)?;
    let mut observer = ArcaneHazardObserver::load(directory.join("manifest.json"))?;
    let cases = json!([
        {"ground":[960,580],"at":10,"now":10,"paste":[{"id":"ghoul-alive","at":[800,330]}]},
        {"ground":[960,580],"aim":[1380,260],"at":10.1,"now":10.1,"paste":[{"id":"ghoul-alive","at":[800,330]},{"id":"hell-clan-alive","at":[1360,120]}]},
        {"ground":[960,580],"aim":[960,580],"at":10.2,"now":10.2,"paste":[{"id":"ghoul-alive","at":[800,330]}]},
        {"ground":[960,580],"aim":[1000,580],"at":10.3,"now":10.3,"paste":[{"id":"ghoul-alive","at":[800,330]}]},
        {"ground":[960,580],"at":10.4,"now":10.65,"paste":[{"id":"ghoul-alive","at":[800,330]}]},
        {"ground":[960,580],"at":10.5,"now":10.750001,"paste":[{"id":"ghoul-alive","at":[800,330]}]},
        {"ground":[960,580],"at":10.6,"now":10.5,"paste":[{"id":"ghoul-alive","at":[800,330]}]},
        {"ground":[960,580],"at":10.7,"now":10.7,"paste":[{"id":"ghoul-corpse","at":[800,330]}]},
        {"ground":[960,580],"at":10.8,"now":10.8}
    ]);
    let expected =
        support::legacy_expectation("observer-oracle", &json!({"kind":"hazard","cases":cases}))?;
    for (index, case) in cases.as_array().ok_or("cases")?.iter().enumerate() {
        let mut frame = BgrFrame::zeros(Size::new(1922, 1140)?)?;
        if let Some(items) = case["paste"].as_array() {
            for item in items {
                let rule = manifest["templates"]
                    .as_array()
                    .ok_or("templates")?
                    .iter()
                    .find(|r| r["id"] == item["id"])
                    .ok_or("template")?;
                let template =
                    codec::load_color(directory.join(rule["path"].as_str().ok_or("path")?))?;
                let [x, y]: [usize; 2] = serde_json::from_value(item["at"].clone())?;
                for row in 0..template.size().height() {
                    let begin = ((y + row) * 1922 + x) * 3;
                    frame.pixels_mut()[begin..begin + template.size().width() * 3]
                        .copy_from_slice(template.view().row(row).ok_or("row")?);
                }
            }
        }
        let hits = observer.observe(
            frame.view(),
            serde_json::from_value(case["ground"].clone())?,
            case["at"].as_f64().ok_or("at")?,
            case["now"].as_f64().ok_or("now")?,
            serde_json::from_value(case["aim"].clone())?,
        )?;
        compare(&serde_json::to_value(hits)?, &expected[index]);
        assert_eq!(observer.sequence(), index as u64 + 1);
    }
    Ok(())
}

use super::*;
use crate::common::frame::Size;
use serde_json::{Value, json};

#[test]
fn source_bgr_annotation_pixels() -> Result<(), Box<dyn std::error::Error>> {
    let mut cases = vec![];
    for first in [[0, 0], [-5, -7], [3, 5], [95, 63], [150, 100], [-100, -100]] {
        for last in [[20, 20], [95, 63], [-1, 10], [3, 5]] {
            cases.push(json!({"size":[96,64],"operations":[{"kind":"rectangle","first":first,"last":last,"color":[0,255,0]}]}));
        }
        for (radius, filled) in [(3, true), (8, false), (10, false), (0, false)] {
            cases.push(json!({"size":[96,64],"operations":[{"kind":"circle","center":first,"radius":radius,"filled":filled,"color":[255,0,255]}]}));
        }
    }
    let alphabet: String = (32..127).filter_map(char::from_u32).collect();
    for text in [
        alphabet.as_str(),
        "한글 α é 😀 | watching | DRY-RUN | PAUSED",
        "gem 0.92",
        "A",
    ] {
        for scale in [0.6, 0.8] {
            for origin in [[20, 30], [-3, 8], [20, 95]] {
                cases.push(json!({"size":[1900,80],"operations":[{"kind":"text","text":text,"origin":origin,"scale":scale,"color":[0,215,255]}]}));
            }
        }
    }
    cases.push(json!({"size":[1,1],"operations":[{"kind":"text","text":"Test","origin":[0,0],"scale":0.6,"color":[255,255,255]},{"kind":"circle","center":[0,0],"radius":3,"filled":true,"color":[0,255,0]}]}));
    for case in &mut cases {
        let width = case["size"][0].as_u64().ok_or("width")? as usize;
        let height = case["size"][1].as_u64().ok_or("height")? as usize;
        let mut frame = BgrFrame::from_vec(
            Size::new(width, height)?,
            (0..width * height * 3)
                .map(|n| ((n * 37 + 11) % 256) as u8)
                .collect(),
        )?;
        let point = |v: &Value| -> Result<[i32; 2], Box<dyn std::error::Error>> {
            Ok([
                v[0].as_i64().ok_or("x")? as i32,
                v[1].as_i64().ok_or("y")? as i32,
            ])
        };
        for op in case["operations"].as_array().ok_or("operations")? {
            let color = [
                op["color"][0].as_u64().ok_or("b")? as u8,
                op["color"][1].as_u64().ok_or("g")? as u8,
                op["color"][2].as_u64().ok_or("r")? as u8,
            ];
            match op["kind"].as_str() {
                Some("rectangle") => {
                    rectangle(&mut frame, point(&op["first"])?, point(&op["last"])?, color)?
                }
                Some("circle") => circle(
                    &mut frame,
                    point(&op["center"])?,
                    op["radius"].as_u64().ok_or("radius")? as u8,
                    op["filled"].as_bool().ok_or("filled")?,
                    color,
                )?,
                _ => text(
                    &mut frame,
                    op["text"].as_str().ok_or("text")?,
                    point(&op["origin"])?,
                    op["scale"].as_f64().ok_or("scale")?,
                    color,
                )?,
            }
        }
        case["actual"] = json!(frame.pixels());
    }
    let result = crate::test_support::legacy_expectation("drawing-oracle", &json!(cases))?;
    for (mut case, result) in cases
        .into_iter()
        .zip(result.as_array().ok_or("oracle list")?)
    {
        case.as_object_mut().ok_or("case")?.remove("actual");
        assert_eq!(result["equal"], true, "{case}: {result}");
    }
    Ok(())
}

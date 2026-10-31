use d2bot::{
    common::{
        frame::{BgrFrame, GrayFrame, PixelRect, Size},
        vision_ops::codec,
    },
    runs::summoner::navigation::{
        ArcanePathObserver, ChooseOptions, PathContext, PathSettings, TeleportGeometry, Wing,
    },
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{error::Error, path::Path};
mod support;
#[test]
fn path_policy_rejects_invalid_required_geometry_before_observation() -> Result<(), Box<dyn Error>>
{
    assert!(PathSettings::new(1, 0.5, 20.0, 48.0, 40.0, 16.0, 8).is_err());
    assert!(PathSettings::new(4, f64::NAN, 20.0, 48.0, 40.0, 16.0, 8).is_err());
    assert!(TeleportGeometry::new(f64::INFINITY, 12.0, None).is_err());
    assert!(TeleportGeometry::new(120.0, 120.0, None).is_err());
    assert!(
        TeleportGeometry::from_value(
            &json!({"maximum_distance":120,"clearance_pixels":12,"preferred_distance":null})
        )
        .is_err()
    );
    assert!(
        TeleportGeometry::from_value(
            &json!({"maximum_distance":120,"clearance_pixels":12,"typo":1})
        )
        .is_err()
    );
    let settings = PathSettings::new(4, 0.5, 20.0, 48.0, 40.0, 16.0, 8)?;
    assert!(
        ArcanePathObserver::new(
            settings.clone(),
            None,
            None,
            Some(serde_json::from_value(
                json!({"neutral_pixel_fraction":true})
            )?)
        )
        .is_ok()
    );
    assert!(
        ArcanePathObserver::new(
            settings,
            None,
            None,
            Some(serde_json::from_value(
                json!({"neutral_pixel_fraction":false})
            )?)
        )
        .is_err()
    );
    Ok(())
}
fn compare(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert!(
            (a.as_f64().unwrap_or(f64::NAN) - b.as_f64().unwrap_or(f64::NAN)).abs() <= 1e-9,
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
fn exercise(groups: Vec<Value>) -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let expected = support::legacy_expectation("navigation-oracle", &json!(groups))?;
    for (g, group) in groups.iter().enumerate() {
        let observer = ArcanePathObserver::new(
            PathSettings::from_value(&group["settings"])?,
            if group["terrain"].is_null() {
                None
            } else {
                Some(serde_json::from_value(group["terrain"].clone())?)
            },
            if group["teleport"].is_null() {
                None
            } else {
                Some(TeleportGeometry::from_value(&group["teleport"])?)
            },
            if group["automap_overlay"].is_null() {
                None
            } else {
                Some(serde_json::from_value(group["automap_overlay"].clone())?)
            },
        )?;
        for (index, case) in group["cases"].as_array().ok_or("cases")?.iter().enumerate() {
            let anchor = serde_json::from_value(case["anchor"].clone())?;
            let view = if case["path"].is_string() || case["automap_reference_frame"] == true {
                let frame = if let Some(path) = case["path"].as_str() {
                    codec::load_color(root.join(path))?
                } else {
                    let owner =
                        root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
                    let refs: Value = serde_json::from_slice(&std::fs::read(
                        owner.join("automap-floor-references.json"),
                    )?)?;
                    let mut frame = BgrFrame::zeros(Size::new(1922, 1140)?)?;
                    for reference in refs["references"].as_array().ok_or("references")? {
                        let [l, t, r, b]: [usize; 4] =
                            serde_json::from_value(reference["box"].clone())?;
                        let image = codec::load_color(
                            owner.join(reference["path"].as_str().ok_or("path")?),
                        )?;
                        for row in 0..b - t {
                            frame.pixels_mut()
                                [((t + row) * 1922 + l) * 3..((t + row) * 1922 + r) * 3]
                                .copy_from_slice(image.view().row(row).ok_or("row")?);
                        }
                    }
                    frame
                };
                let [l, t, r, b]: [usize; 4] = serde_json::from_value(case["viewport"].clone())?;
                observer.observe(
                    frame.view(),
                    PathContext {
                        sequence: 1,
                        generation: 0,
                        captured_at: 1.0,
                        frame_size: frame.size(),
                        viewport: PixelRect::new(l, t, r, b)?,
                        anchor,
                    },
                )?
            } else {
                let [w, h]: [usize; 2] = serde_json::from_value(case["size"].clone())?;
                let size = Size::new(w, h)?;
                let mut mask = GrayFrame::zeros(size)?;
                mask.pixels_mut().fill(u8::from(case["fill"] == true));
                for r in case["rects"].as_array().into_iter().flatten() {
                    let l = r[0].as_u64().ok_or("l")? as usize;
                    let t = r[1].as_u64().ok_or("t")? as usize;
                    let right = r[2].as_u64().ok_or("r")? as usize;
                    let bottom = r[3].as_u64().ok_or("b")? as usize;
                    for y in t..bottom {
                        mask.pixels_mut()[y * w + l..y * w + right].fill(u8::from(r[4] == true));
                    }
                }
                observer.from_mask(
                    mask.view(),
                    PathContext {
                        sequence: 1,
                        generation: 0,
                        captured_at: 1.0,
                        frame_size: size,
                        viewport: PixelRect::full(size),
                        anchor,
                    },
                )?
            };
            let mut choices = Vec::new();
            for option in case["choices"].as_array().ok_or("choices")? {
                let visited: Vec<[f64; 2]> = if option["visited"].is_null() {
                    Vec::new()
                } else {
                    serde_json::from_value(option["visited"].clone())?
                };
                let excluded: Vec<[f64; 4]> = if option["excluded"].is_null() {
                    Vec::new()
                } else {
                    serde_json::from_value(option["excluded"].clone())?
                };
                let wing: Wing = serde_json::from_value(option["wing"].clone())?;
                let result = observer.choose(
                    &view,
                    wing,
                    ChooseOptions {
                        position: if option["position"].is_null() {
                            [0.0, 0.0]
                        } else {
                            serde_json::from_value(option["position"].clone())?
                        },
                        visited: &visited,
                        destination: serde_json::from_value(option["destination"].clone())?,
                        excluded: &excluded,
                        allow_detour: option["allow_detour"] == true,
                        heading: serde_json::from_value(option["heading"].clone())?,
                        prefer_frontier: option["prefer_frontier"] == true,
                    },
                );
                choices.push(serde_json::to_value(result?)?);
            }
            let floor = view.floor().to_owned()?;
            let excluded = view.excluded_pixels().map(|m| m.to_owned()).transpose()?;
            let actual = json!({"size":[view.floor().size().width(),view.floor().size().height()],"floor":format!("{:x}",Sha256::digest(floor.pixels())),"excluded":excluded.map(|m|format!("{:x}",Sha256::digest(m.pixels()))),"choices":choices});
            compare(
                &actual,
                &expected[g][index],
                &format!("group{g}.case{index}"),
            );
        }
    }
    Ok(())
}
#[test]
fn connected_and_teleport_paths_preserve_order_geometry_and_refusals() -> Result<(), Box<dyn Error>>
{
    let settings = json!({"cell_pixels":4,"floor_fraction":0.5,"seed_radius":20,"maximum_step":48,"turn_degrees":40,"revisit_radius":16,"minimum_component_cells":8});
    let choices = json!([{"wing":"north"},{"wing":"east"},{"wing":"south"},{"wing":"west"},{"wing":"north","destination":[60,-60]},{"wing":"north","visited":[[20,-20],[60,-40]]},{"wing":"north","excluded":[[100,0,150,100]]}]);
    let mut cases = vec![
        json!({"size":[200,200],"anchor":[100,100],"fill":true,"choices":choices}),
        json!({"size":[200,200],"anchor":[100,100],"choices":choices}),
        json!({"size":[200,200],"anchor":[100,100],"rects":[[60,80,116,116,true],[136,0,200,64,true]],"choices":choices}),
        json!({"size":[200,200],"anchor":[100,100],"rects":[[96,96,104,104,true],[100,76,180,92,true]],"choices":choices}),
    ];
    let mut state = 701u32;
    for _ in 0..12 {
        let mut rects = Vec::new();
        for _ in 0..8 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let x = (state % 160) as usize;
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let y = (state % 160) as usize;
            rects.push(json!([x, y, (x + 48).min(200), (y + 48).min(200), true]));
        }
        cases.push(json!({"size":[200,200],"anchor":[100,100],"rects":rects,"choices":choices}));
    }
    let gap = json!({"size":[200,200],"anchor":[100,100],"rects":[[64,80,112,120,true],[140,20,196,84,true]],"choices":choices});
    let narrow = json!({"size":[200,200],"anchor":[100,100],"rects":[[188,0,200,200,true],[108,80,184,92,true]],"choices":choices});
    let detour_choices = json!([{"wing":"north"},{"wing":"north","allow_detour":true},{"wing":"north","allow_detour":true,"prefer_frontier":true},{"wing":"north","heading":[-1,-0.5]},{"wing":"north","destination":[-180,-100]}]);
    let detours = vec![
        json!({"size":[400,200],"anchor":[300,140],"rects":[[20,20,160,100,true]],"choices":detour_choices}),
        json!({"size":[400,240],"anchor":[300,140],"rects":[[140,100,380,180,true],[0,80,180,140,true]],"choices":detour_choices}),
    ];
    exercise(vec![
        json!({"settings":settings,"cases":cases}),
        json!({"settings":settings,"teleport":{"maximum_distance":120,"clearance_pixels":12},"cases":[gap,narrow]}),
        json!({"settings":settings,"teleport":{"maximum_distance":300,"preferred_distance":300,"clearance_pixels":12},"cases":detours}),
    ])
}
#[test]
fn native_palette_and_automap_grids_match_original_pixel_decisions() -> Result<(), Box<dyn Error>> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
    let calibration: Value = serde_json::from_slice(&std::fs::read(
        directory.join("navigation-calibration.json"),
    )?)?;
    let cases = json!([{"path":"assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/navigation/return-portal-corner-short-goal.png","viewport":[242,55,1682,900],"anchor":[962,580],"choices":[{"wing":"north"},{"wing":"south"},{"wing":"north","prefer_frontier":true}]},{"automap_reference_frame":true,"viewport":[24,55,1900,900],"anchor":[1480,600],"choices":[{"wing":"north"}]}]);
    exercise(vec![
        json!({"settings":calibration["path"],"terrain":calibration["terrain"],"teleport":calibration["teleport"],"automap_overlay":calibration["automap_overlay"],"cases":cases}),
    ])
}

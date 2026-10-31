mod support;

use d2bot::common::frame::{BgrFrame, BgrView, PixelRect, Size};
use d2bot::common::frame_geometry::{
    CurrentGeometry, FrameGeometry, GeometryError, PhysicalTarget, ReferenceLayout,
    desktop_to_frame, frame_to_desktop, geometry_is_current, rect_from_value,
};
use serde_json::{Value, json};
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

fn target(raw: &Value) -> Result<PhysicalTarget, Box<dyn Error>> {
    Ok(PhysicalTarget::new(
        i32::try_from(raw["left"].as_i64().ok_or("left")?)?,
        i32::try_from(raw["top"].as_i64().ok_or("top")?)?,
        usize::try_from(raw["width"].as_u64().ok_or("width")?)?,
        usize::try_from(raw["height"].as_u64().ok_or("height")?)?,
    )?)
}

fn outcome<T: Into<Value>>(result: Result<T, GeometryError>) -> Value {
    match result {
        Ok(value) => json!({"value":value.into()}),
        Err(error) => json!({"error_type":error.kind(),"error_message":error.message()}),
    }
}

fn rust_geometry(request: &Value) -> Result<Value, Box<dyn Error>> {
    let target = target(&request["target"])?;
    let layout = request
        .get("layout")
        .filter(|value| !value.is_null())
        .map(ReferenceLayout::from_value)
        .transpose();
    let result = layout.and_then(|layout| {
        FrameGeometry::create(
            target,
            layout,
            request.get("client").map(rect_from_value).transpose()?,
        )
    });
    let geometry = match result {
        Ok(geometry) => geometry,
        Err(error) => return Ok(outcome::<Value>(Err(error))),
    };
    let mut queries = Vec::new();
    for query in request["queries"].as_array().ok_or("queries")? {
        let mut point = [0.0; 2];
        if let Some(values) = query["point"].as_array() {
            for (output, value) in point.iter_mut().zip(values) {
                *output = match value {
                    Value::String(text) => text.parse::<f64>()?,
                    _ => value.as_f64().ok_or("point")?,
                };
            }
        }
        let current = CurrentGeometry {
            target: query
                .get("current_target")
                .map(target_from)
                .transpose()?
                .unwrap_or(target),
            client: if query["missing_client_query"] == true {
                None
            } else {
                Some(
                    query
                        .get("current_client")
                        .map(rect_from_value)
                        .transpose()?
                        .unwrap_or(geometry.source_client()),
                )
            },
        };
        let mapped = if request["legacy"] == true {
            None
        } else {
            Some(&geometry)
        };
        let value = match query["op"].as_str().ok_or("op")? {
            "to" => outcome(geometry.to_desktop(point).map(|value| json!(value))),
            "from" => outcome(geometry.from_desktop(point).map(|value| json!(value))),
            "frame_to" => outcome(
                frame_to_desktop(target, mapped, point, || Ok(current)).map(|value| json!(value)),
            ),
            "desktop_to" => outcome(
                desktop_to_frame(target, mapped, point, || Ok(current)).map(|value| json!(value)),
            ),
            "current" => outcome(geometry_is_current(mapped, || Ok(current)).map(Value::Bool)),
            _ => return Err("unknown oracle op".into()),
        };
        queries.push(value);
    }
    Ok(
        json!({"value":{"geometry":geometry.as_value(),"native":geometry.is_native(),"queries":queries}}),
    )
}

fn target_from(raw: &Value) -> Result<PhysicalTarget, Box<dyn Error>> {
    target(raw)
}

fn assert_near(actual: &Value, expected: &Value) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => {
            if let (Some(a), Some(b)) = (a.as_f64(), b.as_f64()) {
                assert!((a - b).abs() <= 1e-10, "{actual} != {expected}");
            } else {
                assert_eq!(actual, expected);
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                assert_near(a, b);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (key, a) in a {
                assert_near(a, &b[key]);
            }
        }
        _ => assert_eq!(actual, expected),
    }
}

fn compare(request: Value) -> TestResult {
    let expected = support::legacy_expectation("geometry-oracle", &request)?;
    assert_near(&rust_geometry(&request)?, &expected);
    Ok(())
}

fn scaled() -> Value {
    json!({"target":{"left":-300,"top":140,"width":126,"height":84},
        "layout":{"schema":1,"frame_size":[200,140],"client_box":[10,30,190,130],"aspect_tolerance":0.01},
        "client":[3,12,123,79],"queries":[]})
}

#[test]
fn pixel_centers_borders_inverse_and_negative_origins_match_python() -> TestResult {
    let mut request = scaled();
    let mut queries = Vec::new();
    for point in [
        [0.0, 0.0],
        [8.0, 20.0],
        [10.0, 30.0],
        [100.0, 70.0],
        [189.0, 129.0],
        [199.0, 139.0],
        [10.5, 30.5],
        [189.5, 129.5],
    ] {
        queries.push(json!({"op":"to","point":point}));
        queries.push(json!({"op":"frame_to","point":point}));
    }
    for point in [
        [-300.0, 140.0],
        [-297.0, 152.0],
        [-237.0, 179.0],
        [-178.0, 218.0],
        [-175.0, 223.0],
    ] {
        queries.push(json!({"op":"from","point":point}));
        queries.push(json!({"op":"desktop_to","point":point}));
    }
    request["queries"] = json!(queries);
    compare(request)
}

#[test]
fn ties_even_adds_origin_before_rounding_and_native_legacy_keeps_fractions() -> TestResult {
    for origin in [-301, -300, 0, 1] {
        let mut request =
            json!({"target":{"left":origin,"top":origin,"width":200,"height":140},"queries":[]});
        let mut queries = Vec::new();
        for point in [[0.5, 1.5], [2.5, 3.5], [100.5, 70.5], [199.5, 139.5]] {
            queries.push(json!({"op":"to","point":point}));
            queries.push(json!({"op":"frame_to","point":point}));
        }
        request["queries"] = json!(queries);
        compare(request.clone())?;
        request["legacy"] = json!(true);
        compare(request)?;
    }
    Ok(())
}

#[test]
fn stale_target_client_and_optional_client_query_match_python() -> TestResult {
    let mut request = scaled();
    let mut queries = Vec::new();
    for op in ["current", "frame_to", "desktop_to"] {
        let point = if op == "desktop_to" {
            [-237, 179]
        } else {
            [100, 70]
        };
        queries.push(json!({"op":op,"point":point}));
        queries.push(json!({"op":op,"point":point,"current_target":{"left":-299,"top":140,"width":126,"height":84}}));
        queries.push(json!({"op":op,"point":point,"current_client":[4,12,124,79]}));
        queries.push(json!({"op":op,"point":point,"missing_client_query":true}));
    }
    request["queries"] = json!(queries);
    compare(request)
}

#[test]
fn outside_nonfinite_points_and_bad_aspect_match_python() -> TestResult {
    let mut request = scaled();
    request["queries"] = json!([
        {"op":"to","point":[200,70]}, {"op":"to","point":[-1,30]},
        {"op":"to","point":[100,"NaN"]}, {"op":"to","point":["inf",70]},
        {"op":"from","point":[-174,179]}, {"op":"from","point":[-237,224]}]);
    compare(request)?;
    for client in [json!([3, 12, 120, 70]), json!([3, 12, 127, 79])] {
        let mut request = scaled();
        request["client"] = client;
        compare(request)?;
    }
    Ok(())
}

#[test]
fn collapsed_borders_and_actual_reference_layout_match_python() -> TestResult {
    compare(
        json!({"target":{"left":-100,"top":-200,"width":100,"height":50},
        "layout":{"schema":1,"frame_size":[200,100],"client_box":[0,0,200,100],"aspect_tolerance":0.01},
        "client":[0,0,100,50],"queries":[{"op":"to","point":[0,0]},{"op":"to","point":[199,99]},{"op":"from","point":[-100,-200]}]}),
    )?;
    let path = "assets/ui/layouts/remastered-1922x1140/viewport.json";
    let layout = ReferenceLayout::load(path)?;
    let expected =
        support::legacy_expectation("geometry-oracle", &json!({"mode":"layout","path":path}))?;
    assert_near(
        &json!({"value":{"frame_size":[layout.size().width(),layout.size().height()],
        "client_box":[layout.client().left(),layout.client().top(),layout.client().right(),layout.client().bottom()],
        "aspect_tolerance":layout.aspect_tolerance()}}),
        &expected,
    );
    compare(
        json!({"target":{"left":135,"top":1061,"width":1696,"height":1005},
        "layout":serde_json::from_slice::<Value>(&std::fs::read(path)?)?,"client":[11,40,1685,994],
        "queries":[{"op":"desktop_to","point":[1454,1246]},{"op":"to","point":[963,577]}]}),
    )
}

#[test]
fn layout_validation_and_native_freshness_laziness() -> TestResult {
    for change in [
        json!({"schema":2}),
        json!({"frame_size":[0,2]}),
        json!({"frame_size":[true,2]}),
        json!({"client_box":[0,0,201,140]}),
        json!({"client_box":[-1,0,200,140]}),
        json!({"client_box":[0,0,0,140]}),
        json!({"aspect_tolerance":0.031}),
        json!({"aspect_tolerance":-0.1}),
    ] {
        let mut raw = scaled()["layout"].clone();
        for (key, value) in change.as_object().ok_or("change")? {
            raw[key] = value.clone();
        }
        let actual = outcome(ReferenceLayout::from_value(&raw).map(|_| Value::Null));
        let expected =
            support::legacy_expectation("geometry-oracle", &json!({"mode":"layout","layout":raw}))?;
        assert_eq!(actual, expected);
    }
    let called = std::cell::Cell::new(false);
    assert!(geometry_is_current(None, || {
        called.set(true);
        Ok(CurrentGeometry {
            target: PhysicalTarget::new(0, 0, 1, 1)?,
            client: None,
        })
    })?);
    assert!(!called.get());
    assert!(Size::new(usize::MAX, 2).is_err());
    assert!(ReferenceLayout::new(Size::new(2, 2)?, PixelRect::new(0, 0, 2, 2)?, f64::NAN).is_err());
    Ok(())
}

#[test]
fn owned_and_borrowed_frames_validate_stride_crop_and_copy() -> TestResult {
    let size = Size::new(3, 2)?;
    assert!(BgrFrame::from_vec(size, vec![0; 17]).is_err());
    assert!(BgrView::from_strided(size, 8, &[0; 18]).is_err());
    assert!(BgrView::from_strided(size, 12, &[0; 20]).is_err());
    assert!(BgrView::from_strided(size, usize::MAX, &[0; 18]).is_err());
    let mut data: Vec<u8> = (0..24).collect();
    let copied = {
        let view = BgrView::from_strided(size, 12, &data)?;
        assert_eq!(view.row(0), Some(&data[..9]));
        assert_eq!(view.row(1), Some(&data[12..21]));
        assert!(view.row(2).is_none());
        assert!(view.crop(PixelRect::new(0, 0, 4, 1)?).is_err());
        let crop = view.crop(PixelRect::new(1, 0, 3, 2)?)?;
        assert_eq!(crop.stride(), 12);
        let nested = crop.crop(PixelRect::new(0, 1, 1, 2)?)?;
        assert_eq!(nested.row(0), Some(&data[15..18]));
        crop.to_owned()?
    };
    data.fill(255);
    assert_eq!(copied.bytes(), &[3, 4, 5, 6, 7, 8, 15, 16, 17, 18, 19, 20]);
    assert_eq!(copied.view().stride(), 6);
    Ok(())
}

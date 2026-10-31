use super::*;
use crate::common::{
    error::ObservationError,
    frame::{BgrFrame, GrayFrame, Size},
    scene_motion::{BackgroundMotionObserver, CameraMotion, MotionLimits},
    vision_ops::codec,
};
use std::path::Path;
#[derive(Debug, Serialize)]
pub struct SavedRouteFlow {
    pub detour: Option<[f64; 2]>,
    pub motion: CameraMotion,
}
struct SavedContract {
    detour: Option<[f64; 2]>,
    ground: [i64; 2],
    frame_name: String,
}
fn reject(message: &str) -> RunError {
    RunError::Runtime(message.into())
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64() != Some(0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}
fn contract(
    report: &Value,
    wing: Wing,
    return_from: Option<Wing>,
) -> Result<SavedContract, RunError> {
    let navigation = &report["navigation"];
    let actual_wing = navigation.get("wing").unwrap_or(&report["wing"]);
    let actual_return = navigation
        .get("return_from")
        .unwrap_or(&report["return_from"]);
    let matches_return = match return_from {
        Some(from) => actual_return.as_str() == Some(from.name()),
        None => actual_return.is_null(),
    };
    if report["continuous"] != true || actual_wing.as_str() != Some(wing.name()) || !matches_return
    {
        return Err(reject("continuation route contract differs"));
    }
    let detour = match navigation.get("route_detour") {
        Some(Value::Null) => None,
        Some(Value::Array(values)) if values.len() == 2 => {
            let point = finite(&values[0])
                .zip(finite(&values[1]))
                .map(|(x, y)| [x, y])
                .ok_or_else(|| reject("continuation bend missing"))?;
            if point[0].hypot(point[1]) == 0.0 {
                return Err(reject("continuation bend missing"));
            }
            Some(point)
        }
        _ => return Err(reject("continuation bend missing")),
    };
    let rows = navigation["continuous_frames"]
        .as_array()
        .filter(|rows| !rows.is_empty())
        .ok_or_else(|| reject("continuation ground missing"))?;
    let point = &rows[rows.len() - 1]["ground"]["point"];
    if point.is_null() {
        return Err(reject("continuation ground missing"));
    }
    let ground = serde_json::from_value(point.clone()).map_err(|error| {
        RunError::from(ObservationError::from(error)).context("continuation ground missing")
    })?;
    let fallback = Value::String(format!("continuous-{:04}-after.png", rows.len()));
    let name = navigation
        .get("final_frame")
        .filter(|value| truthy(value))
        .unwrap_or(&fallback);
    let name = name
        .as_str()
        .ok_or_else(|| reject("continuation frame must belong to report directory"))?;
    if Path::new(name).file_name().and_then(|s| s.to_str()) != Some(name) {
        return Err(reject("continuation frame must belong to report directory"));
    }
    Ok(SavedContract {
        detour,
        ground,
        frame_name: name.into(),
    })
}
pub fn restore_route_flow(
    report_path: impl AsRef<Path>,
    frame: &BgrFrame,
    wing: Wing,
    return_from: Option<Wing>,
    maximum_pending_distance: f64,
) -> Result<SavedRouteFlow, RunError> {
    let report_path = report_path.as_ref();
    let bytes = std::fs::read(report_path).map_err(ObservationError::from)?;
    let report: Value = serde_json::from_slice(&bytes).map_err(ObservationError::from)?;
    let saved = contract(&report, wing, return_from)?;
    let parent = report_path.parent().unwrap_or_else(|| Path::new(""));
    let source = codec::load_color(parent.join(&saved.frame_name)).map_err(|error| {
        RunError::from(ObservationError::from(error)).context("continuation native scene missing")
    })?;
    let size = Size::new(1922, 1140).map_err(ObservationError::from)?;
    if source.size() != frame.size() || frame.size() != size {
        return Err(reject("continuation native scene missing"));
    }
    let mut mask = GrayFrame::zeros(size).map_err(ObservationError::from)?;
    for y in 180..890 {
        mask.pixels_mut()[y * 1922 + 120..y * 1922 + 1720].fill(255);
    }
    let [x, y] = saved.ground;
    let left = x.saturating_sub(260).max(120).clamp(0, 1922) as usize;
    let top = y.saturating_sub(230).max(180).clamp(0, 1140) as usize;
    // NumPy의 음수 끝 인덱스도 그대로 보존한다. 정상 보고서의 발 위치는 native 정수다.
    let right = slice_end(x.saturating_add(260).min(1720), 1922);
    let bottom = slice_end(y.saturating_add(120).min(890), 1140);
    if left < right && top < bottom {
        for row in top..bottom {
            mask.pixels_mut()[row * 1922 + left..row * 1922 + right].fill(0);
        }
    }
    let mut observer = BackgroundMotionObserver::new(MotionLimits::new(24, 0.65, 3.0, 0.5)?, None);
    observer.observe_mask(source.view(), mask.view(), 1, 0.0, None)?;
    let motion = observer.observe_mask(frame.view(), mask.view(), 2, 1.0, None)?;
    if motion
        .displacement
        .is_none_or(|[x, y]| x.hypot(y) > maximum_pending_distance)
    {
        return Err(reject("continuation current scene unverified"));
    }
    Ok(SavedRouteFlow {
        detour: saved.detour,
        motion,
    })
}
fn slice_end(value: i64, length: i64) -> usize {
    if value < 0 {
        value.saturating_add(length)
    } else {
        value
    }
    .clamp(0, length) as usize
}

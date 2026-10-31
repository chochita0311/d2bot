//! Pixel-center mapping between a reference image and a captured physical window.

use super::frame::{FrameError, PixelRect, Size};
use serde_json::{Value, json};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::Path;

#[derive(Debug)]
pub struct GeometryError {
    kind: &'static str,
    message: String,
}

impl GeometryError {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
    fn value(message: impl Into<String>) -> Self {
        Self::new("ValueError", message)
    }
    pub fn kind(&self) -> &'static str {
        self.kind
    }
    pub fn message(&self) -> &str {
        &self.message
    }
}
impl fmt::Display for GeometryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}
impl Error for GeometryError {}
impl From<FrameError> for GeometryError {
    fn from(error: FrameError) -> Self {
        Self::value(error.to_string())
    }
}

fn native_size(size: Size) -> Result<Size, GeometryError> {
    if size.width() > i32::MAX as usize || size.height() > i32::MAX as usize {
        return Err(GeometryError::value(
            "frame size exceeds native Windows dimensions",
        ));
    }
    Ok(size)
}

fn box_in(rect: PixelRect, size: Size) -> Result<PixelRect, GeometryError> {
    rect.checked_in(size)
        .map_err(|_| GeometryError::value("client rectangle outside captured frame"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalTarget {
    pub left: i32,
    pub top: i32,
    size: Size,
}
impl PhysicalTarget {
    pub fn new(left: i32, top: i32, width: usize, height: usize) -> Result<Self, GeometryError> {
        let size = Size::new(width, height)
            .map_err(|_| GeometryError::value("integer physical capture rectangle required"))?;
        Ok(Self {
            left,
            top,
            size: native_size(size)?,
        })
    }
    pub fn size(self) -> Size {
        self.size
    }
    pub fn as_value(self) -> Value {
        json!({"left":self.left,"top":self.top,"width":self.size.width(),"height":self.size.height()})
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceLayout {
    size: Size,
    client: PixelRect,
    aspect_tolerance: f64,
}
impl ReferenceLayout {
    pub fn new(
        size: Size,
        client: PixelRect,
        aspect_tolerance: f64,
    ) -> Result<Self, GeometryError> {
        native_size(size)?;
        box_in(client, size)?;
        if !(0.0..=0.03).contains(&aspect_tolerance) {
            return Err(GeometryError::value(
                "bounded client aspect tolerance required",
            ));
        }
        Ok(Self {
            size,
            client,
            aspect_tolerance,
        })
    }
    pub fn size(&self) -> Size {
        self.size
    }
    pub fn client(&self) -> PixelRect {
        self.client
    }
    pub fn aspect_tolerance(&self) -> f64 {
        self.aspect_tolerance
    }

    pub fn from_value(raw: &Value) -> Result<Self, GeometryError> {
        // Python's schema equality also accepts true and 1.0; do not tighten it silently.
        if raw
            .get("schema")
            .is_none_or(|schema| schema != &Value::Bool(true) && schema.as_f64() != Some(1.0))
        {
            return Err(GeometryError::value("unsupported reference layout schema"));
        }
        let required = |key: &str| {
            raw.get(key)
                .ok_or_else(|| GeometryError::new("KeyError", format!("'{key}'")))
        };
        let frame_size = required("frame_size")?;
        let positive_size = || {
            let values = frame_size.as_array()?;
            if values.len() != 2 {
                return None;
            }
            Size::new(
                usize::try_from(values[0].as_u64()?).ok()?,
                usize::try_from(values[1].as_u64()?).ok()?,
            )
            .ok()
        };
        let size = positive_size()
            .ok_or_else(|| GeometryError::value("positive reference frame size required"))?;
        let client = rect_from_value(required("client_box")?)?;
        let tolerance = match required("aspect_tolerance")? {
            Value::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
            Value::Number(value) => value.as_f64(),
            _ => None,
        }
        .ok_or_else(|| GeometryError::value("bounded client aspect tolerance required"))?;
        Self::new(size, client, tolerance)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, GeometryError> {
        let text = fs::read_to_string(path).map_err(|error| {
            GeometryError::new(
                match error.kind() {
                    std::io::ErrorKind::NotFound => "FileNotFoundError",
                    std::io::ErrorKind::PermissionDenied => "PermissionError",
                    std::io::ErrorKind::InvalidData => "UnicodeDecodeError",
                    _ => "OSError",
                },
                error.to_string(),
            )
        })?;
        let raw = serde_json::from_str(&text)
            .map_err(|error| GeometryError::new("JSONDecodeError", error.to_string()))?;
        Self::from_value(&raw)
    }
}

pub fn rect_from_value(raw: &Value) -> Result<PixelRect, GeometryError> {
    if let Some(values) = raw.as_array()
        && values.len() == 4
        && values
            .iter()
            .all(|value| value.as_i64().is_some() || value.as_u64().is_some())
        && values
            .iter()
            .any(|value| value.as_i64().is_some_and(|value| value < 0))
    {
        return Err(GeometryError::value(
            "client rectangle outside captured frame",
        ));
    }
    let parse = || {
        let values = raw.as_array()?;
        if values.len() != 4 {
            return None;
        }
        let mut parts = [0; 4];
        for (output, value) in parts.iter_mut().zip(values) {
            *output = usize::try_from(value.as_u64()?).ok()?;
        }
        Some(parts)
    };
    let [left, top, right, bottom] =
        parse().ok_or_else(|| GeometryError::value("integer client rectangle required"))?;
    PixelRect::new(left, top, right, bottom)
        .map_err(|_| GeometryError::value("client rectangle outside captured frame"))
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameGeometry {
    target: PhysicalTarget,
    source_client: PixelRect,
    layout: ReferenceLayout,
}
impl FrameGeometry {
    pub fn create(
        target: PhysicalTarget,
        layout: Option<ReferenceLayout>,
        client: Option<PixelRect>,
    ) -> Result<Self, GeometryError> {
        let (layout, source_client) = match layout {
            Some(layout) => {
                let client = client
                    .ok_or_else(|| GeometryError::value("integer client rectangle required"))?;
                (layout, box_in(client, target.size)?)
            }
            None => {
                let client = PixelRect::full(target.size);
                (ReferenceLayout::new(target.size, client, 0.01)?, client)
            }
        };
        let source_aspect = source_client.width() as f64 / source_client.height() as f64;
        let reference_aspect = layout.client.width() as f64 / layout.client.height() as f64;
        let error = (source_aspect / reference_aspect - 1.0).abs();
        if error > layout.aspect_tolerance {
            return Err(GeometryError::value(format!(
                "game client aspect differs from reference layout: {:.3}%",
                error * 100.0
            )));
        }
        Ok(Self {
            target,
            source_client,
            layout,
        })
    }
    pub fn target(&self) -> PhysicalTarget {
        self.target
    }
    pub fn source_client(&self) -> PixelRect {
        self.source_client
    }
    pub fn layout(&self) -> &ReferenceLayout {
        &self.layout
    }
    pub fn is_native(&self) -> bool {
        self.target.size == self.layout.size && self.source_client == self.layout.client
    }

    fn coordinates(&self, point: [f64; 2], inverse: bool) -> Result<[f64; 2], GeometryError> {
        let mut output = [0.0; 2];
        let reference = [
            [
                0,
                self.layout.client.left(),
                self.layout.client.right(),
                self.layout.size.width(),
            ],
            [
                0,
                self.layout.client.top(),
                self.layout.client.bottom(),
                self.layout.size.height(),
            ],
        ];
        let source = [
            [
                0,
                self.source_client.left(),
                self.source_client.right(),
                self.target.size.width(),
            ],
            [
                0,
                self.source_client.top(),
                self.source_client.bottom(),
                self.target.size.height(),
            ],
        ];
        for axis in 0..2 {
            output[axis] = if inverse {
                map_axis(point[axis], source[axis], reference[axis])?
            } else {
                map_axis(point[axis], reference[axis], source[axis])?
            };
        }
        Ok(output)
    }
    pub fn to_desktop(&self, point: [f64; 2]) -> Result<[i64; 2], GeometryError> {
        let local = self.coordinates(point, false)?;
        Ok([
            round_integer(local[0] + f64::from(self.target.left))?,
            round_integer(local[1] + f64::from(self.target.top))?,
        ])
    }
    pub fn from_desktop(&self, point: [f64; 2]) -> Result<[f64; 2], GeometryError> {
        self.coordinates(
            [
                point[0] - f64::from(self.target.left),
                point[1] - f64::from(self.target.top),
            ],
            true,
        )
    }
    pub fn is_current(&self, current: CurrentGeometry) -> bool {
        current.target == self.target
            && current
                .client
                .is_none_or(|client| client == self.source_client)
    }
    pub fn as_value(&self) -> Value {
        let rect = |rect: PixelRect| [rect.left(), rect.top(), rect.right(), rect.bottom()];
        json!({"source_target":self.target.as_value(),"source_client":rect(self.source_client),
            "frame_size":[self.layout.size.width(),self.layout.size.height()],"client_box":rect(self.layout.client)})
    }
}

fn map_axis(value: f64, source: [usize; 4], target: [usize; 4]) -> Result<f64, GeometryError> {
    if !value.is_finite() || !(0.0..source[3] as f64).contains(&value) {
        return Err(GeometryError::value("point outside coordinate frame"));
    }
    for segment in 0..3 {
        let (start, end) = (source[segment] as f64, source[segment + 1] as f64);
        if start <= value && value < end {
            let (left, right) = (target[segment] as f64, target[segment + 1] as f64);
            return Ok(left + (value - start + 0.5) * (right - left) / (end - start) - 0.5);
        }
    }
    Err(GeometryError::value("unmapped coordinate"))
}

fn round_integer(value: f64) -> Result<i64, GeometryError> {
    if value.is_nan() {
        return Err(GeometryError::value("cannot convert float NaN to integer"));
    }
    if value.is_infinite() {
        return Err(GeometryError::new(
            "OverflowError",
            "cannot convert float infinity to integer",
        ));
    }
    let rounded = value.round_ties_even();
    if rounded < i64::MIN as f64 || rounded >= -(i64::MIN as f64) {
        return Err(GeometryError::new(
            "OverflowError",
            "coordinate exceeds native integer range",
        ));
    }
    Ok(rounded as i64)
}

#[derive(Debug, Clone, Copy)]
pub struct CurrentGeometry {
    pub target: PhysicalTarget,
    pub client: Option<PixelRect>,
}

pub fn geometry_is_current(
    geometry: Option<&FrameGeometry>,
    current: impl FnOnce() -> Result<CurrentGeometry, GeometryError>,
) -> Result<bool, GeometryError> {
    match geometry {
        Some(geometry) => Ok(geometry.is_current(current()?)),
        None => Ok(true),
    }
}

fn ensure_current(
    geometry: &FrameGeometry,
    current: impl FnOnce() -> Result<CurrentGeometry, GeometryError>,
) -> Result<(), GeometryError> {
    if !geometry.is_current(current()?) {
        return Err(GeometryError::new(
            "RuntimeError",
            "window geometry changed after capture",
        ));
    }
    Ok(())
}

pub fn frame_to_desktop(
    target: PhysicalTarget,
    geometry: Option<&FrameGeometry>,
    point: [f64; 2],
    current: impl FnOnce() -> Result<CurrentGeometry, GeometryError>,
) -> Result<[f64; 2], GeometryError> {
    if let Some(geometry) = geometry {
        ensure_current(geometry, current)?;
        let [x, y] = geometry.to_desktop(point)?;
        Ok([x as f64, y as f64])
    } else {
        Ok([
            point[0] + f64::from(target.left),
            point[1] + f64::from(target.top),
        ])
    }
}

pub fn desktop_to_frame(
    target: PhysicalTarget,
    geometry: Option<&FrameGeometry>,
    point: [f64; 2],
    current: impl FnOnce() -> Result<CurrentGeometry, GeometryError>,
) -> Result<[i64; 2], GeometryError> {
    let local = if let Some(geometry) = geometry {
        ensure_current(geometry, current)?;
        geometry.from_desktop(point)?
    } else {
        [
            point[0] - f64::from(target.left),
            point[1] - f64::from(target.top),
        ]
    };
    Ok([round_integer(local[0])?, round_integer(local[1])?])
}

//! 사용 중인 BGR8 주석만 재현한다. 고정 OpenCV 원문/고지는 THIRD-PARTY-NOTICES 참조.
mod raster;
mod tables;
use super::VisionError;
use crate::common::frame::BgrFrame;
use raster::{Canvas, ONE, Point};

fn validate(frame: &BgrFrame, points: &[[i32; 2]]) -> Result<(), VisionError> {
    if frame.size().width() > 1_048_576
        || frame.size().height() > 1_048_576
        || points
            .iter()
            .flatten()
            .any(|n| i64::from(*n).abs() > 16_777_216)
    {
        return Err(VisionError::Invalid(
            "drawing dimensions or coordinates exceed fixed-point range",
        ));
    }
    Ok(())
}
pub fn rectangle(
    frame: &mut BgrFrame,
    first: [i32; 2],
    last: [i32; 2],
    color: [u8; 3],
) -> Result<(), VisionError> {
    validate(frame, &[first, last])?;
    let points = [first, [last[0], first[1]], last, [first[0], last[1]]].map(Point::pixel);
    let mut canvas = Canvas::new(frame, color);
    let mut previous = points[3];
    for point in points {
        canvas.thick(previous, point, false, 2, true);
        previous = point;
    }
    Ok(())
}
pub fn circle(
    frame: &mut BgrFrame,
    center: [i32; 2],
    radius: u8,
    filled: bool,
    color: [u8; 3],
) -> Result<(), VisionError> {
    validate(frame, &[center])?;
    let mut canvas = Canvas::new(frame, color);
    if filled {
        canvas.disk(Point::pixel(center), i64::from(radius));
    } else {
        let points = raster::ellipse(Point::pixel(center), i64::from(radius) * ONE);
        canvas.polyline(&points, false);
    }
    Ok(())
}
/// Simplex는 UTF-8 문자 단위가 아니라 범위 밖 바이트마다 '?'를 그린다.
pub fn text(
    frame: &mut BgrFrame,
    text: &str,
    origin: [i32; 2],
    scale: f64,
    color: [u8; 3],
) -> Result<(), VisionError> {
    validate(frame, &[origin])?;
    if !scale.is_finite() || !(0.0..=8.0).contains(&scale) || text.len() > 1_048_576 {
        return Err(VisionError::Invalid("bounded text and font scale required"));
    }
    let scale = (scale * ONE as f64).round_ties_even() as i64;
    let mut x = i64::from(origin[0]) * ONE;
    let y = i64::from(origin[1]) * ONE - 9 * scale;
    let mut canvas = Canvas::new(frame, color);
    let mut points = Vec::with_capacity(64);
    for c in text.bytes() {
        let c = if (b' '..127).contains(&c) { c } else { b'?' };
        let glyph = tables::GLYPHS[usize::from(c - b' ')].as_bytes();
        x -= (i64::from(glyph[0]) - i64::from(b'R')) * scale;
        let advance = (i64::from(glyph[1]) - i64::from(b'R')) * scale;
        points.clear();
        let mut index = 2;
        while index < glyph.len() {
            if glyph[index] == b' ' {
                canvas.polyline(&points, true);
                points.clear();
                index += 1;
            } else {
                points.push(Point {
                    x: x + (i64::from(glyph[index]) - 82) * scale,
                    y: y + (i64::from(glyph[index + 1]) - 82) * scale,
                });
                index += 2;
            }
        }
        canvas.polyline(&points, true);
        x += advance;
    }
    Ok(())
}

#[cfg(test)]
mod tests;

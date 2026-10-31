//! 고정 소스 resize.cpp의 정수·분수 INTER_AREA 순서. 고지: THIRD-PARTY-NOTICES.md.
use super::*;
use crate::common::frame::BgrFrame;
fn axis(from: usize, to: usize) -> Vec<Vec<(usize, f32)>> {
    let scale = 1.0 / (to as f64 / from as f64);
    (0..to)
        .map(|d| {
            let start = d as f64 * scale;
            let end = start + scale;
            let width = scale.min(from as f64 - start);
            let last = (end.floor() as usize).min(from - 1);
            let first = (start.ceil() as usize).min(last);
            let mut weights = Vec::new();
            if first as f64 - start > 1e-3 {
                weights.push((first - 1, ((first as f64 - start) / width) as f32));
            }
            for x in first..last {
                weights.push((x, (1.0 / width) as f32));
            }
            if end - last as f64 > 1e-3 {
                weights.push((
                    last,
                    ((end - last as f64).min(1.0).min(width) / width) as f32,
                ));
            }
            weights
        })
        .collect()
}
pub(super) fn sample(
    frame: BgrView<'_>,
    mask: Option<GrayView<'_>>,
    size: Size,
) -> Result<(BgrFrame, Option<FloatMap>), ObservationError> {
    let source = frame.size();
    let mut output = BgrFrame::zeros(size)?;
    let mut neutral = mask.map(|_| FloatMap::zeros(size)).transpose()?;
    if mask.is_some_and(|m| m.size() != source)
        || size.width() > source.width()
        || size.height() > source.height()
    {
        return Err(ObservationError::invalid(
            "area samples require matching shrinking inputs",
        ));
    }
    let sx = 1.0 / (size.width() as f64 / source.width() as f64);
    let sy = 1.0 / (size.height() as f64 / source.height() as f64);
    let ix = sx.round_ties_even() as usize;
    let iy = sy.round_ties_even() as usize;
    let fast = (sx - ix as f64).abs() < f64::EPSILON && (sy - iy as f64).abs() < f64::EPSILON;
    let (xs, ys) = if fast {
        (Vec::new(), Vec::new())
    } else {
        (
            axis(source.width(), size.width()),
            axis(source.height(), size.height()),
        )
    };
    for (index, destination) in output
        .pixels_mut()
        .as_chunks_mut::<3>()
        .0
        .iter_mut()
        .enumerate()
    {
        let [x, y] = [index % size.width(), index / size.width()];
        let mut sums = [0.0_f32; 4];
        if fast {
            for yy in y * iy..((y + 1) * iy).min(source.height()) {
                let row = frame
                    .row(yy)
                    .ok_or_else(|| ObservationError::invalid("area sample row missing"))?;
                let m = mask.and_then(|m| m.row(yy));
                for xx in x * ix..((x + 1) * ix).min(source.width()) {
                    for c in 0..3 {
                        sums[c] += f32::from(row[xx * 3 + c]);
                    }
                    if let Some(m) = m {
                        sums[3] += f32::from(m[xx]);
                    }
                }
            }
            let scale = 1.0_f32 / (ix * iy) as f32;
            for value in &mut sums {
                *value *= scale;
            }
        } else {
            for (yy, beta) in &ys[y] {
                let row = frame
                    .row(*yy)
                    .ok_or_else(|| ObservationError::invalid("area sample row missing"))?;
                let m = mask.and_then(|m| m.row(*yy));
                let mut horizontal = [0.0_f32; 4];
                for (xx, alpha) in &xs[x] {
                    for c in 0..3 {
                        horizontal[c] += f32::from(row[xx * 3 + c]) * alpha;
                    }
                    if let Some(m) = m {
                        horizontal[3] += f32::from(m[*xx]) * alpha;
                    }
                }
                for c in 0..4 {
                    sums[c] += *beta * horizontal[c];
                }
            }
        }
        for (destination, sum) in destination.iter_mut().zip(sums) {
            *destination = if fast && ix == 2 && iy == 2 {
                (sum + 0.5).floor() as u8
            } else {
                sum.round_ties_even().clamp(0.0, 255.0) as u8
            };
        }
        if let Some(n) = &mut neutral {
            n.pixels_mut()[y * size.width() + x] = sums[3];
        }
    }
    Ok((output, neutral))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::fmt::Write;
    #[test]
    fn every_reviewed_cell_size_preserves_color_and_float_neutral_samples()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut cases = Vec::new();
        let mut actual = Vec::new();
        let mut state = 91u32;
        for cell in 2..=64 {
            let size = Size::new(cell * 5, cell * 3)?;
            let mut frame = BgrFrame::zeros(size)?;
            let mut mask = GrayFrame::zeros(size)?;
            for y in 0..size.height() {
                for x in 0..size.width() {
                    let index = y * size.width() + x;
                    for c in 0..3 {
                        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                        frame.pixels_mut()[index * 3 + c] = (state >> 24) as u8;
                    }
                    if (x * 3 + y * 11) % 5 == 0 {
                        frame.pixels_mut()[index * 3..index * 3 + 3].fill(44);
                    }
                    let p = &frame.pixels()[index * 3..index * 3 + 3];
                    mask.pixels_mut()[index] = u8::from(p[0] == p[1] && p[1] == p[2] && p[0] >= 30);
                }
            }
            let mut hex = String::new();
            for b in frame.pixels() {
                write!(&mut hex, "{b:02x}")?;
            }
            cases.push(json!({"frame":{"width":size.width(),"height":size.height(),"pixels":hex},"output":[10,6]}));
            let (color, neutral) = sample(frame.view(), Some(mask.view()), Size::new(10, 6)?)?;
            let neutral = neutral.ok_or("neutral samples")?;
            actual.push(json!({"color":format!("{:x}",Sha256::digest(color.pixels())),"neutral":format!("{:x}",Sha256::digest(neutral.pixels().iter().flat_map(|v|v.to_le_bytes()).collect::<Vec<_>>()))}));
        }
        let expected = crate::test_support::legacy_expectation(
            "observer-oracle",
            &json!({"kind":"grid_samples","cases":cases}),
        )?;
        for (index, actual) in actual.iter().enumerate() {
            assert_eq!(*actual, expected[index], "cell {}", index + 2);
        }
        Ok(())
    }
}

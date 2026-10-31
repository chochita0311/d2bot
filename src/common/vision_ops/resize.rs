use super::VisionError;
use crate::common::frame::{BgrFrame, Image, ImageView, PixelRect, Size};
use crate::common::frame_geometry::FrameGeometry;
use std::borrow::Cow;

#[derive(Debug, Clone, Copy)]
pub enum Interpolation {
    Nearest,
    Linear,
    Area,
}

/// 대략적인 후보 검색에 쓰는 명시적 fx=fy=0.5 축소.
/// 홀수 크기의 반올림 뒤에도 배율은 정확히 절반이다.
/// 출력 크기에서 배율을 다시 구하면 후보 위치와 가장자리 픽셀이 바뀐다.
pub fn half<const C: usize>(source: ImageView<'_, u8, C>) -> Result<Image<u8, C>, VisionError> {
    let input = source.size();
    let size = Size::new(
        (input.width() as f64 * 0.5).round_ties_even() as usize,
        (input.height() as f64 * 0.5).round_ties_even() as usize,
    )?;
    let mut output = Image::<u8, C>::zeros(size)?;
    for y in 0..size.height() {
        for x in 0..size.width() {
            for c in 0..C {
                let mut sum = 0_u32;
                let mut count = 0_u32;
                for sy in y * 2..(y * 2 + 2).min(input.height()) {
                    let row = source
                        .row(sy)
                        .ok_or(VisionError::Invalid("missing half-size row"))?;
                    for sx in x * 2..(x * 2 + 2).min(input.width()) {
                        sum += u32::from(row[sx * C + c]);
                        count += 1;
                    }
                }
                output.pixels_mut()[(y * size.width() + x) * C + c] = if count == 4 {
                    ((sum + 2) >> 2) as u8
                } else {
                    // 부분 가장자리는 나눗셈 뒤 saturate_cast를 거치므로
                    // 전체 2x2 경로와 달리 .5 동점에서 짝수로 반올림한다.
                    (f64::from(sum) / f64::from(count)).round_ties_even() as u8
                };
            }
        }
    }
    Ok(output)
}

fn linear_axis(index: usize, from: usize, to: usize) -> (usize, usize, f64) {
    let position = ((index as f64 + 0.5) * from as f64 / to as f64 - 0.5) as f32;
    let base = position.floor();
    let fraction = f64::from(position - base);
    let first = (base as isize).clamp(0, from as isize - 1) as usize;
    let next = (base as isize + 1).clamp(0, from as isize - 1) as usize;
    (first, next, fraction)
}

fn area_linear_axis(index: usize, from: usize, to: usize) -> (usize, usize, f64) {
    let base = (index as f64 * from as f64 / to as f64).floor() as usize;
    let fraction = ((index + 1) as f64 - (base + 1) as f64 * to as f64 / from as f64) as f32;
    let fraction = if fraction <= 0.0 {
        0.0
    } else {
        fraction - fraction.floor()
    };
    (
        base.min(from - 1),
        (base + 1).min(from - 1),
        f64::from(fraction),
    )
}

/// uint8 포화·짝수 반올림. 양수의 정수/소수 분리로 픽셀마다 CRT 반올림 호출을 피한다.
pub(crate) fn round_byte(value: f64) -> u8 {
    let value = value.clamp(0.0, 255.0);
    let lower = value as u16;
    let fraction = value - f64::from(lower);
    (lower + u16::from(fraction > 0.5 || (fraction == 0.5 && lower % 2 == 1))) as u8
}

struct AreaSpan {
    weights: Vec<(usize, f64)>,
    extent: f64,
}
fn area_axis(from: usize, to: usize) -> Result<Vec<AreaSpan>, VisionError> {
    area_axis_range(from, to, 0..to)
}
fn area_axis_range(
    from: usize,
    to: usize,
    range: std::ops::Range<usize>,
) -> Result<Vec<AreaSpan>, VisionError> {
    let mut spans = Vec::new();
    spans
        .try_reserve_exact(range.len())
        .map_err(crate::common::frame::FrameError::Allocation)?;
    for index in range {
        let start = index as f64 * from as f64 / to as f64;
        let end = (index + 1) as f64 * from as f64 / to as f64;
        let first = start.floor() as usize;
        let last = (end.ceil() as usize).min(from);
        let mut weights = Vec::new();
        weights
            .try_reserve_exact(last - first)
            .map_err(crate::common::frame::FrameError::Allocation)?;
        for pixel in first..last {
            weights.push((
                pixel,
                (end.min((pixel + 1) as f64) - start.max(pixel as f64)).max(0.0),
            ));
        }
        spans.push(AreaSpan {
            weights,
            extent: end - start,
        });
    }
    Ok(spans)
}

/// 전체 Area 축소 후 crop한 것과 같은 픽셀을 필요한 출력 영역에만 계산한다.
/// 원본을 먼저 crop하면 경계의 가중치와 반올림이 달라지므로 전체 target 좌표를 쓴다.
pub fn area_region<const C: usize>(
    source: ImageView<'_, u8, C>,
    target: Size,
    region: PixelRect,
) -> Result<Image<u8, C>, VisionError> {
    region.checked_in(target)?;
    let input = source.size();
    if input == target {
        return Ok(source.crop(region)?.to_owned()?);
    }
    if input.width() < target.width() || input.height() < target.height() {
        return Ok(resize(source, target, Interpolation::Area)?
            .view()
            .crop(region)?
            .to_owned()?);
    }
    let columns = area_axis_range(input.width(), target.width(), region.left()..region.right())?;
    let rows = area_axis_range(
        input.height(),
        target.height(),
        region.top()..region.bottom(),
    )?;
    let mut output = Image::<u8, C>::zeros(Size::new(region.width(), region.height())?)?;
    area_rows(source, &columns, &rows, output.pixels_mut())?;
    Ok(output)
}

fn area_rows<const C: usize>(
    source: ImageView<'_, u8, C>,
    columns: &[AreaSpan],
    rows: &[AreaSpan],
    output: &mut [u8],
) -> Result<(), VisionError> {
    for (row, target) in rows.iter().zip(output.chunks_exact_mut(columns.len() * C)) {
        for (x, column) in columns.iter().enumerate() {
            let mut sums = [0.0; C];
            for &(sy, wy) in &row.weights {
                let pixels = source
                    .row(sy)
                    .ok_or(VisionError::Invalid("missing resize row"))?;
                for &(sx, wx) in &column.weights {
                    for (c, sum) in sums.iter_mut().enumerate() {
                        *sum += f64::from(pixels[sx * C + c]) * wx * wy;
                    }
                }
            }
            for (c, sum) in sums.into_iter().enumerate() {
                target[x * C + c] = round_byte(sum / (column.extent * row.extent));
            }
        }
    }
    Ok(())
}

pub fn resize<const C: usize>(
    source: ImageView<'_, u8, C>,
    size: Size,
    mode: Interpolation,
) -> Result<Image<u8, C>, VisionError> {
    if size == source.size() {
        return Ok(source.to_owned()?);
    }
    let mut output = Image::<u8, C>::zeros(size)?;
    let input = source.size();
    if matches!(mode, Interpolation::Area)
        && input.width() >= size.width()
        && input.height() >= size.height()
    {
        // 원래 누적 순서·f64 가중치를 유지하고 반복되는 축 계산만 밖으로 꺼낸다.
        let columns = area_axis(input.width(), size.width())?;
        let rows = area_axis(input.height(), size.height())?;
        let workers = if output.pixels().len() >= 1_000_000 {
            std::thread::available_parallelism()
                .map_or(1, |n| n.get().min(4))
                .min(size.height())
        } else {
            1
        };
        if workers == 1 {
            area_rows(source, &columns, &rows, output.pixels_mut())?;
        } else {
            // 행 사이에는 누적 상태가 없다. 같은 픽셀의 f64 계산 순서는 유지한다.
            let band_rows = size.height().div_ceil(workers);
            let mut results: Vec<_> = (0..workers).map(|_| Ok(())).collect();
            crate::common::compute::scope(|scope| {
                for ((index, target), result) in output
                    .pixels_mut()
                    .chunks_mut(band_rows * size.width() * C)
                    .enumerate()
                    .zip(&mut results)
                {
                    let columns = &columns;
                    let start = index * band_rows;
                    let rows = &rows[start..(start + band_rows).min(rows.len())];
                    scope.spawn(move |_| *result = area_rows(source, columns, rows, target));
                }
            })
            .map_err(VisionError::from)?;
            for result in results {
                result?;
            }
        }
        return Ok(output);
    }
    let at = |x: usize, y: usize, c: usize| -> Result<f64, VisionError> {
        Ok(f64::from(
            source
                .row(y)
                .ok_or(VisionError::Invalid("missing resize row"))?[x * C + c],
        ))
    };
    if matches!(mode, Interpolation::Nearest) {
        let mut columns = Vec::new();
        columns
            .try_reserve_exact(size.width())
            .map_err(crate::common::frame::FrameError::Allocation)?;
        columns.extend((0..size.width()).map(|x| x * input.width() / size.width() * C));
        for y in 0..size.height() {
            let source = source
                .row(y * input.height() / size.height())
                .ok_or(VisionError::Invalid("missing resize row"))?;
            for (target, &x) in output.pixels_mut()
                [y * size.width() * C..(y + 1) * size.width() * C]
                .as_chunks_mut::<C>()
                .0
                .iter_mut()
                .zip(&columns)
            {
                target.copy_from_slice(&source[x..x + C]);
            }
        }
        return Ok(output);
    }
    for y in 0..size.height() {
        for x in 0..size.width() {
            for c in 0..C {
                let value = match mode {
                    Interpolation::Nearest => at(
                        x * input.width() / size.width(),
                        y * input.height() / size.height(),
                        c,
                    )?,
                    _ => {
                        let axis = if matches!(mode, Interpolation::Area) {
                            area_linear_axis
                        } else {
                            linear_axis
                        };
                        let (x0, x1, wx) = axis(x, input.width(), size.width());
                        let (y0, y1, wy) = axis(y, input.height(), size.height());
                        let upper = at(x0, y0, c)? * (1.0 - wx) + at(x1, y0, c)? * wx;
                        let lower = at(x0, y1, c)? * (1.0 - wx) + at(x1, y1, c)? * wx;
                        upper * (1.0 - wy) + lower * wy
                    }
                };
                output.pixels_mut()[(y * size.width() + x) * C + c] = round_byte(value);
            }
        }
    }
    Ok(output)
}

pub fn normalize<'a>(
    geometry: &FrameGeometry,
    frame: &'a BgrFrame,
) -> Result<Cow<'a, BgrFrame>, VisionError> {
    if frame.size() != geometry.target().size() {
        return Err(VisionError::Invalid("frame and captured geometry differ"));
    }
    if geometry.is_native() {
        return Ok(Cow::Borrowed(frame));
    }
    let mut output = resize(
        frame.view(),
        geometry.layout().size(),
        Interpolation::Linear,
    )?;
    let source = geometry.source_client();
    let target = geometry.layout().client();
    let mode = if source.width() >= target.width() && source.height() >= target.height() {
        Interpolation::Area
    } else {
        Interpolation::Linear
    };
    let resized = resize(
        frame.view().crop(source)?,
        Size::new(target.width(), target.height())?,
        mode,
    )?;
    let stride = output.size().row_bytes();
    for y in 0..target.height() {
        let row = resized
            .view()
            .row(y)
            .ok_or(VisionError::Invalid("missing normalized client row"))?;
        let start = (y + target.top()) * stride + target.left() * 3;
        output.pixels_mut()[start..start + row.len()].copy_from_slice(row);
    }
    Ok(Cow::Owned(output))
}

/// 출력 크기를 반올림해도 호출자가 지정한 fx/fy를 유지한다.
pub fn scale_linear<const C: usize>(
    source: ImageView<'_, u8, C>,
    scale: f64,
) -> Result<Image<u8, C>, VisionError> {
    let dimension = |value: usize| -> Result<usize, VisionError> {
        let value = (value as f64 * scale).round_ties_even();
        if !(1.0..=i32::MAX as f64).contains(&value) {
            return Err(VisionError::Invalid(
                "scale produces invalid frame dimensions",
            ));
        }
        Ok(value as usize)
    };
    let size = Size::new(
        dimension(source.size().width())?,
        dimension(source.size().height())?,
    )
    .map_err(VisionError::from)?;
    if size == source.size() {
        return source.to_owned().map_err(VisionError::from);
    }
    let mut output = Image::<u8, C>::zeros(size).map_err(VisionError::from)?;
    let input = source.size();
    if scale == 0.5 {
        for y in 0..size.height() {
            let full_width = if 2 * y + 1 < input.height() {
                input.width() / 2
            } else {
                0
            };
            if full_width != 0 {
                let top = source
                    .row(2 * y)
                    .ok_or(VisionError::Invalid("scaled image row absent"))?;
                let bottom = source
                    .row(2 * y + 1)
                    .ok_or(VisionError::Invalid("scaled image row absent"))?;
                let target = &mut output.pixels_mut()
                    [y * size.width() * C..y * size.width() * C + full_width * C];
                for ((a, b), out) in top
                    .chunks_exact(2 * C)
                    .zip(bottom.chunks_exact(2 * C))
                    .zip(target.as_chunks_mut::<C>().0.iter_mut())
                {
                    for c in 0..C {
                        out[c] = ((u32::from(a[c])
                            + u32::from(a[c + C])
                            + u32::from(b[c])
                            + u32::from(b[c + C])
                            + 2)
                            >> 2) as u8;
                    }
                }
            }
            // 부분 행/열만 원래의 짝수 반올림을 거친다.
            for x in full_width..size.width() {
                for c in 0..C {
                    let (mut sum, mut count) = (0u32, 0u32);
                    for sy in y * 2..(y * 2 + 2).min(input.height()) {
                        let row = source
                            .row(sy)
                            .ok_or_else(|| VisionError::Invalid("scaled image row absent"))?;
                        for sx in x * 2..(x * 2 + 2).min(input.width()) {
                            sum += u32::from(row[sx * C + c]);
                            count += 1;
                        }
                    }
                    output.pixels_mut()[(y * size.width() + x) * C + c] = if count == 4 {
                        ((sum + 2) >> 2) as u8
                    } else {
                        (sum as f32 / count as f32).round_ties_even() as u8
                    };
                }
            }
        }
        return Ok(output);
    }
    let coefficient = |d: usize, length: usize, horizontal: bool| {
        let coordinate = ((d as f64 + 0.5) / scale - 0.5) as f32;
        let base = coordinate.floor();
        let weight = if horizontal && (base < 0.0 || base >= (length - 1) as f32) {
            0.0
        } else {
            coordinate - base
        };
        let first = (base as i64).clamp(0, length as i64 - 1) as usize;
        let next = (base as i64 + 1).clamp(0, length as i64 - 1) as usize;
        (
            first,
            next,
            ((1.0 - weight) * 2048.0).round_ties_even() as u32,
            (weight * 2048.0).round_ties_even() as u32,
        )
    };
    // 가로 가중치는 모든 행에서 동일하다. 반올림 규칙은 그대로 두고 한 번만 계산한다.
    let mut horizontal = Vec::new();
    horizontal
        .try_reserve_exact(size.width())
        .map_err(crate::common::frame::FrameError::Allocation)?;
    horizontal.extend((0..size.width()).map(|x| coefficient(x, input.width(), true)));
    for y in 0..size.height() {
        let (y0, y1, b0, b1) = coefficient(y, input.height(), false);
        let row0 = source
            .row(y0)
            .ok_or_else(|| VisionError::Invalid("scaled image row absent"))?;
        let row1 = source
            .row(y1)
            .ok_or_else(|| VisionError::Invalid("scaled image row absent"))?;
        for (x, &(x0, x1, a0, a1)) in horizontal.iter().enumerate() {
            for c in 0..C {
                let top = u32::from(row0[x0 * C + c]) * a0 + u32::from(row0[x1 * C + c]) * a1;
                let bottom = u32::from(row1[x0 * C + c]) * a0 + u32::from(row1[x1 * C + c]) * a1;
                output.pixels_mut()[(y * size.width() + x) * C + c] =
                    ((((b0 * (top >> 4)) >> 16) + ((b1 * (bottom >> 4)) >> 16) + 2) >> 2) as u8;
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod rounding_tests {
    #[test]
    fn byte_conversion_preserves_halfway_neighbors_and_saturation() {
        for integer in 0..=255 {
            for fraction in [
                0.0,
                0.25,
                0.5 - f64::EPSILON * 256.0,
                0.5,
                0.5 + f64::EPSILON * 256.0,
                0.75,
            ] {
                let value = f64::from(integer) + fraction;
                assert_eq!(
                    super::round_byte(value),
                    value.round_ties_even().clamp(0.0, 255.0) as u8,
                    "{value:?}"
                );
            }
        }
        for value in [-f64::INFINITY, -1.0, -0.0, f64::INFINITY, f64::NAN] {
            assert_eq!(
                super::round_byte(value),
                value.round_ties_even().clamp(0.0, 255.0) as u8
            );
        }
    }
}

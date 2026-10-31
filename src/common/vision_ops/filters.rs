use super::VisionError;
use crate::common::frame::{FloatMap, GrayFrame, GrayView, Image, ImageView};

pub(crate) fn reflect101(index: isize, length: usize) -> usize {
    if length == 1 {
        return 0;
    }
    let period = 2 * (length as isize - 1);
    let index = index.rem_euclid(period);
    if index < length as isize {
        index as usize
    } else {
        (period - index) as usize
    }
}

fn reflected_axis(length: usize, taps: usize) -> Result<Vec<usize>, VisionError> {
    let count = length
        .checked_mul(taps)
        .ok_or(VisionError::Invalid("filter index size overflow"))?;
    let mut indices = Vec::new();
    indices
        .try_reserve_exact(count)
        .map_err(crate::common::frame::FrameError::Allocation)?;
    for position in 0..length {
        for tap in 0..taps {
            indices.push(reflect101(
                position as isize + tap as isize - (taps / 2) as isize,
                length,
            ));
        }
    }
    Ok(indices)
}

/// Q8 가중치를 적용하고 마지막에 한 번 반올림하는 전체 uint8 이미지 필터.
/// 고정 OpenCV 빌드의 전체 행렬 Gaussian 경로이며,
/// ORB가 쓰는 부동소수 ROI 필터와 별도로 유지한다.
pub(crate) fn separable_q8<const C: usize>(
    source: ImageView<'_, u8, C>,
    kernel: &[u32],
) -> Result<Image<u8, C>, VisionError> {
    if kernel.is_empty()
        || kernel.len().is_multiple_of(2)
        || kernel.len() > i32::MAX as usize
        || kernel
            .iter()
            .try_fold(0_u32, |sum, value| sum.checked_add(*value))
            != Some(256)
    {
        return Err(VisionError::Invalid(
            "odd Q8 kernel summing to 256 required",
        ));
    }
    let size = source.size();
    let w = size.width();
    let h = size.height();
    let columns = reflected_axis(w, kernel.len())?;
    let rows = reflected_axis(h, kernel.len())?;
    let mut horizontal = Image::<u32, C>::zeros(size)?;
    for y in 0..h {
        let row = source
            .row(y)
            .ok_or(VisionError::Invalid("missing filter row"))?;
        for x in 0..w {
            for c in 0..C {
                horizontal.pixels_mut()[(y * w + x) * C + c] = kernel
                    .iter()
                    .zip(&columns[x * kernel.len()..(x + 1) * kernel.len()])
                    .map(|(weight, sx)| u32::from(row[sx * C + c]) * weight)
                    .sum();
            }
        }
    }
    let mut result = Image::<u8, C>::zeros(size)?;
    for y in 0..h {
        for x in 0..w {
            for c in 0..C {
                let value: u32 = kernel
                    .iter()
                    .zip(&rows[y * kernel.len()..(y + 1) * kernel.len()])
                    .map(|(weight, sy)| horizontal.pixels()[(sy * w + x) * C + c] * weight)
                    .sum();
                // 최대값이 255 * 256^2이므로 두 단계와 반올림 모두 u32 안에 든다.
                result.pixels_mut()[(y * w + x) * C + c] = ((value + 32768) >> 16) as u8;
            }
        }
    }
    Ok(result)
}

fn gaussian_kernel(mut width: usize, sigma: f64) -> Result<Vec<f64>, VisionError> {
    if !sigma.is_finite() || sigma < 0.0 {
        return Err(VisionError::Invalid(
            "finite nonnegative Gaussian sigma required",
        ));
    }
    if width == 0 {
        let automatic = (sigma * 6.0 + 1.0).round_ties_even();
        if automatic >= i32::MAX as f64 || sigma == 0.0 {
            return Err(VisionError::Invalid("Gaussian kernel size required"));
        }
        width = automatic as usize | 1;
    }
    if width.is_multiple_of(2) || width > i32::MAX as usize {
        return Err(VisionError::Invalid(
            "odd positive Gaussian kernel required",
        ));
    }
    if sigma == 0.0 {
        match width {
            1 => return Ok(vec![1.0]),
            3 => return Ok(vec![0.25, 0.5, 0.25]),
            5 => return Ok(vec![0.0625, 0.25, 0.375, 0.25, 0.0625]),
            7 => {
                return Ok(vec![
                    0.03125, 0.109375, 0.21875, 0.28125, 0.21875, 0.109375, 0.03125,
                ]);
            }
            _ => (),
        }
    }
    let sigma = if sigma == 0.0 {
        0.3 * ((width as f64 - 1.0) * 0.5 - 1.0) + 0.8
    } else {
        sigma
    };
    let mut kernel = Vec::new();
    kernel
        .try_reserve_exact(width)
        .map_err(crate::common::frame::FrameError::Allocation)?;
    for x in 0..width {
        let distance = x as f64 - (width - 1) as f64 * 0.5;
        kernel.push((-0.5 * distance * distance / (sigma * sigma)).exp());
    }
    let total: f64 = kernel.iter().sum();
    for weight in &mut kernel {
        *weight /= total;
    }
    Ok(kernel)
}

pub fn gaussian<const C: usize>(
    source: ImageView<'_, u8, C>,
    width: usize,
    sigma: f64,
) -> Result<Image<u8, C>, VisionError> {
    let kernel = gaussian_kernel(width, sigma)?;
    let size = source.size();
    let columns = reflected_axis(size.width(), kernel.len())?;
    let rows = reflected_axis(size.height(), kernel.len())?;
    let mut horizontal = Image::<f64, C>::zeros(size)?;
    for y in 0..size.height() {
        let row = source
            .row(y)
            .ok_or(VisionError::Invalid("missing filter row"))?;
        for x in 0..size.width() {
            for c in 0..C {
                let mut sum = 0.0;
                for (weight, sx) in kernel
                    .iter()
                    .zip(&columns[x * kernel.len()..(x + 1) * kernel.len()])
                {
                    sum += f64::from(row[sx * C + c]) * weight;
                }
                horizontal.pixels_mut()[(y * size.width() + x) * C + c] = sum;
            }
        }
    }
    let mut output = Image::<u8, C>::zeros(size)?;
    for y in 0..size.height() {
        for x in 0..size.width() {
            for c in 0..C {
                let mut sum = 0.0;
                for (weight, sy) in kernel
                    .iter()
                    .zip(&rows[y * kernel.len()..(y + 1) * kernel.len()])
                {
                    sum += horizontal.pixels()[(sy * size.width() + x) * C + c] * weight;
                }
                output.pixels_mut()[(y * size.width() + x) * C + c] =
                    super::resize::round_byte(sum);
            }
        }
    }
    Ok(output)
}

pub fn box_filter(
    source: ImageView<'_, f32, 1>,
    width: usize,
    height: usize,
) -> Result<FloatMap, VisionError> {
    if width == 0 || height == 0 || width > i32::MAX as usize || height > i32::MAX as usize {
        return Err(VisionError::Invalid(
            "positive native box filter dimensions required",
        ));
    }
    let size = source.size();
    let columns = reflected_axis(size.width(), width)?;
    let rows = reflected_axis(size.height(), height)?;
    let mut horizontal = Image::<f64, 1>::zeros(size)?;
    for y in 0..size.height() {
        let row = source
            .row(y)
            .ok_or(VisionError::Invalid("missing filter row"))?;
        if row.iter().any(|value| !value.is_finite()) {
            return Err(VisionError::Invalid("finite box filter samples required"));
        }
        for x in 0..size.width() {
            let sum: f64 = columns[x * width..(x + 1) * width]
                .iter()
                .map(|sx| f64::from(row[*sx]))
                .sum();
            horizontal.pixels_mut()[y * size.width() + x] = sum / width as f64;
        }
    }
    let mut output = FloatMap::zeros(size)?;
    for y in 0..size.height() {
        for x in 0..size.width() {
            let sum: f64 = rows[y * height..(y + 1) * height]
                .iter()
                .map(|sy| horizontal.pixels()[sy * size.width() + x])
                .sum();
            output.pixels_mut()[y * size.width() + x] = (sum / height as f64) as f32;
        }
    }
    Ok(output)
}

pub fn threshold(source: GrayView<'_>, threshold: f64) -> Result<GrayFrame, VisionError> {
    if !threshold.is_finite() {
        return Err(VisionError::Invalid("finite threshold required"));
    }
    let mut output = GrayFrame::zeros(source.size())?;
    for y in 0..source.size().height() {
        let row = source
            .row(y)
            .ok_or(VisionError::Invalid("missing threshold row"))?;
        for (x, value) in row.iter().enumerate() {
            output.pixels_mut()[y * source.size().width() + x] = if f64::from(*value) > threshold {
                255
            } else {
                0
            };
        }
    }
    Ok(output)
}

pub fn otsu(source: GrayView<'_>) -> Result<(u8, GrayFrame), VisionError> {
    let mut histogram = [0usize; 256];
    for y in 0..source.size().height() {
        for value in source
            .row(y)
            .ok_or(VisionError::Invalid("missing threshold row"))?
        {
            histogram[*value as usize] += 1;
        }
    }
    let total = source.size().width() * source.size().height();
    let total_sum: f64 = histogram
        .iter()
        .enumerate()
        .map(|(value, count)| value as f64 * *count as f64)
        .sum();
    let (mut below, mut below_sum, mut best_score, mut best) = (0usize, 0.0, 0.0, 0u8);
    for (value, count) in histogram.into_iter().enumerate() {
        below += count;
        below_sum += value as f64 * count as f64;
        if below == 0 || below == total {
            continue;
        }
        let delta = below_sum / below as f64 - (total_sum - below_sum) / (total - below) as f64;
        let score = below as f64 * (total - below) as f64 * delta * delta;
        if score > best_score {
            best_score = score;
            best = value as u8;
        }
    }
    Ok((best, threshold(source, f64::from(best))?))
}

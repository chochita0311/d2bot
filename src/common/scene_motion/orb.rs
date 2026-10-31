//! ORB600 with the reference app's fixed 31px / Harris / WTA2 configuration.
//! 이미지·피라미드·descriptor는 소유한 Rust 버퍼이며 외부 영상 라이브러리를 호출하지 않는다.
//! OpenCV algorithm/pattern attribution: THIRD-PARTY-NOTICES.md.
use super::{ConfigError, FeatureSet, allocation, selection::retain_best};
use crate::common::frame::{GrayFrame, GrayView, Size};
use serde::Serialize;
use std::sync::Arc;

mod pattern;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Keypoint {
    pub point: [f32; 2],
    pub size: f32,
    pub angle: f32,
    pub response: f32,
    pub octave: usize,
}
#[derive(Debug)]
pub struct OrbFeatures {
    pub keypoints: Vec<Keypoint>,
    pub descriptors: Vec<[u8; 32]>,
}
impl OrbFeatures {
    pub fn into_features(self) -> Result<FeatureSet, ConfigError> {
        let mut points = Vec::new();
        points
            .try_reserve_exact(self.keypoints.len())
            .map_err(allocation)?;
        for point in self.keypoints {
            points.push(point.point);
        }
        FeatureSet::new(points, self.descriptors)
    }
}
fn image_error(error: impl std::fmt::Display) -> ConfigError {
    ConfigError::new("VisionError", error.to_string())
}
fn zeros(width: usize, height: usize) -> Result<GrayFrame, ConfigError> {
    GrayFrame::zeros(Size::new(width, height).map_err(image_error)?).map_err(image_error)
}
fn pixel(image: &GrayFrame, x: usize, y: usize) -> u8 {
    image.pixels()[y * image.size().width() + x]
}
fn sample(image: &GrayFrame, x: isize, y: isize) -> u8 {
    // Callers provide the ORB 31px edge margin, including rotated BRIEF samples.
    pixel(image, x as usize, y as usize)
}

// 회전 BRIEF 오프셋은 유한한 [-32,32] 범위다. 정수/소수 분리로 같은 짝수
// 반올림을 수행해 디스크립터마다 수십만 번의 CRT 반올림 호출을 피한다.
fn round_offset(value: f32) -> isize {
    let whole = value as isize;
    let fraction = (value - whole as f32).abs();
    if fraction > 0.5 || (fraction == 0.5 && whole % 2 != 0) {
        whole + if value < 0.0 { -1 } else { 1 }
    } else {
        whole
    }
}

/// Bit-exact linear resize of a gray/mask pyramid (Q8 coefficient pairs).
fn resize_exact(source: GrayView<'_>, size: Size) -> Result<GrayFrame, ConfigError> {
    let mut output = GrayFrame::zeros(size).map_err(image_error)?;
    let src = source.size();
    let coefficient = |d: usize, input: usize, output: usize| {
        let scale = 1.0 / (output as f64 / input as f64);
        let coordinate = scale * (d as f64 + 0.5) - 0.5;
        let first = coordinate.floor();
        if first < 0.0 {
            (0, 0, 0u32)
        } else if first >= (input - 1) as f64 {
            (input - 1, input - 1, 0)
        } else {
            (
                first as usize,
                first as usize + 1,
                ((coordinate - first) * 256.0).round_ties_even() as u32,
            )
        }
    };
    // 동일한 Q8 좌표·가중치를 행마다 다시 계산하지 않는다.
    let mut horizontal = Vec::new();
    horizontal
        .try_reserve_exact(size.width())
        .map_err(allocation)?;
    horizontal.extend((0..size.width()).map(|x| coefficient(x, src.width(), size.width())));
    for y in 0..size.height() {
        let (y0, y1, cy) = coefficient(y, src.height(), size.height());
        let row0 = source
            .row(y0)
            .ok_or_else(|| ConfigError::value("pyramid row absent"))?;
        let row1 = source
            .row(y1)
            .ok_or_else(|| ConfigError::value("pyramid row absent"))?;
        let row = &mut output.pixels_mut()[y * size.width()..(y + 1) * size.width()];
        let mut x = 0;
        // Q8의 최대 중간값은255*65536이며 u32 lane에서도 같은 반올림을 유지한다.
        while row.len() - x >= 4 {
            use wide::u32x4;
            let coefficients = &horizontal[x..x + 4];
            let cx = u32x4::from(std::array::from_fn(|i| coefficients[i].2));
            let a = u32x4::from(std::array::from_fn(|i| u32::from(row0[coefficients[i].0])));
            let b = u32x4::from(std::array::from_fn(|i| u32::from(row0[coefficients[i].1])));
            let c = u32x4::from(std::array::from_fn(|i| u32::from(row1[coefficients[i].0])));
            let d = u32x4::from(std::array::from_fn(|i| u32::from(row1[coefficients[i].1])));
            let h0 = a * (u32x4::splat(256) - cx) + b * cx;
            let h1 = c * (u32x4::splat(256) - cx) + d * cx;
            let values =
                ((h0 * u32x4::splat(256 - cy) + h1 * u32x4::splat(cy) + u32x4::splat(32768))
                    >> 16u32)
                    .to_array();
            for (pixel, value) in row[x..x + 4].iter_mut().zip(values) {
                *pixel = value as u8;
            }
            x += 4;
        }
        for (pixel, &(x0, x1, cx)) in row[x..].iter_mut().zip(&horizontal[x..]) {
            let h0 = u32::from(row0[x0]) * (256 - cx) + u32::from(row0[x1]) * cx;
            let h1 = u32::from(row1[x0]) * (256 - cx) + u32::from(row1[x1]) * cx;
            *pixel = ((h0 * (256 - cy) + h1 * cy + 32768) >> 16) as u8;
        }
    }
    Ok(output)
}
const CIRCLE: [[isize; 2]; 16] = [
    [0, 3],
    [1, 3],
    [2, 2],
    [3, 1],
    [3, 0],
    [3, -1],
    [2, -2],
    [1, -3],
    [0, -3],
    [-1, -3],
    [-2, -2],
    [-3, -1],
    [-3, 0],
    [-3, 1],
    [-2, 2],
    [-1, 3],
];
fn cyclic_nine(mask: &[wide::u8x16; 16]) -> wide::u8x16 {
    let pairs: [wide::u8x16; 16] = std::array::from_fn(|i| mask[i] & mask[(i + 1) & 15]);
    let quads: [wide::u8x16; 16] = std::array::from_fn(|i| pairs[i] & pairs[(i + 2) & 15]);
    let mut found = wide::u8x16::ZERO;
    for i in 0..16 {
        found |= quads[i] & quads[(i + 4) & 15] & mask[(i + 8) & 15];
    }
    found
}
fn candidate_lanes(image: &GrayFrame, x: usize, y: usize) -> u32 {
    use wide::u8x16;
    let width = image.size().width();
    let at = y * width + x;
    let center = u8x16::from(&image.pixels()[at..at + 16]);
    let mut light = [u8x16::ZERO; 16];
    let mut dark = light;
    for (i, [dx, dy]) in CIRCLE.into_iter().enumerate() {
        let start = (y as isize + dy) as usize * width + (x as isize + dx) as usize;
        let neighbor = u8x16::from(&image.pixels()[start..start + 16]);
        light[i] = center.saturating_sub(neighbor).simd_gt(u8x16::splat(20));
        dark[i] = neighbor.saturating_sub(center).simd_gt(u8x16::splat(20));
    }
    (cyclic_nine(&light) | cyclic_nine(&dark)).to_bitmask()
}
fn score_at(image: &GrayFrame, x: usize, y: usize) -> u8 {
    let center = i16::from(pixel(image, x, y));
    let mut differences = [0; 25];
    for (value, [dx, dy]) in differences[..16].iter_mut().zip(CIRCLE) {
        *value = center - i16::from(sample(image, x as isize + dx, y as isize + dy));
    }
    differences.copy_within(..9, 16);
    ring_score(&differences)
}
fn fast(image: &GrayFrame, mask: Option<&GrayFrame>) -> Result<Vec<Keypoint>, ConfigError> {
    let size = image.size();
    let mut scores = zeros(size.width(), size.height())?;
    let mut result = Vec::new();
    if size.width() <= 62 || size.height() <= 62 {
        return Ok(result);
    }
    // 최종 후보는31px 안쪽이고 NMS는 그 한 픽셀 이웃만 읽는다. mask 밖의
    // 이웃 점수도 남겨 억제 결과를 보존하면서 사용되지 않는 바깥 영역을 건너뛴다.
    for y in 30..size.height() - 30 {
        let mut left = size.width();
        let mut right = 0;
        if let Some(mask) = mask {
            for row in y.saturating_sub(1).max(31)..(y + 2).min(size.height() - 31) {
                let line = &mask.pixels()[row * size.width() + 31..(row + 1) * size.width() - 31];
                if let Some(first) = line.iter().position(|value| *value != 0) {
                    left = left.min(first + 30);
                    if let Some(last) = line.iter().rposition(|value| *value != 0) {
                        right = right.max(last + 33);
                    }
                }
            }
        } else {
            left = 30;
            right = size.width() - 30;
        }
        for x in (left..right).step_by(16) {
            if right - x >= 16 {
                let mut lanes = candidate_lanes(image, x, y);
                while lanes != 0 {
                    let selected = x + lanes.trailing_zeros() as usize;
                    scores.pixels_mut()[y * size.width() + selected] = score_at(image, selected, y);
                    lanes &= lanes - 1;
                }
            } else {
                for column in x..right {
                    scores.pixels_mut()[y * size.width() + column] = score_at(image, column, y);
                }
            }
        }
    }
    // FAST non-maximum suppression occurs before mask/border filtering.
    for y in 31..size.height() - 31 {
        for x in 31..size.width() - 31 {
            let score = pixel(&scores, x, y);
            if score == 0 || mask.is_some_and(|mask| pixel(mask, x, y) == 0) {
                continue;
            }
            if (-1..=1).all(|dy| {
                (-1..=1).all(|dx| {
                    (dx == 0 && dy == 0)
                        || score > sample(&scores, x as isize + dx, y as isize + dy)
                })
            }) {
                result.try_reserve(1).map_err(allocation)?;
                result.push(Keypoint {
                    point: [x as f32, y as f32],
                    size: 31.0,
                    angle: -1.0,
                    response: f32::from(score),
                    octave: 0,
                });
            }
        }
    }
    Ok(result)
}
fn ring_score(differences: &[i16; 25]) -> u8 {
    let mut best = 20i16;
    // 인접한 두 길이9 창은 가운데8개를 공유한다. 원래 min/max 점수를 그대로 묶는다.
    for start in (0..16).step_by(2) {
        let middle = &differences[start + 1..start + 9];
        let low = middle.iter().copied().min().unwrap_or(0);
        let high = middle.iter().copied().max().unwrap_or(0);
        for edge in [differences[start], differences[start + 9]] {
            best = best.max(low.min(edge)).max(-high.max(edge));
        }
    }
    if best > 20 { (best - 1) as u8 } else { 0 }
}
fn harris(image: &GrayFrame, keypoint: &mut Keypoint) {
    let [x, y] = keypoint.point.map(|n| n as isize);
    let (mut a, mut b, mut c) = (0i32, 0i32, 0i32);
    for dy in -3..=3 {
        for dx in -3..=3 {
            let value = |ox, oy| i32::from(sample(image, x + dx + ox, y + dy + oy));
            let ix = (value(1, 0) - value(-1, 0)) * 2 + value(1, -1) - value(-1, -1) + value(1, 1)
                - value(-1, 1);
            let iy = (value(0, 1) - value(0, -1)) * 2 + value(-1, 1) - value(-1, -1) + value(1, 1)
                - value(1, -1);
            a += ix * ix;
            b += iy * iy;
            c += ix * iy;
        }
    }
    let scale = 1.0f32 / (4.0 * 7.0 * 255.0);
    let fourth = scale * scale * scale * scale;
    keypoint.response = (a as f32 * b as f32
        - c as f32 * c as f32
        - 0.04f32 * (a as f32 + b as f32) * (a as f32 + b as f32))
        * fourth;
}
fn fast_atan(y: f32, x: f32) -> f32 {
    let degrees = (180.0 / std::f64::consts::PI) as f32;
    let p1 = 0.999_787_841_279_480_7_f64 as f32 * degrees;
    let p3 = -0.325_808_397_464_097_5_f64 as f32 * degrees;
    let p5 = 0.155_578_651_846_328_1_f64 as f32 * degrees;
    let p7 = -0.044_326_555_547_921_28_f64 as f32 * degrees;
    let (ax, ay) = (x.abs(), y.abs());
    let c = ax.min(ay) / (ax.max(ay) + f64::EPSILON as f32);
    let c2 = c * c;
    let mut angle = (((p7 * c2 + p5) * c2 + p3) * c2 + p1) * c;
    if ax < ay {
        angle = 90.0 - angle;
    }
    if x < 0.0 {
        angle = 180.0 - angle;
    }
    if y < 0.0 {
        angle = 360.0 - angle;
    }
    angle
}
fn orientation(image: &GrayFrame, point: [f32; 2]) -> f32 {
    let mut umax = [0isize; 17];
    for (v, value) in umax.iter_mut().enumerate().take(12) {
        *value = (225.0 - (v * v) as f64).sqrt().round_ties_even() as isize;
    }
    let mut v0 = 0;
    for v in (11..=15).rev() {
        while umax[v0] == umax[v0 + 1] {
            v0 += 1;
        }
        umax[v] = v0 as isize;
        v0 += 1;
    }
    let [x, y] = point.map(|n| n as isize);
    let (mut m01, mut m10) = (0i32, 0i32);
    for u in -15..=15 {
        m10 += u as i32 * i32::from(sample(image, x + u, y));
    }
    for (v, d) in umax.iter().enumerate().take(16).skip(1) {
        let mut sum = 0;
        for u in -*d..=*d {
            let plus = i32::from(sample(image, x + u, y + v as isize));
            let minus = i32::from(sample(image, x + u, y - v as isize));
            sum += plus - minus;
            m10 += u as i32 * (plus + minus);
        }
        m01 += v as i32 * sum;
    }
    fast_atan(m01 as f32, m10 as f32)
}
fn blur(image: &GrayFrame) -> Result<GrayFrame, ConfigError> {
    // ORB uses a non-isolated ROI: the general float separable filter, rather
    // than GaussianBlur's whole-Mat Q8 bit-exact kernel.
    let mut kernel = [0.0f64; 7];
    for (index, value) in kernel.iter_mut().enumerate() {
        let x = index as f64 - 3.0;
        *value = (-x * x / 8.0).exp();
    }
    let total: f64 = kernel.iter().sum();
    let kernel = kernel.map(|n| (n / total) as f32);
    let size = image.size();
    let mut horizontal = crate::common::frame::Image::<f32, 1>::zeros(size).map_err(image_error)?;
    let reflect = |index: isize, length: usize| {
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
    };
    let mut columns = Vec::new();
    columns
        .try_reserve_exact(size.width())
        .map_err(allocation)?;
    columns.extend((0..size.width()).map(|x| {
        std::array::from_fn::<_, 7, _>(|k| reflect(x as isize + k as isize - 3, size.width()))
    }));
    for y in 0..size.height() {
        let row = &image.pixels()[y * size.width()..(y + 1) * size.width()];
        let target = &mut horizontal.pixels_mut()[y * size.width()..(y + 1) * size.width()];
        if size.width() >= 7 {
            // 안쪽 행은 연속 슬라이스로 순회한다. 각 픽셀의 k=0..6 누적 순서는 같다.
            for (k, weight) in kernel.iter().enumerate() {
                for (value, source) in target[3..size.width() - 3]
                    .iter_mut()
                    .zip(&row[k..k + size.width() - 6])
                {
                    *value += *weight * f32::from(*source);
                }
            }
        }
        let left = size.width().min(3);
        let right = size.width().saturating_sub(3).max(left);
        for x in (0..left).chain(right..size.width()) {
            let mut sum = 0.0f32;
            for (k, weight) in kernel.iter().enumerate() {
                sum += *weight * f32::from(row[columns[x][k]]);
            }
            target[x] = sum;
        }
    }
    let mut output = zeros(size.width(), size.height())?;
    for y in 0..size.height() {
        let above: [usize; 3] = std::array::from_fn(|k| {
            reflect(y as isize - k as isize - 1, size.height()) * size.width()
        });
        let below: [usize; 3] = std::array::from_fn(|k| {
            reflect(y as isize + k as isize + 1, size.height()) * size.width()
        });
        let vector_end = size.width() / 4 * 4;
        for x in (0..vector_end).step_by(4) {
            let load = |start: usize| wide::f32x4::from(&horizontal.pixels()[start..start + 4]);
            let mut sum = wide::f32x4::splat(kernel[3]) * load(y * size.width() + x);
            for k in 1..=3 {
                sum += wide::f32x4::splat(kernel[3 + k])
                    * (load(above[k - 1] + x) + load(below[k - 1] + x));
            }
            let values = sum.round_ties_even().to_array();
            for (target, value) in output.pixels_mut()
                [y * size.width() + x..y * size.width() + x + 4]
                .iter_mut()
                .zip(values)
            {
                *target = value.clamp(0.0, 255.0) as u8;
            }
        }
        for x in vector_end..size.width() {
            let mut sum = kernel[3] * horizontal.pixels()[y * size.width() + x];
            for k in 1..=3 {
                sum += kernel[3 + k]
                    * (horizontal.pixels()[above[k - 1] + x]
                        + horizontal.pixels()[below[k - 1] + x]);
            }
            output.pixels_mut()[y * size.width() + x] =
                crate::common::vision_ops::resize::round_byte(f64::from(sum));
        }
    }
    Ok(output)
}
fn extract_level(
    image: &GrayFrame,
    mask: Option<&GrayFrame>,
    level: usize,
    count: usize,
    scale: f32,
) -> Result<OrbFeatures, ConfigError> {
    let mut keypoints = fast(image, mask)?;
    retain_best(&mut keypoints, count * 2);
    for point in &mut keypoints {
        point.octave = level;
        point.size = 31.0 * scale;
        harris(image, point);
    }
    retain_best(&mut keypoints, count);
    for point in &mut keypoints {
        point.angle = orientation(image, point.point);
        point.point = point.point.map(|n| n * scale);
    }
    let mut descriptors = Vec::new();
    descriptors
        .try_reserve_exact(keypoints.len())
        .map_err(allocation)?;
    if keypoints.is_empty() {
        return Ok(OrbFeatures {
            keypoints,
            descriptors,
        });
    }
    let blurred = blur(image)?;
    for point in &keypoints {
        let inverse = 1.0f32 / scale;
        let [x, y] = point
            .point
            .map(|n| (n * inverse).round_ties_even() as isize);
        let angle = point.angle * (std::f64::consts::PI / 180.0) as f32;
        let a = f64::from(angle).cos() as f32;
        let b = f64::from(angle).sin() as f32;
        let value = |p: [i8; 2]| {
            let px = f32::from(p[0]);
            let py = f32::from(p[1]);
            let dx = round_offset(px * a - py * b);
            let dy = round_offset(px * b + py * a);
            sample(&blurred, x + dx, y + dy)
        };
        descriptors.push(std::array::from_fn(|byte| {
            let mut output = 0;
            for bit in 0..8 {
                let index = byte * 16 + bit * 2;
                output |=
                    u8::from(value(pattern::PATTERN[index]) < value(pattern::PATTERN[index + 1]))
                        << bit;
            }
            output
        }));
    }
    Ok(OrbFeatures {
        keypoints,
        descriptors,
    })
}

fn pyramid_levels(
    image: GrayView<'_>,
    mask: Option<GrayView<'_>>,
    scales: &[f32; 8],
    mut visit: impl FnMut(usize, Arc<GrayFrame>, Option<Arc<GrayFrame>>) -> Result<(), ConfigError>,
) -> Result<(), ConfigError> {
    let mut previous = Arc::new(image.to_owned().map_err(image_error)?);
    let mut previous_mask = mask
        .map(|m| m.to_owned().map_err(image_error))
        .transpose()?;
    if let Some(mask) = &mut previous_mask {
        for pixel in mask.pixels_mut() {
            *pixel = if *pixel == 0 { 0 } else { 255 };
        }
    }
    let mut previous_mask = previous_mask.map(Arc::new);
    for (level, scale) in scales.iter().enumerate() {
        if level != 0 {
            let inverse = 1.0f32 / scale;
            let size = Size::new(
                (image.size().width() as f32 * inverse).round_ties_even() as usize,
                (image.size().height() as f32 * inverse).round_ties_even() as usize,
            )
            .map_err(image_error)?;
            previous = Arc::new(resize_exact(previous.view(), size)?);
            let mut mask = previous_mask
                .as_ref()
                .map(|m| resize_exact(m.view(), size))
                .transpose()?;
            if let Some(mask) = &mut mask {
                for pixel in mask.pixels_mut() {
                    if *pixel < 255 {
                        *pixel = 0;
                    }
                }
            }
            previous_mask = mask.map(Arc::new);
        }
        visit(level, previous.clone(), previous_mask.clone())?;
    }
    Ok(())
}
pub fn extract(
    image: GrayView<'_>,
    mask: Option<GrayView<'_>>,
) -> Result<OrbFeatures, ConfigError> {
    if mask.is_some_and(|mask| mask.size() != image.size())
        || image.size().width() > i32::MAX as usize
        || image.size().height() > i32::MAX as usize
    {
        return Err(ConfigError::value(
            "ORB requires matching gray image/mask sizes within i32",
        ));
    }
    let factor = 1.0f32 / 1.2f32;
    let mut desired = 600.0f32 * (1.0 - factor) / (1.0 - f64::from(factor).powi(8) as f32);
    let mut counts = [0usize; 8];
    for count in &mut counts[..7] {
        *count = desired.round_ties_even() as usize;
        desired *= factor;
    }
    counts[7] = 600usize.saturating_sub(counts[..7].iter().sum());
    let scales: [f32; 8] = std::array::from_fn(|level| f64::from(1.2f32).powi(level as i32) as f32);
    let mut levels: [Option<Result<OrbFeatures, ConfigError>>; 8] = std::array::from_fn(|_| None);
    if image.size().width() * image.size().height() < 100_000
        || crate::common::compute::parallelism() == 1
    {
        pyramid_levels(image, mask, &scales, |level, image, mask| {
            levels[level] = Some(extract_level(
                &image,
                mask.as_deref(),
                level,
                counts[level],
                scales[level],
            ));
            Ok(())
        })?;
    } else {
        let mut slots = levels.iter_mut();
        crate::common::compute::scope(|scope| {
            pyramid_levels(image, mask, &scales, |level, image, mask| {
                let slot = slots
                    .next()
                    .ok_or_else(|| ConfigError::value("ORB level slot missing"))?;
                scope.spawn(move |_| {
                    *slot = Some(extract_level(
                        &image,
                        mask.as_deref(),
                        level,
                        counts[level],
                        scales[level],
                    ))
                });
                Ok(())
            })
        })
        .map_err(|e| match e {
            crate::common::compute::ComputeError::Start(e) => {
                ConfigError::new("OSError", e.to_string())
            }
            crate::common::compute::ComputeError::Panicked => {
                ConfigError::value("ORB worker panicked")
            }
        })??;
    }
    let mut keypoints = Vec::new();
    let mut descriptors = Vec::new();
    keypoints.try_reserve(600).map_err(allocation)?;
    descriptors.try_reserve(600).map_err(allocation)?;
    // 단계 번호와 내부 순서를 보존해 응답 동점·매칭 선택을 바꾸지 않는다.
    for level in levels {
        let level = level.ok_or_else(|| ConfigError::value("ORB level missing"))??;
        keypoints
            .try_reserve(level.keypoints.len())
            .map_err(allocation)?;
        descriptors
            .try_reserve(level.descriptors.len())
            .map_err(allocation)?;
        keypoints.extend(level.keypoints);
        descriptors.extend(level.descriptors);
    }
    Ok(OrbFeatures {
        keypoints,
        descriptors,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn vector_ring_detects_every_cyclic_nine_pattern() {
        for start in (0u32..65536).step_by(16) {
            let masks = std::array::from_fn(|bit| {
                wide::u8x16::new(std::array::from_fn(|lane| {
                    if (start + lane as u32) & (1 << bit) != 0 {
                        255
                    } else {
                        0
                    }
                }))
            });
            let found = cyclic_nine(&masks).to_array();
            for (lane, actual) in found.into_iter().enumerate() {
                let bits = (start + lane as u32) as u16;
                let expected = (0..16).any(|shift| bits.rotate_right(shift) & 511 == 511);
                assert_eq!(actual != 0, expected, "ring{bits:016b}");
            }
        }
    }
    #[test]
    fn vector_candidates_match_scalar_at_strict_contrast_boundaries() -> Result<(), ConfigError> {
        let mut image = zeros(96, 80)?;
        let mut state = 739u32;
        for range in [21u32, 42, 256] {
            for value in image.pixels_mut() {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                *value = ((state >> 16) % range) as u8;
            }
            for y in 30..50 {
                for x in [30, 46, 50] {
                    let actual = candidate_lanes(&image, x, y);
                    for lane in 0..16 {
                        assert_eq!(
                            actual & (1 << lane) != 0,
                            score_at(&image, x + lane, y) != 0
                        );
                    }
                }
            }
        }
        Ok(())
    }
    #[test]
    fn paired_ring_scores_preserve_cyclic_windows() {
        let mut state = 739u32;
        for iteration in 0..20000 {
            let mut differences = [0i16; 25];
            for value in &mut differences[..16] {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                *value = if iteration < 16 {
                    if iteration == state as usize % 16 {
                        255
                    } else {
                        -255
                    }
                } else {
                    ((state >> 16) % 511) as i16 - 255
                };
            }
            differences.copy_within(..9, 16);
            let expected = (0..16).fold(20i16, |best, start| {
                let window = &differences[start..start + 9];
                best.max(window.iter().copied().min().unwrap_or(0))
                    .max(-window.iter().copied().max().unwrap_or(0))
            });
            assert_eq!(
                ring_score(&differences),
                if expected > 20 {
                    (expected - 1) as u8
                } else {
                    0
                }
            );
        }
    }
    #[test]
    #[ignore = "explicit input-free performance measurement"]
    fn efficiency_profile_orb_stages() -> Result<(), Box<dyn std::error::Error>> {
        let _compute = crate::common::compute::ComputeLease::acquire()?;
        use super::*;
        use crate::common::vision_ops::codec;
        use std::{hint::black_box, path::Path, time::Instant};
        let source = codec::load_gray(Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/navigation/north-step-before.png",
        ))?;
        let image = super::super::preprocess::scaled_gray(source.view(), 0.5)?;
        let mut mask = zeros(image.size().width(), image.size().height())?;
        for y in 90..445 {
            for x in 60..860 {
                mask.pixels_mut()[y * image.size().width() + x] = 255;
            }
        }
        for name in ["fast", "blur", "extract_level", "extract_all"] {
            let mut times = Vec::new();
            for iteration in 0..33 {
                let started = Instant::now();
                match name {
                    "fast" => {
                        black_box(fast(&image, Some(&mask))?);
                    }
                    "blur" => {
                        black_box(blur(&image)?);
                    }
                    "extract_level" => {
                        black_box(extract_level(&image, Some(&mask), 0, 130, 1.0)?);
                    }
                    _ => {
                        black_box(extract(image.view(), Some(mask.view()))?);
                    }
                }
                if iteration >= 3 {
                    times.push(started.elapsed().as_secs_f64() * 1000.0);
                }
            }
            times.sort_by(f64::total_cmp);
            println!(
                "{}",
                serde_json::json!({"profile":"orb","stage":name,"samples":30,"warmups":3,"p50_ms":times[14],"p95_ms":times[28]})
            );
        }
        Ok(())
    }
    use super::*;

    #[test]
    fn vector_pyramid_resize_matches_scalar_q8_edges_and_tails() -> Result<(), ConfigError> {
        for (width, height) in [(1, 1), (2, 3), (9, 11), (383, 287)] {
            let mut source = zeros(width, height)?;
            let mut state = 739u32;
            for pixel in source.pixels_mut() {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                *pixel = (state >> 24) as u8;
            }
            for (out_width, out_height) in [(1, 2), (3, 5), (8, 7), (319, 239)] {
                let size = Size::new(out_width, out_height).map_err(image_error)?;
                let actual = resize_exact(source.view(), size)?;
                let axis = |index: usize, input: usize, output: usize| {
                    let position =
                        (1.0 / (output as f64 / input as f64)) * (index as f64 + 0.5) - 0.5;
                    let first = position.floor();
                    if first < 0.0 {
                        (0, 0, 0u32)
                    } else if first >= (input - 1) as f64 {
                        (input - 1, input - 1, 0)
                    } else {
                        (
                            first as usize,
                            first as usize + 1,
                            ((position - first) * 256.0).round_ties_even() as u32,
                        )
                    }
                };
                for y in 0..out_height {
                    let (y0, y1, cy) = axis(y, height, out_height);
                    for x in 0..out_width {
                        let (x0, x1, cx) = axis(x, width, out_width);
                        let h0 = u32::from(pixel(&source, x0, y0)) * (256 - cx)
                            + u32::from(pixel(&source, x1, y0)) * cx;
                        let h1 = u32::from(pixel(&source, x0, y1)) * (256 - cx)
                            + u32::from(pixel(&source, x1, y1)) * cx;
                        assert_eq!(
                            pixel(&actual, x, y),
                            ((h0 * (256 - cy) + h1 * cy + 32768) >> 16) as u8
                        );
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn contiguous_blur_matches_scalar_taps_on_tiny_edges_and_large_images()
    -> Result<(), ConfigError> {
        for (width, height) in [(1, 1), (2, 5), (6, 3), (7, 7), (31, 33), (383, 287)] {
            for seed in [0_u32, 255, 739] {
                let mut state = seed;
                let mut source = zeros(width, height)?;
                for pixel in source.pixels_mut() {
                    state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    *pixel = if seed < 256 {
                        seed as u8
                    } else {
                        (state >> 24) as u8
                    };
                }
                let mut kernel = std::array::from_fn::<_, 7, _>(|k| {
                    let x = k as f64 - 3.0;
                    (-x * x / 8.0).exp()
                });
                let total: f64 = kernel.iter().sum();
                for value in &mut kernel {
                    *value /= total;
                }
                let kernel = kernel.map(|n| n as f32);
                let mut horizontal = crate::common::frame::Image::<f32, 1>::zeros(source.size())
                    .map_err(image_error)?;
                let reflect = crate::common::vision_ops::filters::reflect101;
                for y in 0..height {
                    for x in 0..width {
                        let mut sum = 0.0_f32;
                        for (k, weight) in kernel.iter().enumerate() {
                            sum += *weight
                                * f32::from(pixel(
                                    &source,
                                    reflect(x as isize + k as isize - 3, width),
                                    y,
                                ));
                        }
                        horizontal.pixels_mut()[y * width + x] = sum;
                    }
                }
                let mut expected = zeros(width, height)?;
                for y in 0..height {
                    for x in 0..width {
                        let mut sum = kernel[3] * horizontal.pixels()[y * width + x];
                        for k in 1..=3 {
                            let above = reflect(y as isize - k as isize, height);
                            let below = reflect(y as isize + k as isize, height);
                            sum += kernel[3 + k]
                                * (horizontal.pixels()[above * width + x]
                                    + horizontal.pixels()[below * width + x]);
                        }
                        expected.pixels_mut()[y * width + x] =
                            crate::common::vision_ops::resize::round_byte(f64::from(sum));
                    }
                }
                assert_eq!(
                    blur(&source)?.bytes(),
                    expected.bytes(),
                    "{width}x{height}, seed={seed}"
                );
            }
        }
        Ok(())
    }
    #[test]
    fn brief_signed_offsets_preserve_even_rounding_and_near_ties() {
        for quarter in -128..=128 {
            let value = quarter as f32 * 0.25;
            for sample in [value.next_down(), value, value.next_up()] {
                assert_eq!(
                    super::round_offset(sample),
                    sample.round_ties_even() as isize,
                    "{sample}"
                );
            }
        }
    }
}

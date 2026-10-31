//! Partial similarity estimation: x'=a*x-b*y+tx, y'=b*x+a*y+ty.
//! The two-point model and fixed-seed RANSAC follow the installed OpenCV4.13
//! contract; inlier refinement solves the same linear least-squares objective.
//! Source attribution and terms are retained in repository THIRD-PARTY-NOTICES.md.
use super::{ConfigError, allocation};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AffineModel {
    pub a: f64,
    pub b: f64,
    pub translation: [f64; 2],
    pub inliers: Vec<bool>,
}
struct Rng(u64);
impl Rng {
    fn index(&mut self, count: usize) -> usize {
        self.0 = (self.0 as u32 as u64)
            .wrapping_mul(4_164_903_690)
            .wrapping_add(self.0 >> 32);
        (self.0 as u32 as usize) % count
    }
}
fn two_point(from: [[f32; 2]; 2], to: [[f32; 2]; 2]) -> [f64; 4] {
    let [[x1, y1], [x2, y2]] = from.map(|point| point.map(f64::from));
    let [[u1, v1], [u2, v2]] = to.map(|point| point.map(f64::from));
    let d = 1.0 / ((x1 - x2) * (x1 - x2) + (y1 - y2) * (y1 - y2));
    [
        d * ((u1 - u2) * (x1 - x2) + (v1 - v2) * (y1 - y2)),
        d * ((v1 - v2) * (x1 - x2) - (u1 - u2) * (y1 - y2)),
        d * ((v1 - v2) * (x1 * y2 - x2 * y1)
            - (u1 * y2 - u2 * y1) * (y1 - y2)
            - (u1 * x2 - u2 * x1) * (x1 - x2)),
        d * (-(u1 - u2) * (x1 * y2 - x2 * y1)
            - (v1 * x2 - v2 * x1) * (x1 - x2)
            - (v1 * y2 - v2 * y1) * (y1 - y2)),
    ]
}
fn inlier(source: [f32; 2], target: [f32; 2], model: [f64; 4], threshold: f32) -> bool {
    let [a, b, tx, ty] = model.map(|n| n as f32);
    let x = a * source[0] - b * source[1] + tx - target[0];
    let y = b * source[0] + a * source[1] + ty - target[1];
    x * x + y * y <= threshold
}
fn update_iterations(count: usize, accepted: usize, maximum: usize) -> usize {
    let outliers = (count - accepted) as f64 / count as f64;
    let denominator = 1.0 - (1.0 - outliers).powi(2);
    if denominator < f64::MIN_POSITIVE {
        return 0;
    }
    let numerator = (1.0_f64 - 0.99).max(f64::MIN_POSITIVE).ln();
    let denominator = denominator.ln();
    if denominator >= 0.0 || -numerator >= maximum as f64 * (-denominator) {
        maximum
    } else {
        (numerator / denominator).round_ties_even() as usize
    }
}
pub fn estimate_partial_affine(
    from: &[[f32; 2]],
    to: &[[f32; 2]],
    threshold: f64,
) -> Result<Option<AffineModel>, ConfigError> {
    if from.len() != to.len()
        || from.len() > i32::MAX as usize
        || from.iter().chain(to).flatten().any(|n| !n.is_finite())
    {
        return Err(ConfigError::value(
            "finite paired affine point arrays required",
        ));
    }
    super::time::positive(threshold, "affine reprojection threshold")?;
    if from.len() < 2 {
        return Ok(None);
    }
    let mut best = Vec::new();
    best.try_reserve_exact(from.len()).map_err(allocation)?;
    best.resize(from.len(), false);
    let mut current = Vec::new();
    current.try_reserve_exact(from.len()).map_err(allocation)?;
    current.resize(from.len(), false);
    let mut selected = None;
    let mut best_count = 0;
    if from.len() == 2 {
        let model = two_point([from[0], from[1]], [to[0], to[1]]);
        if model.iter().any(|n| !n.is_finite()) {
            return Ok(None);
        }
        best.fill(true);
        selected = Some(model);
    } else {
        let mut rng = Rng(u64::MAX);
        let mut iterations = 2000;
        let mut iteration = 0;
        let squared = (threshold * threshold) as f32;
        while iteration < iterations {
            iteration += 1;
            let first = rng.index(from.len());
            let mut second = rng.index(from.len());
            while second == first {
                second = rng.index(from.len());
            }
            let model = two_point([from[first], from[second]], [to[first], to[second]]);
            let mut count = 0;
            for (index, (source, target)) in from.iter().zip(to).enumerate() {
                current[index] = inlier(*source, *target, model, squared);
                count += usize::from(current[index]);
            }
            if count > best_count.max(1) {
                std::mem::swap(&mut current, &mut best);
                best_count = count;
                selected = Some(model);
                iterations = update_iterations(from.len(), count, iterations);
            }
        }
    }
    let Some(mut model) = selected else {
        return Ok(None);
    };
    if from.len() > 2
        && let Some(refined) = refine(from, to, &best)
    {
        model = refined;
    }
    if model.iter().any(|n| !n.is_finite()) {
        return Ok(None);
    }
    Ok(Some(AffineModel {
        a: model[0],
        b: model[1],
        translation: [model[2], model[3]],
        inliers: best,
    }))
}
fn refine(from: &[[f32; 2]], to: &[[f32; 2]], mask: &[bool]) -> Option<[f64; 4]> {
    let mut center = [0.0; 4];
    let mut count = 0.0;
    for ((left, right), accepted) in from.iter().zip(to).zip(mask) {
        if *accepted {
            for (sum, value) in center
                .iter_mut()
                .zip([left[0], left[1], right[0], right[1]])
            {
                *sum += f64::from(value);
            }
            count += 1.0;
        }
    }
    if count == 0.0 {
        return None;
    }
    center = center.map(|n| n / count);
    let [cx, cy, cu, cv] = center;
    let (mut denominator, mut real, mut imaginary) = (0.0, 0.0, 0.0);
    for ((left, right), accepted) in from.iter().zip(to).zip(mask) {
        if *accepted {
            let x = f64::from(left[0]) - cx;
            let y = f64::from(left[1]) - cy;
            let u = f64::from(right[0]) - cu;
            let v = f64::from(right[1]) - cv;
            denominator += x * x + y * y;
            real += x * u + y * v;
            imaginary += x * v - y * u;
        }
    }
    if denominator == 0.0 {
        return None;
    }
    let a = real / denominator;
    let b = imaginary / denominator;
    Some([a, b, cu - a * cx + b * cy, cv - b * cx - a * cy])
}

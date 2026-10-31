//! Background feature correspondence. Image extraction and observer state build on
//! this input-free, deterministic matching/model boundary.

mod affine;
mod observer;
pub mod orb;
mod preprocess;
mod selection;
use super::{config::ConfigError, time};
pub use affine::{AffineModel, estimate_partial_affine};
pub use observer::{BackgroundMotionObserver, LandmarkMask, MotionView, SceneFeatures};
use serde::Serialize;

#[derive(Debug, Clone, Copy)]
pub struct MotionLimits {
    minimum_matches: usize,
    minimum_inlier_ratio: f64,
    maximum_error: f64,
    scale: f64,
}
impl MotionLimits {
    pub fn new(
        minimum_matches: usize,
        minimum_inlier_ratio: f64,
        maximum_error: f64,
        scale: f64,
    ) -> Result<Self, ConfigError> {
        if minimum_matches < 4 {
            return Err(ConfigError::value("minimum_matches must be at least 4"));
        }
        time::positive(minimum_inlier_ratio, "minimum_inlier_ratio")?;
        time::positive(maximum_error, "maximum_error")?;
        time::positive(scale, "scale")?;
        if minimum_inlier_ratio > 1.0 || scale > 1.0 {
            return Err(ConfigError::value(
                "motion ratio and scale must not exceed 1",
            ));
        }
        Ok(Self {
            minimum_matches,
            minimum_inlier_ratio,
            maximum_error,
            scale,
        })
    }
    pub fn scale(self) -> f64 {
        self.scale
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MotionReason {
    MappingDeferred,
    FrameNotNewer,
    CorrespondenceUnavailable,
    CorrespondenceInsufficient,
    TranslationUnverified,
    BackgroundModelRejected,
    BackgroundError,
    BackgroundTranslation,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CameraMotion {
    pub displacement: Option<[f64; 2]>,
    pub inliers: usize,
    pub reason: MotionReason,
    pub matches: usize,
    pub inlier_ratio: Option<f64>,
    pub linear_error: Option<f64>,
}
impl CameraMotion {
    fn unknown(reason: MotionReason, inliers: usize, matches: usize) -> Self {
        Self {
            displacement: None,
            inliers,
            reason,
            matches,
            inlier_ratio: None,
            linear_error: None,
        }
    }
}
#[derive(Debug, Clone)]
pub struct FeatureSet {
    points: Vec<[f32; 2]>,
    descriptors: Vec<[u8; 32]>,
}
impl FeatureSet {
    pub fn new(points: Vec<[f32; 2]>, descriptors: Vec<[u8; 32]>) -> Result<Self, ConfigError> {
        if points.len() != descriptors.len()
            || points.len() > i32::MAX as usize
            || points.iter().flatten().any(|n| !n.is_finite())
        {
            return Err(ConfigError::value(
                "finite paired feature points and descriptors required",
            ));
        }
        Ok(Self {
            points,
            descriptors,
        })
    }
    pub fn points(&self) -> &[[f32; 2]] {
        &self.points
    }
    pub fn descriptors(&self) -> &[[u8; 32]] {
        &self.descriptors
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DescriptorMatch {
    pub query: usize,
    pub train: usize,
    pub distance: u32,
}
pub fn hamming_matches(
    source: &[[u8; 32]],
    target: &[[u8; 32]],
) -> Result<Vec<DescriptorMatch>, ConfigError> {
    let mut matches = Vec::new();
    if target.len() < 2 {
        return Ok(matches);
    }
    matches.try_reserve(source.len()).map_err(allocation)?;
    for (query, left) in source.iter().enumerate() {
        let mut first = (u32::MAX, 0);
        let mut second = u32::MAX;
        for (train, right) in target.iter().enumerate() {
            let distance = descriptor_distance(left, right);
            if distance < first.0 {
                second = first.0;
                first = (distance, train);
            } else if distance < second {
                second = distance;
            }
        }
        if f64::from(first.0) < 0.7 * f64::from(second) {
            matches.push(DescriptorMatch {
                query,
                train: first.1,
                distance: first.0,
            });
        }
    }
    Ok(matches)
}

fn descriptor_distance(left: &[u8; 32], right: &[u8; 32]) -> u32 {
    // 정렬되지 않은 주소도 안전한 바이트 변환을 사용한다. 32번의 작은 popcount를
    // 네 개의 정수로 묶으며 비트 개수와 동점 선택은 그대로다.
    left.as_chunks::<8>()
        .0
        .iter()
        .zip(right.as_chunks::<8>().0)
        .map(|(a, b)| (u64::from_ne_bytes(*a) ^ u64::from_ne_bytes(*b)).count_ones())
        .sum()
}

#[cfg(test)]
mod packed_distance_tests {
    #[test]
    fn packed_distance_preserves_every_bit_and_mixed_byte_patterns() {
        for bit in 0..256 {
            let mut descriptor = [0; 32];
            descriptor[bit / 8] = 1 << (bit % 8);
            assert_eq!(super::descriptor_distance(&descriptor, &[0; 32]), 1);
            assert_eq!(super::descriptor_distance(&descriptor, &[255; 32]), 255);
        }
        let mut random = 739_u32;
        for _ in 0..1000 {
            let mut next = || {
                random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (random >> 24) as u8
            };
            let a = std::array::from_fn(|_| next());
            let b = std::array::from_fn(|_| next());
            let expected: u32 = a.iter().zip(b).map(|(x, y)| (x ^ y).count_ones()).sum();
            assert_eq!(super::descriptor_distance(&a, &b), expected);
        }
    }
}
pub fn measure(
    source: &FeatureSet,
    target: &FeatureSet,
    limits: MotionLimits,
) -> Result<CameraMotion, ConfigError> {
    if source.points.len().min(target.points.len()) < 2 {
        return Ok(CameraMotion::unknown(
            MotionReason::CorrespondenceInsufficient,
            0,
            0,
        ));
    }
    let matches = hamming_matches(&source.descriptors, &target.descriptors)?;
    if matches.len() < limits.minimum_matches {
        return Ok(CameraMotion::unknown(
            MotionReason::CorrespondenceInsufficient,
            matches.len(),
            matches.len(),
        ));
    }
    let mut from = Vec::new();
    let mut to = Vec::new();
    from.try_reserve_exact(matches.len()).map_err(allocation)?;
    to.try_reserve_exact(matches.len()).map_err(allocation)?;
    for pair in &matches {
        from.push(source.points[pair.query]);
        to.push(target.points[pair.train]);
    }
    let Some(model) = estimate_partial_affine(&from, &to, limits.maximum_error * limits.scale)?
    else {
        return Ok(CameraMotion::unknown(
            MotionReason::TranslationUnverified,
            0,
            matches.len(),
        ));
    };
    classify_model(&from, &to, &model, limits)
}
fn classify_model(
    from: &[[f32; 2]],
    to: &[[f32; 2]],
    model: &AffineModel,
    limits: MotionLimits,
) -> Result<CameraMotion, ConfigError> {
    let count = model.inliers.iter().filter(|value| **value).count();
    let ratio = count as f64 / from.len() as f64;
    let linear_error = (model.a - 1.0).abs().max(model.b.abs());
    let mut result = CameraMotion {
        displacement: None,
        inliers: count,
        reason: MotionReason::BackgroundModelRejected,
        matches: from.len(),
        inlier_ratio: Some(ratio),
        linear_error: Some(linear_error),
    };
    if count < limits.minimum_matches || ratio < limits.minimum_inlier_ratio || linear_error > 0.03
    {
        return Ok(result);
    }
    let mut errors = Vec::new();
    errors.try_reserve_exact(count).map_err(allocation)?;
    for ((source, target), accepted) in from.iter().zip(to).zip(&model.inliers) {
        if *accepted {
            let x = model.a * f64::from(source[0]) - model.b * f64::from(source[1])
                + model.translation[0]
                - f64::from(target[0]);
            let y = model.b * f64::from(source[0])
                + model.a * f64::from(source[1])
                + model.translation[1]
                - f64::from(target[1]);
            errors.push((x * x + y * y).sqrt());
        }
    }
    errors.sort_by(f64::total_cmp);
    let middle = errors.len() / 2;
    let median = if errors.len() % 2 == 0 {
        (errors[middle - 1] + errors[middle]) / 2.0
    } else {
        errors[middle]
    };
    if median > limits.maximum_error * limits.scale {
        result.reason = MotionReason::BackgroundError;
        return Ok(result);
    }
    result.reason = MotionReason::BackgroundTranslation;
    result.displacement = Some(model.translation.map(|n| -n / limits.scale));
    Ok(result)
}
fn allocation(error: std::collections::TryReserveError) -> ConfigError {
    ConfigError::new("MemoryError", format!("motion allocation failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn acceptance_boundaries_use_source_strict_predicates() -> Result<(), ConfigError> {
        let source = [[0.0f32; 2]; 8];
        let target = source;
        let mut model = AffineModel {
            a: 1.0,
            b: 0.03,
            translation: [0.0; 2],
            inliers: vec![true, true, true, true, false, false, false, false],
        };
        let limits = MotionLimits::new(4, 0.5, 3.0, 0.5)?;
        assert_eq!(
            classify_model(&source, &target, &model, limits)?.reason,
            MotionReason::BackgroundTranslation
        );
        model.b = 0.03f64.next_up();
        assert_eq!(
            classify_model(&source, &target, &model, limits)?.reason,
            MotionReason::BackgroundModelRejected
        );
        model.b = 0.0;
        let stricter = MotionLimits::new(4, 0.5f64.next_up(), 3.0, 0.5)?;
        assert_eq!(
            classify_model(&source, &target, &model, stricter)?.reason,
            MotionReason::BackgroundModelRejected
        );
        let target = [[1.5, 0.0]; 8];
        assert_eq!(
            classify_model(&source, &target, &model, limits)?.reason,
            MotionReason::BackgroundTranslation
        );
        let target = [[1.5f32.next_up(), 0.0]; 8];
        assert_eq!(
            classify_model(&source, &target, &model, limits)?.reason,
            MotionReason::BackgroundError
        );
        model.inliers[3] = false;
        assert_eq!(
            classify_model(&source, &source, &model, limits)?.reason,
            MotionReason::BackgroundModelRejected
        );
        Ok(())
    }
}

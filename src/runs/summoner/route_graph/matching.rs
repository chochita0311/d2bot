use super::*;
use crate::common::scene_motion::{estimate_partial_affine, hamming_matches};
impl ArcaneRouteGraph {
    pub(super) fn relocate(
        &mut self,
        features: Option<&Arc<SceneFeatures>>,
        anchor: [f64; 2],
    ) -> Result<Option<(usize, [f64; 2])>, ObservationError> {
        let Some(features) = features else {
            return Ok(None);
        };
        let Some(descriptors) = features.descriptors().filter(|d| d.len() >= 24) else {
            return Ok(None);
        };
        // 최근 두 참조와 순환하는 두 참조만 비교하며 원본 이미지는 보관하지 않는다.
        let count = self.references.len();
        let mut indices: Vec<_> = (count.saturating_sub(2)..count).collect();
        if count > 0 {
            for offset in 0..count.min(2) {
                let index = (self.reference_cursor + offset) % count;
                if !indices.contains(&index) {
                    indices.push(index);
                }
            }
            self.reference_cursor = (self.reference_cursor + 2) % count;
        }
        let mut found = Vec::new();
        for index in indices {
            let reference = &self.references[index];
            let source = &reference.features;
            if source.frame_size != features.frame_size {
                continue;
            }
            let Some(source_descriptors) = source.descriptors() else {
                continue;
            };
            let pairs = hamming_matches(source_descriptors, descriptors)?;
            if pairs.len() < 24 {
                continue;
            }
            let left: Vec<_> = pairs.iter().map(|p| source.points()[p.query]).collect();
            let right: Vec<_> = pairs.iter().map(|p| features.points()[p.train]).collect();
            let Some(model) = estimate_partial_affine(&left, &right, 3.0)? else {
                continue;
            };
            let inliers = model.inliers.iter().filter(|n| **n).count();
            if inliers < 24
                || inliers as f64 / (pairs.len() as f64) < 0.7
                || (model.a - 1.0).abs().max(model.b.abs()) > 0.03
            {
                continue;
            }
            let mut errors: Vec<_> = left
                .iter()
                .zip(&right)
                .zip(&model.inliers)
                .filter(|(_, valid)| **valid)
                .map(|((p, q), _)| {
                    let [x, y] = p.map(f64::from);
                    let dx = model.a * x - model.b * y + model.translation[0] - f64::from(q[0]);
                    let dy = model.b * x + model.a * y + model.translation[1] - f64::from(q[1]);
                    (dx * dx + dy * dy).sqrt()
                })
                .collect();
            errors.sort_by(f64::total_cmp);
            let mid = errors.len() / 2;
            let median = if errors.len().is_multiple_of(2) {
                (errors[mid - 1] + errors[mid]) * 0.5
            } else {
                errors[mid]
            };
            if median > 3.0 {
                continue;
            }
            let position = std::array::from_fn(|i| {
                reference.position[i] + anchor[i] - reference.anchor[i] - model.translation[i]
            });
            found.push((inliers, position, reference.chart));
        }
        let mut best = None;
        for item in &found {
            if best.is_none_or(|before: &(usize, [f64; 2], usize)| item.0 > before.0) {
                best = Some(item);
            }
        }
        let Some(best) = best else {
            return Ok(None);
        };
        if found
            .iter()
            .any(|(_, p, chart)| *chart == best.2 && distance(*p, best.1) > self.place_radius)
        {
            return Ok(None);
        }
        Ok(Some((best.2, best.1)))
    }
}

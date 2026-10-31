use super::{
    CameraMotion, ConfigError, FeatureSet, MotionLimits, MotionReason, allocation, measure, orb,
    preprocess::{image_error, scaled_gray},
};
use crate::common::{
    frame::{BgrView, GrayFrame, GrayView, Image, PixelRect, Size},
    vision_ops::{
        color,
        resize::{self, Interpolation},
    },
};
use std::sync::Arc;

#[derive(Debug, Clone, Copy)]
pub struct LandmarkMask {
    pub viewport: PixelRect,
    /// Left, above, right, below, in native pixels around the anchor.
    pub actor_exclusion: [usize; 4],
}
pub struct MotionView<'a> {
    pub sequence: u64,
    pub captured_at: f64,
    pub frame_size: Size,
    pub viewport: PixelRect,
    pub anchor: [f64; 2],
    pub cell_pixels: usize,
    pub floor: &'a Image<bool, 1>,
    pub excluded_pixels: Option<&'a Image<bool, 1>>,
}
#[derive(Debug)]
pub struct SceneFeatures {
    pub sequence: u64,
    pub captured_at: f64,
    pub frame_size: Size,
    native_points: Vec<[f32; 2]>,
    scaled: FeatureSet,
}
impl SceneFeatures {
    /// 진단·재생에서 이미 추출한 native 특징을 검증해 소유한다. 원본 영상은 받지 않는다.
    pub fn from_native(
        sequence: u64,
        captured_at: f64,
        frame_size: Size,
        points: Vec<[f32; 2]>,
        descriptors: Option<Vec<[u8; 32]>>,
    ) -> Result<Self, ConfigError> {
        super::time::nonnegative(captured_at, "feature captured_at")?;
        if points.len() > i32::MAX as usize || points.iter().flatten().any(|v| !v.is_finite()) {
            return Err(ConfigError::value("finite native feature points required"));
        }
        let scaled = match descriptors {
            Some(descriptors) => FeatureSet::new(points.clone(), descriptors)?,
            None => FeatureSet::new(Vec::new(), Vec::new())?,
        };
        Ok(Self {
            sequence,
            captured_at,
            frame_size,
            native_points: points,
            scaled,
        })
    }
    pub fn points(&self) -> &[[f32; 2]] {
        &self.native_points
    }
    pub fn descriptors(&self) -> Option<&[[u8; 32]]> {
        (!self.scaled.descriptors().is_empty()).then(|| self.scaled.descriptors())
    }
    pub fn feature_bytes(&self) -> usize {
        self.native_points.len() * 8 + self.scaled.descriptors().len() * 32
    }
}
pub struct BackgroundMotionObserver {
    compute: Option<crate::common::compute::ComputeLease>,
    limits: MotionLimits,
    landmark: Option<LandmarkMask>,
    previous: Option<Arc<SceneFeatures>>,
    last_seen: Option<(u64, f64)>,
    latest: Option<Arc<SceneFeatures>>,
}
impl BackgroundMotionObserver {
    pub fn new(limits: MotionLimits, landmark: Option<LandmarkMask>) -> Self {
        Self {
            compute: None,
            limits,
            landmark,
            previous: None,
            last_seen: None,
            latest: None,
        }
    }
    pub fn reset(&mut self) {
        self.previous = None;
        self.last_seen = None;
        self.latest = None;
    }
    pub fn features(&self) -> Option<&Arc<SceneFeatures>> {
        self.latest.as_ref()
    }
    pub fn previous_sequence(&self) -> Option<u64> {
        self.previous.as_ref().map(|f| f.sequence)
    }
    pub fn observe(
        &mut self,
        frame: BgrView<'_>,
        view: MotionView<'_>,
        overlap_hint: Option<[f64; 2]>,
    ) -> Result<CameraMotion, ConfigError> {
        if frame.size() != view.frame_size {
            return Err(ConfigError::value("motion frame and path view differ"));
        }
        view.viewport
            .checked_in(view.frame_size)
            .map_err(image_error)?;
        if view
            .anchor
            .iter()
            .any(|n| !n.is_finite() || *n < 0.0 || *n > i32::MAX as f64)
            || view.cell_pixels == 0
        {
            return Err(ConfigError::value(
                "finite native anchor and positive cell size required",
            ));
        }
        if view
            .excluded_pixels
            .is_some_and(|mask| mask.size() != view.frame_size)
        {
            return Err(ConfigError::value(
                "same-frame excluded-pixel mask required",
            ));
        }
        let mut mask = GrayFrame::zeros(view.frame_size).map_err(image_error)?;
        let [ax, ay] = view.anchor.map(|n| n as usize);
        let viewport = self.landmark.map_or(view.viewport, |m| m.viewport);
        viewport.checked_in(view.frame_size).map_err(image_error)?;
        let exclusion = if let Some(landmark) = self.landmark {
            for y in viewport.top()..viewport.bottom() {
                mask.pixels_mut()[y * view.frame_size.width() + viewport.left()
                    ..y * view.frame_size.width() + viewport.right()]
                    .fill(255);
            }
            landmark.actor_exclusion
        } else {
            let occupied_width = view
                .floor
                .size()
                .width()
                .checked_mul(view.cell_pixels)
                .ok_or_else(|| ConfigError::value("floor width overflow"))?;
            let occupied_height = view
                .floor
                .size()
                .height()
                .checked_mul(view.cell_pixels)
                .ok_or_else(|| ConfigError::value("floor height overflow"))?;
            if occupied_width > view.frame_size.width() - viewport.left()
                || occupied_height > view.frame_size.height() - viewport.top()
            {
                return Err(ConfigError::value("floor mask outside frame"));
            }
            for y in 0..occupied_height {
                for x in 0..occupied_width {
                    let occupied = view.floor.pixels()
                        [(y / view.cell_pixels) * view.floor.size().width() + x / view.cell_pixels];
                    mask.pixels_mut()
                        [(y + viewport.top()) * view.frame_size.width() + x + viewport.left()] =
                        if occupied { 255 } else { 0 };
                }
            }
            let radius = (0.1 * viewport.width().min(viewport.height()) as f64) as usize;
            [radius; 4]
        };
        let [left, above, right, below] = exclusion;
        let x0 = viewport.left().max(ax.saturating_sub(left));
        let y0 = viewport.top().max(ay.saturating_sub(above));
        let x1 = viewport.right().min(ax.saturating_add(right));
        let y1 = viewport.bottom().min(ay.saturating_add(below));
        if x0 < x1 && y0 < y1 {
            for y in y0..y1 {
                mask.pixels_mut()
                    [y * view.frame_size.width() + x0..y * view.frame_size.width() + x1]
                    .fill(0);
            }
        }
        if let Some(excluded) = view.excluded_pixels {
            for (value, excluded) in mask.pixels_mut().iter_mut().zip(excluded.pixels()) {
                if *excluded {
                    *value = 0;
                }
            }
        }
        self.observe_mask(
            frame,
            mask.view(),
            view.sequence,
            view.captured_at,
            overlap_hint,
        )
    }
    pub fn observe_mask(
        &mut self,
        frame: BgrView<'_>,
        mask: GrayView<'_>,
        sequence: u64,
        captured_at: f64,
        overlap_hint: Option<[f64; 2]>,
    ) -> Result<CameraMotion, ConfigError> {
        super::time::nonnegative(captured_at, "motion captured_at")?;
        let gray = color::gray(frame).map_err(image_error)?;
        let gray = scaled_gray(gray.view(), self.limits.scale)?;
        if mask.size() != frame.size() {
            return Err(ConfigError::value("motion mask and frame differ"));
        }
        let mask =
            resize::resize(mask, gray.size(), Interpolation::Nearest).map_err(image_error)?;
        if self.compute.is_none() {
            self.compute = Some(
                crate::common::compute::ComputeLease::acquire()
                    .map_err(|e| ConfigError::new("OSError", e.to_string()))?,
            );
        }
        let extracted = orb::extract(gray.view(), Some(mask.view()))?;
        // Extraction intentionally precedes the stamp check, matching source order.
        if self.last_seen.is_some_and(|(last_sequence, last_time)| {
            sequence <= last_sequence || captured_at < last_time
        }) {
            return Ok(CameraMotion::unknown(MotionReason::FrameNotNewer, 0, 0));
        }
        self.last_seen = Some((sequence, captured_at));
        let scaled = extracted.into_features()?;
        let mut native_points = Vec::new();
        native_points
            .try_reserve_exact(scaled.points().len())
            .map_err(allocation)?;
        for point in scaled.points() {
            native_points.push(point.map(|n| n / self.limits.scale as f32));
        }
        let current = Arc::new(SceneFeatures {
            sequence,
            captured_at,
            frame_size: frame.size(),
            native_points,
            scaled,
        });
        self.latest = Some(Arc::clone(&current));
        let Some(previous) = &self.previous else {
            self.previous = Some(current);
            return Ok(CameraMotion::unknown(
                MotionReason::CorrespondenceUnavailable,
                0,
                0,
            ));
        };
        if previous.frame_size != current.frame_size
            || previous.descriptors().is_none()
            || current.descriptors().is_none()
        {
            if previous.frame_size != current.frame_size || previous.descriptors().is_none() {
                self.previous = Some(current);
            }
            return Ok(CameraMotion::unknown(
                MotionReason::CorrespondenceUnavailable,
                0,
                0,
            ));
        }
        let mut result = if let Some(hint) = overlap_hint {
            let viewport = self
                .landmark
                .ok_or_else(|| {
                    ConfigError::value("finite native overlap hint and landmark viewport required")
                })?
                .viewport;
            if hint.iter().any(|n| !n.is_finite()) {
                return Err(ConfigError::value(
                    "finite native overlap hint and landmark viewport required",
                ));
            }
            let source = visible(previous, hint, -1.0, viewport)?;
            let target = visible(&current, hint, 1.0, viewport)?;
            measure(&source, &target, self.limits)?
        } else {
            measure(&previous.scaled, &current.scaled, self.limits)?
        };
        if result.displacement.is_none() && overlap_hint.is_some() {
            let stationary = measure(&previous.scaled, &current.scaled, self.limits)?;
            if stationary
                .displacement
                .is_some_and(|[x, y]| (x * x + y * y).sqrt() < 2.0)
            {
                result = stationary;
            }
        }
        if result.displacement.is_some() {
            self.previous = Some(current);
        }
        Ok(result)
    }
}
fn visible(
    features: &SceneFeatures,
    hint: [f64; 2],
    sign: f64,
    viewport: PixelRect,
) -> Result<FeatureSet, ConfigError> {
    let mut points = Vec::new();
    let mut descriptors = Vec::new();
    points
        .try_reserve(features.scaled.points().len())
        .map_err(allocation)?;
    descriptors
        .try_reserve(features.scaled.points().len())
        .map_err(allocation)?;
    for ((native, point), descriptor) in features
        .native_points
        .iter()
        .zip(features.scaled.points())
        .zip(features.scaled.descriptors())
    {
        let x = f64::from(native[0]) + sign * hint[0];
        let y = f64::from(native[1]) + sign * hint[1];
        if x >= viewport.left() as f64
            && x < viewport.right() as f64
            && y >= viewport.top() as f64
            && y < viewport.bottom() as f64
        {
            points.push(*point);
            descriptors.push(*descriptor);
        }
    }
    FeatureSet::new(points, descriptors)
}

use super::*;
use crate::{
    actions::ActionError,
    common::{
        automap_vision::AutomapObserver,
        frame::{BgrFrame, BgrView, GrayFrame, GrayView, PixelRect, Size},
        scene_motion::{BackgroundMotionObserver, MotionLimits},
        vision_ops::{
            codec, color,
            components::{self, Connectivity},
            morphology::{self, Kernel, Morphology},
            resize::{self, Interpolation},
            template::{self, GrayMatcher, MatchMethod},
        },
    },
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path, sync::Arc};

#[derive(Deserialize)]
struct Manifest {
    reference_size: [usize; 2],
    templates: serde_json::Map<String, serde_json::Value>,
}
#[derive(Deserialize)]
struct Definition {
    file: String,
    search: [usize; 4],
    #[serde(default = "threshold")]
    threshold: f64,
    #[serde(default)]
    variants: Vec<String>,
    #[serde(default = "width_scales")]
    width_scales: Vec<f64>,
    #[serde(default)]
    foreground: Option<String>,
    #[serde(default)]
    click_offset: Option<[f64; 2]>,
    #[serde(default)]
    variant_click_offsets: HashMap<String, [f64; 2]>,
    #[serde(default)]
    blue_nearby: Option<[f64; 2]>,
    #[serde(default)]
    blue_pair: Option<BluePair>,
}
fn threshold() -> f64 {
    0.88
}
fn width_scales() -> Vec<f64> {
    vec![1.0]
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BluePair {
    pub search: [usize; 4],
    pub hsv_lower: [u8; 3],
    pub hsv_upper: [u8; 3],
    pub minimum_area: usize,
    pub width: [usize; 2],
    pub height: [usize; 2],
    pub maximum_aspect_ratio: f64,
    pub separation: [f64; 2],
    pub vertical_difference: f64,
    pub click_offset: [f64; 2],
}
struct Variant {
    filename: String,
    scale: f64,
    image: GrayFrame,
}
struct Template {
    definition: Definition,
    variants: Vec<Variant>,
}
type NativeWaypoint =
    Box<dyn FnMut(&Arc<BgrFrame>) -> Result<Option<[f64; 2]>, ActionError> + Send>;
pub struct TownVision {
    templates: HashMap<String, Template>,
    titles: AutomapObserver,
    native: Option<Arc<BgrFrame>>,
    color: Option<BgrFrame>,
    gray: Option<GrayFrame>,
    native_waypoint: Option<NativeWaypoint>,
    motion: BackgroundMotionObserver,
    camera_sequence: u64,
    camera_stable_frames: usize,
    camera_stable_since: Option<f64>,
    camera_reading: serde_json::Value,
}
impl TownVision {
    pub fn load(root: impl AsRef<Path>) -> Result<Self, ActionError> {
        let root = root.as_ref();
        let directory = root.join("assets/ui/town-loop/remastered-ko-window");
        Self::load_directory(root, &directory)
    }
    /// 수동 보정 도구도 같은 판독 사전 검증을 사용하되 명시한 자산 은행을 소유한다.
    pub fn load_directory(root: &Path, directory: &Path) -> Result<Self, ActionError> {
        let path = directory.join("manifest.json");
        if !path.exists() {
            return Err(ActionError::runtime(
                "Shared town-loop calibration is missing. See assets/ui/town-loop/remastered-ko-window/manifest.json.",
            ));
        }
        let manifest: Manifest = serde_json::from_slice(&std::fs::read(path)?)?;
        if manifest.reference_size != REFERENCE_SIZE {
            return Err(ActionError::runtime(
                "town calibration reference size differs from planner",
            ));
        }
        let mut templates = HashMap::new();
        for (name, value) in manifest.templates {
            let definition: Definition = serde_json::from_value(value)?;
            rect(definition.search)?
                .checked_in(Size::new(REFERENCE_SIZE[0], REFERENCE_SIZE[1])?)?;
            if definition.width_scales.is_empty()
                || definition
                    .width_scales
                    .iter()
                    .any(|s| !s.is_finite() || !(0.96..=1.04).contains(s))
            {
                return Err(ActionError::runtime(format!(
                    "Town label width scales must stay within four percent: {name}"
                )));
            }
            if !definition.threshold.is_finite() {
                return Err(ActionError::runtime("finite town threshold required"));
            }
            let mut variants = Vec::new();
            for (index, filename) in std::iter::once(&definition.file)
                .chain(&definition.variants)
                .enumerate()
            {
                let image = codec::load_gray(directory.join(filename)).map_err(|_| {
                    ActionError::runtime(format!(
                        "Town loop calibration {} is unreadable: {name}",
                        if index == 0 { "image" } else { "variant" }
                    ))
                })?;
                for &scale in &definition.width_scales {
                    let scaled = if scale == 1.0 {
                        image.clone()
                    } else {
                        scale_width(image.view(), scale)?
                    };
                    let processed = foreground(scaled.view(), definition.foreground.as_deref())?;
                    variants.push(Variant {
                        filename: filename.clone(),
                        scale,
                        image: gaussian(processed.view(), false)?,
                    });
                }
            }
            templates.insert(
                name,
                Template {
                    definition,
                    variants,
                },
            );
        }
        Ok(Self {
            templates,
            titles: AutomapObserver::load(root.join("assets/ui/automap"))?,
            native: None,
            color: None,
            gray: None,
            native_waypoint: None,
            motion: BackgroundMotionObserver::new(MotionLimits::new(12, 0.65, 3.0, 0.5)?, None),
            camera_sequence: 0,
            camera_stable_frames: 0,
            camera_stable_since: None,
            camera_reading: serde_json::Value::Null,
        })
    }
    pub fn calibrated(&self, name: &str) -> bool {
        self.templates.contains_key(name)
    }
    pub fn set_frame(&mut self, frame: Arc<BgrFrame>) {
        self.native = Some(frame);
        self.color = None;
        self.gray = None;
    }
    /// 같은 프레임 판독이 끝난 뒤 픽셀만 반환하며 경로/카메라 추적 상태는 유지한다.
    pub fn clear_frame(&mut self) {
        self.native = None;
        self.color = None;
        self.gray = None;
    }
    /// 패널 유무만 필요한 필드에서는 전체 축소 영상 대신 동일한 두 검색 ROI만 만든다.
    pub fn panels_open(&mut self, frame: &Arc<BgrFrame>) -> Result<bool, ActionError> {
        validate_shape(frame.size())?;
        let names = ["waypoint_header", "save_exit"];
        if names.iter().any(|name| {
            self.templates.get(*name).is_some_and(|t| {
                t.definition.blue_pair.is_some() || t.definition.blue_nearby.is_some()
            })
        }) {
            // 색상 보조 조건이 추가된 보정 은행은 기존 전체 영상 계약으로 판독한다.
            self.set_frame(frame.clone());
            let result = (|| {
                self.normalize()?;
                for name in names {
                    if self.find(name)?.is_some() {
                        return Ok(true);
                    }
                }
                Ok(false)
            })();
            self.clear_frame();
            return result;
        }
        let size = Size::new(REFERENCE_SIZE[0], REFERENCE_SIZE[1])?;
        for name in names {
            let Some(label) = self.templates.get(name) else {
                continue;
            };
            let region = resize::area_region(frame.view(), size, rect(label.definition.search)?)?;
            let gray = color::gray(region.view())?;
            let foreground = foreground(gray.view(), label.definition.foreground.as_deref())?;
            let matcher = GrayMatcher::new(gaussian(foreground.view(), false)?)?;
            let mut best: Option<f32> = None;
            for variant in &label.variants {
                if matcher.size().width() < variant.image.size().width()
                    || matcher.size().height() < variant.image.size().height()
                {
                    continue;
                }
                let scores = matcher
                    .match_pattern(variant.image.view(), MatchMethod::NormalizedCoefficient)?;
                if let Some(peak) = template::maximum(&scores)
                    && best.is_none_or(|value| peak.score > value)
                {
                    best = Some(peak.score);
                }
            }
            if best.is_some_and(|score| f64::from(score) >= label.definition.threshold) {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub fn set_native_waypoint_observer(
        &mut self,
        observer: impl FnMut(&Arc<BgrFrame>) -> Result<Option<[f64; 2]>, ActionError> + Send + 'static,
    ) {
        self.native_waypoint = Some(Box::new(observer));
    }
    pub fn normalized(&self) -> Option<&BgrFrame> {
        self.color.as_ref()
    }
    pub fn find_blue_pair(
        color: BgrView<'_>,
        definition: &BluePair,
    ) -> Result<Option<TownPoint>, ActionError> {
        let search = rect(definition.search)?;
        let hsv = color::hsv(color.crop(search)?)?;
        let mask = color::in_range(hsv.view(), definition.hsv_lower, definition.hsv_upper)?;
        let mask = morphology::morphology(
            mask.view(),
            Kernel::Rectangle {
                width: 7,
                height: 7,
            },
            Morphology::Close,
        )?;
        let labels = components::connected_components(mask.view(), Connectivity::Eight)?;
        let centers = labels
            .stats
            .into_iter()
            .skip(1)
            .filter_map(|c| {
                let r = c.rect?;
                let center = c.centroid?;
                (c.area >= definition.minimum_area
                    && (definition.width[0]..=definition.width[1]).contains(&r.width())
                    && (definition.height[0]..=definition.height[1]).contains(&r.height())
                    && r.width() as f64 / r.height() as f64 <= definition.maximum_aspect_ratio)
                    .then_some(center)
            })
            .collect::<Vec<_>>();
        let mut pairs = Vec::new();
        for (i, left) in centers.iter().enumerate() {
            for right in &centers[i + 1..] {
                let dx = (left[0] - right[0]).abs();
                if (definition.separation[0]..=definition.separation[1]).contains(&dx)
                    && (left[1] - right[1]).abs() <= definition.vertical_difference
                {
                    pairs.push([(left[0] + right[0]) / 2.0, (left[1] + right[1]) / 2.0]);
                }
            }
        }
        if pairs.len() != 1 {
            return Ok(None);
        }
        let [x, y] = pairs[0];
        Ok(Some(point(
            [
                search.left() as f64 + x + definition.click_offset[0],
                search.top() as f64 + y + definition.click_offset[1],
            ],
            Some("paired_blue_braziers"),
        )))
    }
}
impl TownSight for TownVision {
    fn normalize(&mut self) -> Result<(), ActionError> {
        let native = self
            .native
            .as_ref()
            .ok_or_else(|| ActionError::runtime("current town frame missing"))?;
        validate_shape(native.size())?;
        let color = resize::resize(
            native.view(),
            Size::new(REFERENCE_SIZE[0], REFERENCE_SIZE[1])?,
            Interpolation::Area,
        )?;
        self.gray = Some(color::gray(color.view())?);
        self.color = Some(color);
        Ok(())
    }
    fn find(&mut self, name: &str) -> Result<Option<TownPoint>, ActionError> {
        let Some(template) = self.templates.get(name) else {
            return Ok(None);
        };
        if name == "world_arcane_waypoint"
            && let (Some(observer), Some(native)) = (&mut self.native_waypoint, &self.native)
            && native.size() == Size::new(1922, 1140)?
            && let Some(p) = observer(native)?
        {
            if p.iter().any(|v| !v.is_finite()) {
                return Err(ActionError::runtime(
                    "native waypoint observer returned a non-finite point",
                ));
            }
            return Ok(Some(point(
                [
                    p[0] * REFERENCE_SIZE[0] as f64 / 1922.0,
                    p[1] * REFERENCE_SIZE[1] as f64 / 1140.0,
                ],
                Some("current_native_hub"),
            )));
        }
        let color = self
            .color
            .as_ref()
            .ok_or_else(|| ActionError::runtime("town frame not normalized"))?;
        let area = match name {
            "area_arcane" => Some("arcane_sanctuary"),
            "area_canyon" => Some("canyon_of_the_magi"),
            _ => None,
        };
        if let Some(area) = area
            && self.titles.visible(color.view(), area)?
            && let Some(found) = self.titles.last_evidence()
        {
            let mut result = point(
                [
                    (found.title_at[0] + 34) as f64,
                    (found.title_at[1] + 7) as f64,
                ],
                None,
            );
            result.score = Some(found.score);
            return Ok(Some(result));
        }
        let definition = &template.definition;
        if let Some(pair) = &definition.blue_pair {
            return Self::find_blue_pair(color.view(), pair);
        }
        let gray = self
            .gray
            .as_ref()
            .ok_or_else(|| ActionError::runtime("town gray frame missing"))?;
        let search = rect(definition.search)?;
        let region = foreground(gray.view().crop(search)?, definition.foreground.as_deref())?;
        let region = gaussian(region.view(), false)?;
        let mut best: Option<TownPoint> = None;
        for variant in &template.variants {
            if region.size().width() < variant.image.size().width()
                || region.size().height() < variant.image.size().height()
            {
                continue;
            }
            let Some(found) =
                crate::actions::locate(region.view(), variant.image.view(), definition.threshold)?
            else {
                continue;
            };
            if best
                .as_ref()
                .is_some_and(|b| b.score.is_some_and(|score| score >= found.score))
            {
                continue;
            }
            let offset = definition
                .variant_click_offsets
                .get(&variant.filename)
                .copied()
                .or(definition.click_offset)
                .unwrap_or([found.width as f64 / 2.0, found.height as f64 / 2.0]);
            let p = [
                (search.left() + found.top_left[0]) as f64 + offset[0],
                (search.top() + found.top_left[1]) as f64 + offset[1],
            ];
            if let Some([rx, ry]) = definition.blue_nearby {
                let size = color.size();
                let x0 = (p[0] - rx).max(0.0) as usize;
                let x1 = ((p[0] + rx).max(0.0) as usize).min(size.width());
                let y0 = (p[1] - ry).max(0.0) as usize;
                let y1 = ((p[1] + ry).max(0.0) as usize).min(size.height());
                if x0 >= x1 || y0 >= y1 {
                    continue;
                }
                let hsv = color::hsv(color.view().crop(PixelRect::new(x0, y0, x1, y1)?)?)?;
                let blue = color::in_range(hsv.view(), [95, 55, 120], [135, 255, 255])?;
                if blue.pixels().iter().filter(|v| **v != 0).count() < 40 {
                    continue;
                }
            }
            let mut found_point = point(p, None);
            found_point.score = Some(found.score);
            found_point.template = Some(variant.filename.clone());
            found_point.width_scale = Some(variant.scale);
            best = Some(found_point);
        }
        Ok(best)
    }
    fn reset_camera(&mut self) {
        self.motion.reset();
        self.camera_sequence = 0;
        self.camera_stable_frames = 0;
        self.camera_stable_since = None;
        self.camera_reading = serde_json::Value::Null;
    }
    fn camera_stable(&mut self, now: f64) -> Result<bool, ActionError> {
        let native = self
            .native
            .as_ref()
            .ok_or_else(|| ActionError::runtime("current camera frame missing"))?;
        let size = native.size();
        let w = size.width();
        let h = size.height();
        let mut mask = GrayFrame::zeros(size)?;
        for y in (h as f64 * 0.12) as usize..(h as f64 * 0.76) as usize {
            mask.pixels_mut()
                [y * w + (w as f64 * 0.08) as usize..y * w + (w as f64 * 0.85) as usize]
                .fill(255);
        }
        for y in (h as f64 * 0.40) as usize..(h as f64 * 0.60) as usize {
            mask.pixels_mut()
                [y * w + (w as f64 * 0.43) as usize..y * w + (w as f64 * 0.57) as usize]
                .fill(0);
        }
        self.camera_sequence = self
            .camera_sequence
            .checked_add(1)
            .ok_or_else(|| ActionError::runtime("town camera sequence exhausted"))?;
        let reading = self.motion.observe_mask(
            native.view(),
            mask.view(),
            self.camera_sequence,
            now,
            None,
        )?;
        self.camera_reading = serde_json::json!({"displacement":reading.displacement,"inliers":reading.inliers,"reason":reading.reason});
        if reading
            .displacement
            .is_none_or(|[x, y]| (x * x + y * y).sqrt() >= 2.0)
        {
            self.camera_stable_frames = 0;
            self.camera_stable_since = None;
            if reading.displacement.is_none() {
                self.motion.reset();
                self.motion.observe_mask(
                    native.view(),
                    mask.view(),
                    self.camera_sequence,
                    now,
                    None,
                )?;
            }
            return Ok(false);
        }
        let since = *self.camera_stable_since.get_or_insert(now);
        self.camera_stable_frames += 1;
        Ok(self.camera_stable_frames >= 3 && now - since >= 0.12)
    }
    fn camera_reading(&self) -> serde_json::Value {
        self.camera_reading.clone()
    }
}
fn validate_shape(size: Size) -> Result<(), ActionError> {
    let ratio = REFERENCE_SIZE[0] as f64 / REFERENCE_SIZE[1] as f64;
    if size.width() < 800
        || size.height() < 450
        || (size.width() as f64 / size.height() as f64 / ratio - 1.0).abs() > 0.03
    {
        return Err(ActionError::runtime(format!(
            "Uncalibrated window shape: {}x{}; stop and recalibrate instead of guessing.",
            size.width(),
            size.height()
        )));
    }
    Ok(())
}
pub(super) fn rect([x0, y0, x1, y1]: [usize; 4]) -> Result<PixelRect, ActionError> {
    Ok(PixelRect::new(x0, y0, x1, y1)?)
}
pub(super) fn point([x, y]: [f64; 2], evidence: Option<&str>) -> TownPoint {
    TownPoint {
        x,
        y,
        evidence: evidence.map(str::to_owned),
        score: None,
        template: None,
        width_scale: None,
    }
}
fn foreground(source: GrayView<'_>, mode: Option<&str>) -> Result<GrayFrame, ActionError> {
    let mut result = source.to_owned()?;
    if mode == Some("high_pass") {
        let blurred = gaussian(source, true)?;
        for (value, blur) in result.pixels_mut().iter_mut().zip(blurred.pixels()) {
            *value = value.saturating_sub(*blur);
        }
    }
    Ok(result)
}
/// Whole uint8 matrices use Q8 Gaussian coefficients in the pinned OpenCV build.
/// These two fixed kernels reproduce the only town/title sigma+size pairs.
/// Coefficients follow smooth.dispatch.cpp's symmetric error diffusion; notices
/// and pinned source are recorded in THIRD-PARTY-NOTICES.md.
pub(super) fn gaussian(source: GrayView<'_>, high_pass: bool) -> Result<GrayFrame, ActionError> {
    let kernel: &[u32] = if high_pass {
        &[
            0, 1, 0, 1, 2, 3, 4, 6, 7, 10, 13, 15, 17, 19, 20, 20, 20, 19, 17, 15, 13, 10, 7, 6, 4,
            3, 2, 1, 0, 1, 0,
        ]
    } else {
        &[61, 134, 61]
    };
    Ok(crate::common::vision_ops::filters::separable_q8(
        source, kernel,
    )?)
}

// cv.resize(None, fx=scale, fy=1) retains fx after rounding the output width.
// Re-deriving it from that rounded width shifts narrow Korean label glyphs.
fn scale_width(source: GrayView<'_>, scale: f64) -> Result<GrayFrame, ActionError> {
    let input = source.size();
    let width = (input.width() as f64 * scale).round_ties_even() as usize;
    let size = Size::new(width, input.height())?;
    if size == input {
        return Ok(source.to_owned()?);
    }
    let mut result = GrayFrame::zeros(size)?;
    for y in 0..size.height() {
        let row = source
            .row(y)
            .ok_or_else(|| ActionError::runtime("missing scaled label row"))?;
        for x in 0..width {
            let coordinate = ((x as f64 + 0.5) / scale - 0.5) as f32;
            let base = coordinate.floor();
            let weight = if base < 0.0 || base >= (input.width() - 1) as f32 {
                0.0
            } else {
                coordinate - base
            };
            let a = ((1.0 - weight) * 2048.0).round_ties_even() as u32;
            let b = (weight * 2048.0).round_ties_even() as u32;
            let first = (base as i64).clamp(0, input.width() as i64 - 1) as usize;
            let next = (base as i64 + 1).clamp(0, input.width() as i64 - 1) as usize;
            result.pixels_mut()[y * width + x] =
                ((u32::from(row[first]) * a + u32::from(row[next]) * b + 1024) >> 11) as u8;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    #[test]
    fn whole_matrix_gaussian_and_explicit_width_scales_match_exact_pixels()
    -> Result<(), Box<dyn std::error::Error>> {
        for (width, height) in [(33, 29), (5, 7), (64, 17)] {
            let mut seed = 19u32;
            let pixels = (0..width * height)
                .map(|_| {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    (seed >> 24) as u8
                })
                .collect::<Vec<_>>();
            let image = GrayFrame::from_vec(Size::new(width, height)?, pixels.clone())?;
            let bgr = pixels
                .iter()
                .flat_map(|n| [*n; 3])
                .map(|n| format!("{n:02x}"))
                .collect::<String>();
            let scales = [0.96, 0.98, 1.0, 1.02, 1.04];
            let expected = crate::test_support::legacy_expectation(
                "action-oracle",
                &serde_json::json!({"kind":"town_pixels","frame":{"width":width,"height":height,"bgr":bgr},"scales":scales}),
            )?;
            let hash = |image: &GrayFrame| format!("{:x}", Sha256::digest(image.pixels()));
            assert_eq!(
                serde_json::json!([
                    hash(&gaussian(image.view(), false)?),
                    hash(&gaussian(image.view(), true)?)
                ]),
                expected["gaussian"]
            );
            let actual = scales
                .into_iter()
                .map(|scale| Ok(hash(&scale_width(image.view(), scale)?)))
                .collect::<Result<Vec<_>, ActionError>>()?;
            assert_eq!(serde_json::json!(actual), expected["width_scales"]);
        }
        Ok(())
    }
}

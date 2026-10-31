use crate::common::{
    error::ObservationError,
    frame::{BgrFrame, BgrView, FloatMap, GrayFrame, PixelRect, Size},
    vision_ops::{
        components::{Connectivity, connected_components},
        filters::separable_q8,
        morphology::{Kernel, Morphology, morphology},
        template::{self, MatchMethod},
    },
};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroundReading {
    pub point: Option<[i32; 2]>,
    pub reason: String,
    pub score: Option<f64>,
    pub calibration: Option<serde_json::Value>,
}
impl GroundReading {
    pub fn unknown(reason: impl Into<String>) -> Self {
        Self {
            point: None,
            reason: reason.into(),
            score: None,
            calibration: None,
        }
    }
    fn found(point: [i32; 2], reason: &str, score: Option<f64>) -> Self {
        Self {
            point: Some(point),
            reason: reason.into(),
            score,
            calibration: None,
        }
    }
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GroundTracking {
    pub maximum_seconds: f64,
    pub maximum_gap_seconds: f64,
    pub maximum_displacement: f64,
}
pub struct GroundOptions {
    pub search_box: PixelRect,
    pub anchor_offset: [usize; 2],
    pub minimum_score: f64,
    pub variants: Vec<BgrFrame>,
    pub body_templates: Vec<BgrFrame>,
    pub body_box: Option<[i32; 4]>,
    pub body_minimum_score: f64,
    pub tracking: Option<GroundTracking>,
}
impl GroundOptions {
    pub fn new(search_box: PixelRect, anchor_offset: [usize; 2]) -> Self {
        Self {
            search_box,
            anchor_offset,
            minimum_score: 0.88,
            variants: Vec::new(),
            body_templates: Vec::new(),
            body_box: None,
            body_minimum_score: 0.82,
            tracking: None,
        }
    }
}
struct Track {
    point: [i32; 2],
    at: f64,
    strong_at: f64,
}
pub struct FlashGroundObserver {
    markers: Vec<GrayFrame>,
    options: GroundOptions,
    track: Option<Track>,
}
type Candidate = ([i32; 2], f64, &'static str);
const BODY_BLUR_Q8: &[u32] = &[1, 3, 6, 12, 20, 30, 36, 40, 36, 30, 20, 12, 6, 3, 1];
fn cyan_mask(frame: BgrView<'_>) -> Result<GrayFrame, ObservationError> {
    let mut mask = GrayFrame::zeros(frame.size())?;
    for y in 0..frame.size().height() {
        let row = frame
            .row(y)
            .ok_or_else(|| ObservationError::invalid("missing ground row"))?;
        for (x, p) in row.as_chunks::<3>().0.iter().enumerate() {
            let [b, g, r] = p.map(i16::from);
            mask.pixels_mut()[y * frame.size().width() + x] =
                u8::from(b > 120 && g > 90 && b - r > 45 && g - r > 25);
        }
    }
    Ok(mask)
}
fn gold_mask(frame: BgrView<'_>) -> Result<GrayFrame, ObservationError> {
    let mut mask = GrayFrame::zeros(frame.size())?;
    for y in 0..frame.size().height() {
        let row = frame
            .row(y)
            .ok_or_else(|| ObservationError::invalid("missing shield row"))?;
        for (x, p) in row.as_chunks::<3>().0.iter().enumerate() {
            let [b, g, r] = p.map(f64::from);
            mask.pixels_mut()[y * frame.size().width() + x] =
                u8::from(r > 65.0 && g > 55.0 && r > g * 1.04 && g > b * 1.2 && r - b > 25.0);
        }
    }
    Ok(mask)
}
fn suppress(scores: &mut FloatMap, [x, y]: [usize; 2], rx: usize, ry: usize) {
    let w = scores.size().width();
    for row in y.saturating_sub(ry)
        ..y.saturating_add(ry)
            .saturating_add(1)
            .min(scores.size().height())
    {
        scores.pixels_mut()[row * w + x.saturating_sub(rx)
            ..row * w + x.saturating_add(rx).saturating_add(1).min(w)]
            .fill(0.0);
    }
}
fn positioned([x, y]: [i32; 2], [l, t, r, b]: [i32; 4], size: Size) -> Option<PixelRect> {
    let [l, t, r, b] = [
        i64::from(x) + i64::from(l),
        i64::from(y) + i64::from(t),
        i64::from(x) + i64::from(r),
        i64::from(y) + i64::from(b),
    ];
    if l < 0 || t < 0 || r > size.width() as i64 || b > size.height() as i64 {
        return None;
    }
    PixelRect::new(l as usize, t as usize, r as usize, b as usize).ok()
}
fn distance(a: [i32; 2], b: [i32; 2]) -> f64 {
    (f64::from(a[0]) - f64::from(b[0])).hypot(f64::from(a[1]) - f64::from(b[1]))
}
impl FlashGroundObserver {
    pub fn new(marker: BgrFrame, mut options: GroundOptions) -> Result<Self, ObservationError> {
        let mut markers = Vec::new();
        let variants = std::mem::take(&mut options.variants);
        for image in std::iter::once(&marker).chain(&variants) {
            if image.size() != marker.size() {
                return Err(ObservationError::invalid(
                    "reviewed ground variants must match the marker dimensions",
                ));
            }
            let mask = cyan_mask(image.view())?;
            if mask.pixels().iter().filter(|v| **v != 0).count() < 20 {
                return Err(ObservationError::invalid(
                    "ground marker must contain positive cyan shape evidence",
                ));
            }
            markers.push(mask);
        }
        options.search_box.checked_in(Size::new(1922, 900)?)?;
        if options.search_box.width() < marker.size().width()
            || options.search_box.height() < marker.size().height()
        {
            return Err(ObservationError::invalid(
                "native world search rectangle required",
            ));
        }
        if options.anchor_offset[0] >= marker.size().width()
            || options.anchor_offset[1] >= marker.size().height()
        {
            return Err(ObservationError::invalid(
                "anchor must lie inside marker crop",
            ));
        }
        if !options.minimum_score.is_finite()
            || options.minimum_score <= 0.0
            || options.minimum_score > 1.0
        {
            return Err(ObservationError::invalid(
                "positive finite marker score required",
            ));
        }
        if let Some(t) = options.tracking
            && [
                t.maximum_seconds,
                t.maximum_gap_seconds,
                t.maximum_displacement,
            ]
            .into_iter()
            .any(|v| !v.is_finite() || v <= 0.0)
        {
            return Err(ObservationError::invalid(
                "Explicit bounded ground identity tracking required",
            ));
        }
        if !options.body_templates.is_empty() {
            let [l, t, r, b] = options.body_box.ok_or_else(|| {
                ObservationError::invalid("Reviewed marker-relative body box required")
            })?;
            if l >= r
                || t >= b
                || b >= 0
                || !options.body_minimum_score.is_finite()
                || options.body_minimum_score <= 0.0
                || options.body_minimum_score > 1.0
            {
                return Err(ObservationError::invalid(
                    "Reviewed body geometry/score required",
                ));
            }
            let width = i64::from(r) - i64::from(l);
            let height = i64::from(b) - i64::from(t);
            if options
                .body_templates
                .iter()
                .any(|i| i.size().width() as i64 != width || i.size().height() as i64 != height)
            {
                return Err(ObservationError::invalid(
                    "Native reviewed body crops required",
                ));
            }
        }
        Ok(Self {
            markers,
            options,
            track: None,
        })
    }
    pub fn reset(&mut self) {
        self.track = None;
    }
    fn body_at(&self, frame: BgrView<'_>, point: [i32; 2]) -> Result<bool, ObservationError> {
        let Some(body_box) = self.options.body_box else {
            return Ok(false);
        };
        let Some(region) = positioned(point, body_box, frame.size()) else {
            return Ok(false);
        };
        let patch = frame.crop(region)?;
        for body in &self.options.body_templates {
            let scores =
                template::match_template(patch, body.view(), MatchMethod::NormalizedCoefficient)?;
            if f64::from(scores.pixels()[0]) >= self.options.body_minimum_score {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn energy_shield_above(frame: BgrView<'_>, [x, y]: [i32; 2]) -> Result<bool, ObservationError> {
        let [l, t, r, b] = [
            (x - 25).max(0) as usize,
            (y - 180).max(0) as usize,
            (x + 26).min(frame.size().width() as i32) as usize,
            (y - 125).max(0) as usize,
        ];
        if l >= r || t >= b {
            return Ok(false);
        }
        let patch = frame.crop(PixelRect::new(l, t, r, b)?)?;
        let gold = gold_mask(patch)?;
        let opened = morphology(
            gold.view(),
            Kernel::Rectangle {
                width: 3,
                height: 3,
            },
            Morphology::Open,
        )?;
        for mask in [&gold, &opened] {
            let components = connected_components(mask.view(), Connectivity::Eight)?;
            for c in components.stats.iter().skip(1) {
                let (Some(r), Some(center)) = (c.rect, c.centroid) else {
                    continue;
                };
                if (15..=40).contains(&r.width())
                    && (15..=40).contains(&r.height())
                    && (0.7..=1.4).contains(&(r.width() as f64 / r.height() as f64))
                    && (120..=1000).contains(&c.area)
                    && c.area as f64 / (r.width() * r.height()) as f64 >= 0.3
                    && r.left() >= 2
                    && r.top() >= 2
                    && r.right() <= patch.size().width().saturating_sub(2)
                    && r.bottom() <= patch.size().height().saturating_sub(2)
                    && (center[0] - 25.0).abs() <= 8.0
                    && (25.0..=42.0).contains(&center[1])
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    fn body_ground_candidates(
        &self,
        frame: BgrView<'_>,
    ) -> Result<Vec<Candidate>, ObservationError> {
        if self.options.body_templates.is_empty() {
            return Ok(Vec::new());
        }
        let [left, top, right, bottom] = self
            .options
            .body_box
            .ok_or_else(|| ObservationError::invalid("body box missing"))?;
        let search = self.options.search_box;
        let l = search.left() as i32;
        let t = search.top() as i32;
        let r = search.right() as i32;
        let b = search.bottom() as i32;
        let shield_top = (t - 180).max(0);
        if l < 25 || b - 125 <= shield_top {
            return Err(ObservationError::invalid(
                "body shield search rectangle is empty",
            ));
        }
        let shield_patch = frame.crop(PixelRect::new(
            (l - 25) as usize,
            shield_top as usize,
            (r + 25).min(frame.size().width() as i32) as usize,
            (b - 125) as usize,
        )?)?;
        let gold = gold_mask(shield_patch)?;
        let opened = morphology(
            gold.view(),
            Kernel::Rectangle {
                width: 3,
                height: 3,
            },
            Morphology::Open,
        )?;
        let components = connected_components(opened.view(), Connectivity::Eight)?;
        let mut patches = Vec::new();
        let mut shield_points = Vec::new();
        for component in components.stats.iter().skip(1) {
            let (Some(region), Some(center)) = (component.rect, component.centroid) else {
                continue;
            };
            if !(15..=40).contains(&region.width())
                || !(15..=40).contains(&region.height())
                || !(0.7..=1.4).contains(&(region.width() as f64 / region.height() as f64))
                || !(120..=1000).contains(&component.area)
            {
                continue;
            }
            let point = [
                (f64::from(l - 25) + center[0]).round_ties_even() as i32,
                (f64::from(shield_top) + center[1] + 150.0).round_ties_even() as i32,
            ];
            if !(l..r).contains(&point[0]) || !(t..b).contains(&point[1]) {
                continue;
            }
            shield_points.push(point);
            let x0 = (i64::from(point[0]) + i64::from(left) - 12).max(0) as usize;
            let y0 = (i64::from(point[1]) + i64::from(top) - 12).max(0) as usize;
            let x1 = (i64::from(point[0]) + i64::from(right) + 12).min(frame.size().width() as i64)
                as usize;
            let y1 = (i64::from(point[1]) + i64::from(bottom) + 12)
                .min(frame.size().height() as i64) as usize;
            if x0 < x1 && y0 < y1 {
                patches.push(PixelRect::new(x0, y0, x1, y1)?);
            }
        }
        if patches.len() > 8 {
            return Ok(Vec::new());
        }
        let mut found = Vec::new();
        for blur in [false, true] {
            for region in &patches {
                let patch = frame.crop(*region)?;
                let current = if blur {
                    Some(separable_q8(patch, BODY_BLUR_Q8)?)
                } else {
                    None
                };
                let current = current.as_ref().map_or(patch, |p| p.view());
                for body in &self.options.body_templates {
                    if patch.size().width() < body.size().width()
                        || patch.size().height() < body.size().height()
                    {
                        continue;
                    }
                    let blurred = if blur {
                        Some(separable_q8(body.view(), BODY_BLUR_Q8)?)
                    } else {
                        None
                    };
                    let reference = blurred.as_ref().map_or(body.view(), |p| p.view());
                    let mut scores = template::match_template(
                        current,
                        reference,
                        MatchMethod::NormalizedCoefficient,
                    )?;
                    for _ in 0..8 {
                        let Some(peak) = template::maximum(&scores) else {
                            break;
                        };
                        if f64::from(peak.score) < self.options.body_minimum_score {
                            break;
                        }
                        let point = [
                            region.left() as i32 + peak.location[0] as i32 - left,
                            region.top() as i32 + peak.location[1] as i32 - top,
                        ];
                        if (l..r).contains(&point[0])
                            && (t..b).contains(&point[1])
                            && Self::energy_shield_above(frame, point)?
                        {
                            found.push((
                                point,
                                f64::from(peak.score),
                                if blur {
                                    "flash_body_shape_and_es"
                                } else {
                                    "flash_body_and_es"
                                },
                            ));
                        }
                        suppress(&mut scores, peak.location, 12, 12);
                    }
                }
            }
            if !found.is_empty() {
                break;
            }
        }
        if found.is_empty() {
            for point in shield_points {
                if !Self::energy_shield_above(frame, point)? {
                    continue;
                }
                let Some(body_region) = positioned(point, [left, top, right, bottom], frame.size())
                else {
                    continue;
                };
                let Some(lregion) = positioned(
                    point,
                    [left.saturating_sub(24), top, left, bottom],
                    frame.size(),
                ) else {
                    continue;
                };
                let Some(rregion) = positioned(
                    point,
                    [right, top, right.saturating_add(24), bottom],
                    frame.size(),
                ) else {
                    continue;
                };
                if lregion.width() != 24 || rregion.width() != 24 {
                    continue;
                }
                let body = frame.crop(body_region)?;
                let lside = frame.crop(lregion)?;
                let rside = frame.crop(rregion)?;
                let (mut pixels, mut rows) = (0_usize, 0_usize);
                for y in 0..body.size().height() {
                    let lrow = lside
                        .row(y)
                        .ok_or_else(|| ObservationError::invalid("missing body side row"))?;
                    let rrow = rside
                        .row(y)
                        .ok_or_else(|| ObservationError::invalid("missing body side row"))?;
                    let mut background = [0.0_f64; 3];
                    for c in 0..3 {
                        let mut values = [0_u8; 48];
                        for x in 0..24 {
                            values[x] = lrow[x * 3 + c];
                            values[x + 24] = rrow[x * 3 + c];
                        }
                        values.sort_unstable();
                        background[c] = (f64::from(values[23]) + f64::from(values[24])) / 2.0;
                    }
                    let mut count = 0;
                    for p in body
                        .row(y)
                        .ok_or_else(|| ObservationError::invalid("missing body row"))?
                        .as_chunks::<3>()
                        .0
                    {
                        let min = p[0].min(p[1]).min(p[2]);
                        let max = p[0].max(p[1]).max(p[2]);
                        let contrast = (0..3)
                            .map(|c| (f64::from(p[c]) - background[c]).powi(2))
                            .sum::<f64>()
                            .sqrt();
                        count += usize::from(max - min > 12 && max < 170 && contrast > 30.0);
                    }
                    pixels += count;
                    rows += usize::from(count >= 4);
                }
                if pixels >= 120 && rows >= 32 {
                    found.push((point, 0.0, "flash_es_and_body_shape"));
                }
            }
        }
        if found.is_empty() {
            return Ok(found);
        }
        let mut best = found[0];
        for candidate in found.iter().skip(1) {
            if candidate.1 > best.1 {
                best = *candidate;
            }
        }
        if found
            .iter()
            .all(|candidate| distance(candidate.0, best.0) <= 12.0)
        {
            Ok(vec![best])
        } else {
            Ok(found)
        }
    }
    pub fn observe(
        &mut self,
        frame: BgrView<'_>,
        captured_at: Option<f64>,
    ) -> Result<GroundReading, ObservationError> {
        if frame.size() != Size::new(1922, 1140)? {
            self.track = None;
            return Ok(GroundReading::unknown("layout_unverified"));
        }
        if captured_at.is_some_and(|t| !t.is_finite() || t < 0.0) {
            return Err(ObservationError::invalid(
                "Finite ground capture time required",
            ));
        }
        if self
            .track
            .as_ref()
            .zip(captured_at)
            .is_some_and(|(track, at)| at < track.at)
        {
            self.track = None;
            return Ok(GroundReading::unknown("ground_time_reversed"));
        }
        let mask = cyan_mask(frame.crop(self.options.search_box)?)?;
        let mut scores = template::match_template(
            mask.view(),
            self.markers[0].view(),
            MatchMethod::NormalizedCorrelation,
        )?;
        for marker in self.markers.iter().skip(1) {
            let current = template::match_template(
                mask.view(),
                marker.view(),
                MatchMethod::NormalizedCorrelation,
            )?;
            for (a, b) in scores.pixels_mut().iter_mut().zip(current.pixels()) {
                *a = if a.is_nan() || b.is_nan() {
                    f32::NAN
                } else {
                    a.max(*b)
                };
            }
        }
        for value in scores.pixels_mut() {
            if !value.is_finite() {
                *value = 0.0;
            }
        }
        let mut candidates = Vec::new();
        let mut markers = Vec::new();
        for _ in 0..32 {
            let Some(peak) = template::maximum(&scores) else {
                break;
            };
            if f64::from(peak.score) < self.options.minimum_score {
                break;
            }
            let point = [
                (self.options.search_box.left() + peak.location[0] + self.options.anchor_offset[0])
                    as i32,
                (self.options.search_box.top() + peak.location[1] + self.options.anchor_offset[1])
                    as i32,
            ];
            markers.push((point, f64::from(peak.score)));
            let shield = Self::energy_shield_above(frame, point)?;
            if shield || self.body_at(frame, point)? {
                candidates.push((
                    point,
                    f64::from(peak.score),
                    if shield {
                        "flash_marker_and_es"
                    } else {
                        "flash_marker_and_body"
                    },
                ));
            }
            suppress(
                &mut scores,
                peak.location,
                self.markers[0].size().width() / 2,
                self.markers[0].size().height() / 2,
            );
        }
        if template::maximum(&scores)
            .is_some_and(|peak| f64::from(peak.score) >= self.options.minimum_score)
        {
            self.track = None;
            return Ok(GroundReading::unknown("ground_search_overflow"));
        }
        if candidates.is_empty() {
            candidates = self.body_ground_candidates(frame)?;
        }
        if candidates.len() == 1 {
            let (point, score, reason) = candidates[0];
            self.track = captured_at.map(|at| Track {
                point,
                at,
                strong_at: at,
            });
            return Ok(GroundReading::found(
                point,
                reason,
                (reason != "flash_es_and_body_shape").then_some(score),
            ));
        }
        if candidates.is_empty()
            && let (Some(limits), Some(previous), Some(at)) =
                (self.options.tracking, self.track.as_ref(), captured_at)
        {
            let current: Vec<_> = markers
                .into_iter()
                .filter(|(point, _)| {
                    distance(*point, previous.point) <= limits.maximum_displacement
                })
                .collect();
            if (0.0..=limits.maximum_gap_seconds).contains(&(at - previous.at))
                && (0.0..=limits.maximum_seconds).contains(&(at - previous.strong_at))
                && current.len() == 1
            {
                let (point, score) = current[0];
                self.track = Some(Track {
                    point,
                    at,
                    strong_at: previous.strong_at,
                });
                return Ok(GroundReading::found(
                    point,
                    "flash_marker_tracked",
                    Some(score),
                ));
            }
        }
        self.track = None;
        Ok(GroundReading::unknown(if candidates.is_empty() {
            "ground_unverified"
        } else {
            "ground_ambiguous"
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    #[test]
    fn reviewed_body_blur_matches_pinned_whole_matrix_pixels()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut random = 71_u32;
        for (width, height) in [(48, 80), (5, 7), (33, 29)] {
            let frame = BgrFrame::from_vec(
                Size::new(width, height)?,
                (0..width * height * 3)
                    .map(|_| {
                        random = random.wrapping_mul(1664525).wrapping_add(1013904223);
                        (random >> 24) as u8
                    })
                    .collect(),
            )?;
            let expected = crate::test_support::legacy_expectation(
                "observer-oracle",
                &serde_json::json!({"kind":"ground_blur","frame":{"width":width,"height":height,"pixels":frame.pixels().iter().map(|v|format!("{v:02x}")).collect::<String>()}}),
            )?;
            let blurred = separable_q8(frame.view(), BODY_BLUR_Q8)?;
            assert_eq!(
                format!("{:x}", Sha256::digest(blurred.pixels())),
                expected.as_str().ok_or("hash")?
            );
        }
        Ok(())
    }
}

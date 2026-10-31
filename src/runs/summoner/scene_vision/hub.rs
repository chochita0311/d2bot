//! 현재 청색 불꽃 쌍과 서로 다른 바닥 문양으로 확인하는 중앙 웨이포인트.
use crate::common::{
    error::ObservationError,
    frame::{BgrFrame, GrayFrame, GrayView, PixelRect, Size},
    observation_assets::normalize_absolute,
    vision_ops::{
        codec, color,
        components::{Connectivity, connected_components},
        morphology::{Kernel, Morphology, morphology},
        resize,
        template::{self, MatchMethod},
    },
};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::{Arc, Weak},
};
#[derive(Deserialize)]
struct Manifest {
    schema: u32,
    frame_size: [usize; 2],
    minimum_parts: usize,
    location_tolerance: i32,
    parts: Vec<Rule>,
    blue_pair: BluePair,
    #[serde(default)]
    floor_seed_ids: Option<Vec<String>>,
    #[serde(default)]
    floor_seed_id: Option<String>,
}
#[derive(Deserialize)]
struct Rule {
    id: String,
    path: String,
    offset: [i32; 2],
    threshold: f64,
}
#[derive(Deserialize)]
struct BluePair {
    hsv_lower: [u8; 3],
    hsv_upper: [u8; 3],
    minimum_area: usize,
    width: [usize; 2],
    height: [usize; 2],
    separation: [f64; 2],
    vertical_difference: f64,
    ground_offset: [f64; 2],
    local_radius: i32,
}
struct Part {
    rule: Rule,
    image: GrayFrame,
    coarse: GrayFrame,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HubPartEvidence {
    pub id: String,
    pub score: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind")]
pub enum HubEvidence {
    #[serde(rename = "blue_pair_and_floor_ornaments")]
    Current {
        point: [i32; 2],
        parts: Vec<HubPartEvidence>,
        blue_evidence: &'static str,
    },
    #[serde(rename = "recent_floor_and_current_blue_pair")]
    Recent {
        point: [i32; 2],
        reference_observed_at: f64,
        reference_parts: Vec<HubPartEvidence>,
        current_floor_parts: Vec<HubPartEvidence>,
    },
}
struct FloorMatch {
    point: [i32; 2],
    parts: Vec<([i32; 2], HubPartEvidence)>,
}
pub struct ArcaneHubObserver {
    minimum_parts: usize,
    tolerance: i32,
    parts: Vec<Part>,
    blue: BluePair,
    seeds: Vec<String>,
    last_evidence: Option<HubEvidence>,
    last_frame: Weak<BgrFrame>,
    last_pair_points: Vec<[f64; 2]>,
}
fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
fn rounded(point: [f64; 2]) -> [i32; 2] {
    point.map(|v| v.round_ties_even() as i32)
}
fn slice_end(value: i64, length: usize) -> usize {
    let value = if value < 0 {
        value + length as i64
    } else {
        value
    };
    value.clamp(0, length as i64) as usize
}
fn median(points: &[[i32; 2]]) -> [i32; 2] {
    let axis = |axis| {
        let mut values: Vec<_> = points.iter().map(|p| p[axis]).collect();
        values.sort_unstable();
        let middle = values.len() / 2;
        if values.len().is_multiple_of(2) {
            ((f64::from(values[middle - 1]) + f64::from(values[middle])) / 2.0).round_ties_even()
                as i32
        } else {
            values[middle]
        }
    };
    [axis(0), axis(1)]
}
impl ArcaneHubObserver {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ObservationError> {
        let path = path.as_ref().canonicalize()?;
        let owner = path
            .parent()
            .ok_or_else(|| ObservationError::invalid("waypoint manifest owner missing"))?;
        let rules: Manifest = serde_json::from_slice(&std::fs::read(&path)?)?;
        if rules.schema != 1 || rules.frame_size != [1922, 1140] || rules.minimum_parts < 2 {
            return Err(ObservationError::invalid(
                "reviewed native multi-part waypoint required",
            ));
        }
        if rules.location_tolerance < 0
            || rules.blue_pair.local_radius < 0
            || rules
                .blue_pair
                .separation
                .into_iter()
                .chain([rules.blue_pair.vertical_difference])
                .chain(rules.blue_pair.ground_offset)
                .any(|v| !v.is_finite())
        {
            return Err(ObservationError::invalid(
                "finite reviewed waypoint geometry required",
            ));
        }
        let mut parts = Vec::new();
        for rule in rules.parts {
            let declared = normalize_absolute(&owner.join(&rule.path));
            let allowed = |p: &Path| {
                p.parent() == Some(owner)
                    || p.parent() == Some(owner.join("templates/hub").as_path())
            };
            if !allowed(&declared)
                || !rule.threshold.is_finite()
                || !(0.8..=1.0).contains(&rule.threshold)
            {
                return Err(ObservationError::invalid(
                    "waypoint part outside reviewed owner or score",
                ));
            }
            let path = declared.canonicalize()?;
            if !allowed(&path) {
                return Err(ObservationError::invalid(
                    "waypoint part outside reviewed owner or score",
                ));
            }
            let image = codec::load_gray(path)?;
            let mean = image.pixels().iter().map(|v| f64::from(*v)).sum::<f64>()
                / image.pixels().len() as f64;
            let std = (image
                .pixels()
                .iter()
                .map(|v| (f64::from(*v) - mean).powi(2))
                .sum::<f64>()
                / image.pixels().len() as f64)
                .sqrt();
            if image.size().width() < 20 || image.size().height() < 20 || std < 5.0 {
                return Err(ObservationError::invalid(
                    "textured waypoint ornament required",
                ));
            }
            let coarse = resize::half(image.view())?;
            parts.push(Part {
                rule,
                image,
                coarse,
            });
        }
        let seeds = rules
            .floor_seed_ids
            .unwrap_or_else(|| rules.floor_seed_id.into_iter().collect());
        Ok(Self {
            minimum_parts: rules.minimum_parts,
            tolerance: rules.location_tolerance,
            parts,
            blue: rules.blue_pair,
            seeds,
            last_evidence: None,
            last_frame: Weak::new(),
            last_pair_points: Vec::new(),
        })
    }
    pub fn last_evidence(&self) -> Option<&HubEvidence> {
        self.last_evidence.as_ref()
    }
    pub fn pair_points(&self) -> &[[f64; 2]] {
        &self.last_pair_points
    }
    fn match_floor(
        &self,
        gray: GrayView<'_>,
        candidate: [f64; 2],
        minimum_parts: usize,
    ) -> Result<Option<FloorMatch>, ObservationError> {
        let mut matches: Vec<([i32; 2], HubPartEvidence)> = Vec::new();
        for part in &self.parts {
            let [x, y] = rounded([
                candidate[0] + f64::from(part.rule.offset[0]),
                candidate[1] + f64::from(part.rule.offset[1]),
            ]);
            let width = part.image.size().width() as i64;
            let height = part.image.size().height() as i64;
            let x0 = (i64::from(x) - i64::from(self.tolerance)).max(24);
            let y0 = (i64::from(y) - i64::from(self.tolerance)).max(55);
            let x1 = (i64::from(x) + width + i64::from(self.tolerance) + 1).min(1900);
            let y1 = (i64::from(y) + height + i64::from(self.tolerance) + 1).min(900);
            if x1 - x0 < width || y1 - y0 < height {
                continue;
            }
            let patch = gray.crop(PixelRect::new(
                x0 as usize,
                y0 as usize,
                x1 as usize,
                y1 as usize,
            )?)?;
            let scores = template::match_template(
                patch,
                part.image.view(),
                MatchMethod::NormalizedCoefficient,
            )?;
            if let Some(peak) = template::maximum(&scores)
                && f64::from(peak.score) >= part.rule.threshold
            {
                let point = [
                    (x0 + peak.location[0] as i64 - i64::from(part.rule.offset[0])) as i32,
                    (y0 + peak.location[1] as i64 - i64::from(part.rule.offset[1])) as i32,
                ];
                let evidence = HubPartEvidence {
                    id: part.rule.id.clone(),
                    score: f64::from(peak.score),
                };
                if let Some(previous) = matches.iter_mut().find(|(_, e)| e.id == part.rule.id) {
                    if evidence.score > previous.1.score {
                        *previous = (point, evidence);
                    }
                } else {
                    matches.push((point, evidence));
                }
            }
        }
        if matches.len() < minimum_parts {
            return Ok(None);
        }
        let center = median(&matches.iter().map(|(p, _)| *p).collect::<Vec<_>>());
        if matches.iter().all(|(p, _)| {
            distance(p.map(f64::from), center.map(f64::from)) <= f64::from(self.tolerance)
        }) {
            Ok(Some(FloorMatch {
                point: center,
                parts: matches,
            }))
        } else {
            Ok(None)
        }
    }
    fn floor_anchored(
        &mut self,
        gray: GrayView<'_>,
        blue: &GrayFrame,
    ) -> Result<Vec<FloorMatch>, ObservationError> {
        let small = resize::half(gray.crop(PixelRect::new(24, 55, 1900, 900)?)?)?;
        let mut found: Vec<FloorMatch> = Vec::new();
        for part in &self.parts {
            if !self.seeds.contains(&part.rule.id) {
                continue;
            }
            let mut scores = template::match_template(
                small.view(),
                part.coarse.view(),
                MatchMethod::NormalizedCoefficient,
            )?;
            for _ in 0..4 {
                let Some(peak) = template::maximum(&scores) else {
                    break;
                };
                if f64::from(peak.score) < part.rule.threshold {
                    break;
                }
                let [x, y] = peak.location;
                let w = scores.size().width();
                for row in y.saturating_sub(part.image.size().height() / 2)
                    ..(y + part.image.size().height() / 2 + 1).min(scores.size().height())
                {
                    scores.pixels_mut()[row * w + x.saturating_sub(part.image.size().width() / 2)
                        ..row * w + (x + part.image.size().width() / 2 + 1).min(w)]
                        .fill(-1.0);
                }
                let candidate = [
                    24.0 + x as f64 * 2.0 - f64::from(part.rule.offset[0]),
                    55.0 + y as f64 * 2.0 - f64::from(part.rule.offset[1]),
                ];
                let Some(matched) = self.match_floor(gray, candidate, 1)? else {
                    continue;
                };
                if !matched.parts.iter().any(|(_, p)| p.id == part.rule.id)
                    || found.iter().any(|f| {
                        distance(f.point.map(f64::from), matched.point.map(f64::from))
                            <= f64::from(self.tolerance)
                    })
                {
                    continue;
                }
                let separation = (self.blue.separation[0] + self.blue.separation[1]) / 2.0;
                let mut counts = [0_usize; 2];
                for (index, dx) in [-separation / 2.0, separation / 2.0]
                    .into_iter()
                    .enumerate()
                {
                    let [bx, by] = rounded([
                        f64::from(matched.point[0]) + dx - 24.0,
                        f64::from(matched.point[1]) - self.blue.ground_offset[1] - 55.0,
                    ]);
                    let radius = i64::from(self.blue.local_radius);
                    let x0 = (i64::from(bx) - radius)
                        .max(0)
                        .min(blue.size().width() as i64) as usize;
                    let y0 = (i64::from(by) - radius)
                        .max(0)
                        .min(blue.size().height() as i64) as usize;
                    let x1 = slice_end(i64::from(bx) + radius + 1, blue.size().width());
                    let y1 = slice_end(i64::from(by) + radius + 1, blue.size().height());
                    if x0 < x1 && y0 < y1 {
                        for y in y0..y1 {
                            counts[index] += blue.pixels()
                                [y * blue.size().width() + x0..y * blue.size().width() + x1]
                                .iter()
                                .filter(|v| **v != 0)
                                .count();
                        }
                    }
                }
                if counts
                    .into_iter()
                    .all(|count| count >= self.blue.minimum_area)
                {
                    let center = matched.point.map(f64::from);
                    if !self
                        .last_pair_points
                        .iter()
                        .any(|p| distance(*p, center) <= f64::from(self.tolerance))
                    {
                        self.last_pair_points.push(center);
                    }
                    if matched.parts.len() >= self.minimum_parts {
                        found.push(matched);
                    }
                }
            }
        }
        Ok(found)
    }
    pub fn observe(&mut self, frame: &Arc<BgrFrame>) -> Result<Option<[i32; 2]>, ObservationError> {
        if frame.size() != Size::new(1922, 1140)? {
            return Err(ObservationError::invalid(
                "native reviewed waypoint frame required",
            ));
        }
        self.last_evidence = None;
        self.last_frame = Arc::downgrade(frame);
        self.last_pair_points.clear();
        let viewport = frame.view().crop(PixelRect::new(24, 55, 1900, 900)?)?;
        let hsv = color::hsv(viewport)?;
        let mask = color::in_range(hsv.view(), self.blue.hsv_lower, self.blue.hsv_upper)?;
        let mask = morphology(
            mask.view(),
            Kernel::Rectangle {
                width: 3,
                height: 3,
            },
            Morphology::Close,
        )?;
        let components = connected_components(mask.view(), Connectivity::Eight)?;
        let flames: Vec<_> = components
            .stats
            .iter()
            .skip(1)
            .filter_map(|c| c.rect.zip(c.centroid).map(|(r, p)| (c.area, r, p)))
            .filter(|(area, r, _)| {
                *area >= self.blue.minimum_area
                    && (self.blue.width[0]..=self.blue.width[1]).contains(&r.width())
                    && (self.blue.height[0]..=self.blue.height[1]).contains(&r.height())
            })
            .map(|(_, _, p)| [p[0] + 24.0, p[1] + 55.0])
            .collect();
        let gray = color::gray(frame.view())?;
        let mut found = Vec::new();
        if flames.len() <= 24 {
            for (index, left) in flames.iter().enumerate() {
                for right in &flames[index + 1..] {
                    if !(self.blue.separation[0]..=self.blue.separation[1])
                        .contains(&(left[0] - right[0]).abs())
                        || (left[1] - right[1]).abs() > self.blue.vertical_difference
                    {
                        continue;
                    }
                    let candidate = [
                        (left[0] + right[0]) / 2.0 + self.blue.ground_offset[0],
                        (left[1] + right[1]) / 2.0 + self.blue.ground_offset[1],
                    ];
                    self.last_pair_points.push(candidate);
                    if let Some(matched) =
                        self.match_floor(gray.view(), candidate, self.minimum_parts)?
                    {
                        found.push(matched);
                    }
                }
            }
        }
        let blue_evidence = if found.is_empty() {
            found = self.floor_anchored(gray.view(), &mask)?;
            "floor_anchored_current_pixels"
        } else {
            "current_components"
        };
        if found.len() != 1 {
            return Ok(None);
        }
        let matched = found.remove(0);
        let point = matched.point;
        self.last_evidence = Some(HubEvidence::Current {
            point,
            parts: matched.parts.into_iter().map(|(_, part)| part).collect(),
            blue_evidence,
        });
        Ok(Some(point))
    }
    fn pair_near(
        &self,
        frame: &Arc<BgrFrame>,
        point: [f64; 2],
        radius: f64,
    ) -> Result<Option<[i32; 2]>, ObservationError> {
        if !self.last_frame.ptr_eq(&Arc::downgrade(frame)) {
            return Err(ObservationError::invalid(
                "same current hub observation required",
            ));
        }
        let candidates: Vec<_> = self
            .last_pair_points
            .iter()
            .filter(|p| distance(**p, point) <= radius)
            .collect();
        Ok((candidates.len() == 1).then(|| rounded(*candidates[0])))
    }
    pub fn confirm_static_pair(
        &self,
        frame: &Arc<BgrFrame>,
        reference: [i32; 2],
    ) -> Result<Option<[i32; 2]>, ObservationError> {
        self.pair_near(frame, reference.map(f64::from), f64::from(self.tolerance))
    }
    pub fn current_pair_near(
        &self,
        frame: &Arc<BgrFrame>,
        player: [i32; 2],
        radius: f64,
    ) -> Result<Option<[i32; 2]>, ObservationError> {
        self.pair_near(frame, player.map(f64::from), radius)
    }
}
pub struct HubStabilityTracker {
    maximum_age: f64,
    point: Option<[i32; 2]>,
    reference: Option<Vec<HubPartEvidence>>,
    observed_at: Option<f64>,
}
impl HubStabilityTracker {
    pub fn new(maximum_age: f64) -> Result<Self, ObservationError> {
        if !maximum_age.is_finite() || maximum_age <= 0.0 {
            return Err(ObservationError::invalid(
                "positive finite hub reference age required",
            ));
        }
        Ok(Self {
            maximum_age,
            point: None,
            reference: None,
            observed_at: None,
        })
    }
    pub fn reset(&mut self) {
        self.point = None;
        self.reference = None;
        self.observed_at = None;
    }
    pub fn update(
        &mut self,
        observer: &ArcaneHubObserver,
        frame: &Arc<BgrFrame>,
        point: Option<[i32; 2]>,
        evidence: Option<HubEvidence>,
        acquired_at: f64,
        displacement: Option<[f64; 2]>,
    ) -> Result<(Option<[i32; 2]>, Option<HubEvidence>), ObservationError> {
        if let Some(point) = point {
            self.point = Some(point);
            self.observed_at = Some(acquired_at);
            self.reference = match &evidence {
                Some(HubEvidence::Current { parts, .. }) => Some(parts.clone()),
                _ => None,
            };
            return Ok((Some(point), evidence));
        }
        let (Some(reference), Some(point), Some(observed_at)) =
            (&self.reference, self.point, self.observed_at)
        else {
            return Ok((None, None));
        };
        if !(0.0..=self.maximum_age).contains(&(acquired_at - observed_at))
            || displacement.is_none_or(|p| p[0].hypot(p[1]) >= 2.0)
        {
            self.reset();
            return Ok((None, None));
        }
        let Some(current) = observer.confirm_static_pair(frame, point)? else {
            return Ok((None, None));
        };
        Ok((
            Some(current),
            Some(HubEvidence::Recent {
                point: current,
                reference_observed_at: observed_at,
                reference_parts: reference.clone(),
                current_floor_parts: Vec::new(),
            }),
        ))
    }
}

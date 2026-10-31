//! 현재 세 종착 문양과 부분 재확인. 미검출·접근 추적을 도착으로 바꾸지 않는다.
mod matching;
mod tracking;
use crate::common::{
    error::ObservationError,
    frame::{BgrView, GrayFrame, PixelRect, Size},
    label_occlusion::label_occlusion_mask,
    observation_assets::AssetOwner,
    vision_ops::{codec, color, filters, resize},
};
use matching::{MatchCache, PartMatch, Views};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path, sync::Arc};
pub use tracking::{TerminalObservation, TerminalStabilityTracker};
const BOTTOM: usize = 920;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalReading {
    pub kind: String,
    pub point: [f64; 2],
    pub evidence_boxes: Vec<[i32; 4]>,
    pub scores: Vec<Option<f64>>,
    pub part_ids: Vec<String>,
    pub provenance: String,
    pub current_parts: Vec<bool>,
    pub reference_observed_at: Option<f64>,
    pub visible_fractions: Vec<Option<f64>>,
    pub minimum_separation: f64,
    pub minimum_area: f64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Encoding {
    Gray,
    SoftGold,
}
fn gray() -> Encoding {
    Encoding::Gray
}
#[derive(Deserialize)]
struct Rule {
    path: String,
    threshold: f64,
    #[serde(default = "gray")]
    encoding: Encoding,
}
#[derive(Deserialize)]
struct LayoutRules {
    kind: String,
    parts: Vec<Rule>,
}
#[derive(Deserialize)]
struct Manifest {
    schema: u32,
    frame_size: [usize; 2],
    parts: Vec<Rule>,
    #[serde(default)]
    alternate_layouts: Vec<LayoutRules>,
    #[serde(default)]
    chest_parts: Vec<String>,
    #[serde(default)]
    chest_detail_parts: Vec<String>,
    #[serde(default)]
    chest_detail_minimum_scores: BTreeMap<String, f64>,
    #[serde(default)]
    reacquisition_part: Option<Rule>,
}
struct Part {
    rule: Rule,
    native: GrayFrame,
    small: GrayFrame,
}
struct LayoutParts {
    kind: String,
    parts: Vec<Arc<Part>>,
}
pub struct ArcaneTerminalObserver {
    _compute: crate::common::compute::ComputeLease,
    parts: Vec<Arc<Part>>,
    alternates: Vec<LayoutParts>,
    native_parts: BTreeMap<String, Arc<Part>>,
    chests: Vec<Arc<Part>>,
    details: Vec<Arc<Part>>,
    previous_full: Option<TerminalReading>,
    reacquisition_part: Option<Arc<Part>>,
}
fn native(frame: BgrView<'_>) -> Result<(), ObservationError> {
    if frame.size() != Size::new(1922, 1140)? {
        return Err(ObservationError::invalid(
            "native reviewed terminal frame required",
        ));
    }
    Ok(())
}
fn visible([l, t, r, b]: [i32; 4]) -> bool {
    24 <= l
        && l < r
        && r <= 1900
        && 55 <= t
        && t < b
        && b <= BOTTOM as i32
        && (b <= 900 || (650 <= l && r <= 1290))
}
fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
fn center([l, t, r, b]: [i32; 4]) -> [f64; 2] {
    [
        (f64::from(l) + f64::from(r)) / 2.0,
        (f64::from(t) + f64::from(b)) / 2.0,
    ]
}
fn encoded(frame: BgrView<'_>, encoding: Encoding) -> Result<GrayFrame, ObservationError> {
    if encoding == Encoding::Gray {
        return Ok(color::gray(frame)?);
    }
    let mut gold = GrayFrame::zeros(frame.size())?;
    for y in 0..frame.size().height() {
        let row = frame
            .row(y)
            .ok_or_else(|| ObservationError::invalid("terminal row missing"))?;
        for (x, p) in row.as_chunks::<3>().0.iter().enumerate() {
            gold.pixels_mut()[y * frame.size().width() + x] = p[2]
                .saturating_sub(p[1])
                .min(p[1].saturating_sub(p[0]))
                .saturating_mul(5);
        }
    }
    Ok(filters::separable_q8(gold.view(), &[61, 134, 61])?)
}
impl ArcaneTerminalObserver {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ObservationError> {
        let path = path.as_ref();
        let directory = path
            .parent()
            .ok_or_else(|| ObservationError::invalid("terminal manifest owner required"))?
            .canonicalize()?;
        let owner = AssetOwner::new(&directory)?;
        let data: Manifest = serde_json::from_slice(&std::fs::read(path)?)?;
        if data.schema != 1 || data.frame_size != [1922, 1140] || data.parts.len() != 3 {
            return Err(ObservationError::invalid(
                "reviewed three-part terminal layout required",
            ));
        }
        let load_parts = |rules: Vec<Rule>| -> Result<Vec<Arc<Part>>, ObservationError> {
            let mut parts = Vec::new();
            for rule in rules {
                let path = owner.path(&rule.path)?;
                if ![
                    Some(directory.as_path()),
                    Some(directory.join("templates/terminal").as_path()),
                ]
                .contains(&path.parent())
                    || !(0.9..=1.0).contains(&rule.threshold)
                {
                    return Err(ObservationError::invalid(
                        "terminal part outside reviewed owner or score",
                    ));
                }
                let native = match rule.encoding {
                    Encoding::Gray => codec::load_gray(path)?,
                    Encoding::SoftGold => {
                        encoded(codec::load_color(path)?.view(), Encoding::SoftGold)?
                    }
                };
                let mean = native.pixels().iter().map(|v| f64::from(*v)).sum::<f64>()
                    / native.pixels().len() as f64;
                let deviation = (native
                    .pixels()
                    .iter()
                    .map(|v| (f64::from(*v) - mean).powi(2))
                    .sum::<f64>()
                    / native.pixels().len() as f64)
                    .sqrt();
                if native.size().width().min(native.size().height()) < 20 || deviation < 5.0 {
                    return Err(ObservationError::invalid(
                        "positive textured terminal part required",
                    ));
                }
                let small = resize::half(native.view())?;
                parts.push(Arc::new(Part {
                    rule,
                    native,
                    small,
                }));
            }
            Ok(parts)
        };
        let parts = load_parts(data.parts)?;
        let mut alternates = Vec::new();
        for layout in data.alternate_layouts {
            if layout.kind != "nonboss_chest_platform" || layout.parts.len() != 3 {
                return Err(ObservationError::invalid(
                    "three independent reviewed nonboss platform parts required",
                ));
            }
            alternates.push(LayoutParts {
                kind: layout.kind,
                parts: load_parts(layout.parts)?,
            });
        }
        let mut native_parts = BTreeMap::new();
        for part in parts
            .iter()
            .chain(alternates.iter().flat_map(|a| a.parts.iter()))
        {
            native_parts.insert(part.rule.path.clone(), Arc::clone(part));
        }
        let reacquisition_part = data
            .reacquisition_part
            .map(|rule| load_parts(vec![rule]))
            .transpose()?
            .and_then(|mut parts| parts.pop());
        if let Some(part) = &reacquisition_part {
            if part.rule.threshold < 0.92 || native_parts.contains_key(&part.rule.path) {
                return Err(ObservationError::invalid(
                    "separate native .92 reacquisition part required",
                ));
            }
            native_parts.insert(part.rule.path.clone(), Arc::clone(part));
        }
        let mut chests = Vec::new();
        let mut details = Vec::new();
        for name in &data.chest_parts {
            let part = native_parts
                .get(name)
                .ok_or_else(|| ObservationError::invalid("reviewed chest appearance required"))?;
            if part.rule.encoding != Encoding::Gray {
                return Err(ObservationError::invalid(
                    "floor encoding cannot enter chest composition",
                ));
            }
            chests.push(Arc::clone(part));
        }
        for name in &data.chest_detail_parts {
            let part = native_parts
                .get(name)
                .filter(|_| !data.chest_parts.contains(name))
                .ok_or_else(|| {
                    ObservationError::invalid("separate reviewed chest detail required")
                })?;
            if part.rule.encoding != Encoding::Gray {
                return Err(ObservationError::invalid(
                    "floor encoding cannot enter chest composition",
                ));
            }
            let minimum = data
                .chest_detail_minimum_scores
                .get(name)
                .copied()
                .unwrap_or(0.92);
            if !(0.91..=1.0).contains(&minimum) || part.rule.threshold < minimum {
                return Err(ObservationError::invalid(
                    "chest detail native score below reviewed minimum",
                ));
            }
            details.push(Arc::clone(part));
        }
        Ok(Self {
            _compute: crate::common::compute::ComputeLease::acquire()?,
            parts,
            alternates,
            native_parts,
            chests,
            details,
            previous_full: None,
            reacquisition_part,
        })
    }
    fn part(&self, name: &str) -> Result<&Arc<Part>, ObservationError> {
        self.native_parts
            .get(name)
            .ok_or_else(|| ObservationError::invalid("terminal reference part unavailable"))
    }
    pub fn observe(
        &mut self,
        frame: BgrView<'_>,
    ) -> Result<Option<TerminalReading>, ObservationError> {
        native(frame)?;
        if let Some(reference) = &self.previous_full
            && let Some(current) = self
                .confirm_static_parts(frame, reference)?
                .filter(|r| r.current_parts.iter().all(|v| *v))
        {
            let groups: Vec<Vec<PartMatch>> = current
                .evidence_boxes
                .iter()
                .zip(&current.scores)
                .zip(&current.visible_fractions)
                .map(|((b, s), f)| {
                    Ok(vec![PartMatch {
                        bounds: *b,
                        score: s.ok_or_else(|| {
                            ObservationError::invalid("current terminal score required")
                        })?,
                        fraction: f.ok_or_else(|| {
                            ObservationError::invalid("current visibility required")
                        })?,
                    }])
                })
                .collect::<Result<_, ObservationError>>()?;
            let parts = current
                .part_ids
                .iter()
                .map(|name| self.part(name).cloned())
                .collect::<Result<Vec<_>, _>>()?;
            if let Some(reading) = matching::layout(
                &groups,
                &current.kind,
                &parts,
                current.minimum_separation,
                current.minimum_area,
            ) {
                self.previous_full = Some(reading.clone());
                return Ok(Some(reading));
            }
        }
        self.previous_full = self.observe_full(frame)?;
        Ok(self.previous_full.clone())
    }
    fn observe_full(
        &self,
        frame: BgrView<'_>,
    ) -> Result<Option<TerminalReading>, ObservationError> {
        let views = Views::new(
            frame.crop(PixelRect::new(24, 55, 1900, BOTTOM)?)?,
            self.native_parts
                .values()
                .any(|p| p.rule.encoding == Encoding::SoftGold),
        )?;
        let occluded = label_occlusion_mask(frame, true)?
            .view()
            .crop(PixelRect::new(24, 55, 1900, BOTTOM)?)?
            .to_owned()?;
        let mut cache = MatchCache::new();
        if let Some(groups) = matching::groups(&views, &self.parts, &mut cache, occluded.view())?
            && let Some(reading) =
                matching::layout(&groups, "nonboss_chest_trio", &self.parts, 84.0, 8000.0)
        {
            return Ok(Some(reading));
        }
        let mut remaining: Vec<Arc<Part>> = Vec::new();
        for part in self
            .parts
            .iter()
            .chain(self.alternates.iter().flat_map(|a| a.parts.iter()))
        {
            let key = matching::key(part);
            if !cache.contains_key(&key) && !remaining.iter().any(|p| matching::key(p) == key) {
                remaining.push(Arc::clone(part));
            }
        }
        let count = remaining.len().min(3);
        let mut results: Vec<_> = (0..count).map(|_| None).collect();
        crate::common::compute::scope(|scope| {
            for (worker, result) in results.iter_mut().enumerate() {
                let remaining = &remaining;
                let views = &views;
                let occluded = &occluded;
                scope.spawn(move |_| {
                    *result = Some(
                        remaining
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| i % 3 == worker)
                            .map(|(i, part)| {
                                matching::match_part(views, part, occluded.view()).map(|m| (i, m))
                            })
                            .collect::<Result<Vec<_>, ObservationError>>(),
                    );
                });
            }
        })
        .map_err(crate::common::vision_ops::VisionError::from)?;
        for result in results {
            for (index, found) in
                result.ok_or_else(|| ObservationError::invalid("terminal result missing"))??
            {
                cache.insert(matching::key(&remaining[index]), found);
            }
        }
        for layout in &self.alternates {
            if let Some(groups) =
                matching::groups(&views, &layout.parts, &mut cache, occluded.view())?
                && let Some(reading) =
                    matching::layout(&groups, &layout.kind, &layout.parts, 84.0, 8000.0)
            {
                return Ok(Some(reading));
            }
        }
        let reading = matching::composed(
            &views,
            &self.chests,
            &mut cache,
            occluded.view(),
            84.0,
            8000.0,
        )?;
        if reading.is_some() || self.details.is_empty() {
            return Ok(reading);
        }
        matching::composed(
            &views,
            &self
                .chests
                .iter()
                .chain(&self.details)
                .cloned()
                .collect::<Vec<_>>(),
            &mut cache,
            occluded.view(),
            120.0,
            16000.0,
        )
    }
}

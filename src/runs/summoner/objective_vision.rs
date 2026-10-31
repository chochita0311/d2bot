//! 현재 일지·룬 외형과 여섯 자리의 정합. 보스 생사나 상호작용은 추정하지 않는다.
use super::journal_vision::HorazonJournalObserver;
use crate::common::{
    error::ObservationError,
    frame::{BgrView, GrayFrame, PixelRect, Size},
    observation_assets::AssetOwner,
    vision_ops::{
        codec, color,
        components::{self, Connectivity},
        morphology::{self, Kernel, Morphology},
        resize,
        template::{self, MatchMethod},
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ObjectiveReading {
    pub kind: String,
    pub journal_box: Option<[usize; 4]>,
    pub journal_score: Option<f64>,
    pub rune_boxes: Vec<[usize; 4]>,
    pub evidence_kind: String,
    pub glyph_scores: Vec<(String, f64)>,
    pub ground_point: Option<[f64; 2]>,
    pub slot_indices: Vec<usize>,
    pub layout_residual: Option<f64>,
}
#[derive(Deserialize)]
struct Part {
    id: String,
    path: String,
    crop: [usize; 4],
    sha256: String,
}
#[derive(Deserialize)]
struct Manifest {
    parts: Vec<Part>,
    match_threshold: f64,
    slot_layout_tolerance_pixels: f64,
    glyph_slot_centers: [[f64; 2]; 6],
    approach_ground: [i64; 2],
}
struct Glyph {
    part: Part,
    image: GrayFrame,
}
struct GlyphMatch {
    id: String,
    score: f64,
    bounds: [usize; 4],
}
struct Layout {
    group: Vec<usize>,
    translation: [f64; 2],
    assigned: Vec<usize>,
    residual: f64,
}
pub struct ArcaneObjectiveObserver {
    journal: GrayFrame,
    closed_journal: HorazonJournalObserver,
    glyphs: Vec<Glyph>,
    slots: [[f64; 2]; 6],
    assignments: [Vec<Vec<usize>>; 3],
    threshold: f64,
    tolerance: f64,
    approach: [i64; 2],
}
fn index_groups(length: usize, count: usize, ordered: bool) -> Vec<Vec<usize>> {
    fn collect(
        length: usize,
        count: usize,
        ordered: bool,
        current: &mut Vec<usize>,
        out: &mut Vec<Vec<usize>>,
    ) {
        if current.len() == count {
            out.push(current.clone());
            return;
        }
        let start = if ordered {
            0
        } else {
            current.last().map_or(0, |n| n + 1)
        };
        for index in start..length {
            if ordered && current.contains(&index) {
                continue;
            }
            current.push(index);
            collect(length, count, ordered, current, out);
            current.pop();
        }
    }
    let mut out = Vec::new();
    collect(length, count, ordered, &mut Vec::new(), &mut out);
    out
}
fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    }
}
fn center([l, t, r, b]: [usize; 4]) -> [f64; 2] {
    [(l + r) as f64 / 2.0, (t + b) as f64 / 2.0]
}
fn visible_ground(p: [f64; 2]) -> Option<[f64; 2]> {
    ((24.0..1900.0).contains(&p[0]) && (55.0..900.0).contains(&p[1])).then_some(p)
}
fn blue_mask(frame: BgrView<'_>) -> Result<GrayFrame, ObservationError> {
    Ok(color::in_range(
        color::hsv(frame)?.view(),
        [95, 50, 150],
        [135, 255, 255],
    )?)
}
fn morph(mask: &GrayFrame, side: usize, mode: Morphology) -> Result<GrayFrame, ObservationError> {
    Ok(morphology::morphology(
        mask.view(),
        Kernel::Rectangle {
            width: side,
            height: side,
        },
        mode,
    )?)
}
pub fn rune_core_mask(frame: BgrView<'_>) -> Result<GrayFrame, ObservationError> {
    let mut mask = morph(&blue_mask(frame)?, 7, Morphology::Dilate)?;
    for y in 0..frame.size().height() {
        let row = frame
            .row(y)
            .ok_or_else(|| ObservationError::invalid("missing rune row"))?;
        for (x, p) in row.as_chunks::<3>().0.iter().enumerate() {
            if p[0] < 220 || p[1] < 200 || p[2] < 180 {
                mask.pixels_mut()[y * frame.size().width() + x] = 0;
            }
        }
    }
    Ok(mask)
}
impl ArcaneObjectiveObserver {
    pub fn load(directory: impl AsRef<Path>) -> Result<Self, ObservationError> {
        let directory = directory.as_ref();
        let owner = AssetOwner::new(directory)?;
        let journal = codec::load_gray(owner.path("templates/objective/journal.png")?)?;
        let closed_journal = HorazonJournalObserver::load(directory.join("journal-exit.json"))?;
        let manifest: Manifest =
            serde_json::from_slice(&std::fs::read(directory.join("objective-runes.json"))?)?;
        let mut glyphs = Vec::new();
        for part in manifest.parts {
            let asset = owner.path(&part.path)?;
            if format!("{:x}", Sha256::digest(std::fs::read(&asset)?)) != part.sha256 {
                return Err(ObservationError::invalid(
                    "reviewed objective glyph fingerprint required",
                ));
            }
            PixelRect::new(part.crop[0], part.crop[1], part.crop[2], part.crop[3])?;
            let image = resize::half(codec::load_gray(asset)?.view())?;
            glyphs.push(Glyph { part, image });
        }
        if manifest
            .glyph_slot_centers
            .iter()
            .flatten()
            .any(|v| !v.is_finite())
        {
            return Err(ObservationError::invalid(
                "reviewed six-slot objective layout required",
            ));
        }
        Ok(Self {
            journal,
            closed_journal,
            glyphs,
            slots: manifest.glyph_slot_centers,
            assignments: std::array::from_fn(|i| index_groups(6, i + 4, true)),
            threshold: manifest.match_threshold,
            tolerance: manifest.slot_layout_tolerance_pixels,
            approach: manifest.approach_ground,
        })
    }
    pub fn observe(
        &self,
        frame: BgrView<'_>,
    ) -> Result<Option<ObjectiveReading>, ObservationError> {
        if frame.size() != Size::new(1922, 1140)? {
            return Err(ObservationError::invalid(
                "native reviewed objective frame required",
            ));
        }
        let world = template::GrayMatcher::new(color::gray(
            frame.crop(PixelRect::new(24, 55, 1900, 900)?)?,
        )?)?;
        let map = world.match_pattern(self.journal.view(), MatchMethod::NormalizedCoefficient)?;
        if let Some(peak) = template::maximum(&map).filter(|p| f64::from(p.score) >= 0.92) {
            let [x, y] = [peak.location[0] + 24, peak.location[1] + 55];
            if let Some(reading) = self.journal_platform(
                frame,
                f64::from(peak.score),
                [
                    x,
                    y,
                    x + self.journal.size().width(),
                    y + self.journal.size().height(),
                ],
                None,
            )? {
                return Ok(Some(reading));
            }
        }
        if let Some(reading) = self.glyph_platform(frame)? {
            return Ok(Some(reading));
        }
        for found in self.closed_journal.matches_prepared(&world)? {
            if let Some(reading) =
                self.journal_platform(frame, found.score, found.r#box, found.platform_ground)?
            {
                return Ok(Some(reading));
            }
        }
        Ok(None)
    }
    fn journal_platform(
        &self,
        frame: BgrView<'_>,
        score: f64,
        bounds: [usize; 4],
        ground: Option<[f64; 2]>,
    ) -> Result<Option<ObjectiveReading>, ObservationError> {
        let [x, y, r, b] = bounds;
        let [l, t, right, bottom] = [
            x.saturating_sub(500).max(24),
            y.saturating_sub(350).max(55),
            (r + 500).min(1900),
            (y + 80).min(900),
        ];
        let mask = morph(
            &blue_mask(frame.crop(PixelRect::new(l, t, right, bottom)?)?)?,
            5,
            Morphology::Close,
        )?;
        let components = components::connected_components(mask.view(), Connectivity::Eight)?;
        let boxes: Vec<_> = components
            .stats
            .iter()
            .skip(1)
            .filter_map(|c| c.rect.map(|r| (r, c.area)))
            .filter(|(r, a)| {
                (20..=100).contains(&r.width()) && (20..=110).contains(&r.height()) && *a >= 150
            })
            .map(|(r, _)| [l + r.left(), t + r.top(), l + r.right(), t + r.bottom()])
            .collect();
        if !(4..=12).contains(&boxes.len())
            || boxes.iter().map(|b| b[0]).max().unwrap_or(0)
                - boxes.iter().map(|b| b[0]).min().unwrap_or(0)
                < 200
        {
            return Ok(None);
        }
        Ok(Some(ObjectiveReading {
            kind: "summoner_platform".into(),
            journal_box: Some(bounds),
            journal_score: Some(score),
            rune_boxes: boxes,
            evidence_kind: "journal_and_surrounding_runes".into(),
            glyph_scores: Vec::new(),
            ground_point: visible_ground(ground.unwrap_or([(x + r) as f64 / 2.0, (b + 80) as f64])),
            slot_indices: Vec::new(),
            layout_residual: None,
        }))
    }
    fn glyph_platform(
        &self,
        frame: BgrView<'_>,
    ) -> Result<Option<ObjectiveReading>, ObservationError> {
        let mask =
            resize::half(rune_core_mask(frame.crop(PixelRect::new(24, 55, 1900, 900)?)?)?.view())?;
        let mut binary = mask.clone();
        for v in binary.pixels_mut() {
            *v = u8::from(*v > 0);
        }
        let joined = morph(&binary, 5, Morphology::Close)?;
        let components = components::connected_components(joined.view(), Connectivity::Eight)?;
        if components
            .stats
            .iter()
            .skip(1)
            .filter(|c| c.area >= 20 && c.rect.is_some_and(|r| r.width() >= 10 && r.height() >= 6))
            .count()
            < 4
        {
            return Ok(None);
        }
        let mut matches = Vec::new();
        let matcher = template::GrayMatcher::new(mask)?;
        for glyph in &self.glyphs {
            let map =
                matcher.match_pattern(glyph.image.view(), MatchMethod::NormalizedCoefficient)?;
            if let Some(peak) =
                template::maximum(&map).filter(|p| f64::from(p.score) >= self.threshold)
            {
                let [x, y] = [peak.location[0] * 2 + 24, peak.location[1] * 2 + 55];
                let [l, t, r, b] = glyph.part.crop;
                matches.push(GlyphMatch {
                    id: glyph.part.id.clone(),
                    score: f64::from(peak.score),
                    bounds: [x, y, x + r - l, y + b - t],
                });
            }
        }
        if matches.len() > 6 {
            return Err(ObservationError::invalid(
                "six-slot glyph identity limit exceeded",
            ));
        }
        for count in (4..=matches.len()).rev() {
            let mut candidates = Vec::new();
            for group in index_groups(matches.len(), count, false) {
                let centers: Vec<_> = group.iter().map(|i| center(matches[*i].bounds)).collect();
                if centers.iter().enumerate().any(|(i, a)| {
                    centers[i + 1..]
                        .iter()
                        .any(|b| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt() < 100.0)
                }) {
                    continue;
                }
                if [300.0, 70.0].iter().enumerate().any(|(axis, min)| {
                    centers
                        .iter()
                        .map(|p| p[axis])
                        .fold(f64::NEG_INFINITY, f64::max)
                        - centers
                            .iter()
                            .map(|p| p[axis])
                            .fold(f64::INFINITY, f64::min)
                        < *min
                }) {
                    continue;
                }
                for assigned in &self.assignments[count - 4] {
                    let offsets: Vec<[f64; 2]> = centers
                        .iter()
                        .zip(assigned)
                        .map(|(p, i)| [p[0] - self.slots[*i][0], p[1] - self.slots[*i][1]])
                        .collect();
                    let translation = std::array::from_fn(|axis| {
                        median(offsets.iter().map(|p| p[axis]).collect())
                    });
                    let residual = offsets
                        .iter()
                        .flat_map(|p| [0, 1].map(|a| (p[a] - translation[a]).abs()))
                        .fold(0.0, f64::max);
                    if residual <= self.tolerance {
                        candidates.push(Layout {
                            group: group.clone(),
                            translation,
                            assigned: assigned.clone(),
                            residual,
                        });
                    }
                }
            }
            if candidates.is_empty() {
                continue;
            }
            let median_translation: [f64; 2] = std::array::from_fn(|a| {
                median(candidates.iter().map(|c| c.translation[a]).collect())
            });
            if candidates.iter().any(|c| {
                [0, 1]
                    .iter()
                    .any(|a| (c.translation[*a] - median_translation[*a]).abs() > self.tolerance)
            }) {
                return Ok(None);
            }
            let mut selected = &candidates[0];
            for candidate in &candidates[1..] {
                if candidate.residual < selected.residual {
                    selected = candidate;
                }
            }
            return Ok(Some(ObjectiveReading {
                kind: "summoner_platform".into(),
                journal_box: None,
                journal_score: None,
                rune_boxes: selected.group.iter().map(|i| matches[*i].bounds).collect(),
                evidence_kind: "distinct_glyph_shapes_and_slot_layout".into(),
                glyph_scores: selected
                    .group
                    .iter()
                    .map(|i| (matches[*i].id.clone(), matches[*i].score))
                    .collect(),
                ground_point: visible_ground(std::array::from_fn(|a| {
                    self.approach[a] as f64 + selected.translation[a]
                })),
                slot_indices: selected.assigned.clone(),
                layout_residual: Some(selected.residual),
            }));
        }
        Ok(None)
    }
}

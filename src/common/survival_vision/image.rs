use super::{
    HudReading, ResourceReading, SlotReading,
    manifest::{HudAssets, Resource},
    numbers::{mask_score, read_number, text_mask},
};
use crate::common::{
    buffing::BeltVisibility,
    error::ObservationError,
    frame::{BgrView, FloatMap, GrayFrame, PixelRect},
    label_occlusion::{label_occlusion_mask, visible_template_peak},
    observation_assets::color_distance,
    vision_ops::{
        color,
        filters::separable_q8,
        morphology::{Kernel, Morphology, morphology},
        template::{self, MatchMethod},
    },
};
use std::path::Path;
#[derive(Debug, Clone, Copy, Default)]
pub struct HudObserverOptions {
    pub allow_high_orb_fallback: bool,
    pub allow_partial_orb_fallback: bool,
    pub observe_mercenary: bool,
}
pub struct SurvivalHudObserver {
    assets: HudAssets,
}
impl SurvivalHudObserver {
    pub fn load(
        directory: impl AsRef<Path>,
        options: HudObserverOptions,
    ) -> Result<Self, ObservationError> {
        Ok(Self {
            assets: HudAssets::load(directory.as_ref(), options)?,
        })
    }
    fn reading(
        &self,
        life: ResourceReading,
        mana: ResourceReading,
        belt_status: &str,
        belt_visibility: BeltVisibility,
        slots: Vec<SlotReading>,
    ) -> HudReading {
        HudReading {
            layout_id: self.assets.layout_id.clone(),
            revision: self.assets.revision.clone(),
            life,
            mana,
            belt_status: belt_status.into(),
            slots,
            belt_visibility,
            mercenary_status: None,
        }
    }
    fn unavailable(&self) -> HudReading {
        self.reading(
            ResourceReading::unknown("hud_unverified", None),
            ResourceReading::unknown("hud_unverified", None),
            "unverified",
            BeltVisibility::Unknown,
            Vec::new(),
        )
    }
    fn resource_box(
        &self,
        frame: BgrView<'_>,
        resource: &Resource,
    ) -> Result<Option<PixelRect>, ObservationError> {
        let mask = text_mask(frame.crop(resource.search)?)?;
        let floating = |gray: &GrayFrame| -> Result<FloatMap, ObservationError> {
            Ok(FloatMap::from_vec(
                gray.size(),
                gray.pixels().iter().map(|v| f32::from(*v)).collect(),
            )?)
        };
        let target = floating(&mask)?;
        let mut candidates = Vec::new();
        for image in &resource.labels {
            let reference = floating(image)?;
            let intersection = template::match_template(
                target.view(),
                reference.view(),
                MatchMethod::Correlation,
            )?;
            let ones = FloatMap::from_vec(image.size(), vec![1.0; image.pixels().len()])?;
            let total =
                template::match_template(target.view(), ones.view(), MatchMethod::Correlation)?;
            let reference_sum = image.pixels().iter().map(|v| u64::from(*v)).sum::<u64>() as f32;
            let mut scores = FloatMap::zeros(intersection.size())?;
            for (i, score) in scores.pixels_mut().iter_mut().enumerate() {
                let overlap = intersection.pixels()[i];
                *score = overlap / (total.pixels()[i] + reference_sum - overlap).max(1.0);
            }
            let peak = template::maximum(&scores)
                .ok_or_else(|| ObservationError::invalid("label correlation produced no peak"))?;
            let [x, y] = peak.location;
            let region = PixelRect::new(x, y, x + image.size().width(), y + image.size().height())?;
            let score = mask_score(mask.view().crop(region)?, image.view())?;
            candidates.push((score, x, y, image.size().height(), image.size().width()));
        }
        // Python max(tuple)와 같이 동점이면 x, y, 높이, 너비 순으로 비교한다.
        let (score, x, y, height, width) = candidates
            .into_iter()
            .max_by(|a, b| {
                a.0.total_cmp(&b.0)
                    .then(a.1.cmp(&b.1))
                    .then(a.2.cmp(&b.2))
                    .then(a.3.cmp(&b.3))
                    .then(a.4.cmp(&b.4))
            })
            .ok_or_else(|| ObservationError::invalid("resource label variants required"))?;
        if score < self.assets.matching.label_score {
            let patch = mask
                .view()
                .crop(PixelRect::new(x, y, x + width, y + height)?)?;
            let mut outlined = 0.0_f64;
            for image in &resource.labels {
                let support = morphology(image.view(), Kernel::Cross3, Morphology::Dilate)?;
                let mut filtered = patch.to_owned()?;
                for (v, s) in filtered.pixels_mut().iter_mut().zip(support.pixels()) {
                    *v &= s;
                }
                outlined = outlined.max(mask_score(filtered.view(), image.view())?);
            }
            if outlined < self.assets.matching.label_score {
                return Ok(None);
            }
        }
        let left = resource
            .search
            .left()
            .checked_add(x)
            .and_then(|n| n.checked_add(width))
            .and_then(|n| n.checked_add_signed(resource.gap))
            .ok_or_else(|| ObservationError::invalid("resource number box overflow"))?;
        let top = resource.search.top() + y;
        Ok(Some(
            PixelRect::new(left, top, resource.search.right(), top + height)?
                .checked_in(frame.size())?,
        ))
    }
    fn orb_fallback(
        &self,
        frame: BgrView<'_>,
        index: usize,
        mut reading: ResourceReading,
    ) -> Result<ResourceReading, ObservationError> {
        if reading.ratio().is_some() || reading.reason == "number_out_of_range" {
            return Ok(reading);
        }
        for bound in &self.assets.bounds {
            let (region, channel) = bound.resources[index];
            let patch = frame.crop(region)?;
            let mut all_liquid = true;
            for y in 0..region.height() {
                let row = patch
                    .row(y)
                    .ok_or_else(|| ObservationError::invalid("missing orb row"))?;
                let count = row
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .filter(|p| {
                        let other = (0..3)
                            .filter(|c| *c != channel)
                            .map(|c| p[c])
                            .max()
                            .unwrap_or(0);
                        p[channel] >= bound.minimum_value
                            && i16::from(p[channel]) - i16::from(other)
                                >= i16::from(bound.channel_margin)
                    })
                    .count();
                if (count as f64 / region.width() as f64) < bound.minimum_row_coverage {
                    all_liquid = false;
                    break;
                }
            }
            if all_liquid {
                reading.reason.push(';');
                reading.reason.push_str(bound.reason);
                reading.lower_bound = Some(bound.minimum_ratio);
                return Ok(reading);
            }
        }
        Ok(reading)
    }
    fn slot_reading(
        &self,
        frame: BgrView<'_>,
        index: usize,
    ) -> Result<SlotReading, ObservationError> {
        let image = frame.crop(self.assets.belt.slots[index])?;
        let mut candidates = Vec::new();
        for (kind, templates) in &self.assets.belt.items {
            let mut distance = f64::INFINITY;
            for reference in templates {
                distance = distance.min(color_distance(image, reference.view())?);
            }
            candidates.push((distance, kind));
        }
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(b.1)));
        let (distance, kind) = candidates[0];
        let kind = (distance <= self.assets.matching.item_distance
            && candidates[1].0 - distance >= self.assets.matching.item_margin)
            .then(|| kind.clone());
        Ok(SlotReading {
            row: (index / 4 + 1) as u8,
            column: (index % 4 + 1) as u8,
            kind,
            distance,
        })
    }
    pub fn closed_belt_slots(
        &self,
        frame: BgrView<'_>,
        reading: &HudReading,
    ) -> Result<Vec<SlotReading>, ObservationError> {
        if reading.belt_visibility != BeltVisibility::Closed || frame.size() != self.assets.size {
            return Ok(Vec::new());
        }
        (12..16).map(|i| self.slot_reading(frame, i)).collect()
    }
    fn observe_resources(&self, frame: BgrView<'_>) -> Result<HudReading, ObservationError> {
        let unavailable = self.unavailable();
        if frame.size() != self.assets.size {
            return Ok(HudReading {
                life: ResourceReading::unknown("layout_unsupported", None),
                ..unavailable
            });
        }
        let boxes = [
            self.resource_box(frame, &self.assets.resources[0])?,
            self.resource_box(frame, &self.assets.resources[1])?,
        ];
        let belt = &self.assets.belt;
        let settings = &self.assets.matching;
        if boxes.iter().all(Option::is_none) {
            if self.assets.bounds.is_empty() {
                return Ok(unavailable);
            }
            let anchor = frame.crop(belt.closed_box)?;
            if color_distance(anchor, belt.closed.view())? > settings.closed_anchor_distance {
                return Ok(unavailable);
            }
            let a = separable_q8(anchor, &[61, 134, 61])?;
            let b = separable_q8(belt.closed.view(), &[61, 134, 61])?;
            let scores =
                template::match_template(a.view(), b.view(), MatchMethod::NormalizedCoefficient)?;
            let score = f64::from(scores.pixels()[0]);
            if !score.is_finite() || score < settings.closed_anchor_score {
                return Ok(unavailable);
            }
            let life = self.orb_fallback(
                frame,
                0,
                ResourceReading::unknown("label_unverified;closed_hud_anchor", None),
            )?;
            let mana = self.orb_fallback(
                frame,
                1,
                ResourceReading::unknown("label_unverified;closed_hud_anchor", None),
            )?;
            if life.ratio().is_none() || mana.ratio().is_none() {
                return Ok(unavailable);
            }
            return Ok(self.reading(
                life,
                mana,
                "not_verified_expanded",
                BeltVisibility::Closed,
                Vec::new(),
            ));
        }
        let mut readings = Vec::new();
        for (index, region) in boxes.into_iter().enumerate() {
            let reading = if let Some(region) = region {
                read_number(frame.crop(region)?, settings, &self.assets.glyphs)?
            } else {
                ResourceReading::unknown("hud_unverified", None)
            };
            readings.push(self.orb_fallback(frame, index, reading)?);
        }
        let mana = readings
            .pop()
            .ok_or_else(|| ObservationError::invalid("mana reading missing"))?;
        let life = readings
            .pop()
            .ok_or_else(|| ObservationError::invalid("life reading missing"))?;
        let closed = color_distance(frame.crop(belt.closed_box)?, belt.closed.view())?
            <= settings.closed_anchor_distance;
        if color_distance(frame.crop(belt.anchor_box)?, belt.anchor.view())?
            > settings.belt_anchor_distance
        {
            return Ok(self.reading(
                life,
                mana,
                "not_verified_expanded",
                if closed {
                    BeltVisibility::Closed
                } else {
                    BeltVisibility::Unknown
                },
                Vec::new(),
            ));
        }
        if closed {
            return Ok(self.reading(
                life,
                mana,
                "visibility_ambiguous",
                BeltVisibility::Unknown,
                Vec::new(),
            ));
        }
        let slots = (0..16)
            .map(|i| self.slot_reading(frame, i))
            .collect::<Result<Vec<_>, _>>()?;
        let status = if slots.iter().all(|s| s.kind.is_some()) {
            "read"
        } else {
            "slot_unverified"
        };
        Ok(self.reading(life, mana, status, BeltVisibility::Expanded, slots))
    }
    pub fn observe(&self, frame: BgrView<'_>) -> Result<HudReading, ObservationError> {
        let mut reading = self.observe_resources(frame)?;
        let Some(reference) = &self.assets.portrait else {
            return Ok(reading);
        };
        let mut status = "unverified";
        if frame.size() == self.assets.size {
            let face = color::gray(frame.crop(reference.region)?)?;
            let scores = template::match_template(
                face.view(),
                reference.image.view(),
                MatchMethod::NormalizedCoefficient,
            )?;
            let mut score = f64::from(scores.pixels()[0]);
            if score.is_finite() && score < reference.minimum_score {
                let occlusion = label_occlusion_mask(frame, true)?;
                let mut visible = occlusion.view().crop(reference.region)?.to_owned()?;
                for v in visible.pixels_mut() {
                    *v = u8::from(*v == 0);
                }
                if visible.pixels().contains(&0) {
                    score = visible_template_peak(
                        face.view(),
                        reference.image.view(),
                        visible.view(),
                        reference.minimum_visible,
                    )?
                    .score;
                }
            }
            if score.is_finite() && score >= reference.minimum_score {
                status = "observed";
            }
        }
        reading.mercenary_status = Some(status.into());
        Ok(reading)
    }
}

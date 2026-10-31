use super::*;
use crate::common::frame::BgrFrame;
impl ArcaneTerminalObserver {
    fn support_references(
        &self,
        frame: BgrView<'_>,
        reference: &TerminalReading,
    ) -> Result<Vec<TerminalReading>, ObservationError> {
        let Some(part) = &self.reacquisition_part else {
            return Ok(vec![]);
        };
        if reference.provenance != "current_three_parts"
            || reference.kind != "nonboss_chest_platform"
            || reference.part_ids.len() != 3
            || reference.evidence_boxes.len() != 3
            || reference.scores.len() != 3
            || reference.current_parts != [true; 3]
            || reference.visible_fractions != [Some(1.0); 3]
            || reference.part_ids.iter().any(|name| {
                !self
                    .chests
                    .iter()
                    .chain(&self.details)
                    .any(|part| &part.rule.path == name)
            })
        {
            return Ok(vec![]);
        }
        native(frame)?;
        let views = Views::new(
            frame.crop(PixelRect::new(24, 55, 1900, BOTTOM)?)?,
            part.rule.encoding == Encoding::SoftGold,
        )?;
        let occluded = label_occlusion_mask(frame, true)?;
        let mut cache = MatchCache::new();
        let Some(mut found) = matching::groups(
            &views,
            &[Arc::clone(part)],
            &mut cache,
            occluded
                .view()
                .crop(PixelRect::new(24, 55, 1900, BOTTOM)?)?,
        )?
        else {
            return Ok(vec![]);
        };
        let support: Vec<_> = found
            .remove(0)
            .into_iter()
            .filter(|m| m.fraction == 1.0)
            .collect();
        let mut references = Vec::new();
        for (a, b) in [(0, 1), (0, 2), (1, 2)] {
            let old = |i: usize| PartMatch {
                bounds: reference.evidence_boxes[i],
                score: reference.scores[i].unwrap_or(0.0),
                fraction: 1.0,
            };
            let parts = [
                Arc::clone(self.part(&reference.part_ids[a])?),
                Arc::clone(self.part(&reference.part_ids[b])?),
                Arc::clone(part),
            ];
            if let Some(mut reading) = matching::layout(
                &[vec![old(a)], vec![old(b)], support.clone()],
                &reference.kind,
                &parts,
                84.0,
                8000.0,
            ) {
                // 같은 화면의 세 상자로 확인한 목표만 이동시킨다. 보조 문양으로 새 종점을 만들지 않는다.
                reading.point = reference.point;
                reading.provenance = "anchored_support_reference".into();
                references.push(reading);
            }
        }
        Ok(references)
    }
    fn native_images(
        &self,
        frame: BgrView<'_>,
        names: &[String],
    ) -> Result<Vec<(Encoding, GrayFrame)>, ObservationError> {
        let mut encodings = vec![Encoding::Gray];
        for name in names {
            let encoding = self.part(name)?.rule.encoding;
            if !encodings.contains(&encoding) {
                encodings.push(encoding);
            }
        }
        encodings
            .into_iter()
            .map(|e| Ok((e, encoded(frame, e)?)))
            .collect()
    }
    pub fn confirm_static_parts(
        &self,
        frame: BgrView<'_>,
        reference: &TerminalReading,
    ) -> Result<Option<TerminalReading>, ObservationError> {
        native(frame)?;
        let images = self.native_images(frame, &reference.part_ids)?;
        let occluded = label_occlusion_mask(frame, true)?;
        let mut reading = reference.clone();
        reading.evidence_boxes.clear();
        reading.scores.clear();
        reading.current_parts.clear();
        reading.visible_fractions.clear();
        for (name, bounds) in reference.part_ids.iter().zip(&reference.evidence_boxes) {
            let part = self.part(name)?;
            let gray = &images
                .iter()
                .find(|(e, _)| *e == part.rule.encoding)
                .ok_or_else(|| ObservationError::invalid("terminal encoding missing"))?
                .1;
            let found = matching::local(gray, &occluded, part, *bounds, 3)?;
            reading.current_parts.push(found.is_some());
            reading
                .evidence_boxes
                .push(found.as_ref().map_or(*bounds, |m| m.bounds));
            reading.scores.push(found.as_ref().map(|m| m.score));
            reading
                .visible_fractions
                .push(found.as_ref().map(|m| m.fraction));
        }
        if reading.current_parts.len() != 3
            || reading.current_parts.iter().filter(|v| **v).count() < 2
        {
            return Ok(None);
        }
        reading.provenance = "current_static_two_parts".into();
        Ok(Some(reading))
    }
    pub fn confirm_approach_parts(
        &self,
        frame: BgrView<'_>,
        reference: &TerminalReading,
        displacement: [f64; 2],
    ) -> Result<Option<TerminalReading>, ObservationError> {
        native(frame)?;
        if displacement.iter().any(|v| !v.is_finite()) {
            return Err(ObservationError::invalid(
                "finite terminal displacement required",
            ));
        }
        let images = self.native_images(frame, &reference.part_ids)?;
        let occluded = label_occlusion_mask(frame, true)?;
        let shift = displacement.map(|v| -v);
        let mut corrections = Vec::new();
        let mut reading = reference.clone();
        reading.evidence_boxes.clear();
        reading.scores.clear();
        reading.current_parts.clear();
        reading.visible_fractions.clear();
        for (name, bounds) in reference.part_ids.iter().zip(&reference.evidence_boxes) {
            let part = self.part(name)?;
            let gray = &images
                .iter()
                .find(|(e, _)| *e == part.rule.encoding)
                .ok_or_else(|| ObservationError::invalid("terminal encoding missing"))?
                .1;
            let predicted = std::array::from_fn(|i| {
                (f64::from(bounds[i]) + shift[i % 2]).round_ties_even() as i32
            });
            let found = matching::local(gray, &occluded, part, predicted, 8)?;
            if let Some(found) = &found {
                corrections.push([
                    f64::from(found.bounds[0]) - f64::from(predicted[0]),
                    f64::from(found.bounds[1]) - f64::from(predicted[1]),
                ]);
            }
            reading.current_parts.push(found.is_some());
            reading
                .evidence_boxes
                .push(found.as_ref().map_or(predicted, |m| m.bounds));
            reading.scores.push(found.as_ref().map(|m| m.score));
            reading
                .visible_fractions
                .push(found.as_ref().map(|m| m.fraction));
        }
        if reading.current_parts.len() != 3
            || corrections.is_empty()
            || corrections
                .iter()
                .enumerate()
                .any(|(i, a)| corrections[i + 1..].iter().any(|b| distance(*a, *b) > 6.0))
        {
            return Ok(None);
        }
        let correction: [f64; 2] = std::array::from_fn(|a| {
            corrections.iter().map(|p| p[a]).sum::<f64>() / corrections.len() as f64
        });
        reading.point = std::array::from_fn(|a| reference.point[a] + shift[a] + correction[a]);
        reading.provenance = "current_motion_approach_only".into();
        Ok(Some(reading))
    }
    pub fn confirm_relocalized_parts(
        &self,
        frame: BgrView<'_>,
        reference: &TerminalReading,
    ) -> Result<Option<TerminalReading>, ObservationError> {
        native(frame)?;
        if reference.part_ids.len() != 3 || reference.evidence_boxes.len() != 3 {
            return Ok(None);
        }
        let mut parts = Vec::new();
        for name in &reference.part_ids {
            parts.push(Arc::clone(self.part(name)?));
        }
        let views = Views::new(
            frame.crop(PixelRect::new(24, 55, 1900, BOTTOM)?)?,
            parts.iter().any(|p| p.rule.encoding == Encoding::SoftGold),
        )?;
        let occluded = label_occlusion_mask(frame, true)?;
        let mask = occluded
            .view()
            .crop(PixelRect::new(24, 55, 1900, BOTTOM)?)?;
        let mut cache = MatchCache::new();
        let mut groups = Vec::new();
        for part in parts {
            groups.push(
                matching::groups(&views, &[part], &mut cache, mask)?
                    .map_or_else(Vec::new, |mut g| g.remove(0)),
            );
        }
        let mut best: Option<(f64, usize, usize, PartMatch, PartMatch, [f64; 2])> = None;
        for first in 0..groups.len() {
            for second in first + 1..groups.len() {
                for a in &groups[first] {
                    for b in &groups[second] {
                        let shifts = [
                            [
                                f64::from(a.bounds[0])
                                    - f64::from(reference.evidence_boxes[first][0]),
                                f64::from(a.bounds[1])
                                    - f64::from(reference.evidence_boxes[first][1]),
                            ],
                            [
                                f64::from(b.bounds[0])
                                    - f64::from(reference.evidence_boxes[second][0]),
                                f64::from(b.bounds[1])
                                    - f64::from(reference.evidence_boxes[second][1]),
                            ],
                        ];
                        if distance(shifts[0], shifts[1]) > 6.0 {
                            continue;
                        }
                        let shift = std::array::from_fn(|i| (shifts[0][i] + shifts[1][i]) / 2.0);
                        if distance(center(a.bounds), center(b.bounds)) < 84.0
                            || shift[0].hypot(shift[1]) > 900.0
                        {
                            continue;
                        }
                        let score = a.score + b.score;
                        if best.as_ref().is_none_or(|best| score > best.0) {
                            best = Some((score, first, second, a.clone(), b.clone(), shift));
                        }
                    }
                }
            }
        }
        let Some((_, first, second, a, b, shift)) = best else {
            return Ok(None);
        };
        let mut reading = reference.clone();
        reading.evidence_boxes = reference
            .evidence_boxes
            .iter()
            .map(|b| {
                std::array::from_fn(|i| (f64::from(b[i]) + shift[i % 2]).round_ties_even() as i32)
            })
            .collect();
        reading.scores = vec![None; 3];
        reading.current_parts = vec![false; 3];
        reading.visible_fractions = vec![None; 3];
        for (index, found) in [(first, a), (second, b)] {
            reading.evidence_boxes[index] = found.bounds;
            reading.scores[index] = Some(found.score);
            reading.current_parts[index] = true;
            reading.visible_fractions[index] = Some(found.fraction);
        }
        reading.point = std::array::from_fn(|i| reference.point[i] + shift[i]);
        reading.provenance = "current_landmark_approach_only".into();
        Ok(Some(reading))
    }
}
pub struct TerminalObservation<'a> {
    pub frame: &'a Arc<BgrFrame>,
    pub reading: Option<TerminalReading>,
    pub acquired_at: f64,
    pub displacement: Option<[f64; 2]>,
}
#[derive(Serialize)]
pub struct TerminalStabilityTracker {
    #[serde(skip)]
    maximum_age: f64,
    reference: Option<TerminalReading>,
    observed_at: Option<f64>,
    approach: Option<TerminalReading>,
    approach_only: bool,
    landmark_relocalized: bool,
    previous_current_full: Option<(TerminalReading, f64)>,
    approach_displacement: Option<[f64; 2]>,
    approach_static_pairs: u64,
    // ID가 바뀔 때 직전 실제 화면을 확인하는 데 필요한 한 장만 보관한다. 이동 이력에는 넘기지 않는다.
    #[serde(skip)]
    previous_current_frame: Option<Arc<BgrFrame>>,
    #[serde(skip)]
    support_references: Vec<TerminalReading>,
    #[serde(skip)]
    support_approach_only: bool,
}
impl TerminalStabilityTracker {
    pub fn new(maximum_age: f64) -> Result<Self, ObservationError> {
        if !maximum_age.is_finite() || maximum_age < 0.0 {
            return Err(ObservationError::invalid(
                "finite terminal reference age required",
            ));
        }
        Ok(Self {
            maximum_age,
            reference: None,
            observed_at: None,
            approach: None,
            approach_only: false,
            landmark_relocalized: false,
            previous_current_full: None,
            approach_displacement: None,
            approach_static_pairs: 0,
            previous_current_frame: None,
            support_references: vec![],
            support_approach_only: false,
        })
    }
    pub fn approach(&self) -> Option<&TerminalReading> {
        self.approach.as_ref()
    }
    pub fn approach_displacement(&self) -> Option<[f64; 2]> {
        self.approach_displacement
    }
    pub fn approach_static_pairs(&self) -> u64 {
        self.approach_static_pairs
    }
    pub fn reference_state(&self) -> (bool, Option<f64>) {
        (self.reference.is_some(), self.observed_at)
    }
    pub fn reset(&mut self) {
        self.reference = None;
        self.observed_at = None;
        self.approach = None;
        self.approach_only = false;
        self.landmark_relocalized = false;
        self.previous_current_full = None;
        self.approach_displacement = None;
        self.previous_current_frame = None;
        self.approach_static_pairs = 0;
        self.support_references.clear();
        self.support_approach_only = false;
    }
    pub fn require_relocalization(&mut self) {
        self.approach = None;
        self.landmark_relocalized = true;
        self.previous_current_full = None;
        self.approach_displacement = None;
        self.previous_current_frame = None;
        self.approach_static_pairs = 0;
    }
    pub fn update(
        &mut self,
        observer: &ArcaneTerminalObserver,
        observation: TerminalObservation<'_>,
    ) -> Result<Option<TerminalReading>, ObservationError> {
        let TerminalObservation {
            frame,
            reading,
            acquired_at,
            displacement,
        } = observation;
        if !acquired_at.is_finite()
            || acquired_at < 0.0
            || displacement.is_some_and(|d| d.iter().any(|v| !v.is_finite()))
        {
            return Err(ObservationError::invalid(
                "finite terminal acquisition time and displacement required",
            ));
        }
        self.approach = None;
        self.approach_displacement = None;
        let previous = self.previous_current_full.take();
        let previous_frame = self.previous_current_frame.take();
        if let Some(reading) = reading.as_ref().filter(|r| {
            r.provenance == "current_three_parts"
                && r.part_ids.len() == 3
                && r.evidence_boxes.len() == 3
                && r.current_parts == [true, true, true]
        }) {
            self.previous_current_full = Some((reading.clone(), acquired_at));
            self.previous_current_frame = Some(Arc::clone(frame));
        }
        if let (Some((mut before, before_at)), Some((current, _))) =
            (previous, self.previous_current_full.as_ref())
            && (0.0..0.25).contains(&(acquired_at - before_at))
            && acquired_at > before_at
            && before.kind == current.kind
        {
            let mut eligible = true;
            if before.part_ids != current.part_ids {
                if let Some(old) = previous_frame
                    .as_ref()
                    .map(|f| observer.confirm_static_parts(f.view(), current))
                    .transpose()?
                    .flatten()
                    .filter(|r| {
                        r.visible_fractions.len() == 3
                            && r.visible_fractions.iter().all(|v| *v == Some(1.0))
                    })
                {
                    before = old;
                } else {
                    eligible = false;
                }
            }
            if eligible && before.current_parts.iter().all(|v| *v) {
                let shifts: Vec<[f64; 2]> = before
                    .evidence_boxes
                    .iter()
                    .zip(&current.evidence_boxes)
                    .map(|(a, b)| {
                        [
                            f64::from(b[0]) - f64::from(a[0]),
                            f64::from(b[1]) - f64::from(a[1]),
                        ]
                    })
                    .collect();
                if shifts.iter().all(|s| s[0].hypot(s[1]) < 2.0) {
                    self.approach_displacement = Some(std::array::from_fn(|a| {
                        shifts.iter().map(|s| s[a]).sum::<f64>() / 3.0
                    }));
                }
            }
        }
        self.approach_static_pairs = if self.approach_displacement.is_some() {
            self.approach_static_pairs.checked_add(1).ok_or_else(|| {
                ObservationError::invalid("terminal static pair counter exhausted")
            })?
        } else {
            0
        };
        if let Some(mut reading) = reading {
            self.support_references = observer.support_references(frame.view(), &reading)?;
            self.support_approach_only = false;
            self.reference = (reading.part_ids.len() == 3).then(|| reading.clone());
            self.observed_at = Some(acquired_at);
            self.approach_only = false;
            self.landmark_relocalized = false;
            reading.reference_observed_at = Some(acquired_at);
            return Ok(Some(reading));
        }
        let Some(reference) = self.reference.as_ref() else {
            return Ok(None);
        };
        if !self
            .observed_at
            .is_some_and(|t| (0.0..=self.maximum_age).contains(&(acquired_at - t)))
        {
            self.reset();
            return Ok(None);
        }
        let Some(displacement) = displacement.filter(|_| !self.landmark_relocalized) else {
            return self.relocalize(observer, frame, displacement);
        };
        if self.approach_only || displacement[0].hypot(displacement[1]) >= 2.0 {
            let previous = reference.clone();
            let was_approach = self.approach_only;
            let Some(reading) =
                observer.confirm_approach_parts(frame.view(), reference, displacement)?
            else {
                return self.relocalize(observer, frame, Some(displacement));
            };
            self.reference = Some(reading.clone());
            let mut approach = reading.clone();
            approach.reference_observed_at = self.observed_at;
            if self.support_approach_only {
                approach.provenance = "current_anchored_support_approach_only".into();
            }
            self.approach = Some(approach.clone());
            self.approach_only = true;
            if was_approach
                && !self.support_approach_only
                && same_static_parts(&previous, &reading, Some(displacement))
            {
                approach.provenance = "current_motion_settled_two_parts".into();
                return Ok(Some(approach));
            }
            return Ok(None);
        }
        Ok(observer
            .confirm_static_parts(frame.view(), reference)?
            .map(|mut r| {
                r.reference_observed_at = self.observed_at;
                r
            }))
    }
    fn relocalize(
        &mut self,
        observer: &ArcaneTerminalObserver,
        frame: &Arc<BgrFrame>,
        displacement: Option<[f64; 2]>,
    ) -> Result<Option<TerminalReading>, ObservationError> {
        let Some(previous) = self.reference.clone() else {
            return Ok(None);
        };
        let was_relocalized = self.landmark_relocalized;
        self.landmark_relocalized = true;
        self.approach_only = true;
        let mut found = observer.confirm_relocalized_parts(frame.view(), &previous)?;
        let mut previous = previous;
        if found.is_none() {
            for support in &self.support_references {
                if let Some(reading) = observer.confirm_relocalized_parts(frame.view(), support)? {
                    previous = support.clone();
                    found = Some(reading);
                    self.support_approach_only = true;
                    break;
                }
            }
        }
        if let Some(reading) = found {
            self.reference = Some(reading.clone());
            let mut approach = reading.clone();
            approach.reference_observed_at = self.observed_at;
            if self.support_approach_only {
                approach.provenance = "current_anchored_support_approach_only".into();
            }
            self.approach = Some(approach.clone());
            if was_relocalized
                && !self.support_approach_only
                && same_static_parts(&previous, &reading, displacement)
            {
                approach.provenance = "current_relocalized_two_parts".into();
                return Ok(Some(approach));
            }
        }
        Ok(None)
    }
}
fn same_static_parts(
    previous: &TerminalReading,
    reading: &TerminalReading,
    displacement: Option<[f64; 2]>,
) -> bool {
    displacement.is_some_and(|d| d[0].hypot(d[1]) < 2.0)
        && reading.current_parts.iter().filter(|v| **v).count() >= 2
        && previous.current_parts == reading.current_parts
        && previous
            .evidence_boxes
            .iter()
            .zip(&reading.evidence_boxes)
            .zip(&reading.current_parts)
            .all(|((a, b), present)| {
                !*present
                    || distance(
                        [f64::from(a[0]), f64::from(a[1])],
                        [f64::from(b[0]), f64::from(b[1])],
                    ) < 2.0
            })
}

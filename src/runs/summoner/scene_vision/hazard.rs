//! 현재 위치·착지 후보 주변의 검토한 살아 있는 적 외형. 미검출은 clear가 아니다.
use crate::common::{
    encounter_vision::{RegionObserver, ScanOptions},
    encounters::{Appearance, CandidateCategory, SceneContext, VisualCandidate},
    error::ObservationError,
    frame::{BgrView, PixelRect},
    survival::ObservationStamp,
};
use std::{path::Path, sync::Arc};
#[derive(Clone)]
pub struct ArcaneHazardHandle(crate::common::workers::OwnedObserver<ArcaneHazardObserver>);
impl ArcaneHazardHandle {
    pub fn new(observer: ArcaneHazardObserver) -> Self {
        Self(crate::common::workers::OwnedObserver::new(
            observer, "hazard",
        ))
    }
    pub fn observe(
        &self,
        frame: BgrView<'_>,
        ground: [i32; 2],
        captured_at: f64,
        now: f64,
        aim: Option<[i32; 2]>,
    ) -> Result<Vec<VisualCandidate>, ObservationError> {
        self.0
            .with(|observer| observer.observe(frame, ground, captured_at, now, aim))
    }
    pub fn sequence(&self) -> Result<u64, ObservationError> {
        self.0.with(|observer| Ok(observer.sequence()))
    }
}
pub struct ArcaneHazardObserver {
    region: RegionObserver,
    sequence: u64,
}
impl ArcaneHazardObserver {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ObservationError> {
        let mut region = RegionObserver::load(path)?;
        region.retain_alive_monsters();
        Ok(Self {
            region,
            sequence: 0,
        })
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn observe(
        &mut self,
        frame: BgrView<'_>,
        ground: [i32; 2],
        captured_at: f64,
        now: f64,
        aim: Option<[i32; 2]>,
    ) -> Result<Vec<VisualCandidate>, ObservationError> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| ObservationError::invalid("hazard sequence exhausted"))?;
        let mut boxes = Vec::new();
        for [x, y] in std::iter::once(ground).chain(aim) {
            let [l, t, r, b] = [
                (i64::from(x) - 240).max(24),
                (i64::from(y) - 360).max(55),
                (i64::from(x) + 240).min(frame.size().width() as i64 - 24),
                (i64::from(y) + 120).min(900),
            ];
            if l < 0 || t < 0 || r <= l || b <= t {
                return Err(ObservationError::invalid(
                    "hazard viewport must be inside the native frame",
                ));
            }
            let bounds = PixelRect::new(l as usize, t as usize, r as usize, b as usize)?;
            if !boxes.contains(&bounds) {
                boxes.push(bounds);
            }
        }
        let contexts = boxes
            .iter()
            .map(|viewport| {
                Ok(Arc::new(SceneContext::new(
                    ObservationStamp::new("flash", "supervised-north", self.sequence, captured_at)?,
                    self.region.region_id(),
                    self.sequence,
                    self.region.frame_size(),
                    *viewport,
                )?))
            })
            .collect::<Result<Vec<_>, ObservationError>>()?;
        let options = ScanOptions {
            now,
            max_age: 0.25,
            limit_per_template: 1,
            workers: 2,
        };
        let patches = boxes
            .iter()
            .map(|bounds| frame.crop(*bounds))
            .collect::<Result<Vec<_>, _>>()?;
        let mut groups: Vec<_> = (0..patches.len()).map(|_| None).collect();
        crate::common::compute::scope(|scope| {
            let region = &self.region;
            for ((patch, context), result) in patches.into_iter().zip(contexts).zip(&mut groups) {
                scope.spawn(move |_| *result = Some(region.scan(patch, context, options)));
            }
        })
        .map_err(|error| match error {
            crate::common::compute::ComputeError::Start(error) => ObservationError::Io(error),
            crate::common::compute::ComputeError::Panicked => {
                ObservationError::invalid("hazard observer worker panicked")
            }
        })?;
        let mut candidates = Vec::new();
        for group in groups {
            candidates.extend(
                group
                    .ok_or_else(|| ObservationError::invalid("hazard result missing"))??
                    .into_iter()
                    .filter(|candidate| {
                        candidate.identity().category() == CandidateCategory::Monster
                            && candidate.identity().appearance() == Appearance::Alive
                    }),
            );
        }
        Ok(candidates)
    }
}

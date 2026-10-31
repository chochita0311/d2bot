//! 무기 탭과 자원 최대값 증가 후보. 이 관찰만으로 버프 효과를 확정하지 않는다.
use super::{
    error::ObservationError,
    frame::{BgrFrame, BgrView, PixelRect, Size},
    observation_assets::{AssetOwner, color_distance},
    survival_vision::HudReading,
    vision_ops::codec,
};
use serde::Deserialize;
use std::{collections::HashMap, path::Path};

pub fn resource_increase_candidate(before: &HudReading, after: &HudReading) -> bool {
    [(&before.life, &after.life), (&before.mana, &after.mana)]
        .into_iter()
        .all(|(a, b)| {
            a.ratio().is_some()
                && b.ratio().is_some()
                && a.maximum().zip(b.maximum()).is_some_and(|(a, b)| b > a)
        })
}
#[derive(Deserialize)]
struct Manifest {
    schema_version: u32,
    frame_size: [usize; 2],
    regions: Vec<[usize; 4]>,
    templates: HashMap<String, String>,
    maximum_distance: f64,
    minimum_margin: f64,
}
pub struct WeaponSetObserver {
    regions: [PixelRect; 2],
    templates: [(u8, BgrFrame); 2],
    maximum_distance: f64,
    minimum_margin: f64,
}
impl WeaponSetObserver {
    pub fn load(directory: impl AsRef<Path>) -> Result<Self, ObservationError> {
        let directory = directory.as_ref();
        let raw: Manifest =
            serde_json::from_slice(&std::fs::read(directory.join("manifest.json"))?)?;
        if raw.schema_version != 1 || raw.frame_size != [1922, 1140] {
            return Err(ObservationError::invalid(
                "Reviewed weapon-tab layout required",
            ));
        }
        if [raw.maximum_distance, raw.minimum_margin]
            .into_iter()
            .any(|v| !v.is_finite() || v <= 0.0 || v >= 1.0)
        {
            return Err(ObservationError::invalid(
                "Finite weapon matching ratio required",
            ));
        }
        if raw.regions.len() != 2
            || raw.templates.len() != 2
            || !raw.templates.contains_key("1")
            || !raw.templates.contains_key("2")
        {
            return Err(ObservationError::invalid(
                "Both weapon regions and sets required",
            ));
        }
        let region = |[l, t, r, b]: [usize; 4]| {
            PixelRect::new(l, t, r, b)?.checked_in(Size::new(1922, 1140)?)
        };
        let regions = [region(raw.regions[0])?, region(raw.regions[1])?];
        if regions[0].height() != regions[1].height() {
            return Err(ObservationError::invalid(
                "Equal-height tab regions required",
            ));
        }
        let size = Size::new(regions[0].width() + regions[1].width(), regions[0].height())?;
        let owner = AssetOwner::new(directory)?;
        let load = |key: &str| -> Result<BgrFrame, ObservationError> {
            let image = codec::load_color(
                owner.path(
                    raw.templates
                        .get(key)
                        .ok_or_else(|| ObservationError::invalid("weapon set missing"))?,
                )?,
            )?;
            if image.size() != size {
                return Err(ObservationError::invalid(
                    "Weapon reference dimensions do not match regions",
                ));
            }
            Ok(image)
        };
        Ok(Self {
            regions,
            templates: [(1, load("1")?), (2, load("2")?)],
            maximum_distance: raw.maximum_distance,
            minimum_margin: raw.minimum_margin,
        })
    }
    pub fn observe(&self, frame: BgrView<'_>) -> Result<Option<u8>, ObservationError> {
        if frame.size() != Size::new(1922, 1140)? {
            return Ok(None);
        }
        let mut strip = BgrFrame::zeros(self.templates[0].1.size())?;
        for y in 0..strip.size().height() {
            let mut offset = 0;
            for region in self.regions {
                let piece = frame.crop(region)?;
                let row = piece
                    .row(y)
                    .ok_or_else(|| ObservationError::invalid("missing weapon row"))?;
                let index = (y * strip.size().width() + offset) * 3;
                strip.pixels_mut()[index..index + row.len()].copy_from_slice(row);
                offset += region.width();
            }
        }
        let mut candidates = self
            .templates
            .iter()
            .map(|(key, image)| Ok((color_distance(strip.view(), image.view())?, *key)))
            .collect::<Result<Vec<_>, ObservationError>>()?;
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let (distance, number) = candidates[0];
        Ok(
            (distance <= self.maximum_distance
                && candidates[1].0 - distance >= self.minimum_margin)
                .then_some(number),
        )
    }
}

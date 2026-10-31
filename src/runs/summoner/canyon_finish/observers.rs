use crate::common::{
    error::ObservationError,
    frame::{BgrView, GrayFrame, PixelRect, Size},
    observation_assets::AssetOwner,
    vision_ops::{
        codec, color,
        components::{Connectivity, connected_components},
        morphology::{Kernel, Morphology, morphology},
        template::{self, MatchMethod},
    },
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::Path;
#[derive(Deserialize)]
struct Part {
    path: String,
    sha256: String,
    threshold: f64,
}
pub struct CanyonApproachObserver {
    rule: Part,
    template: GrayFrame,
}
fn reviewed(owner: &AssetOwner, rule: &Part, message: &str) -> Result<GrayFrame, ObservationError> {
    let path = owner.path(&rule.path)?;
    if format!("{:x}", Sha256::digest(std::fs::read(&path)?)) != rule.sha256 {
        return Err(ObservationError::invalid(message));
    }
    Ok(codec::load_gray(path)?)
}
impl CanyonApproachObserver {
    pub fn load(root: impl AsRef<Path>) -> Result<Self, ObservationError> {
        let root = root.as_ref();
        let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(
            root.join("assets/regions/canyon-of-the-magi/remastered-ko-1922x1140/manifest.json"),
        )?)?;
        let rule: Part = serde_json::from_value(manifest["parts"]["waypoint-south-steps"].clone())?;
        let template = reviewed(
            &AssetOwner::new(root)?,
            &rule,
            "reviewed Canyon step fingerprint required",
        )?;
        Ok(Self { rule, template })
    }
    pub fn observe(&self, frame: BgrView<'_>) -> Result<Option<[f64; 2]>, ObservationError> {
        if frame.size() != Size::new(1922, 1140)? {
            return Err(ObservationError::invalid("native Canyon frame required"));
        }
        let patch = color::gray(frame.crop(PixelRect::new(24, 55, 1900, 900)?)?)?;
        let scores = template::match_template(
            patch.view(),
            self.template.view(),
            MatchMethod::NormalizedCoefficient,
        )?;
        Ok(template::maximum(&scores)
            .filter(|peak| f64::from(peak.score) >= self.rule.threshold)
            .map(|peak| {
                [
                    peak.location[0] as f64 + self.template.size().width() as f64 / 2.0 + 24.0,
                    peak.location[1] as f64 + self.template.size().height() as f64 / 2.0 + 55.0,
                ]
            }))
    }
}
#[derive(Deserialize)]
struct PortalRule {
    #[serde(flatten)]
    part: Part,
    red_support_box: [i64; 4],
    minimum_red_pixels: usize,
    dark_core_box: [i64; 4],
    minimum_dark_fraction: f64,
}
pub struct RedPortalObserver {
    rule: PortalRule,
    template: GrayFrame,
}
fn relative_box(origin: [usize; 2], bounds: [i64; 4], size: Size) -> Option<PixelRect> {
    let end = |n: i64, len: usize| {
        if n < 0 {
            (len as i64 + n).clamp(0, len as i64) as usize
        } else {
            n.min(len as i64) as usize
        }
    };
    let [x, y] = origin.map(|v| v as i64);
    let l = (x + bounds[0]).clamp(0, size.width() as i64) as usize;
    let t = (y + bounds[1]).clamp(0, size.height() as i64) as usize;
    let r = end(x + bounds[2], size.width());
    let b = end(y + bounds[3], size.height());
    (l < r && t < b)
        .then(|| PixelRect::new(l, t, r, b).ok())
        .flatten()
}
fn dark_fraction(hsv: crate::common::frame::ImageView<'_, u8, 3>) -> Result<f64, ObservationError> {
    let mut dark = 0usize;
    for y in 0..hsv.size().height() {
        dark += hsv
            .row(y)
            .ok_or_else(|| ObservationError::invalid("HSV row missing"))?
            .as_chunks::<3>()
            .0
            .iter()
            .filter(|p| p[2] < 150)
            .count();
    }
    Ok(dark as f64 / (hsv.size().width() * hsv.size().height()) as f64)
}
impl RedPortalObserver {
    pub fn load(directory: impl AsRef<Path>) -> Result<Self, ObservationError> {
        let directory = directory.as_ref();
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(directory.join("journal-exit.json"))?)?;
        let rule: PortalRule = serde_json::from_value(manifest["portal_hover"].clone())?;
        let template = reviewed(
            &AssetOwner::new(directory)?,
            &rule.part,
            "reviewed portal hover fingerprint required",
        )?;
        Ok(Self { rule, template })
    }
    pub fn observe(&self, frame: BgrView<'_>) -> Result<Option<[i32; 2]>, ObservationError> {
        if frame.size() != Size::new(1922, 1140)? {
            return Err(ObservationError::invalid("native portal frame required"));
        }
        let patch = frame.crop(PixelRect::new(24, 55, 1900, 900)?)?;
        let hsv = color::hsv(patch)?;
        let mut mask = GrayFrame::zeros(patch.size())?;
        for (y, row) in hsv
            .pixels()
            .chunks_exact(patch.size().width() * 3)
            .enumerate()
        {
            for (x, p) in row.as_chunks::<3>().0.iter().enumerate() {
                mask.pixels_mut()[y * patch.size().width() + x] =
                    if (p[0] <= 12 || (170..=179).contains(&p[0])) && p[1] >= 95 && p[2] >= 160 {
                        255
                    } else {
                        0
                    };
            }
        }
        let gray = color::gray(patch)?;
        let mut scores = template::match_template(
            gray.view(),
            self.template.view(),
            MatchMethod::NormalizedCoefficient,
        )?;
        let width = self.template.size().width();
        let height = self.template.size().height();
        let mut labels = vec![];
        for _ in 0..3 {
            let Some(peak) = template::maximum(&scores)
                .filter(|p| f64::from(p.score) >= self.rule.part.threshold)
            else {
                break;
            };
            let [x, y] = peak.location;
            labels.push([x, y]);
            let stride = scores.size().width();
            for row in y.saturating_sub(height)..(y + height + 1).min(scores.size().height()) {
                for col in x.saturating_sub(width)..(x + width + 1).min(stride) {
                    scores.pixels_mut()[row * stride + col] = -1.0;
                }
            }
        }
        if labels.len() > 1 {
            return Ok(None);
        }
        if let Some(&origin) = labels.first() {
            let red = relative_box(origin, self.rule.red_support_box, mask.size())
                .map(|r| mask.view().crop(r))
                .transpose()?;
            let core = relative_box(origin, self.rule.dark_core_box, hsv.size())
                .map(|r| hsv.view().crop(r))
                .transpose()?;
            let red_count = red
                .map(|v| {
                    (0..v.size().height())
                        .filter_map(|y| v.row(y))
                        .flatten()
                        .filter(|v| **v != 0)
                        .count()
                })
                .unwrap_or(0);
            if red_count >= self.rule.minimum_red_pixels
                && core
                    .map(dark_fraction)
                    .transpose()?
                    .is_some_and(|v| v >= self.rule.minimum_dark_fraction)
            {
                return Ok(Some([
                    (origin[0] + width / 2 + 24) as i32,
                    (origin[1] + height / 2 + 55) as i32,
                ]));
            }
        }
        let closed = morphology(
            mask.view(),
            Kernel::Rectangle {
                width: 9,
                height: 9,
            },
            Morphology::Close,
        )?;
        let components = connected_components(closed.view(), Connectivity::Eight)?;
        let mut candidates = vec![];
        for part in components.stats.iter().skip(1) {
            let Some(rect) = part.rect else { continue };
            let w = rect.width();
            let h = rect.height();
            if !(70..=250).contains(&w)
                || !(120..=360).contains(&h)
                || !(0.3..=0.85).contains(&(w as f64 / h as f64))
                || part.area < 900
            {
                continue;
            }
            let core = hsv.view().crop(PixelRect::new(
                rect.left() + w / 3,
                rect.top() + h / 3,
                rect.left() + w * 2 / 3,
                rect.top() + h * 2 / 3,
            )?)?;
            if dark_fraction(core)? >= 0.6 {
                candidates.push([
                    (rect.left() + w / 2 + 24) as i32,
                    (rect.top() + h / 2 + 55) as i32,
                ]);
            }
        }
        Ok(if candidates.len() == 1 {
            Some(candidates[0])
        } else {
            None
        })
    }
}

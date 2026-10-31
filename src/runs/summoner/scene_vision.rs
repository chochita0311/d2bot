//! 현재 프레임의 양성 장면 후보. 미검출만으로 안전을 확정하지 않는다.
mod ground;
mod hazard;
mod hub;
use crate::common::{
    error::ObservationError,
    frame::{BgrView, GrayFrame, PixelRect, Size},
    vision_ops::{
        codec, color,
        components::{Connectivity, connected_components},
        template::{self, MatchMethod},
    },
};
pub use ground::{FlashGroundObserver, GroundOptions, GroundReading, GroundTracking};
pub use hazard::{ArcaneHazardHandle, ArcaneHazardObserver};
pub use hub::{ArcaneHubObserver, HubEvidence, HubPartEvidence, HubStabilityTracker};
use serde::Serialize;
use std::path::Path;
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HostileTarget {
    pub kind: String,
    pub evidence_box: [usize; 4],
    pub title_score: Option<f64>,
}
pub struct ArcaneTargetObserver {
    titles: Vec<(String, GrayFrame)>,
}
impl ArcaneTargetObserver {
    pub fn load(owner: impl AsRef<Path>) -> Result<Self, ObservationError> {
        let mut titles = Vec::new();
        for (kind, path, [l, t, r, b]) in [
            (
                "ghoul-lord",
                "templates/encounters/ghoul-name.png",
                [27, 9, 138, 35],
            ),
            (
                "hell-clan",
                "templates/encounters/hell-clan-name.png",
                [25, 10, 148, 36],
            ),
        ] {
            let image = codec::load_color(owner.as_ref().join(path))?;
            titles.push((
                kind.into(),
                color::gray(image.view().crop(PixelRect::new(l, t, r, b)?)?)?,
            ));
        }
        Ok(Self { titles })
    }
    pub fn observe(&self, frame: BgrView<'_>) -> Result<Option<HostileTarget>, ObservationError> {
        if frame.size() != Size::new(1922, 1140)? {
            return Err(ObservationError::invalid(
                "native reviewed target-panel layout required",
            ));
        }
        let patch = frame.crop(PixelRect::new(700, 55, 1250, 180)?)?;
        let mut red = GrayFrame::zeros(patch.size())?;
        for y in 0..patch.size().height() {
            let row = patch
                .row(y)
                .ok_or_else(|| ObservationError::invalid("target row missing"))?;
            for (x, p) in row.as_chunks::<3>().0.iter().enumerate() {
                let [b, g, r] = p.map(i32::from);
                red.pixels_mut()[y * patch.size().width() + x] =
                    u8::from(r >= 35 && g <= 25 && b <= 35 && r > g * 2 && r * 2 > b * 3);
            }
        }
        let components = connected_components(red.view(), Connectivity::Eight)?;
        let panels: Vec<_> = components
            .stats
            .iter()
            .skip(1)
            .filter_map(|c| c.rect.map(|r| (r, c.area)))
            .filter(|(r, area)| {
                (60..=450).contains(&r.width()) && (20..=60).contains(&r.height()) && *area >= 500
            })
            .collect();
        if panels.len() != 1 {
            return Ok(None);
        }
        let panel = panels[0].0;
        let view = patch.crop(panel)?;
        let mut bright = 0;
        for y in 0..view.size().height() {
            bright += view
                .row(y)
                .ok_or_else(|| ObservationError::invalid("target panel row missing"))?
                .as_chunks::<3>()
                .0
                .iter()
                .filter(|p| p[0].min(p[1]).min(p[2]) >= 150)
                .count();
        }
        if bright < 20 {
            return Ok(None);
        }
        let gray = color::gray(patch)?;
        let mut kind = "hostile-target".to_owned();
        let mut title_score = None;
        for (name, title) in &self.titles {
            let scores = template::match_template(
                gray.view(),
                title.view(),
                MatchMethod::NormalizedCoefficient,
            )?;
            if let Some(peak) = template::maximum(&scores)
                && f64::from(peak.score) >= 0.9
                && panel.left() <= peak.location[0]
                && panel.top() <= peak.location[1]
                && peak.location[0] + title.size().width() <= panel.right()
            {
                kind = name.clone();
                title_score = Some(f64::from(peak.score));
                break;
            }
        }
        Ok(Some(HostileTarget {
            kind,
            evidence_box: [
                700 + panel.left(),
                55 + panel.top(),
                700 + panel.right(),
                55 + panel.bottom(),
            ],
            title_score,
        }))
    }
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HoverCandidate {
    pub kind: String,
    pub evidence_box: [usize; 4],
    pub score: f64,
}
pub struct ArcaneHoverObserver {
    templates: Vec<(String, GrayFrame)>,
}
impl ArcaneHoverObserver {
    pub fn load(assets: impl AsRef<Path>) -> Result<Self, ObservationError> {
        let root = assets.as_ref();
        Self::new([
            ("chest","regions/arcane-sanctuary/remastered-ko-1922x1140/templates/hover/chest-name-ko.png"),
            ("coffin","regions/arcane-sanctuary/remastered-ko-1922x1140/templates/hover/small-coffin-hover-ko.png"),
            ("shrine","ui/world-labels/shrine-name-ko.png"),
            ("teleporter","regions/arcane-sanctuary/remastered-ko-1922x1140/templates/hover/teleporter-hover-ko.png"),
        ].into_iter().map(|(kind,path)|Ok((kind.to_owned(),codec::load_color(root.join(path))?))).collect::<Result<Vec<_>,ObservationError>>()?)
    }
    pub fn new(
        templates: Vec<(String, crate::common::frame::BgrFrame)>,
    ) -> Result<Self, ObservationError> {
        if templates.is_empty() {
            return Err(ObservationError::invalid(
                "positive hover references required",
            ));
        }
        let mut result = Vec::new();
        for (kind, image) in templates {
            if !matches!(kind.as_str(), "chest" | "coffin" | "shrine" | "teleporter") {
                return Err(ObservationError::invalid("Arcane hover excludes town NPCs"));
            }
            result.push((kind, color::gray(image.view())?));
        }
        Ok(Self { templates: result })
    }
    pub fn observe(
        &self,
        frame: BgrView<'_>,
        [x, y]: [i32; 2],
    ) -> Result<Vec<HoverCandidate>, ObservationError> {
        if frame.size() != Size::new(1922, 1140)? {
            return Err(ObservationError::invalid(
                "reviewed native Arcane frame required",
            ));
        }
        if !(24..1900).contains(&x) || !(55..900).contains(&y) {
            return Err(ObservationError::invalid(
                "hover aim must be inside world viewport",
            ));
        }
        let x0 = (x - 240).max(24) as usize;
        let y0 = (y - 350).max(55) as usize;
        let patch = template::GrayMatcher::new(color::gray(frame.crop(PixelRect::new(
            x0,
            y0,
            (x + 240).min(1900) as usize,
            (y + 350).min(900) as usize,
        )?)?)?)?;
        let mut candidates = Vec::new();
        for (kind, image) in &self.templates {
            if image.size().width() > patch.size().width()
                || image.size().height() > patch.size().height()
            {
                continue;
            }
            let scores = patch.match_pattern(image.view(), MatchMethod::NormalizedCoefficient)?;
            if let Some(peak) = template::maximum(&scores)
                && f64::from(peak.score) >= 0.82
            {
                candidates.push(HoverCandidate {
                    kind: kind.clone(),
                    evidence_box: [
                        x0 + peak.location[0],
                        y0 + peak.location[1],
                        x0 + peak.location[0] + image.size().width(),
                        y0 + peak.location[1] + image.size().height(),
                    ],
                    score: f64::from(peak.score),
                });
            }
        }
        Ok(candidates)
    }
}

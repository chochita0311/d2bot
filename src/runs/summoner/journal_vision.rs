//! 현재 일지 외형과 검토한 발판 기준점. 상호작용·처치 허가는 별도다.
use crate::common::{
    error::ObservationError,
    frame::{BgrView, GrayFrame, PixelRect, Size},
    observation_assets::AssetOwner,
    vision_ops::{
        codec, color,
        template::{self, MatchMethod},
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JournalMatch {
    pub score: f64,
    pub r#box: [usize; 4],
    pub platform_ground: Option<[f64; 2]>,
}
#[derive(Deserialize)]
struct Rule {
    path: String,
    sha256: String,
    threshold: f64,
    platform_ground_offset: Option<[f64; 2]>,
}
#[derive(Deserialize)]
struct Manifest {
    journal: Rule,
    #[serde(default)]
    journal_variants: Vec<Rule>,
}
pub struct HorazonJournalObserver {
    parts: Vec<(Rule, GrayFrame)>,
}
impl HorazonJournalObserver {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ObservationError> {
        let path = path.as_ref();
        let owner = AssetOwner::new(
            path.parent()
                .ok_or_else(|| ObservationError::invalid("journal manifest owner required"))?,
        )?;
        let manifest: Manifest = serde_json::from_slice(&std::fs::read(path)?)?;
        let mut parts = Vec::new();
        for rule in std::iter::once(manifest.journal).chain(manifest.journal_variants) {
            let asset = owner.path(&rule.path)?;
            let bytes = std::fs::read(&asset)?;
            if format!("{:x}", Sha256::digest(&bytes)) != rule.sha256 {
                return Err(ObservationError::invalid(
                    "reviewed Horazon journal fingerprint required",
                ));
            }
            if rule
                .platform_ground_offset
                .is_some_and(|p| p.iter().any(|v| !v.is_finite()))
            {
                return Err(ObservationError::invalid(
                    "reviewed journal platform-ground offset required",
                ));
            }
            let gray = codec::load_gray(asset)?;
            parts.push((rule, gray));
        }
        Ok(Self { parts })
    }
    pub fn matches(&self, frame: BgrView<'_>) -> Result<Vec<JournalMatch>, ObservationError> {
        if frame.size() != Size::new(1922, 1140)? {
            return Err(ObservationError::invalid("native journal frame required"));
        }
        let patch = template::GrayMatcher::new(color::gray(
            frame.crop(PixelRect::new(24, 55, 1900, 900)?)?,
        )?)?;
        self.matches_prepared(&patch)
    }
    pub(crate) fn matches_prepared(
        &self,
        patch: &template::GrayMatcher,
    ) -> Result<Vec<JournalMatch>, ObservationError> {
        if patch.size() != Size::new(1876, 845)? {
            return Err(ObservationError::invalid(
                "native journal world crop required",
            ));
        }
        let mut matches = Vec::new();
        for (rule, image) in &self.parts {
            let map = patch.match_pattern(image.view(), MatchMethod::NormalizedCoefficient)?;
            if let Some(peak) =
                template::maximum(&map).filter(|p| f64::from(p.score) >= rule.threshold)
            {
                let [l, t] = [peak.location[0] + 24, peak.location[1] + 55];
                matches.push(JournalMatch {
                    score: f64::from(peak.score),
                    r#box: [l, t, l + image.size().width(), t + image.size().height()],
                    platform_ground: rule
                        .platform_ground_offset
                        .map(|p| [l as f64 + p[0], t as f64 + p[1]]),
                });
            }
        }
        matches.sort_by(|a, b| b.score.total_cmp(&a.score));
        Ok(matches)
    }
    pub fn observe(&self, frame: BgrView<'_>) -> Result<Option<[f64; 2]>, ObservationError> {
        Ok(self.matches(frame)?.first().map(|m| {
            let [l, t, r, b] = m.r#box;
            [(l + r) as f64 / 2.0, (t + b) as f64 / 2.0]
        }))
    }
}

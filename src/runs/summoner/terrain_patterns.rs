//! 검토한 계단 외형의 착지 후보. 안전한 착지 자체를 증명하지 않는다.
use crate::common::{
    error::ObservationError,
    frame::{BgrView, GrayFrame, PixelRect, Size},
    observation_assets::normalize_absolute,
    vision_ops::{
        codec, color, resize,
        template::{self, MatchMethod},
    },
};
use serde::{Deserialize, Serialize};
use std::path::Path;
#[derive(Deserialize)]
struct Manifest {
    schema: u32,
    frame_size: [usize; 2],
    patterns: Vec<Rule>,
}
#[derive(Deserialize)]
struct Rule {
    id: String,
    path: String,
    landing_offset: [i32; 2],
    threshold: f64,
}
struct Pattern {
    rule: Rule,
    native: GrayFrame,
    coarse: GrayFrame,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TerrainLanding {
    pub kind: String,
    pub point: [i32; 2],
    pub score: f64,
    pub evidence_box: [usize; 4],
}
pub struct ArcaneTerrainPatterns {
    templates: Vec<Pattern>,
}
impl ArcaneTerrainPatterns {
    pub fn load(manifest: impl AsRef<Path>) -> Result<Self, ObservationError> {
        let manifest = manifest.as_ref().canonicalize()?;
        let directory = manifest
            .parent()
            .ok_or_else(|| ObservationError::invalid("terrain manifest owner missing"))?;
        let data: Manifest = serde_json::from_slice(&std::fs::read(&manifest)?)?;
        if data.schema != 1 || data.frame_size != [1922, 1140] {
            return Err(ObservationError::invalid(
                "reviewed terrain-pattern layout required",
            ));
        }
        let mut templates = Vec::new();
        for rule in data.patterns {
            let path = normalize_absolute(&directory.join(&rule.path));
            // 상위 경로의 대상 파일이 없어도 먼저 선언한 소유 경계를 검사한다.
            let parent = path
                .parent()
                .ok_or_else(|| ObservationError::invalid("terrain pattern outside owner"))?;
            let allowed = parent == directory || parent == directory.join("templates/navigation");
            if !allowed {
                return Err(ObservationError::invalid("terrain pattern outside owner"));
            }
            let path = path.canonicalize()?;
            if path.parent() != Some(directory)
                && path.parent() != Some(directory.join("templates/navigation").as_path())
            {
                return Err(ObservationError::invalid("terrain pattern outside owner"));
            }
            if !rule.threshold.is_finite() || !(0.9..=1.0).contains(&rule.threshold) {
                return Err(ObservationError::invalid(
                    "positive reviewed terrain pattern required",
                ));
            }
            if rule.landing_offset[0].checked_add(1922).is_none()
                || rule.landing_offset[1].checked_add(1140).is_none()
            {
                return Err(ObservationError::invalid(
                    "terrain landing offset overflows native coordinates",
                ));
            }
            let native = codec::load_gray(path)?;
            let coarse = resize::half(native.view())?;
            templates.push(Pattern {
                rule,
                native,
                coarse,
            });
        }
        Ok(Self { templates })
    }
    pub fn observe(&self, frame: BgrView<'_>) -> Result<Vec<TerrainLanding>, ObservationError> {
        if frame.size() != Size::new(1922, 1140)? {
            return Err(ObservationError::invalid(
                "native reviewed terrain frame required",
            ));
        }
        let gray = color::gray(frame.crop(PixelRect::new(24, 55, 1900, 900)?)?)?;
        let small = resize::half(gray.view())?;
        let mut found = Vec::new();
        for pattern in &self.templates {
            let mut scores = template::match_template(
                small.view(),
                pattern.coarse.view(),
                MatchMethod::NormalizedCoefficient,
            )?;
            for _ in 0..4 {
                let Some(peak) = template::maximum(&scores) else {
                    break;
                };
                if f64::from(peak.score) < 0.9 {
                    break;
                }
                let [x, y] = peak.location;
                let left = 24 + x * 2;
                let top = 55 + y * 2;
                let width = pattern.native.size().width();
                let height = pattern.native.size().height();
                let x0 = (left - 24).saturating_sub(3);
                let y0 = (top - 55).saturating_sub(3);
                let x1 = (left - 24 + width + 4).min(gray.size().width());
                let y1 = (top - 55 + height + 4).min(gray.size().height());
                if x1 - x0 >= width && y1 - y0 >= height {
                    let patch = gray.view().crop(PixelRect::new(x0, y0, x1, y1)?)?;
                    let native_scores = template::match_template(
                        patch,
                        pattern.native.view(),
                        MatchMethod::NormalizedCoefficient,
                    )?;
                    if let Some(native) = template::maximum(&native_scores)
                        && f64::from(native.score) >= pattern.rule.threshold
                    {
                        let left = 24 + x0 + native.location[0];
                        let top = 55 + y0 + native.location[1];
                        found.push(TerrainLanding {
                            kind: pattern.rule.id.clone(),
                            point: [
                                left as i32 + pattern.rule.landing_offset[0],
                                top as i32 + pattern.rule.landing_offset[1],
                            ],
                            score: f64::from(native.score),
                            evidence_box: [left, top, left + width, top + height],
                        });
                    }
                }
                for sy in
                    y.saturating_sub(height / 2)..(y + height / 2 + 1).min(scores.size().height())
                {
                    let w = scores.size().width();
                    scores.pixels_mut()
                        [sy * w + x.saturating_sub(width / 2)..sy * w + (x + width / 2 + 1).min(w)]
                        .fill(-1.0);
                }
            }
        }
        Ok(found)
    }
}

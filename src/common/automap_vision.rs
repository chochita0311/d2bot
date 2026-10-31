//! 현재 지역 제목의 근거. 미검출이나 가림만으로 자동지도 OFF를 확정하지 않는다.
use super::{
    error::ObservationError,
    frame::{BgrView, GrayFrame, PixelRect, Size},
    label_occlusion::label_occlusion_mask,
    vision_ops::{
        codec,
        filters::separable_q8,
        resize::{self, Interpolation},
        template::{self, MatchMethod},
    },
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};

#[derive(Deserialize)]
struct Manifest {
    reference_size: [usize; 2],
    areas: serde_json::Map<String, serde_json::Value>,
}
#[derive(Deserialize)]
struct Definition {
    file: String,
    search: [usize; 4],
    minimum_score: f64,
    #[serde(default)]
    variants: Vec<String>,
    #[serde(default)]
    variant_required_parts: HashMap<String, Vec<[usize; 4]>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TitleEncoding {
    SoftGold,
    BinaryGold,
}
impl TitleEncoding {
    fn index(self) -> usize {
        usize::from(self == Self::BinaryGold)
    }
}
struct Variant {
    filename: String,
    image: GrayFrame,
    encoding: TitleEncoding,
    parts: Vec<PixelRect>,
}
struct Area {
    search: PixelRect,
    minimum_score: f64,
    variants: Vec<Variant>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AreaTitleEvidence {
    pub area: String,
    pub score: f64,
    pub title_at: [usize; 2],
    pub template: String,
    pub encoding: TitleEncoding,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub part_scores: Vec<f64>,
}
pub struct AutomapObserver {
    size: Size,
    areas: HashMap<String, Area>,
    last_evidence: Option<AreaTitleEvidence>,
    last_obscured: bool,
}
impl AutomapObserver {
    pub fn load(directory: impl AsRef<Path>) -> Result<Self, ObservationError> {
        let directory = directory.as_ref();
        let manifest: Manifest =
            serde_json::from_slice(&std::fs::read(directory.join("manifest.json"))?)?;
        let size = Size::new(manifest.reference_size[0], manifest.reference_size[1])?;
        let mut areas = HashMap::new();
        for (name, value) in manifest.areas {
            let definition: Definition = serde_json::from_value(value)?;
            let search = rect(definition.search)?.checked_in(size)?;
            if !definition.minimum_score.is_finite()
                || !(-1.0..=1.0).contains(&definition.minimum_score)
            {
                return Err(ObservationError::invalid(
                    "finite automap score between -1 and 1 required",
                ));
            }
            let mut variants = Vec::new();
            for filename in std::iter::once(&definition.file).chain(&definition.variants) {
                let image = codec::load_color(directory.join(filename)).map_err(|e| {
                    ObservationError::invalid(format!(
                        "Unreadable automap title: {name}: {filename}: {e}"
                    ))
                })?;
                if image.size().width() > search.width() || image.size().height() > search.height()
                {
                    return Err(ObservationError::invalid(
                        "automap template must fit its search region",
                    ));
                }
                let parts = definition
                    .variant_required_parts
                    .get(filename)
                    .into_iter()
                    .flatten()
                    .map(|p| rect(*p)?.checked_in(image.size()))
                    .collect::<Result<Vec<_>, _>>()?;
                for encoding in [TitleEncoding::SoftGold, TitleEncoding::BinaryGold] {
                    variants.push(Variant {
                        filename: filename.clone(),
                        image: title_mask(image.view(), encoding)?,
                        encoding,
                        parts: parts.clone(),
                    });
                }
            }
            areas.insert(
                name,
                Area {
                    search,
                    minimum_score: definition.minimum_score,
                    variants,
                },
            );
        }
        Ok(Self {
            size,
            areas,
            last_evidence: None,
            last_obscured: false,
        })
    }
    pub fn last_evidence(&self) -> Option<&AreaTitleEvidence> {
        self.last_evidence.as_ref()
    }
    pub fn last_obscured(&self) -> bool {
        self.last_obscured
    }
    pub fn visible(&mut self, frame: BgrView<'_>, name: &str) -> Result<bool, ObservationError> {
        self.last_evidence = None;
        self.last_obscured = false;
        if (frame.size().width() as f64
            / frame.size().height() as f64
            / (self.size.width() as f64 / self.size.height() as f64)
            - 1.0)
            .abs()
            > 0.03
        {
            return Ok(false);
        }
        let area = self
            .areas
            .get(name)
            .ok_or_else(|| ObservationError::invalid(format!("unknown automap area: {name}")))?;
        let normalized = resize::resize(frame, self.size, Interpolation::Area)?;
        let region = normalized.view().crop(area.search)?;
        let patches = [
            title_mask(region, TitleEncoding::SoftGold)?,
            title_mask(region, TitleEncoding::BinaryGold)?,
        ];
        let mut score = -1.0;
        let mut best = None;
        for variant in &area.variants {
            let patch = &patches[variant.encoding.index()];
            let scores = template::match_template(
                patch.view(),
                variant.image.view(),
                MatchMethod::NormalizedCoefficient,
            )?;
            let Some(peak) = template::maximum(&scores) else {
                continue;
            };
            let candidate_score = f64::from(peak.score);
            let mut part_scores = Vec::new();
            if candidate_score >= area.minimum_score {
                for part in &variant.parts {
                    let [x, y] = peak.location;
                    let positioned = PixelRect::new(
                        x + part.left(),
                        y + part.top(),
                        x + part.right(),
                        y + part.bottom(),
                    )?;
                    let scores = template::match_template(
                        patch.view().crop(positioned)?,
                        variant.image.view().crop(*part)?,
                        MatchMethod::NormalizedCoefficient,
                    )?;
                    part_scores.push(f64::from(scores.pixels()[0]));
                }
                if part_scores.iter().any(|v| *v < area.minimum_score) {
                    continue;
                }
            }
            if candidate_score > score {
                score = candidate_score;
                best = Some(AreaTitleEvidence {
                    area: name.to_owned(),
                    score,
                    title_at: [
                        area.search.left() + peak.location[0],
                        area.search.top() + peak.location[1],
                    ],
                    template: variant.filename.clone(),
                    encoding: variant.encoding,
                    part_scores,
                });
            }
        }
        if score < area.minimum_score {
            let mask = label_occlusion_mask(normalized.view(), false)?;
            let search = mask.view().crop(area.search)?;
            self.last_obscured = (0..area.search.height())
                .any(|y| search.row(y).is_some_and(|row| row.iter().any(|v| *v != 0)));
            return Ok(false);
        }
        self.last_evidence = best;
        Ok(true)
    }
}
fn rect([x0, y0, x1, y1]: [usize; 4]) -> Result<PixelRect, super::frame::FrameError> {
    PixelRect::new(x0, y0, x1, y1)
}
fn title_mask(frame: BgrView<'_>, encoding: TitleEncoding) -> Result<GrayFrame, ObservationError> {
    let size = frame.size();
    let mut mask = GrayFrame::zeros(size)?;
    for y in 0..size.height() {
        let row = frame
            .row(y)
            .ok_or_else(|| ObservationError::invalid("missing title row"))?;
        for (x, p) in row.as_chunks::<3>().0.iter().enumerate() {
            let [b, g, r] = p.map(i32::from);
            mask.pixels_mut()[y * size.width() + x] = match encoding {
                TitleEncoding::BinaryGold => {
                    if r > 70 && g > 45 && r - g > 8 && g - b > 12 && r - b > 35 {
                        255
                    } else {
                        0
                    }
                }
                TitleEncoding::SoftGold => ((r - g).min(g - b) * 5).clamp(0, 255) as u8,
            };
        }
    }
    Ok(separable_q8(mask.view(), &[61, 134, 61])?)
}

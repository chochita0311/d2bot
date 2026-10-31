//! Rule-ordered, one-maximum-per-template matching used by the legacy CLI.

use super::frame::BgrView;
use super::vision_ops::{
    VisionError,
    codec::{self, DecodedImage},
    template::{self, MatchMethod},
};
use super::{frame::BgrFrame, vision_ops::drawing};
use std::path::PathBuf;

pub fn draw_overlay(
    frame: &BgrFrame,
    matches: &[TemplateMatch],
    status: &str,
) -> Result<BgrFrame, VisionError> {
    let mut output = frame.clone();
    let point = |p: [usize; 2]| -> Result<[i32; 2], VisionError> {
        Ok([
            i32::try_from(p[0]).map_err(|_| VisionError::Invalid("overlay x exceeds range"))?,
            i32::try_from(p[1]).map_err(|_| VisionError::Invalid("overlay y exceeds range"))?,
        ])
    };
    for found in matches {
        let first = point(found.top_left)?;
        drawing::rectangle(&mut output, first, point(found.bottom_right)?, [0, 255, 0])?;
        drawing::text(
            &mut output,
            &format!("{} {:.2}", found.name, found.confidence),
            [first[0], 20.max(first[1] - 8)],
            0.6,
            [0, 255, 0],
        )?;
    }
    drawing::text(&mut output, status, [20, 30], 0.8, [0, 215, 255])?;
    Ok(output)
}

#[derive(Debug, Clone)]
pub struct TemplateRule {
    pub name: String,
    pub path: PathBuf,
    pub threshold: f64,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TemplateMatch {
    pub name: String,
    pub confidence: f64,
    pub top_left: [usize; 2],
    pub bottom_right: [usize; 2],
    pub action: String,
}

pub struct TemplateMatcher {
    templates: Vec<(TemplateRule, DecodedImage)>,
}
impl TemplateMatcher {
    pub fn load(rules: impl IntoIterator<Item = TemplateRule>) -> Result<Self, VisionError> {
        let mut templates = Vec::new();
        for rule in rules {
            let image = codec::load_unchanged(&rule.path)?;
            templates.push((rule, image));
        }
        Ok(Self { templates })
    }

    pub fn scan(&self, frame: BgrView<'_>) -> Result<Vec<TemplateMatch>, VisionError> {
        let mut matches = Vec::new();
        for (rule, image) in &self.templates {
            let DecodedImage::Bgr(image) = image else {
                return Err(VisionError::Invalid(
                    "frame and template channel/depth modes differ",
                ));
            };
            let scores =
                template::match_template(frame, image.view(), MatchMethod::NormalizedCoefficient)?;
            let Some(peak) = template::maximum(&scores) else {
                continue;
            };
            if f64::from(peak.score) < rule.threshold {
                continue;
            }
            matches.push(TemplateMatch {
                name: rule.name.clone(),
                confidence: f64::from(peak.score),
                top_left: peak.location,
                bottom_right: [
                    peak.location[0] + image.size().width(),
                    peak.location[1] + image.size().height(),
                ],
                action: rule.action.clone(),
            });
        }
        Ok(matches)
    }
}

use super::{FrameCounts, GEM_NAMES, LEVEL_NAMES};
use crate::{
    actions::{ActionError, MatchResult, locate},
    common::{
        frame::{BgrFrame, BgrView, GrayFrame, GrayView, PixelRect, Size},
        vision_ops::{
            codec, color, filters,
            morphology::{self, Kernel, Morphology},
            resize::{self, Interpolation},
            template::{self, MatchMethod},
        },
    },
};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const PANEL_SIZE: [usize; 2] = [576, 757];
const VARIANTS: [&str; 6] = [
    "thr_110", "thr_120", "thr_130", "thr_140", "thr_150", "otsu",
];
#[derive(Deserialize)]
struct AtlasManifest {
    entries: Vec<AtlasEntry>,
}
#[derive(Deserialize)]
struct AtlasEntry {
    variant: String,
    bank: String,
    digit: Option<String>,
    #[serde(rename = "box")]
    bounds: [usize; 4],
}
struct DigitGlyph {
    digit: char,
    glyphs: Vec<GrayFrame>,
}
#[derive(Default)]
struct VariantBank {
    ones: Vec<DigitGlyph>,
    tens: Vec<DigitGlyph>,
    blank: Vec<GrayFrame>,
}
#[derive(Debug, Clone, Copy, Serialize)]
pub struct CountReading {
    pub value: u32,
    pub confidence: f64,
}
pub struct GemVision {
    header: BgrFrame,
    buttons: [BgrFrame; 2],
    icons: Vec<BgrFrame>,
    banks: Vec<VariantBank>,
}
fn crop<const C: usize>(
    image: crate::common::frame::ImageView<'_, u8, C>,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> Result<crate::common::frame::ImageView<'_, u8, C>, ActionError> {
    let right = x
        .checked_add(width)
        .ok_or_else(|| ActionError::runtime("crop extent overflow"))?;
    let bottom = y
        .checked_add(height)
        .ok_or_else(|| ActionError::runtime("crop extent overflow"))?;
    Ok(image.crop(PixelRect::new(x, y, right, bottom)?)?)
}
fn load(root: &Path, path: &str) -> Result<BgrFrame, ActionError> {
    let resolved = root.join(path);
    if !resolved.exists() {
        return Err(ActionError::runtime(format!(
            "Required asset is missing: {path}"
        )));
    }
    codec::load_color(resolved)
        .map_err(|_| ActionError::runtime(format!("Failed to load asset: {path}")))
}
impl GemVision {
    pub fn load(root: impl AsRef<Path>) -> Result<Self, ActionError> {
        let root = root.as_ref();
        let header = load(root, "assets/ui/stash/templates/header-ko-576x74.png")?;
        let buttons = [
            load(
                root,
                "assets/ui/horadric-cube/templates/transmute-button-hover-ko.png",
            )?,
            load(
                root,
                "assets/ui/horadric-cube/templates/transmute-button.png",
            )?,
        ];
        let mut icons = Vec::new();
        for gem in GEM_NAMES {
            for level in LEVEL_NAMES {
                icons.push(load(
                    root,
                    &format!("assets/items/gems/icons/{level}-{gem}.png"),
                )?);
            }
        }
        let atlas = color::gray(load(root, "assets/ui/gem-count-digits/atlas.png")?.view())?;
        let manifest =
            std::fs::read_to_string(root.join("assets/ui/gem-count-digits/manifest.json"))?;
        let banks = Self::parse_atlas(atlas.view(), &manifest)?;
        Ok(Self {
            header,
            buttons,
            icons,
            banks,
        })
    }
    fn parse_atlas(atlas: GrayView<'_>, manifest: &str) -> Result<Vec<VariantBank>, ActionError> {
        let raw: serde_json::Value = serde_json::from_str(manifest)?;
        if raw.get("schema") != Some(&serde_json::json!(1))
            || raw.get("tile_size") != Some(&serde_json::json!([12, 14]))
            || raw.get("preprocess_variants")
                != Some(&serde_json::json!([
                    ["thr_110", 110],
                    ["thr_120", 120],
                    ["thr_130", 130],
                    ["thr_140", 140],
                    ["thr_150", 150],
                    ["otsu", null]
                ]))
        {
            return Err(ActionError::runtime(
                "Gem digit atlas manifest does not match the configured recognition contract.",
            ));
        }
        let manifest: AtlasManifest = serde_json::from_value(raw)?;
        let mut banks = (0..6).map(|_| VariantBank::default()).collect::<Vec<_>>();
        for entry in manifest.entries {
            let index = VARIANTS
                .iter()
                .position(|v| *v == entry.variant)
                .ok_or_else(|| {
                    ActionError::runtime("Gem digit atlas contains an unknown recognition bank.")
                })?;
            if !["ones", "tens", "blank"].contains(&entry.bank.as_str()) {
                return Err(ActionError::runtime(
                    "Gem digit atlas contains an unknown recognition bank.",
                ));
            }
            let [x, y, width, height] = entry.bounds;
            if width != 12 || height != 14 {
                return Err(ActionError::runtime(
                    "Gem digit atlas contains an invalid glyph box.",
                ));
            }
            let glyph = crop(atlas, x, y, width, height)
                .map_err(|_| {
                    ActionError::runtime("Gem digit atlas contains an invalid glyph box.")
                })?
                .to_owned()?;
            if entry.bank == "blank" {
                if entry.digit.is_some() {
                    return Err(ActionError::runtime(
                        "Gem digit atlas blank glyph must not have a digit label.",
                    ));
                }
                banks[index].blank.push(glyph);
            } else {
                let label = entry.digit.ok_or_else(|| {
                    ActionError::runtime("Gem digit atlas contains an invalid digit label.")
                })?;
                if label.len() != 1 || !label.as_bytes()[0].is_ascii_digit() {
                    return Err(ActionError::runtime(
                        "Gem digit atlas contains an invalid digit label.",
                    ));
                }
                let digit = label.as_bytes()[0] as char;
                let bank = if entry.bank == "ones" {
                    &mut banks[index].ones
                } else {
                    &mut banks[index].tens
                };
                if let Some(entry) = bank.iter_mut().find(|g| g.digit == digit) {
                    entry.glyphs.push(glyph);
                } else {
                    bank.push(DigitGlyph {
                        digit,
                        glyphs: vec![glyph],
                    });
                }
            }
        }
        if banks
            .iter()
            .any(|b| b.ones.is_empty() || b.tens.is_empty() || b.blank.is_empty())
        {
            return Err(ActionError::runtime(
                "Gem digit atlas is missing a required recognition bank.",
            ));
        }
        Ok(banks)
    }
    pub fn locate_stash_panel(
        &self,
        frame: BgrView<'_>,
    ) -> Result<Option<MatchResult>, ActionError> {
        let [width, height] = PANEL_SIZE;
        if frame.size().width() < width || frame.size().height() < height {
            return Ok(None);
        }
        let Some(mut header) = locate(frame, self.header.view(), 0.82)? else {
            return Ok(None);
        };
        if header.top_left[0] + width > frame.size().width()
            || header.top_left[1] + height > frame.size().height()
        {
            return Ok(None);
        }
        header.width = width;
        header.height = height;
        Ok(Some(header))
    }
    pub fn locate_cube_button(
        &self,
        frame: BgrView<'_>,
    ) -> Result<Option<MatchResult>, ActionError> {
        let mut best: Option<MatchResult> = None;
        for button in &self.buttons {
            if let Some(found) = locate(frame, button.view(), 0.78)?
                && best.is_none_or(|best| found.score > best.score)
            {
                best = Some(found);
            }
        }
        Ok(best)
    }
    pub fn read_frame_counts(
        &self,
        frame: BgrView<'_>,
        panel: MatchResult,
    ) -> Result<FrameCounts, ActionError> {
        let stash = crop(
            frame,
            panel.top_left[0],
            panel.top_left[1],
            panel.width,
            panel.height,
        )?;
        let mut output = FrameCounts {
            counts: [[0; 7]; 5],
            confidences: [[0.0; 7]; 5],
            icon_scores: [[0.0; 7]; 5],
        };
        for row in 0..5 {
            for col in 0..7 {
                let slot = crop(stash, 82 + col * 61, 117 + row * 50, 46, 46)?;
                let icon = Self::icon_crop(slot)?;
                output.icon_scores[row][col] = self.slot_icon_score(row, col, icon.view())?;
                let count = crop(stash, 100 + col * 61, 145 + row * 50, 28, 18)?;
                let reading = self.read_count(count)?;
                output.counts[row][col] = reading.value;
                output.confidences[row][col] = reading.confidence;
            }
        }
        Ok(output)
    }
    pub fn read_count(&self, source: BgrView<'_>) -> Result<CountReading, ActionError> {
        if source.size() != Size::new(28, 18)? {
            return Err(ActionError::runtime("28x18 gem count crop required"));
        }
        let variants = preprocess(source)?;
        let ones = self.classify(&variants, false)?;
        let tens = self.classify(&variants, true)?;
        let mut blank = 0.0;
        for (image, bank) in variants.iter().zip(&self.banks) {
            let left = resize::resize(
                crop(image.view(), 0, 0, 14, 18)?,
                Size::new(12, 14)?,
                Interpolation::Nearest,
            )?;
            let best = bank
                .blank
                .iter()
                .map(|glyph| glyph_similarity(left.view(), glyph.view()))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .fold(f64::NEG_INFINITY, f64::max);
            blank += best;
        }
        let (ones_digit, ones_score) = best_digit(&ones)?;
        let (tens_digit, tens_score) = best_digit(&tens)?;
        let tens_present = tens_score > blank + 0.12 && tens_score - second_score(&tens) >= 0.02;
        let value = ones_digit
            .to_digit(10)
            .ok_or_else(|| ActionError::runtime("ones digit unavailable"))?
            + if tens_present {
                tens_digit
                    .to_digit(10)
                    .ok_or_else(|| ActionError::runtime("tens digit unavailable"))?
                    * 10
            } else {
                0
            };
        let ones_margin = ones_score - second_score(&ones);
        let left_margin = if tens_present {
            tens_score - blank.max(second_score(&tens))
        } else {
            blank - tens_score
        };
        Ok(CountReading {
            value,
            confidence: (ones_margin + left_margin).max(0.01),
        })
    }
    fn classify(
        &self,
        variants: &[GrayFrame],
        tens: bool,
    ) -> Result<Vec<(char, f64)>, ActionError> {
        let mut scores: Vec<(char, f64)> = Vec::new();
        for (variant, bank) in variants.iter().zip(&self.banks) {
            let normalized = resize::resize(
                crop(variant.view(), if tens { 0 } else { 14 }, 0, 14, 18)?,
                Size::new(12, 14)?,
                Interpolation::Nearest,
            )?;
            let bank = if tens { &bank.tens } else { &bank.ones };
            for digit in bank {
                let mut best = f64::NEG_INFINITY;
                for glyph in &digit.glyphs {
                    best = best.max(glyph_similarity(normalized.view(), glyph.view())?);
                }
                if let Some(score) = scores.iter_mut().find(|s| s.0 == digit.digit) {
                    score.1 += best;
                } else {
                    scores.push((digit.digit, best));
                }
            }
        }
        Ok(scores)
    }
    pub fn icon_crop(cell: BgrView<'_>) -> Result<BgrFrame, ActionError> {
        if cell.size().width() <= 12 || cell.size().height() <= 12 {
            return Err(ActionError::runtime("gem icon cell is too small"));
        }
        Ok(resize::resize(
            crop(
                cell,
                6,
                6,
                cell.size().width() - 12,
                cell.size().height() - 12,
            )?,
            Size::new(28, 28)?,
            Interpolation::Area,
        )?)
    }
    pub fn slot_icon_score(
        &self,
        row: usize,
        col: usize,
        icon: BgrView<'_>,
    ) -> Result<f64, ActionError> {
        if row >= 5 || col >= 7 {
            return Err(ActionError::runtime("gem slot outside stash"));
        }
        icon_similarity(icon, self.icons[col * 5 + row].view())
    }
    pub fn expected_result(
        &self,
        col: usize,
        level: usize,
        icon: BgrView<'_>,
    ) -> Result<bool, ActionError> {
        let expected = self.slot_icon_score(level, col, icon)?;
        let threshold = if level == 0 {
            0.56
        } else {
            0.56f64.max(self.slot_icon_score(level - 1, col, icon)? + 0.02)
        };
        Ok(expected >= threshold)
    }
    pub fn occupied_cube_cells(
        &self,
        frame: BgrView<'_>,
        panel: MatchResult,
    ) -> Result<Vec<([usize; 2], f64)>, ActionError> {
        let stash = crop(
            frame,
            panel.top_left[0],
            panel.top_left[1],
            panel.width,
            panel.height,
        )?;
        let mut occupied = Vec::new();
        for row in 0..4 {
            for col in 0..3 {
                let x = 216 + col * 49;
                let y = 396 + row * 49;
                let gray = color::gray(crop(stash, x, y, 46, 45)?)?;
                let mean = gray.pixels().iter().map(|n| f64::from(*n)).sum::<f64>()
                    / gray.pixels().len() as f64;
                let variance = gray
                    .pixels()
                    .iter()
                    .map(|n| (f64::from(*n) - mean).powi(2))
                    .sum::<f64>()
                    / gray.pixels().len() as f64;
                let score = mean + variance.sqrt();
                if score >= 18.0 {
                    occupied.push((
                        [panel.top_left[0] + x + 23, panel.top_left[1] + y + 22],
                        score,
                    ));
                }
            }
        }
        occupied.sort_by(|a, b| b.1.total_cmp(&a.1));
        Ok(occupied)
    }
    pub fn cube_result_icon(
        &self,
        frame: BgrView<'_>,
        panel: MatchResult,
        frame_point: [i64; 2],
    ) -> Result<BgrFrame, ActionError> {
        let x = frame_point[0] - panel.top_left[0] as i64 - 23;
        let y = frame_point[1] - panel.top_left[1] as i64 - 22;
        if x < 0 || y < 0 {
            return Err(ActionError::runtime("cube result outside stash panel"));
        }
        let stash = crop(
            frame,
            panel.top_left[0],
            panel.top_left[1],
            panel.width,
            panel.height,
        )?;
        Self::icon_crop(crop(stash, x as usize, y as usize, 46, 45)?)
    }
}
fn icon_similarity(icon: BgrView<'_>, expected: BgrView<'_>) -> Result<f64, ActionError> {
    let expected = resize::resize(expected, icon.size(), Interpolation::Area)?;
    let a = color::gray(icon)?;
    let b = color::gray(expected.view())?;
    let scores = template::match_template(a.view(), b.view(), MatchMethod::NormalizedCoefficient)?;
    Ok(f64::from(*scores.pixels().first().ok_or_else(|| {
        ActionError::runtime("gem icon correlation unavailable")
    })?))
}
fn best_digit(scores: &[(char, f64)]) -> Result<(char, f64), ActionError> {
    let mut best = None;
    for score in scores {
        if best.is_none_or(|(_, best)| score.1 > best) {
            best = Some(*score);
        }
    }
    best.ok_or_else(|| ActionError::runtime("gem digit bank is empty"))
}
fn second_score(scores: &[(char, f64)]) -> f64 {
    if scores.len() < 2 {
        return -1.0;
    }
    let mut values = scores.iter().map(|s| s.1).collect::<Vec<_>>();
    values.sort_by(|a, b| b.total_cmp(a));
    values[1]
}
fn glyph_similarity(a: GrayView<'_>, b: GrayView<'_>) -> Result<f64, ActionError> {
    if a.size() != b.size() {
        return Err(ActionError::runtime("gem glyph sizes differ"));
    }
    let mut sum = 0.0f32;
    for y in 0..a.size().height() {
        let left = a
            .row(y)
            .ok_or_else(|| ActionError::runtime("glyph row missing"))?;
        let right = b
            .row(y)
            .ok_or_else(|| ActionError::runtime("glyph row missing"))?;
        for (a, b) in left.iter().zip(right) {
            sum += f32::from(a.abs_diff(*b));
        }
    }
    Ok(1.0 - f64::from(sum / (a.size().width() * a.size().height()) as f32) / 255.0)
}
fn preprocess(source: BgrView<'_>) -> Result<Vec<GrayFrame>, ActionError> {
    let gray = color::gray(source)?;
    let size = gray.size();
    let mut blurred = GrayFrame::zeros(size)?;
    let reflect = |i: isize, n: usize| {
        if i < 0 {
            (-i) as usize
        } else if i >= n as isize {
            (2 * n as isize - i - 2) as usize
        } else {
            i as usize
        }
    };
    for y in 0..size.height() {
        for x in 0..size.width() {
            let mut sum = 0u32;
            for (dy, wy) in [(-1, 1), (0, 2), (1, 1)] {
                for (dx, wx) in [(-1, 1), (0, 2), (1, 1)] {
                    let sy = reflect(y as isize + dy, size.height());
                    let sx = reflect(x as isize + dx, size.width());
                    sum += u32::from(gray.pixels()[sy * size.width() + sx]) * wy * wx;
                }
            }
            blurred.pixels_mut()[y * size.width() + x] = ((sum + 8) >> 4) as u8;
        }
    }
    let mut output = Vec::new();
    for threshold in [
        Some(110.0),
        Some(120.0),
        Some(130.0),
        Some(140.0),
        Some(150.0),
        None,
    ] {
        let thresholded = if let Some(threshold) = threshold {
            filters::threshold(blurred.view(), threshold)?
        } else {
            filters::otsu(blurred.view())?.1
        };
        output.push(morphology::morphology(
            thresholded.view(),
            Kernel::Rectangle {
                width: 2,
                height: 2,
            },
            Morphology::Close,
        )?);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sha2::{Digest, Sha256};
    #[test]
    fn atlas_glyph_order_and_exact_pixels_match_baseline() -> Result<(), Box<dyn std::error::Error>>
    {
        let vision = GemVision::load(env!("CARGO_MANIFEST_DIR"))?;
        let hash = |image: &GrayFrame| format!("{:x}", Sha256::digest(image.pixels()));
        let actual=vision.banks.iter().enumerate().map(|(index,bank)| {
            let digits=|bank:&[DigitGlyph]|bank.iter().map(|glyph|json!({"digit":glyph.digit.to_string(),"hashes":glyph.glyphs.iter().map(&hash).collect::<Vec<_>>()})).collect::<Vec<_>>();
            json!({"variant":VARIANTS[index],"ones":digits(&bank.ones),"tens":digits(&bank.tens),"blank":bank.blank.iter().map(&hash).collect::<Vec<_>>()})
        }).collect::<Vec<_>>();
        let expected =
            crate::test_support::legacy_expectation("action-oracle", &json!({"kind":"gem_atlas"}))?;
        assert_eq!(json!(actual), expected);
        Ok(())
    }
    #[test]
    fn invalid_atlas_contracts_fail_before_indexing_glyphs() -> Result<(), ActionError> {
        let atlas = GrayFrame::zeros(Size::new(12, 14)?)?;
        let invalid = json!({"schema":2,"tile_size":[12,14],"entries":[]}).to_string();
        let error = GemVision::parse_atlas(atlas.view(), &invalid)
            .err()
            .ok_or_else(|| ActionError::runtime("invalid atlas accepted"))?;
        assert!(error.to_string().contains("Gem digit atlas manifest"));
        let mut manifest = json!({"schema":1,"tile_size":[12,14],"preprocess_variants":[["thr_110",110],["thr_120",120],["thr_130",130],["thr_140",140],["thr_150",150],["otsu",null]],"entries":[]});
        for (entry, message) in [
            (
                json!({"variant":"wrong","bank":"ones","digit":"1","box":[0,0,12,14]}),
                "unknown recognition bank",
            ),
            (
                json!({"variant":"thr_110","bank":"ones","digit":"1","box":[1,0,12,14]}),
                "invalid glyph box",
            ),
            (
                json!({"variant":"thr_110","bank":"blank","digit":"1","box":[0,0,12,14]}),
                "blank glyph",
            ),
            (
                json!({"variant":"thr_110","bank":"ones","digit":"","box":[0,0,12,14]}),
                "invalid digit label",
            ),
        ] {
            manifest["entries"] = json!([entry]);
            let error = GemVision::parse_atlas(atlas.view(), &manifest.to_string())
                .err()
                .ok_or_else(|| ActionError::runtime("invalid atlas accepted"))?;
            assert!(error.to_string().contains(message), "{error}");
        }
        Ok(())
    }
}

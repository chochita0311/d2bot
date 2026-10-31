use super::{ResourceReading, manifest::Matching};
use crate::common::{
    error::ObservationError,
    frame::{BgrView, GrayFrame, GrayView, PixelRect, Size},
    vision_ops::{
        morphology::{Kernel, Morphology, morphology},
        resize::{self, Interpolation},
    },
};

pub(super) fn text_mask(image: BgrView<'_>) -> Result<GrayFrame, ObservationError> {
    let mut mask = GrayFrame::zeros(image.size())?;
    for y in 0..image.size().height() {
        let row = image
            .row(y)
            .ok_or_else(|| ObservationError::invalid("missing number row"))?;
        for (x, p) in row.as_chunks::<3>().0.iter().enumerate() {
            let minimum = p[0].min(p[1]).min(p[2]);
            let maximum = p[0].max(p[1]).max(p[2]);
            mask.pixels_mut()[y * image.size().width() + x] =
                u8::from(minimum > 155 && maximum - minimum < 85);
        }
    }
    Ok(mask)
}
pub(super) fn mask_score(a: GrayView<'_>, b: GrayView<'_>) -> Result<f64, ObservationError> {
    if a.size() != b.size() {
        return Err(ObservationError::invalid("equal mask dimensions required"));
    }
    let (mut intersection, mut union) = (0_u64, 0_u64);
    for y in 0..a.size().height() {
        let left = a
            .row(y)
            .ok_or_else(|| ObservationError::invalid("missing mask row"))?;
        let right = b
            .row(y)
            .ok_or_else(|| ObservationError::invalid("missing mask row"))?;
        for (a, b) in left.iter().zip(right) {
            intersection += u64::from(a & b != 0);
            union += u64::from(a | b != 0);
        }
    }
    Ok(if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    })
}
fn columns(mask: GrayView<'_>) -> Result<Vec<usize>, ObservationError> {
    let mut occupied = vec![false; mask.size().width()];
    for y in 0..mask.size().height() {
        for (x, v) in mask
            .row(y)
            .ok_or_else(|| ObservationError::invalid("missing mask row"))?
            .iter()
            .enumerate()
        {
            occupied[x] |= *v != 0;
        }
    }
    Ok(occupied
        .into_iter()
        .enumerate()
        .filter_map(|(x, v)| v.then_some(x))
        .collect())
}
fn glyph_parts(mask: GrayView<'_>) -> Result<Vec<PixelRect>, ObservationError> {
    let occupied = columns(mask)?;
    let mut runs = Vec::new();
    for x in occupied {
        match runs.last_mut() {
            Some((_, right)) if *right == x => {
                *right = x + 1;
            }
            _ => runs.push((x, x + 1)),
        }
    }
    let mut result = Vec::new();
    for (left, right) in runs {
        let (mut top, mut bottom) = (mask.size().height(), 0);
        for y in 0..mask.size().height() {
            if mask
                .row(y)
                .ok_or_else(|| ObservationError::invalid("missing glyph row"))?[left..right]
                .iter()
                .any(|v| *v != 0)
            {
                top = top.min(y);
                bottom = y + 1;
            }
        }
        result.push(PixelRect::new(left, top, right, bottom)?);
    }
    Ok(result)
}
fn normalize(mask: GrayView<'_>) -> Result<GrayFrame, ObservationError> {
    let scale = (28.0 / mask.size().height() as f64).min(36.0 / mask.size().width() as f64);
    let width = (mask.size().width() as f64 * scale)
        .round_ties_even()
        .max(1.0) as usize;
    let height = (mask.size().height() as f64 * scale)
        .round_ties_even()
        .max(1.0) as usize;
    let resized = resize::resize(mask, Size::new(width, height)?, Interpolation::Nearest)?;
    let mut result = GrayFrame::zeros(Size::new(40, 32)?)?;
    let top = (32 - height) / 2;
    let left = (40 - width) / 2;
    for y in 0..height {
        result.pixels_mut()[(top + y) * 40 + left..(top + y) * 40 + left + width].copy_from_slice(
            resized
                .view()
                .row(y)
                .ok_or_else(|| ObservationError::invalid("missing resized glyph row"))?,
        );
    }
    Ok(result)
}
type GlyphMatch = (f64, char, f64);
fn compare(a: &GlyphMatch, b: &GlyphMatch) -> std::cmp::Ordering {
    a.0.total_cmp(&b.0)
        .then(a.1.cmp(&b.1))
        .then(a.2.total_cmp(&b.2))
}
fn glyph_match(
    part: GrayView<'_>,
    glyphs: &[(char, Vec<GrayFrame>)],
) -> Result<GlyphMatch, ObservationError> {
    let glyph = normalize(part)?;
    let mut candidates = Vec::new();
    for (key, templates) in glyphs {
        let mut score = 0.0_f64;
        for template in templates {
            score = score.max(mask_score(glyph.view(), template.view())?);
        }
        candidates.push((score, *key));
    }
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.1.cmp(&a.1)));
    if candidates.len() < 2 {
        return Err(ObservationError::invalid("glyph alternatives required"));
    }
    Ok((
        candidates[0].0,
        candidates[0].1,
        candidates[0].0 - candidates[1].0,
    ))
}
fn from_mask(
    mut mask: GrayFrame,
    settings: &Matching,
    glyphs: &[(char, Vec<GrayFrame>)],
) -> Result<ResourceReading, ObservationError> {
    let size = mask.size();
    let w = size.width();
    // 잘린 영역 가장자리의 고립된 점만 제거하고 숫자 내부나 잡음은 고치지 않는다.
    for (edge, start, end) in [(0, 1, w.min(7)), (w - 1, w.saturating_sub(7), w - 1)] {
        let occupied: Vec<_> = (0..size.height())
            .filter(|y| mask.pixels()[y * w + edge] != 0)
            .collect();
        if occupied.is_empty()
            || occupied.len() > 3
            || occupied[occupied.len() - 1] - occupied[0] >= 3
        {
            continue;
        }
        let neighboring = (0..size.height()).any(|y| {
            mask.pixels()[y * w + start.min(end)..y * w + end]
                .iter()
                .any(|v| *v != 0)
        });
        if !neighboring {
            for y in 0..size.height() {
                mask.pixels_mut()[y * w + edge] = 0;
            }
        }
    }
    let occupied = columns(mask.view())?;
    let cutoff = occupied
        .windows(2)
        .find(|p| p[1] - p[0] > settings.number_max_gap.saturating_add(1))
        .map_or(w, |p| p[0] + 1);
    let view = mask
        .view()
        .crop(PixelRect::new(0, 0, cutoff, size.height())?)?;
    let parts = glyph_parts(view)?;
    if !(3..=15).contains(&parts.len())
        || parts
            .iter()
            .any(|r| r.height() < settings.minimum_glyph_height)
    {
        return Ok(ResourceReading::unknown("number_unreadable", None));
    }
    let mut matches = parts
        .iter()
        .map(|part| glyph_match(view.crop(*part)?, glyphs))
        .collect::<Result<Vec<_>, ObservationError>>()?;
    let invalid: Vec<_> = matches
        .iter()
        .enumerate()
        .filter_map(|(i, (score, _, margin))| {
            (*score < settings.glyph_score || *margin < settings.glyph_margin).then_some(i)
        })
        .collect();
    if invalid.len() == 1 {
        let index = invalid[0];
        if index > 0
            && index + 1 < parts.len()
            && matches
                .iter()
                .enumerate()
                .all(|(i, (_, c, _))| i == index || c.is_ascii_digit())
            && parts[index].left() - parts[index - 1].right() >= 6
            && parts[index + 1].left() - parts[index].right() >= 6
        {
            let part = view.crop(parts[index])?;
            let mut recovered = Vec::new();
            for top in 0..3 {
                for left in 0..2 {
                    for bottom in 0..3 {
                        for right in 0..2 {
                            if top + bottom >= part.size().height()
                                || left + right >= part.size().width()
                                || part.size().height() - top - bottom
                                    < settings.minimum_glyph_height
                            {
                                continue;
                            }
                            let trimmed = part.crop(PixelRect::new(
                                left,
                                top,
                                part.size().width() - right,
                                part.size().height() - bottom,
                            )?)?;
                            if !(0..trimmed.size().height()).any(|y| {
                                trimmed
                                    .row(y)
                                    .is_some_and(|row| row.iter().any(|v| *v != 0))
                            }) {
                                continue;
                            }
                            let candidate = glyph_match(trimmed, glyphs)?;
                            if candidate.1 == '/'
                                && candidate.0 >= settings.glyph_score
                                && candidate.2 >= settings.glyph_margin
                            {
                                recovered.push(candidate);
                            }
                        }
                    }
                }
            }
            if let Some(best) = recovered.into_iter().max_by(compare) {
                matches[index] = best;
            }
        }
    }
    for (score, _, margin) in &matches {
        if *score < settings.glyph_score || *margin < settings.glyph_margin {
            return Ok(ResourceReading::unknown("glyph_ambiguous", Some(*score)));
        }
    }
    let score = matches.iter().map(|v| v.0).fold(f64::INFINITY, f64::min);
    let text: String = matches.iter().map(|v| v.1).collect();
    Ok(ResourceReading::from_text(&text, score))
}
pub(super) fn read_number(
    image: BgrView<'_>,
    settings: &Matching,
    glyphs: &[(char, Vec<GrayFrame>)],
) -> Result<ResourceReading, ObservationError> {
    let mask = text_mask(image)?;
    let reading = from_mask(mask.clone(), settings, glyphs)?;
    if reading.ratio().is_some() || reading.reason == "number_out_of_range" {
        return Ok(reading);
    }
    let mut dark = GrayFrame::zeros(image.size())?;
    for y in 0..image.size().height() {
        let row = image
            .row(y)
            .ok_or_else(|| ObservationError::invalid("missing number row"))?;
        for (x, p) in row.as_chunks::<3>().0.iter().enumerate() {
            dark.pixels_mut()[y * image.size().width() + x] =
                u8::from(p[0].min(p[1]).min(p[2]) < settings.number_outline_dark_limit);
        }
    }
    let expanded = morphology(
        dark.view(),
        Kernel::Rectangle {
            width: 3,
            height: 3,
        },
        Morphology::Dilate,
    )?;
    let mut outlined = mask.clone();
    for (v, dark) in outlined.pixels_mut().iter_mut().zip(expanded.pixels()) {
        *v &= dark;
    }
    if outlined == mask {
        return Ok(reading);
    }
    let mut current = from_mask(outlined, settings, glyphs)?;
    if current.ratio().is_some() {
        current.reason = "read_outlined".into();
        Ok(current)
    } else {
        Ok(reading)
    }
}

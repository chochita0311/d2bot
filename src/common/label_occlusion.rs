//! 이름표 가림 근거와 현재 보이는 픽셀만 사용하는 상관 비교.
use super::{
    frame::{BgrView, FloatMap, GrayFrame, GrayView, Image, PixelRect, Size},
    vision_ops::{
        VisionError,
        components::{Connectivity, connected_components},
        filters::reflect101,
        morphology::{Kernel, Morphology, morphology},
        template::{MatchMethod, match_template, maximum},
    },
};

// 입력은 v<65의 0/1 값이다. 9x9 평균>=0.2는 정확히 dark 개수>=17과 같다.
// 정수 창 합을 갱신해 원래 reflect101 경계와 판정을 유지한다.
fn dark_neighborhood(source: GrayView<'_>) -> Result<GrayFrame, VisionError> {
    let size = source.size();
    let (w, h) = (size.width(), size.height());
    let mut horizontal = Image::<u16, 1>::zeros(size)?;
    for y in 0..h {
        let row = source
            .row(y)
            .ok_or(VisionError::Invalid("missing label row"))?;
        let mut sum: u16 = (-4..=4).map(|x| u16::from(row[reflect101(x, w)])).sum();
        for x in 0..w {
            horizontal.pixels_mut()[y * w + x] = sum;
            sum -= u16::from(row[reflect101(x as isize - 4, w)]);
            sum += u16::from(row[reflect101(x as isize + 5, w)]);
        }
    }
    let mut sums = Vec::new();
    sums.try_reserve_exact(w)
        .map_err(super::frame::FrameError::Allocation)?;
    sums.resize(w, 0_u16);
    for dy in -4..=4 {
        let start = reflect101(dy, h) * w;
        for (sum, value) in sums.iter_mut().zip(&horizontal.pixels()[start..start + w]) {
            *sum += value;
        }
    }
    let mut output = GrayFrame::zeros(size)?;
    for y in 0..h {
        for (value, count) in output.pixels_mut()[y * w..(y + 1) * w]
            .iter_mut()
            .zip(&sums)
        {
            *value = u8::from(*count >= 17);
        }
        let leaving = reflect101(y as isize - 4, h) * w;
        let entering = reflect101(y as isize + 5, h) * w;
        for (x, sum) in sums.iter_mut().enumerate() {
            *sum -= horizontal.pixels()[leaving + x];
            *sum += horizontal.pixels()[entering + x];
        }
    }
    Ok(output)
}

pub fn label_occlusion_mask(
    frame: BgrView<'_>,
    include_gold: bool,
) -> Result<GrayFrame, VisionError> {
    let size = frame.size();
    let w = size.width();
    let top = (size.height() as f64 * 0.05).round_ties_even() as usize;
    let bottom = (size.height() as f64 * 0.79).round_ties_even() as usize;
    let view = frame.crop(PixelRect::new(0, top, w, bottom)?)?;
    let mut value = GrayFrame::zeros(view.size())?;
    let mut dark = GrayFrame::zeros(view.size())?;
    for y in 0..view.size().height() {
        let row = view
            .row(y)
            .ok_or(VisionError::Invalid("missing label row"))?;
        for (x, p) in row.as_chunks::<3>().0.iter().enumerate() {
            let v = p[0].max(p[1]).max(p[2]);
            value.pixels_mut()[y * w + x] = v;
            dark.pixels_mut()[y * w + x] = u8::from(v < 65);
        }
    }
    let neighborhood = dark_neighborhood(dark.view())?;
    let mut ink = GrayFrame::zeros(view.size())?;
    for y in 0..view.size().height() {
        let row = view
            .row(y)
            .ok_or(VisionError::Invalid("missing label row"))?;
        for (x, p) in row.as_chunks::<3>().0.iter().enumerate() {
            let i = y * w + x;
            let [b, g, r] = p.map(i16::from);
            let gold = r - g > 8 && g - b > 12 && r - b > 35;
            ink.pixels_mut()[i] = u8::from(
                value.pixels()[i] >= 135
                    && neighborhood.pixels()[i] != 0
                    && (include_gold || !gold),
            );
        }
    }
    let rows = morphology(
        ink.view(),
        Kernel::Rectangle {
            width: 19,
            height: 1,
        },
        Morphology::Close,
    )?;
    let joined = morphology(
        rows.view(),
        Kernel::Rectangle {
            width: 1,
            height: 3,
        },
        Morphology::Dilate,
    )?;
    let components = connected_components(joined.view(), Connectivity::Eight)?;
    let mut candidates: Vec<PixelRect> = components
        .stats
        .iter()
        .skip(1)
        .filter_map(|c| c.rect)
        .collect();
    let scale = size.height() as f64 / 1140.0;
    let width_ok = |r: PixelRect| (24.0 * scale..=650.0 * scale).contains(&(r.width() as f64));
    let merged: Vec<_> = candidates
        .iter()
        .copied()
        .filter(|r| width_ok(*r) && r.height() as f64 > 31.0 * scale)
        .collect();
    if !merged.is_empty() {
        let row_components = connected_components(rows.view(), Connectivity::Eight)?;
        candidates.extend(
            row_components
                .stats
                .iter()
                .skip(1)
                .filter_map(|c| c.rect)
                .filter(|r| {
                    merged.iter().any(|m| {
                        m.left() <= r.left()
                            && m.top() <= r.top()
                            && r.right() <= m.right()
                            && r.bottom() <= m.bottom()
                    })
                }),
        );
    }
    let mut mask = GrayFrame::zeros(size)?;
    for r in candidates {
        if !width_ok(r) || !(10.0 * scale..=31.0 * scale).contains(&(r.height() as f64)) {
            continue;
        }
        let patch = value.view().crop(r)?;
        let mut bright = 0_usize;
        let mut dark = 0_usize;
        for y in 0..r.height() {
            for v in patch
                .row(y)
                .ok_or(VisionError::Invalid("missing label patch row"))?
            {
                bright += usize::from(*v >= 135);
                dark += usize::from(*v < 65);
            }
        }
        let count = (r.width() * r.height()) as f64;
        if !(0.06..=0.65).contains(&(bright as f64 / count)) || (dark as f64 / count) < 0.25 {
            continue;
        }
        let x0 = r.left().saturating_sub(3);
        let x1 = r.right().saturating_add(3).min(w);
        let y0 = (top + r.top()).saturating_sub(2).max(top);
        let y1 = (top + r.bottom()).saturating_add(2).min(bottom);
        for y in y0..y1 {
            mask.pixels_mut()[y * w + x0..y * w + x1].fill(255);
        }
    }
    Ok(mask)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisibleTemplatePeak {
    pub score: f64,
    pub location: [usize; 2],
    pub visible_fraction: f64,
}

fn float_map(
    size: Size,
    mut value: impl FnMut(usize, usize) -> Result<f32, VisionError>,
) -> Result<FloatMap, VisionError> {
    let mut result = FloatMap::zeros(size)?;
    for y in 0..size.height() {
        for x in 0..size.width() {
            result.pixels_mut()[y * size.width() + x] = value(x, y)?;
        }
    }
    Ok(result)
}

pub fn visible_template_peak(
    gray: GrayView<'_>,
    template: GrayView<'_>,
    visible: GrayView<'_>,
    minimum_visible: f64,
) -> Result<VisibleTemplatePeak, VisionError> {
    if gray.size() != visible.size()
        || gray.size().width() < template.size().width()
        || gray.size().height() < template.size().height()
    {
        return Err(VisionError::Invalid(
            "matching grayscale patch and visibility mask required",
        ));
    }
    if !minimum_visible.is_finite() || !(0.0..=1.0).contains(&minimum_visible) {
        return Err(VisionError::Invalid(
            "visibility fraction must be between zero and one",
        ));
    }
    let valid = float_map(gray.size(), |x, y| {
        Ok(f32::from(u8::from(
            visible
                .row(y)
                .ok_or(VisionError::Invalid("missing visibility row"))?[x]
                != 0,
        )))
    })?;
    let target = float_map(gray.size(), |x, y| {
        Ok(f32::from(
            gray.row(y)
                .ok_or(VisionError::Invalid("missing target row"))?[x],
        ))
    })?;
    let reference = float_map(template.size(), |x, y| {
        Ok(f32::from(
            template
                .row(y)
                .ok_or(VisionError::Invalid("missing template row"))?[x],
        ))
    })?;
    let ones = float_map(template.size(), |_, _| Ok(1.0))?;
    let weighted = float_map(gray.size(), |x, y| {
        let i = y * gray.size().width() + x;
        Ok(target.pixels()[i] * valid.pixels()[i])
    })?;
    let squared = float_map(gray.size(), |x, y| {
        let i = y * gray.size().width() + x;
        Ok(target.pixels()[i] * target.pixels()[i] * valid.pixels()[i])
    })?;
    let reference_squared = float_map(template.size(), |x, y| {
        let v = reference.pixels()[y * template.size().width() + x];
        Ok(v * v)
    })?;
    let correlation =
        |a: &FloatMap, b: &FloatMap| match_template(a.view(), b.view(), MatchMethod::Correlation);
    let count = correlation(&valid, &ones)?;
    let sum_target = correlation(&weighted, &ones)?;
    let sum_reference = correlation(&valid, &reference)?;
    let variance_target = correlation(&squared, &ones)?;
    let variance_reference = correlation(&valid, &reference_squared)?;
    let numerator = correlation(&weighted, &reference)?;
    let mut scores = FloatMap::zeros(count.size())?;
    let total = (template.size().width() * template.size().height()) as f64;
    for (i, score) in scores.pixels_mut().iter_mut().enumerate() {
        let n = count.pixels()[i];
        let safe_count = n.max(1.0);
        let a = sum_target.pixels()[i];
        let b = sum_reference.pixels()[i];
        let vt = variance_target.pixels()[i] - a * a / safe_count;
        let vr = variance_reference.pixels()[i] - b * b / safe_count;
        let numerator = numerator.pixels()[i] - a * b / safe_count;
        let denominator = (vt * vr).max(0.0).sqrt().max(1.0);
        *score = if n >= (total * minimum_visible) as f32 && vt >= n * 25.0 && vr >= n * 25.0 {
            numerator / denominator
        } else {
            -1.0
        };
    }
    let peak =
        maximum(&scores).ok_or(VisionError::Invalid("visible correlation produced no peak"))?;
    Ok(VisibleTemplatePeak {
        score: f64::from(peak.score).clamp(-1.0, 1.0),
        location: peak.location,
        visible_fraction: f64::from(
            count.pixels()[peak.location[1] * count.size().width() + peak.location[0]],
        ) / total,
    })
}

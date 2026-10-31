use super::*;
use crate::common::{
    frame::{FloatMap, GrayView},
    label_occlusion::visible_template_peak,
    vision_ops::template::{self, MatchMethod},
};
#[derive(Debug, Clone)]
pub(super) struct PartMatch {
    pub bounds: [i32; 4],
    pub score: f64,
    pub fraction: f64,
}
pub(super) type MatchCache = BTreeMap<(String, u64), Vec<PartMatch>>;
pub(super) struct Views {
    gray: (GrayFrame, template::GrayMatcher),
    gold: Option<(GrayFrame, template::GrayMatcher)>,
}
impl Views {
    pub fn new(frame: BgrView<'_>, gold: bool) -> Result<Self, ObservationError> {
        let pair = |encoding| -> Result<_, ObservationError> {
            let image = encoded(frame, encoding)?;
            let small = resize::half(image.view())?;
            Ok((image, template::GrayMatcher::new(small)?))
        };
        Ok(Self {
            gray: pair(Encoding::Gray)?,
            gold: if gold {
                Some(pair(Encoding::SoftGold)?)
            } else {
                None
            },
        })
    }
    pub fn get(
        &self,
        encoding: Encoding,
    ) -> Result<&(GrayFrame, template::GrayMatcher), ObservationError> {
        match encoding {
            Encoding::Gray => Ok(&self.gray),
            Encoding::SoftGold => self
                .gold
                .as_ref()
                .ok_or_else(|| ObservationError::invalid("terminal gold image unavailable")),
        }
    }
}
pub(super) fn key(part: &Part) -> (String, u64) {
    (part.rule.path.clone(), part.rule.threshold.to_bits())
}
fn suppress(scores: &mut FloatMap, [x, y]: [usize; 2], w: usize, h: usize) {
    let width = scores.size().width();
    for row in y.saturating_sub(h / 2)..(y + h / 2 + 1).min(scores.size().height()) {
        scores.pixels_mut()
            [row * width + x.saturating_sub(w / 2)..row * width + (x + w / 2 + 1).min(width)]
            .fill(-1.0);
    }
}
fn visible_mask(source: GrayView<'_>) -> Result<GrayFrame, ObservationError> {
    let mut output = GrayFrame::zeros(source.size())?;
    for y in 0..source.size().height() {
        let row = source
            .row(y)
            .ok_or_else(|| ObservationError::invalid("missing occlusion row"))?;
        for (x, v) in row.iter().enumerate() {
            output.pixels_mut()[y * source.size().width() + x] = u8::from(*v == 0);
        }
    }
    Ok(output)
}
pub(super) fn groups(
    views: &Views,
    parts: &[Arc<Part>],
    cache: &mut MatchCache,
    occluded: GrayView<'_>,
) -> Result<Option<Vec<Vec<PartMatch>>>, ObservationError> {
    let mut groups = Vec::new();
    for part in parts {
        let key = key(part);
        if !cache.contains_key(&key) {
            let found = match_part(views, part, occluded)?;
            cache.insert(key.clone(), found);
        }
        let found = cache
            .get(&key)
            .ok_or_else(|| ObservationError::invalid("current terminal match missing"))?;
        if found.is_empty() {
            return Ok(None);
        }
        groups.push(found.clone());
    }
    Ok(Some(groups))
}
pub(super) fn match_part(
    views: &Views,
    part: &Part,
    occluded: GrayView<'_>,
) -> Result<Vec<PartMatch>, ObservationError> {
    let (gray, small) = views.get(part.rule.encoding)?;
    let coarse = small.match_pattern(part.small.view(), MatchMethod::NormalizedCoefficient)?;
    let mut scores = coarse.clone();
    let mut found = Vec::new();
    let [w, h] = [part.native.size().width(), part.native.size().height()];
    let threshold = if w.min(h) <= 48 { 0.8 } else { 0.85 };
    for _ in 0..4 {
        let Some(peak) = template::maximum(&scores).filter(|p| f64::from(p.score) >= threshold)
        else {
            break;
        };
        let [x, y] = peak.location;
        let [x0, y0] = [(x * 2).saturating_sub(3), (y * 2).saturating_sub(3)];
        let [x1, y1] = [
            (x * 2 + w + 4).min(gray.size().width()),
            (y * 2 + h + 4).min(gray.size().height()),
        ];
        if x1 - x0 >= w && y1 - y0 >= h {
            let patch = gray.view().crop(PixelRect::new(x0, y0, x1, y1)?)?;
            let exact = template::match_template(
                patch,
                part.native.view(),
                MatchMethod::NormalizedCoefficient,
            )?;
            if let Some(peak) =
                template::maximum(&exact).filter(|p| f64::from(p.score) >= part.rule.threshold)
            {
                let [l, t] = [
                    (24 + x0 + peak.location[0]) as i32,
                    (55 + y0 + peak.location[1]) as i32,
                ];
                let bounds = [l, t, l + w as i32, t + h as i32];
                if visible(bounds) {
                    found.push(PartMatch {
                        bounds,
                        score: f64::from(peak.score),
                        fraction: 1.0,
                    });
                }
            }
        }
        suppress(&mut scores, [x, y], w, h);
    }
    let any_occlusion = (0..occluded.size().height()).any(|y| {
        occluded
            .row(y)
            .is_some_and(|row| row.iter().any(|v| *v != 0))
    });
    if any_occlusion && found.len() < 4 {
        let mut scores = coarse;
        for _ in 0..8 {
            let Some(peak) = template::maximum(&scores).filter(|p| f64::from(p.score) >= 0.45)
            else {
                break;
            };
            let [x, y] = peak.location;
            let [x0, y0] = [(x * 2).saturating_sub(3), (y * 2).saturating_sub(3)];
            let [x1, y1] = [
                (x * 2 + w + 4).min(gray.size().width()),
                (y * 2 + h + 4).min(gray.size().height()),
            ];
            if x1 - x0 >= w && y1 - y0 >= h {
                let bounds = PixelRect::new(x0, y0, x1, y1)?;
                let mask = occluded.crop(bounds)?;
                if (0..mask.size().height())
                    .any(|y| mask.row(y).is_some_and(|r| r.iter().any(|v| *v != 0)))
                {
                    let peak = visible_template_peak(
                        gray.view().crop(bounds)?,
                        part.native.view(),
                        visible_mask(mask)?.view(),
                        0.6,
                    )?;
                    let [l, t] = [
                        (24 + x0 + peak.location[0]) as i32,
                        (55 + y0 + peak.location[1]) as i32,
                    ];
                    let bounds = [l, t, l + w as i32, t + h as i32];
                    if peak.score >= part.rule.threshold
                        && peak.visible_fraction < 1.0
                        && visible(bounds)
                        && !found.iter().any(|p| {
                            distance(
                                [f64::from(l), f64::from(t)],
                                [f64::from(p.bounds[0]), f64::from(p.bounds[1])],
                            ) < 10.0
                        })
                    {
                        found.push(PartMatch {
                            bounds,
                            score: peak.score,
                            fraction: peak.visible_fraction,
                        });
                    }
                }
            }
            suppress(&mut scores, [x, y], w, h);
        }
    }
    Ok(found)
}
pub(super) fn layout(
    groups: &[Vec<PartMatch>],
    kind: &str,
    parts: &[Arc<Part>],
    separation: f64,
    area: f64,
) -> Option<TerminalReading> {
    if groups.len() != 3 || parts.len() != 3 {
        return None;
    }
    let mut best: Option<(f64, [&PartMatch; 3])> = None;
    for a in &groups[0] {
        for b in &groups[1] {
            for c in &groups[2] {
                let trio = [a, b, c];
                let [a, b, c] = trio.map(|m| center(m.bounds));
                let distances = [distance(a, b), distance(a, c), distance(b, c)];
                if distances.iter().any(|d| *d < separation || *d > 700.0)
                    || ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() / 2.0
                        < area
                {
                    continue;
                }
                let score = trio.iter().map(|m| m.score).sum();
                if best.is_none_or(|b| score > b.0) {
                    best = Some((score, trio));
                }
            }
        }
    }
    best.map(|(_, trio)| TerminalReading {
        kind: kind.into(),
        point: std::array::from_fn(|axis| {
            trio.iter().map(|m| center(m.bounds)[axis]).sum::<f64>() / 3.0
        }),
        evidence_boxes: trio.iter().map(|m| m.bounds).collect(),
        scores: trio.iter().map(|m| Some(m.score)).collect(),
        part_ids: parts.iter().map(|p| p.rule.path.clone()).collect(),
        provenance: if trio.iter().any(|m| m.fraction < 1.0) {
            "current_masked_three_parts"
        } else {
            "current_three_parts"
        }
        .into(),
        current_parts: vec![true; 3],
        reference_observed_at: None,
        visible_fractions: trio.iter().map(|m| Some(m.fraction)).collect(),
        minimum_separation: separation,
        minimum_area: area,
    })
}
pub(super) fn composed(
    views: &Views,
    parts: &[Arc<Part>],
    cache: &mut MatchCache,
    mask: GrayView<'_>,
    separation: f64,
    area: f64,
) -> Result<Option<TerminalReading>, ObservationError> {
    let mut candidates = Vec::new();
    for part in parts {
        if let Some(groups) = groups(views, &[Arc::clone(part)], cache, mask)? {
            for m in &groups[0] {
                if m.fraction == 1.0 {
                    candidates.push((m.clone(), Arc::clone(part)));
                }
            }
        }
    }
    let mut best: Option<TerminalReading> = None;
    for a in 0..candidates.len() {
        for b in a + 1..candidates.len() {
            for c in b + 1..candidates.len() {
                let trio = [&candidates[a], &candidates[b], &candidates[c]];
                let groups = trio.map(|p| vec![p.0.clone()]);
                let parts = trio.map(|p| Arc::clone(&p.1));
                if let Some(reading) =
                    layout(&groups, "nonboss_chest_platform", &parts, separation, area)
                {
                    let sum = reading.scores.iter().flatten().sum::<f64>();
                    if best
                        .as_ref()
                        .is_none_or(|b| sum > b.scores.iter().flatten().sum::<f64>())
                    {
                        best = Some(reading);
                    }
                }
            }
        }
    }
    Ok(best)
}
pub(super) fn local(
    frame: &GrayFrame,
    occluded: &GrayFrame,
    part: &Part,
    predicted: [i32; 4],
    radius: i32,
) -> Result<Option<PartMatch>, ObservationError> {
    let [l, t, r, b] = predicted;
    let [left, top] = [
        (i64::from(l) - i64::from(radius)).max(24),
        (i64::from(t) - i64::from(radius)).max(55),
    ];
    let [right, bottom] = [
        (i64::from(r) + i64::from(radius)).clamp(24, 1900),
        (i64::from(b) + i64::from(radius)).clamp(55, BOTTOM as i64),
    ];
    if right - left < part.native.size().width() as i64
        || bottom - top < part.native.size().height() as i64
    {
        return Ok(None);
    }
    let bounds = PixelRect::new(left as usize, top as usize, right as usize, bottom as usize)?;
    let patch = frame.view().crop(bounds)?;
    let map = template::match_template(
        patch,
        part.native.view(),
        MatchMethod::NormalizedCoefficient,
    )?;
    let peak = template::maximum(&map)
        .ok_or_else(|| ObservationError::invalid("terminal score map empty"))?;
    let mut score = f64::from(peak.score);
    let mut location = peak.location;
    let mut fraction = 1.0;
    if score < part.rule.threshold {
        let mask = occluded.view().crop(bounds)?;
        if (0..mask.size().height()).any(|y| mask.row(y).is_some_and(|r| r.iter().any(|v| *v != 0)))
        {
            let peak =
                visible_template_peak(patch, part.native.view(), visible_mask(mask)?.view(), 0.6)?;
            score = peak.score;
            location = peak.location;
            fraction = peak.visible_fraction;
        }
    }
    let [x, y] = [
        left as i32 + location[0] as i32,
        top as i32 + location[1] as i32,
    ];
    let bounds = [
        x,
        y,
        x + part.native.size().width() as i32,
        y + part.native.size().height() as i32,
    ];
    Ok(
        (score >= part.rule.threshold && visible(bounds)).then_some(PartMatch {
            bounds,
            score,
            fraction,
        }),
    )
}

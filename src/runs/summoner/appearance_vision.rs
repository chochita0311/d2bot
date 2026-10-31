//! 현재 룬 발판의 로브 외형 후보. 중복 몸체·서로 다른 위치는 신원으로 승인하지 않는다.
use super::objective_vision::ObjectiveReading;
use crate::common::{
    encounters::Appearance,
    error::ObservationError,
    frame::{BgrFrame, BgrView, FloatMap, GrayFrame, PixelRect, Size},
    observation_assets::AssetOwner,
    vision_ops::{
        codec, color, resize,
        template::{self, MatchMethod},
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SummonerAppearance {
    pub r#box: [usize; 4],
    pub ground: [f64; 2],
    pub score: f64,
    pub appearance: Appearance,
    pub asset_id: String,
}

#[cfg(test)]
mod lighting_tests {
    use super::*;
    #[test]
    fn right_band_lighting_rejects_native_alive_and_scene_corpus()
    -> Result<(), Box<dyn std::error::Error>> {
        let owner = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
        let mut observer = SummonerAppearanceObserver::load(owner.join("summoner-encounter.json"))?;
        observer
            .parts
            .retain(|part| part.rule.id == "summoner-robe-corpse-lower-band-right");
        assert_eq!(observer.parts.len(), 1);
        let platform = ObjectiveReading {
            kind: "summoner_platform".into(),
            journal_box: None,
            journal_score: None,
            rune_boxes: vec![[24, 55, 1900, 900]],
            evidence_kind: "permissive_test_roi".into(),
            glyph_scores: vec![],
            ground_point: Some([962.0, 580.0]),
            slot_indices: vec![],
            layout_residual: None,
        };
        let mut paths = Vec::new();
        for folder in ["terminal", "hub", "navigation", "summoner"] {
            for entry in std::fs::read_dir(owner.join("references").join(folder))? {
                let path = entry?.path();
                if path.extension().is_some_and(|extension| extension == "png")
                    && (folder != "summoner"
                        || path.file_name().is_some_and(|name| {
                            name.to_string_lossy().starts_with("platform-alive")
                        }))
                {
                    paths.push(path);
                }
            }
        }
        paths.sort();
        let (mut native, mut size_rejected) = (0, 0);
        for path in paths {
            let frame = codec::load_color(&path)?;
            let result = observer.observe_corpse(frame.view(), Some(&platform));
            if frame.size() != Size::new(1922, 1140)? {
                assert!(result.is_err());
                size_rejected += 1;
                continue;
            }
            assert!(result?.is_none(), "false corpse: {}", path.display());
            native += 1;
        }
        assert!(native >= 119);
        eprintln!(
            "right-band corpus: {native} native non-detections, {size_rejected} size rejections"
        );
        Ok(())
    }
}
fn alive() -> Appearance {
    Appearance::Alive
}
fn proposal_threshold() -> f64 {
    0.65
}
#[derive(Deserialize)]
struct Rule {
    id: String,
    path: String,
    sha256: String,
    threshold: f64,
    ground_offset: [i32; 2],
    #[serde(default = "alive")]
    appearance: Appearance,
    #[serde(default = "proposal_threshold")]
    proposal_threshold: f64,
    minimum_blue_gold_agreement: Option<f64>,
}
#[derive(Deserialize)]
struct Review {
    reviewed_by: String,
}
#[derive(Deserialize)]
struct Manifest {
    life_review: Review,
    parts: Vec<Rule>,
}
struct Part {
    rule: Rule,
    image: BgrFrame,
    proposal: GrayFrame,
}
enum Match {
    Missing,
    Ambiguous,
    Found(SummonerAppearance),
}
pub struct SummonerAppearanceObserver {
    parts: Vec<Part>,
}
fn suppress(map: &mut FloatMap, [x, y]: [usize; 2], width: usize, height: usize) {
    let w = map.size().width();
    for row in y.saturating_sub(height)..(y + height + 1).min(map.size().height()) {
        map.pixels_mut()[row * w + x.saturating_sub(width)..row * w + (x + width + 1).min(w)]
            .fill(-1.0);
    }
}
fn bands(p: &[u8]) -> [bool; 2] {
    let [b, g, r] = [i16::from(p[0]), i16::from(p[1]), i16::from(p[2])];
    [b - r >= 15 && b - g >= 5, r - b >= 20 && g - b >= 10]
}
fn blue_gold_matches(
    patch: BgrView<'_>,
    reference: BgrView<'_>,
    minimum: Option<f64>,
) -> Result<bool, ObservationError> {
    let Some(minimum) = minimum else {
        return Ok(true);
    };
    let mut totals = [0usize; 2];
    let mut hits = [0usize; 2];
    for y in 0..patch.size().height() {
        let a = patch
            .row(y)
            .ok_or_else(|| ObservationError::invalid("missing appearance row"))?;
        let b = reference
            .row(y)
            .ok_or_else(|| ObservationError::invalid("missing reference row"))?;
        for (a, b) in a.as_chunks::<3>().0.iter().zip(b.as_chunks::<3>().0) {
            let current = bands(a);
            let prior = bands(b);
            for i in 0..2 {
                if prior[i] {
                    totals[i] += 1;
                    hits[i] += usize::from(current[i]);
                }
            }
        }
    }
    Ok((0..2).all(|i| totals[i] > 0 && hits[i] as f64 / totals[i] as f64 >= minimum))
}
impl SummonerAppearanceObserver {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ObservationError> {
        let path = path.as_ref();
        let owner = AssetOwner::new(
            path.parent()
                .ok_or_else(|| ObservationError::invalid("appearance manifest owner required"))?,
        )?;
        let data: Manifest = serde_json::from_slice(&std::fs::read(path)?)?;
        if data.life_review.reviewed_by != "human-owner" {
            return Err(ObservationError::invalid(
                "reviewed Summoner life calibration required",
            ));
        }
        let mut parts = Vec::new();
        for rule in data.parts {
            let bytes = std::fs::read(owner.path(&rule.path)?)?;
            if format!("{:x}", Sha256::digest(&bytes)) != rule.sha256 {
                return Err(ObservationError::invalid(
                    "reviewed Summoner appearance fingerprint required",
                ));
            }
            let image = codec::decode_color(&bytes)?;
            let proposal = resize::half(color::gray(image.view())?.view())?;
            parts.push(Part {
                rule,
                image,
                proposal,
            });
        }
        Ok(Self { parts })
    }
    pub fn observe(
        &self,
        frame: BgrView<'_>,
        platform: Option<&ObjectiveReading>,
    ) -> Result<Option<SummonerAppearance>, ObservationError> {
        self.observe_state(frame, platform, Appearance::Alive)
    }
    pub fn observe_corpse(
        &self,
        frame: BgrView<'_>,
        platform: Option<&ObjectiveReading>,
    ) -> Result<Option<SummonerAppearance>, ObservationError> {
        self.observe_state(frame, platform, Appearance::Corpse)
    }
    fn observe_state(
        &self,
        frame: BgrView<'_>,
        platform: Option<&ObjectiveReading>,
        state: Appearance,
    ) -> Result<Option<SummonerAppearance>, ObservationError> {
        if frame.size() != Size::new(1922, 1140)? {
            return Err(ObservationError::invalid(
                "native Summoner appearance frame required",
            ));
        }
        let Some(platform) = platform.filter(|p| !p.rune_boxes.is_empty()) else {
            return Ok(None);
        };
        let mut candidates = Vec::new();
        for part in &self.parts {
            if part.rule.appearance != state {
                continue;
            }
            match self.match_part(frame, &platform.rune_boxes, part)? {
                Match::Missing => (),
                Match::Ambiguous => return Ok(None),
                Match::Found(candidate) => candidates.push(candidate),
            }
        }
        let mut best = None;
        for candidate in &candidates {
            if best.is_none_or(|b: &SummonerAppearance| candidate.score > b.score) {
                best = Some(candidate);
            }
        }
        Ok(best
            .filter(|best| {
                candidates.iter().all(|c| {
                    (best.ground[0] - c.ground[0]).hypot(best.ground[1] - c.ground[1]) <= 60.0
                })
            })
            .cloned())
    }
    fn match_part(
        &self,
        frame: BgrView<'_>,
        boxes: &[[usize; 4]],
        part: &Part,
    ) -> Result<Match, ObservationError> {
        let rule = &part.rule;
        let [w, h] = [part.image.size().width(), part.image.size().height()];
        let [dx, dy] = rule.ground_offset.map(i64::from);
        let [minx, miny, maxx, maxy] = [
            boxes.iter().map(|b| b[0]).min().unwrap_or(0) as i64,
            boxes.iter().map(|b| b[1]).min().unwrap_or(0) as i64,
            boxes.iter().map(|b| b[2]).max().unwrap_or(0) as i64,
            boxes.iter().map(|b| b[3]).max().unwrap_or(0) as i64,
        ];
        let [l, t, r, b] = [
            (minx - 100 - dx).max(24),
            (miny - 50 - dy).max(55),
            (maxx + 100 - dx + w as i64).min(1900),
            (maxy + 500 - dy + h as i64).min(900),
        ];
        if r - l < w as i64 || b - t < h as i64 {
            return Ok(Match::Missing);
        }
        let patch = frame.crop(PixelRect::new(
            l as usize, t as usize, r as usize, b as usize,
        )?)?;
        let coarse = resize::half(color::gray(patch)?.view())?;
        let mut scores = template::match_template(
            coarse.view(),
            part.proposal.view(),
            MatchMethod::NormalizedCoefficient,
        )?;
        let mut found = Vec::new();
        let mut exhausted = true;
        for _ in 0..16 {
            let Some(peak) = template::maximum(&scores)
                .filter(|p| f64::from(p.score) >= rule.proposal_threshold)
            else {
                exhausted = false;
                break;
            };
            let [cx, cy] = peak.location;
            let [x0, y0] = [(cx * 2).saturating_sub(3), (cy * 2).saturating_sub(3)];
            let [x1, y1] = [
                (cx * 2 + w + 4).min(patch.size().width()),
                (cy * 2 + h + 4).min(patch.size().height()),
            ];
            if x1 - x0 >= w && y1 - y0 >= h {
                let native = patch.crop(PixelRect::new(x0, y0, x1, y1)?)?;
                let exact = template::match_template(
                    native,
                    part.image.view(),
                    MatchMethod::NormalizedCoefficient,
                )?;
                if let Some(peak) =
                    template::maximum(&exact).filter(|p| f64::from(p.score) >= rule.threshold)
                {
                    let [x, y] = peak.location;
                    if blue_gold_matches(
                        native.crop(PixelRect::new(x, y, x + w, y + h)?)?,
                        part.image.view(),
                        rule.minimum_blue_gold_agreement,
                    )? {
                        found.push((f64::from(peak.score), x0 + x, y0 + y));
                    }
                }
            }
            suppress(
                &mut scores,
                [cx, cy],
                part.proposal.size().width(),
                part.proposal.size().height(),
            );
        }
        if exhausted {
            return Ok(Match::Missing);
        }
        let Some((score, x, y)) = found
            .iter()
            .copied()
            .max_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)))
        else {
            return Ok(Match::Missing);
        };
        if found
            .iter()
            .any(|(_, ox, oy)| x.abs_diff(*ox) > w || y.abs_diff(*oy) > h)
        {
            return Ok(Match::Ambiguous);
        }
        let mut uniqueness =
            template::match_template(patch, part.image.view(), MatchMethod::NormalizedCoefficient)?;
        suppress(&mut uniqueness, [x, y], w, h);
        if template::maximum(&uniqueness).is_some_and(|p| f64::from(p.score) >= rule.threshold) {
            return Ok(Match::Ambiguous);
        }
        let [x, y] = [x + l as usize, y + t as usize];
        let point = [x as f64 + dx as f64, y as f64 + dy as f64];
        if point[0] < (minx - 100) as f64
            || point[0] > (maxx + 100) as f64
            || point[1] < (miny - 50) as f64
            || point[1] > (maxy + 500) as f64
            || point[1] >= 900.0
        {
            return Ok(Match::Missing);
        }
        Ok(Match::Found(SummonerAppearance {
            r#box: [x, y, x + w, y + h],
            ground: point,
            score,
            appearance: rule.appearance,
            asset_id: rule.id.clone(),
        }))
    }
}

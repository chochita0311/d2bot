use super::{ActionError, MatchResult, locate};
use super::{
    EventLevel,
    runtime::{ActionBackend, ActionContext},
};
use crate::common::{
    config::BotConfig,
    frame::{BgrFrame, BgrView},
    unicode_casefold::casefold,
    vision_ops::codec,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CandidateSpec {
    pub label: String,
    pub path: String,
    pub threshold: f64,
}
#[derive(Debug, Clone)]
pub struct LootCandidate {
    pub label: String,
    pub template: Arc<BgrFrame>,
    pub threshold: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LootScanHit {
    pub label: String,
    #[serde(rename = "match")]
    pub match_result: MatchResult,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LootActionResult {
    pub found: bool,
    pub picked_up: bool,
    pub label: Option<String>,
    pub message: String,
}

#[derive(Deserialize)]
struct FixedItem {
    label: String,
    ground_template: Option<String>,
    threshold: f64,
}
#[derive(Deserialize)]
struct FarmTemplate {
    name: String,
    path: String,
    threshold: f64,
    label: Option<String>,
    context: String,
}

pub fn candidate_specs(config: &BotConfig) -> Result<Vec<CandidateSpec>, ActionError> {
    let mut specs = Vec::new();
    let mut seen = HashSet::new();
    let fixed = config
        .section("shared_loot")
        .and_then(|s| s.get("fixed_items"))
        .ok_or_else(|| ActionError::runtime("validated shared loot missing"))?;
    for item in serde_json::from_value::<Vec<FixedItem>>(fixed.clone())? {
        let Some(path) = item.ground_template.filter(|p| !p.is_empty()) else {
            continue;
        };
        let marker = (item.label.clone(), path.clone());
        if seen.insert(marker) {
            specs.push(CandidateSpec {
                label: item.label,
                path,
                threshold: item.threshold,
            });
        }
    }
    let templates = config
        .farm()
        .get("templates")
        .ok_or_else(|| ActionError::runtime("validated farm templates missing"))?;
    for rule in serde_json::from_value::<Vec<FarmTemplate>>(templates.clone())? {
        let marker = (rule.name.clone(), rule.path.clone());
        if seen.contains(&marker) {
            continue;
        }
        let context = casefold(&rule.context);
        if context != "any"
            && context != "ground"
            && !casefold(&rule.name).contains("ground")
            && !casefold(&rule.path).contains("ground")
        {
            continue;
        }
        seen.insert(marker);
        let label = rule
            .label
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| rule.name.replace('_', " "));
        specs.push(CandidateSpec {
            label,
            path: rule.path,
            threshold: rule.threshold,
        });
    }
    if specs.iter().any(|s| !s.threshold.is_finite()) {
        return Err(ActionError::runtime("finite loot thresholds required"));
    }
    Ok(specs)
}
pub struct LootScanner {
    specs: Vec<CandidateSpec>,
    cache: Option<Vec<LootCandidate>>,
    root: PathBuf,
}
impl LootScanner {
    pub fn new(config: &BotConfig, root: impl AsRef<Path>) -> Result<Self, ActionError> {
        Ok(Self {
            specs: candidate_specs(config)?,
            cache: None,
            root: root.as_ref().to_owned(),
        })
    }
    pub fn update_config(&mut self, config: &BotConfig) -> Result<(), ActionError> {
        let specs = candidate_specs(config)?;
        self.specs = specs;
        self.cache = None;
        Ok(())
    }
    pub fn specs(&self) -> &[CandidateSpec] {
        &self.specs
    }
    pub fn candidates(&mut self) -> Result<&[LootCandidate], ActionError> {
        if self.cache.is_none() {
            let mut candidates = Vec::new();
            for spec in &self.specs {
                let path = self.root.join(&spec.path);
                if !path.exists() {
                    return Err(ActionError::runtime(format!(
                        "Required asset is missing: {}",
                        spec.path.replace('\\', "/")
                    )));
                }
                let template = codec::load_color(&path).map_err(|_| {
                    ActionError::runtime(format!(
                        "Failed to load asset: {}",
                        spec.path.replace('\\', "/")
                    ))
                })?;
                candidates.push(LootCandidate {
                    label: spec.label.clone(),
                    template: Arc::new(template),
                    threshold: spec.threshold,
                });
            }
            self.cache = Some(candidates);
        }
        self.cache
            .as_deref()
            .ok_or_else(|| ActionError::runtime("loot candidate cache unavailable"))
    }
    pub fn scan_frame(
        &mut self,
        frame: BgrView<'_>,
        candidates: Option<&[LootCandidate]>,
    ) -> Result<Option<LootScanHit>, ActionError> {
        // An explicitly empty list has the same fallback as Python's `or`.
        let candidates = match candidates.filter(|c| !c.is_empty()) {
            Some(candidates) => candidates,
            None => self.candidates()?,
        };
        scan(frame, candidates)
    }
}
pub fn scan(
    frame: BgrView<'_>,
    candidates: &[LootCandidate],
) -> Result<Option<LootScanHit>, ActionError> {
    if candidates.is_empty() {
        return Ok(None);
    }
    let count = candidates.len().min(3);
    let mut groups: Vec<_> = (0..count).map(|_| None).collect();
    crate::common::compute::scope(|scope| {
        for (worker, result) in groups.iter_mut().enumerate() {
            scope.spawn(move |_| {
                *result = Some(
                    (worker..candidates.len())
                        .step_by(count)
                        .map(|index| {
                            let candidate = &candidates[index];
                            (
                                index,
                                locate(frame, candidate.template.view(), candidate.threshold),
                            )
                        })
                        .collect::<Vec<_>>(),
                );
            });
        }
    })
    .map_err(|error| match error {
        crate::common::compute::ComputeError::Start(error) => ActionError::Io(error),
        crate::common::compute::ComputeError::Panicked => {
            ActionError::runtime("loot matching worker panicked")
        }
    })?;
    let mut results = std::iter::repeat_with(|| None)
        .take(candidates.len())
        .collect::<Vec<_>>();
    for group in groups {
        for (index, result) in
            group.ok_or_else(|| ActionError::runtime("loot matching result missing"))?
        {
            results[index] = Some(result);
        }
    }
    // As pool.map does, observe results/errors in candidate order after all
    // scoped readers finish. A later failure does not override an earlier hit.
    for (candidate, result) in candidates.iter().zip(results) {
        if let Some(match_result) =
            result.ok_or_else(|| ActionError::runtime("loot matching result missing"))??
        {
            return Ok(Some(LootScanHit {
                label: candidate.label.clone(),
                match_result,
            }));
        }
    }
    Ok(None)
}
pub fn duplicate_hit(
    current: &LootScanHit,
    previous: Option<&LootScanHit>,
    age_seconds: f64,
) -> bool {
    let Some(previous) = previous else {
        return false;
    };
    if age_seconds > 1.4 {
        return false;
    }
    current.label == previous.label
        && current.match_result.top_left[0].abs_diff(previous.match_result.top_left[0]) <= 18
        && current.match_result.top_left[1].abs_diff(previous.match_result.top_left[1]) <= 18
}

const NO_CANDIDATES: &str =
    "No ground-loot items are configured in shared_loot.fixed_items or the active run profile.";
const ACTION_DELAY: [f64; 2] = [0.14, 0.25];
pub fn scan_visible_loot<B: ActionBackend>(
    scanner: &mut LootScanner,
    ctx: &mut ActionContext<B>,
) -> Result<Option<LootScanHit>, ActionError> {
    let candidates = scanner.candidates()?;
    let packet = ctx.capture()?;
    scan(packet.frame.view(), candidates)
}
pub fn try_pickup_visible_loot<B: ActionBackend>(
    scanner: &mut LootScanner,
    ctx: &mut ActionContext<B>,
    focus_window_first: bool,
) -> Result<LootActionResult, ActionError> {
    if scanner.candidates()?.is_empty() {
        return Err(ActionError::runtime(NO_CANDIDATES));
    }
    if focus_window_first {
        ctx.focus_configured()?;
    }
    let Some(hit) = scan_visible_loot(scanner, ctx)? else {
        return Ok(LootActionResult {
            found: false,
            picked_up: false,
            label: None,
            message: "No configured loot is visible right now.".into(),
        });
    };
    click_hit(ctx, &hit)?;
    Ok(LootActionResult {
        found: true,
        picked_up: true,
        label: Some(hit.label.clone()),
        message: format!("Clicked {}.", hit.label),
    })
}
pub fn run_session<B: ActionBackend>(
    scanner: &mut LootScanner,
    ctx: &mut ActionContext<B>,
) -> Result<usize, ActionError> {
    let result = (|| {
        let candidates = scanner.candidates()?;
        if candidates.is_empty() {
            return Err(ActionError::runtime(NO_CANDIDATES));
        }
        ctx.prepare()?;
        let labels = candidates
            .iter()
            .map(|c| c.label.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        ctx.emit(
            EventLevel::Info,
            format!("Loot pickup: scanning for configured ground items: {labels}"),
        );
        let count = pickup_until_quiet(ctx, candidates)?;
        ctx.emit(
            EventLevel::Info,
            if count == 0 {
                "Loot pickup: no configured loot appeared before the quiet timeout.".into()
            } else {
                format!("Loot pickup: finished after {count} pickup(s).")
            },
        );
        Ok(count)
    })();
    ctx.finish_logged(result, "Loot pickup")
}
fn click_hit<B: ActionBackend>(
    ctx: &mut ActionContext<B>,
    hit: &LootScanHit,
) -> Result<(), ActionError> {
    let point = [
        (hit.match_result.top_left[0] + hit.match_result.width / 2) as f64,
        (hit.match_result.top_left[1] + hit.match_result.height / 2) as f64,
    ];
    ctx.click_frame(point, "left", false)?;
    ctx.sleep_range(ACTION_DELAY)
}
fn pickup_until_quiet<B: ActionBackend>(
    ctx: &mut ActionContext<B>,
    candidates: &[LootCandidate],
) -> Result<usize, ActionError> {
    let mut deadline = ctx.now()? + 1.2;
    let mut count = 0usize;
    let mut last_hit = None;
    let mut last_hit_time = 0.0;
    while ctx.now()? < deadline {
        if ctx.user_interrupted()? {
            return Err(ActionError::runtime("stopped by user interference"));
        }
        let packet = ctx.capture()?;
        let hit = scan(packet.frame.view(), candidates)?;
        drop(packet);
        let Some(hit) = hit else {
            ctx.raw_sleep(0.12)?;
            continue;
        };
        let now = ctx.now()?;
        if duplicate_hit(&hit, last_hit.as_ref(), now - last_hit_time) {
            ctx.raw_sleep(0.12)?;
            continue;
        }
        click_hit(ctx, &hit)?;
        count = count
            .checked_add(1)
            .ok_or_else(|| ActionError::runtime("loot click count overflow"))?;
        last_hit_time = now;
        deadline = ctx.now()? + 1.2;
        ctx.emit(
            EventLevel::Info,
            format!("Loot pickup: clicked {}.", hit.label),
        );
        last_hit = Some(hit);
        ctx.sleep_range([0.9, 1.1])?;
    }
    Ok(count)
}

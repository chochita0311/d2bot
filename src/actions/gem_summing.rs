//! Gem recognition/planning and the independent combine session.
mod execution;
mod recognition;
use super::{ActionError, ActionEvent, EventLevel};
use crate::{
    actions::MatchResult,
    common::frame::{BgrFrame, BgrView},
};
pub use execution::{GemOutcome, run_session, scan_counts};
pub use recognition::{CountReading, GemVision, PANEL_SIZE};
use serde::Serialize;
use std::collections::BTreeSet;

pub const GEM_NAMES: [&str; 7] = [
    "diamond", "emerald", "ruby", "topaz", "amethyst", "sapphire", "skull",
];
pub const LEVEL_NAMES: [&str; 5] = ["chipped", "flawed", "normal", "flawless", "perfect"];
pub type GemCounts = [[u32; 7]; 5];
#[derive(Debug, Clone, Serialize)]
pub struct FrameCounts {
    pub counts: GemCounts,
    pub confidences: [[f64; 7]; 5],
    pub icon_scores: [[f64; 7]; 5],
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GemPlanStep {
    pub gem_name: &'static str,
    pub level_name: &'static str,
    pub col: usize,
    pub row: usize,
    pub repeats: u32,
}
/// Immutable recognition boundary shared by the asset implementation and
/// scripted, input-free session comparisons.
pub trait GemRecognition: Send + Sync {
    fn locate_stash_panel(&self, frame: BgrView<'_>) -> Result<Option<MatchResult>, ActionError>;
    fn locate_cube_button(&self, frame: BgrView<'_>) -> Result<Option<MatchResult>, ActionError>;
    fn read_frame_counts(
        &self,
        frame: BgrView<'_>,
        panel: MatchResult,
    ) -> Result<FrameCounts, ActionError>;
    fn occupied_cube_cells(
        &self,
        frame: BgrView<'_>,
        panel: MatchResult,
    ) -> Result<Vec<([usize; 2], f64)>, ActionError>;
    fn cube_result_icon(
        &self,
        frame: BgrView<'_>,
        panel: MatchResult,
        point: [i64; 2],
    ) -> Result<BgrFrame, ActionError>;
    fn expected_result(
        &self,
        col: usize,
        level: usize,
        icon: BgrView<'_>,
    ) -> Result<bool, ActionError>;
}
impl GemRecognition for GemVision {
    fn locate_stash_panel(&self, f: BgrView<'_>) -> Result<Option<MatchResult>, ActionError> {
        GemVision::locate_stash_panel(self, f)
    }
    fn locate_cube_button(&self, f: BgrView<'_>) -> Result<Option<MatchResult>, ActionError> {
        GemVision::locate_cube_button(self, f)
    }
    fn read_frame_counts(
        &self,
        f: BgrView<'_>,
        p: MatchResult,
    ) -> Result<FrameCounts, ActionError> {
        GemVision::read_frame_counts(self, f, p)
    }
    fn occupied_cube_cells(
        &self,
        f: BgrView<'_>,
        p: MatchResult,
    ) -> Result<Vec<([usize; 2], f64)>, ActionError> {
        GemVision::occupied_cube_cells(self, f, p)
    }
    fn cube_result_icon(
        &self,
        f: BgrView<'_>,
        p: MatchResult,
        q: [i64; 2],
    ) -> Result<BgrFrame, ActionError> {
        GemVision::cube_result_icon(self, f, p, q)
    }
    fn expected_result(&self, c: usize, l: usize, i: BgrView<'_>) -> Result<bool, ActionError> {
        GemVision::expected_result(self, c, l, i)
    }
}
pub fn build_plan(
    counts: &GemCounts,
    blocked: &BTreeSet<(usize, usize)>,
) -> Result<Vec<GemPlanStep>, ActionError> {
    let mut working = *counts;
    let mut plan = Vec::new();
    for (col, gem_name) in GEM_NAMES.iter().enumerate() {
        for (row, level_name) in LEVEL_NAMES.iter().enumerate().take(4) {
            if blocked.contains(&(col, row)) {
                continue;
            }
            let repeats = working[row][col].saturating_sub(9).div_ceil(3);
            if repeats == 0 {
                continue;
            }
            working[row][col] -= repeats * 3;
            working[row + 1][col] = working[row + 1][col]
                .checked_add(repeats)
                .ok_or_else(|| ActionError::runtime("gem count overflow"))?;
            plan.push(GemPlanStep {
                gem_name,
                level_name,
                col,
                row,
                repeats,
            });
        }
    }
    Ok(plan)
}
pub fn successful_combine(
    counts: &GemCounts,
    row: usize,
    col: usize,
) -> Result<GemCounts, ActionError> {
    if row >= 4 || col >= 7 {
        return Err(ActionError::runtime("non-perfect gem slot required"));
    }
    let mut updated = *counts;
    updated[row][col] = updated[row][col].saturating_sub(3);
    updated[row + 1][col] = updated[row + 1][col]
        .checked_add(1)
        .ok_or_else(|| ActionError::runtime("gem count overflow"))?;
    Ok(updated)
}
pub fn consensus_counts(
    samples: &[FrameCounts; 3],
    fallback: Option<&GemCounts>,
) -> Result<(GemCounts, Vec<ActionEvent>), ActionError> {
    if samples.iter().any(|sample| {
        sample
            .confidences
            .iter()
            .flatten()
            .chain(sample.icon_scores.iter().flatten())
            .any(|n| !n.is_finite())
    }) {
        return Err(ActionError::runtime("finite gem scan scores required"));
    }
    let mut resolved = [[0; 7]; 5];
    let mut invalid = Vec::new();
    let mut corrected = Vec::new();
    for row in 0..5 {
        for col in 0..7 {
            let mut votes: Vec<(u32, f64)> = Vec::new();
            for sample in samples {
                if sample.icon_scores[row][col] < 0.42 {
                    continue;
                }
                let value = sample.counts[row][col];
                let weight = sample.icon_scores[row][col] + sample.confidences[row][col];
                if let Some(vote) = votes.iter_mut().find(|v| v.0 == value) {
                    vote.1 += weight;
                } else {
                    votes.push((value, weight));
                }
            }
            let mut best = None;
            for vote in votes {
                if best.is_none_or(|(_, score)| vote.1 > score) {
                    best = Some(vote);
                }
            }
            resolved[row][col] = if let Some((value, _)) = best {
                value
            } else {
                invalid.push(format!("{} {}", LEVEL_NAMES[row], GEM_NAMES[col]));
                fallback.map_or(samples[0].counts[row][col], |f| f[row][col])
            };
            if let Some(fallback) = fallback {
                let tracked = fallback[row][col];
                let reread = resolved[row][col];
                if reread.abs_diff(tracked) > 6 {
                    resolved[row][col] = tracked;
                    corrected.push(format!(
                        "{} {} {tracked}->{reread}",
                        LEVEL_NAMES[row], GEM_NAMES[col]
                    ));
                }
            }
        }
    }
    let mut events = Vec::new();
    if !corrected.is_empty() {
        events.push(ActionEvent {
            level: EventLevel::Warning,
            message: format!(
                "Count resync kept tracked values on {} slot(s): {}",
                corrected.len(),
                corrected.join(", ")
            ),
        });
    }
    if !invalid.is_empty() {
        events.push(ActionEvent {
            level: EventLevel::Warning,
            message: format!(
                "Count scan fell back on {} slot(s): {}",
                invalid.len(),
                invalid.join(", ")
            ),
        });
    }
    events.push(ActionEvent {
        level: EventLevel::Info,
        message: format!("Detected gem counts (consensus): {resolved:?}"),
    });
    Ok((resolved, events))
}

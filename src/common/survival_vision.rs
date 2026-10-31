//! 현재 프레임의 HUD 근거와 이미지 판독기. 게임 입력을 보내지 않는다.

mod image;
mod manifest;
mod numbers;
pub use image::{HudObserverOptions, SurvivalHudObserver};

use super::buffing::BeltVisibility;
use super::config::ConfigError;
use super::survival::{
    BeltContents, BeltReading, CharacterSurvivalPolicy, Location, ObservationStamp,
    SafetyObservation,
};
use super::time;
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize)]
pub struct ResourceReading {
    current: Option<u64>,
    maximum: Option<u64>,
    pub reason: String,
    pub score: Option<f64>,
    lower_bound: Option<f64>,
}
impl Default for ResourceReading {
    fn default() -> Self {
        Self::unknown("unverified", None)
    }
}
impl ResourceReading {
    pub fn unknown(reason: impl Into<String>, score: Option<f64>) -> Self {
        Self {
            current: None,
            maximum: None,
            reason: reason.into(),
            score,
            lower_bound: None,
        }
    }
    pub fn from_counts(
        current: u64,
        maximum: u64,
        reason: impl Into<String>,
        score: Option<f64>,
    ) -> Self {
        if maximum == 0 || current > maximum {
            return Self::unknown("number_out_of_range", score);
        }
        Self {
            current: Some(current),
            maximum: Some(maximum),
            reason: reason.into(),
            score,
            lower_bound: None,
        }
    }
    pub fn from_text(value: &str, score: f64) -> Self {
        let parse = |text: &str| {
            if text.is_empty()
                || text.len() > 7
                || !text.bytes().all(|byte| byte.is_ascii_digit())
                || (text.len() > 1 && text.starts_with('0'))
            {
                None
            } else {
                text.parse::<u64>().ok()
            }
        };
        let Some((current, maximum)) = value
            .split_once('/')
            .and_then(|(current, maximum)| parse(current).zip(parse(maximum)))
        else {
            return Self::unknown("number_unreadable", Some(score));
        };
        Self::from_counts(current, maximum, "read", Some(score))
    }
    pub fn with_lower_bound(
        value: f64,
        reason: impl Into<String>,
        score: Option<f64>,
    ) -> Result<Self, ConfigError> {
        time::nonnegative(value, "resource lower bound")?;
        if value > 1.0 {
            return Err(ConfigError::value("resource lower bound is out of range"));
        }
        Ok(Self {
            lower_bound: Some(value),
            ..Self::unknown(reason, score)
        })
    }
    pub fn ratio(&self) -> Option<f64> {
        self.current
            .zip(self.maximum)
            .map(|(current, maximum)| current as f64 / maximum as f64)
            .or(self.lower_bound)
    }
    pub fn current(&self) -> Option<u64> {
        self.current
    }
    pub fn maximum(&self) -> Option<u64> {
        self.maximum
    }
    pub fn lower_bound(&self) -> Option<f64> {
        self.lower_bound
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SlotReading {
    pub row: u8,
    pub column: u8,
    pub kind: Option<String>,
    pub distance: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct HudReading {
    pub layout_id: String,
    pub revision: String,
    pub life: ResourceReading,
    pub mana: ResourceReading,
    pub belt_status: String,
    pub slots: Vec<SlotReading>,
    pub belt_visibility: BeltVisibility,
    pub mercenary_status: Option<String>,
}
/// 읽힌 위험 비율을 숫자 미확인이나 열린 벨트보다 먼저 처리한다.
pub fn resource_guard(
    reading: &HudReading,
    policy: &CharacterSurvivalPolicy,
) -> Option<&'static str> {
    let life = reading.life.ratio();
    let mana = reading.mana.ratio();
    if life.is_some_and(|ratio| ratio < policy.life_potion_below())
        || mana.is_some_and(|ratio| ratio < policy.mana_potion_below())
    {
        return Some("resource_low");
    }
    if reading.belt_visibility != BeltVisibility::Closed || life.is_none() || mana.is_none() {
        return Some("observation_unverified");
    }
    if reading.mercenary_status.as_deref() == Some("unverified") {
        return Some("mercenary_unverified");
    }
    None
}
impl HudReading {
    pub fn belt_contents(&self, policy: &CharacterSurvivalPolicy) -> Option<BeltContents> {
        if self.belt_status != "read" || self.slots.len() != 16 {
            return None;
        }
        let positions: BTreeSet<_> = self
            .slots
            .iter()
            .map(|slot| (slot.row, slot.column))
            .collect();
        if positions.len() != 16
            || positions
                .iter()
                .any(|&(row, column)| !(1..=4).contains(&row) || !(1..=4).contains(&column))
        {
            return None;
        }
        let mut contents = BeltContents::new();
        for column in policy.belt_columns() {
            let mut slots: Vec<_> = self
                .slots
                .iter()
                .filter(|slot| slot.column == column.column())
                .collect();
            slots.sort_by_key(|slot| slot.row);
            if slots.len() != 4 || slots.iter().any(|slot| slot.kind.is_none()) {
                return None;
            }
            let inactive = 4 - usize::from(column.capacity());
            if slots[..inactive]
                .iter()
                .any(|slot| slot.kind.as_deref() != Some("empty"))
                || slots[inactive..].iter().any(|slot| {
                    !matches!(slot.kind.as_deref(), Some("empty"))
                        && slot.kind.as_deref() != Some(column.kind())
                })
            {
                return None;
            }
            let count = slots[inactive..]
                .iter()
                .filter(|slot| slot.kind.as_deref() == Some(column.kind()))
                .count();
            contents.insert(
                column.column(),
                BeltReading {
                    kind: if count > 0 {
                        Some(column.kind().into())
                    } else {
                        None
                    },
                    count: i64::try_from(count).ok()?,
                },
            );
        }
        Some(contents)
    }
    pub fn safety_observation(
        &self,
        stamp: ObservationStamp,
    ) -> Result<SafetyObservation, ConfigError> {
        SafetyObservation::new(
            stamp,
            self.life.ratio(),
            self.mana.ratio(),
            Location::Unknown,
            None,
        )
    }
}

#[derive(Debug, Clone)]
pub struct HudEvidence {
    pub stamp: ObservationStamp,
    pub reading: HudReading,
}

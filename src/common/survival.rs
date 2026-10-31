//! Confirmed character/room state. A requested input never changes stock or buff timers.

use super::config::{ConfigError, schema};
use super::time;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TimedBuffPolicy {
    pub(crate) name: String,
    pub(crate) key: String,
    pub(crate) duration_seconds: f64,
    pub(crate) renew_before_seconds: f64,
}

impl TimedBuffPolicy {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn key(&self) -> &str {
        &self.key
    }
    pub fn duration_seconds(&self) -> f64 {
        self.duration_seconds
    }
    pub fn renew_before_seconds(&self) -> f64 {
        self.renew_before_seconds
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BeltColumnPolicy {
    column: u8,
    key: String,
    kind: String,
    capacity: u8,
    target_count: Option<u8>,
}
impl BeltColumnPolicy {
    pub fn column(&self) -> u8 {
        self.column
    }
    pub fn key(&self) -> &str {
        &self.key
    }
    pub fn kind(&self) -> &str {
        &self.kind
    }
    pub fn capacity(&self) -> u8 {
        self.capacity
    }
    pub fn target_count(&self) -> Option<u8> {
        self.target_count
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PotionPickupPolicy {
    preferred_kind: String,
    fallback_kind: Option<String>,
    fallback_at_or_below: Option<u8>,
}
impl PotionPickupPolicy {
    pub fn preferred_kind(&self) -> &str {
        &self.preferred_kind
    }
    pub fn fallback_kind(&self) -> Option<&str> {
        self.fallback_kind.as_deref()
    }
    pub fn fallback_at_or_below(&self) -> Option<u8> {
        self.fallback_at_or_below
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct PolicyData {
    life_potion_below: f64,
    mana_potion_below: f64,
    potion_kind: String,
    replenish_at_or_below: u8,
    exit_on_empty: bool,
    belt_toggle_key: String,
    belt_columns: Vec<BeltColumnPolicy>,
    buffs: Vec<TimedBuffPolicy>,
    potion_pickup: Option<PotionPickupPolicy>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct CharacterSurvivalPolicy(PolicyData);

impl CharacterSurvivalPolicy {
    pub fn life_potion_below(&self) -> f64 {
        self.0.life_potion_below
    }
    pub fn mana_potion_below(&self) -> f64 {
        self.0.mana_potion_below
    }
    pub fn potion_kind(&self) -> &str {
        &self.0.potion_kind
    }
    pub fn potion_pickup(&self) -> Option<&PotionPickupPolicy> {
        self.0.potion_pickup.as_ref()
    }
    pub fn from_value(raw: &Value) -> Result<Self, ConfigError> {
        let normalized = schema::survival(raw)?;
        serde_json::from_value(normalized)
            .map(Self)
            .map_err(|error| ConfigError::value(error.to_string()))
    }
    pub fn buffs(&self) -> &[TimedBuffPolicy] {
        &self.0.buffs
    }
    pub fn belt_columns(&self) -> &[BeltColumnPolicy] {
        &self.0.belt_columns
    }
    pub fn belt_toggle_key(&self) -> &str {
        &self.0.belt_toggle_key
    }
    pub(crate) fn exit_on_empty(&self) -> bool {
        self.0.exit_on_empty
    }
    pub(crate) fn potion_target(&self) -> u8 {
        self.0
            .belt_columns
            .iter()
            .filter(|column| column.kind == self.0.potion_kind)
            .filter_map(|column| column.target_count)
            .sum()
    }
    pub(crate) fn replenish_at_or_below(&self) -> u8 {
        self.0.replenish_at_or_below
    }
    pub(crate) fn potion_column(&self, number: u8, key: &str) -> Option<&str> {
        self.0
            .belt_columns
            .iter()
            .find(|column| column.column == number && column.key == key)
            .map(|column| column.kind.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ObservationStamp {
    character_id: String,
    room_id: String,
    sequence: u64,
    observed_at: f64,
}

impl ObservationStamp {
    pub fn new(
        character_id: impl Into<String>,
        room_id: impl Into<String>,
        sequence: u64,
        observed_at: f64,
    ) -> Result<Self, ConfigError> {
        let character_id = character_id.into();
        let room_id = room_id.into();
        time::text(&character_id, "character_id")?;
        time::text(&room_id, "room_id")?;
        time::nonnegative(observed_at, "observed_at")?;
        Ok(Self {
            character_id,
            room_id,
            sequence,
            observed_at,
        })
    }
    pub fn character_id(&self) -> &str {
        &self.character_id
    }
    pub fn room_id(&self) -> &str {
        &self.room_id
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn observed_at(&self) -> f64 {
        self.observed_at
    }
    pub fn newer_than(&self, previous: Option<&Self>) -> bool {
        previous.is_none_or(|previous| {
            self.sequence > previous.sequence && self.observed_at >= previous.observed_at
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Location {
    Field,
    Town,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct SafetyObservation {
    stamp: ObservationStamp,
    life_ratio: Option<f64>,
    mana_ratio: Option<f64>,
    location: Location,
    monsters_clear: Option<bool>,
}

impl SafetyObservation {
    pub fn new(
        stamp: ObservationStamp,
        life_ratio: Option<f64>,
        mana_ratio: Option<f64>,
        location: Location,
        monsters_clear: Option<bool>,
    ) -> Result<Self, ConfigError> {
        for (name, value) in [("life_ratio", life_ratio), ("mana_ratio", mana_ratio)] {
            if let Some(value) = value {
                time::nonnegative(value, name)?;
                if value > 1.0 {
                    return Err(ConfigError::value(format!("{name} is out of range")));
                }
            }
        }
        Ok(Self {
            stamp,
            life_ratio,
            mana_ratio,
            location,
            monsters_clear,
        })
    }
    pub fn stamp(&self) -> &ObservationStamp {
        &self.stamp
    }
    pub fn location(&self) -> Location {
        self.location
    }
    pub fn monsters_clear(&self) -> Option<bool> {
        self.monsters_clear
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurvivalAction {
    Hold,
    Exit,
    UsePotion,
    Buff,
    Replenish,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObservationLimits {
    screen_age: f64,
    belt_age: f64,
}

impl ObservationLimits {
    pub fn new(screen_age: f64, belt_age: f64) -> Result<Self, ConfigError> {
        time::positive(screen_age, "max_observation_age_seconds")?;
        time::positive(belt_age, "max_belt_age_seconds")?;
        Ok(Self {
            screen_age,
            belt_age,
        })
    }
    pub fn screen_age(self) -> f64 {
        self.screen_age
    }
    pub fn belt_age(self) -> f64 {
        self.belt_age
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SurvivalDecision {
    pub action: SurvivalAction,
    pub reason: &'static str,
    pub potion_key: Option<String>,
    pub buffs_due: Vec<String>,
    pub deficits: Vec<(u8, u8)>,
    pub replenishment_required: bool,
}

impl SurvivalDecision {
    pub(crate) fn hold(reason: &'static str) -> Self {
        Self {
            action: SurvivalAction::Hold,
            reason,
            potion_key: None,
            buffs_due: Vec::new(),
            deficits: Vec::new(),
            replenishment_required: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BeltReading {
    pub kind: Option<String>,
    pub count: i64,
}
pub type BeltContents = BTreeMap<u8, BeltReading>;

#[derive(Debug)]
pub struct CharacterSurvivalState {
    identity: Arc<()>,
    character_id: String,
    room_id: String,
    policy: Option<Arc<CharacterSurvivalPolicy>>,
    targets: BTreeMap<u8, Option<u8>>,
    counts: Option<BTreeMap<u8, u8>>,
    belt_stamp: Option<ObservationStamp>,
    buff_stamps: BTreeMap<String, ObservationStamp>,
}

impl CharacterSurvivalState {
    pub fn new(
        character_id: impl Into<String>,
        room_id: impl Into<String>,
        policy: Option<Arc<CharacterSurvivalPolicy>>,
    ) -> Result<Self, ConfigError> {
        let character_id = character_id.into();
        let room_id = room_id.into();
        time::text(&character_id, "character_id")?;
        time::text(&room_id, "room_id")?;
        let targets = policy
            .as_ref()
            .map(|policy| {
                policy
                    .0
                    .belt_columns
                    .iter()
                    .map(|column| (column.column, column.target_count))
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            identity: Arc::new(()),
            character_id,
            room_id,
            policy,
            targets,
            counts: None,
            belt_stamp: None,
            buff_stamps: BTreeMap::new(),
        })
    }
    pub fn character_id(&self) -> &str {
        &self.character_id
    }
    pub fn room_id(&self) -> &str {
        &self.room_id
    }
    pub fn policy(&self) -> Option<&CharacterSurvivalPolicy> {
        self.policy.as_deref()
    }
    pub(crate) fn shared_policy(&self) -> Option<Arc<CharacterSurvivalPolicy>> {
        self.policy.clone()
    }
    pub(crate) fn identity(&self) -> &Arc<()> {
        &self.identity
    }
    pub fn same_context(&self, stamp: &ObservationStamp) -> bool {
        stamp.character_id == self.character_id && stamp.room_id == self.room_id
    }
    pub fn belt_counts(&self) -> Option<&BTreeMap<u8, u8>> {
        self.counts.as_ref()
    }
    pub fn potions_remaining(&self) -> Option<u8> {
        let counts = self.counts.as_ref()?;
        let policy = &self.policy.as_ref()?.0;
        Some(
            policy
                .belt_columns
                .iter()
                .filter(|column| column.kind == policy.potion_kind)
                .map(|column| counts[&column.column])
                .sum(),
        )
    }
    fn accept_belt_stamp(&mut self, stamp: &ObservationStamp) -> bool {
        if self.policy.is_none()
            || !self.same_context(stamp)
            || !stamp.newer_than(self.belt_stamp.as_ref())
        {
            return false;
        }
        // Admission consumes the stamp even when the newer contents are invalid.
        self.belt_stamp = Some(stamp.clone());
        true
    }
    pub fn observe_belt(
        &mut self,
        stamp: &ObservationStamp,
        contents: Option<&BeltContents>,
    ) -> bool {
        if !self.accept_belt_stamp(stamp) {
            return false;
        }
        self.counts = None;
        let Some(contents) = contents else {
            return false;
        };
        let Some(policy) = &self.policy else {
            return false;
        };
        if contents.len() != policy.0.belt_columns.len() {
            return false;
        }
        let mut counts = BTreeMap::new();
        for column in &policy.0.belt_columns {
            let Some(value) = contents.get(&column.column) else {
                return false;
            };
            if value.count < 0
                || value.count > i64::from(column.capacity)
                || (value.count > 0 && value.kind.as_deref() != Some(&column.kind))
            {
                return false;
            }
            let Ok(count) = u8::try_from(value.count) else {
                return false;
            };
            counts.insert(column.column, count);
        }
        for (&number, &count) in &counts {
            if self.targets.get(&number) == Some(&None) {
                self.targets.insert(number, Some(count));
            }
        }
        self.counts = Some(counts);
        true
    }
    pub fn confirm_resource_change(
        &mut self,
        stamp: &ObservationStamp,
        column: u8,
        kind: &str,
        delta: i64,
    ) -> bool {
        if !self.accept_belt_stamp(stamp) {
            return false;
        }
        let Some(counts) = &mut self.counts else {
            return false;
        };
        let rule = self.policy.as_ref().and_then(|policy| {
            policy
                .0
                .belt_columns
                .iter()
                .find(|rule| rule.column == column)
        });
        let count = counts
            .get(&column)
            .and_then(|count| i64::from(*count).checked_add(delta));
        let valid = rule.zip(count).filter(|(rule, count)| {
            kind == rule.kind && delta != 0 && *count >= 0 && *count <= i64::from(rule.capacity)
        });
        if let Some((_, count)) = valid
            && let Ok(count) = u8::try_from(count)
        {
            counts.insert(column, count);
            return true;
        }
        self.counts = None;
        false
    }
    pub fn invalidate_belt(&mut self, stamp: &ObservationStamp) -> bool {
        if !self.accept_belt_stamp(stamp) {
            return false;
        }
        self.counts = None;
        true
    }
    pub fn confirm_buff(
        &mut self,
        stamp: &ObservationStamp,
        name: &str,
        location: Location,
        monsters_clear: Option<bool>,
    ) -> bool {
        if !self.same_context(stamp)
            || !self
                .policy
                .as_ref()
                .is_some_and(|policy| policy.0.buffs.iter().any(|buff| buff.name == name))
        {
            return false;
        }
        if location != Location::Field
            || monsters_clear != Some(true)
            || !stamp.newer_than(self.buff_stamps.get(name))
        {
            return false;
        }
        self.buff_stamps.insert(name.into(), stamp.clone());
        true
    }
    pub fn next_buff_renewal_at(&self) -> Option<f64> {
        let policy = &self.policy.as_ref()?.0;
        let mut next: Option<f64> = None;
        for buff in &policy.buffs {
            let stamp = self.buff_stamps.get(&buff.name)?;
            let due = stamp.observed_at + buff.duration_seconds - buff.renew_before_seconds;
            next = Some(next.map_or(due, |previous| previous.min(due)));
        }
        next
    }
    pub fn potion_pickup_kinds(
        &self,
        now: f64,
        max_belt_age_seconds: f64,
    ) -> Result<Vec<String>, ConfigError> {
        time::nonnegative(now, "now")?;
        time::nonnegative(max_belt_age_seconds, "max_belt_age_seconds")?;
        if max_belt_age_seconds == 0.0 {
            return Err(ConfigError::value("positive belt age limit required"));
        }
        let Some(policy) = self.policy.as_ref().map(|policy| &policy.0) else {
            return Ok(Vec::new());
        };
        let Some(pickup) = &policy.potion_pickup else {
            return Ok(Vec::new());
        };
        let Some(stamp) = &self.belt_stamp else {
            return Ok(Vec::new());
        };
        let Some(count) = self.potions_remaining() else {
            return Ok(Vec::new());
        };
        let age = now - stamp.observed_at;
        let target: u8 = policy
            .belt_columns
            .iter()
            .filter(|column| column.kind == policy.potion_kind)
            .filter_map(|column| column.target_count)
            .sum();
        if age < 0.0
            || age > max_belt_age_seconds
            || count >= target
            || (count == 0 && policy.exit_on_empty)
        {
            return Ok(Vec::new());
        }
        let mut kinds = vec![pickup.preferred_kind.clone()];
        if let Some(kind) = &pickup.fallback_kind
            && pickup
                .fallback_at_or_below
                .is_some_and(|limit| count <= limit)
        {
            kinds.push(kind.clone());
        }
        Ok(kinds)
    }
    pub fn decide(
        &self,
        observation: &SafetyObservation,
        now: f64,
        max_observation_age_seconds: f64,
        max_belt_age_seconds: f64,
    ) -> Result<SurvivalDecision, ConfigError> {
        time::nonnegative(now, "now")?;
        time::positive(max_observation_age_seconds, "max_observation_age_seconds")?;
        time::positive(max_belt_age_seconds, "max_belt_age_seconds")?;
        let Some(policy) = &self.policy else {
            return Ok(SurvivalDecision::hold("policy_unconfigured"));
        };
        let policy = &policy.0;
        let stamp = &observation.stamp;
        if !self.same_context(stamp) {
            return Ok(SurvivalDecision::hold("wrong_context"));
        }
        let age = now - stamp.observed_at;
        if age < 0.0 || age > max_observation_age_seconds {
            return Ok(SurvivalDecision::hold("observation_stale"));
        }
        if self
            .buff_stamps
            .values()
            .chain(self.belt_stamp.iter())
            .any(|known| stamp.sequence < known.sequence || stamp.observed_at < known.observed_at)
        {
            return Ok(SurvivalDecision::hold("observation_precedes_state"));
        }
        let Some((counts, belt_stamp)) = self.counts.as_ref().zip(self.belt_stamp.as_ref()) else {
            return Ok(SurvivalDecision::hold("belt_unknown"));
        };
        if now - belt_stamp.observed_at > max_belt_age_seconds {
            return Ok(SurvivalDecision::hold("belt_stale"));
        }
        let potions: u8 = policy
            .belt_columns
            .iter()
            .filter(|column| column.kind == policy.potion_kind)
            .map(|column| counts[&column.column])
            .sum();
        let mut result = SurvivalDecision::hold("ready");
        result.deficits = self
            .targets
            .iter()
            .filter_map(|(&number, &target)| {
                target
                    .filter(|&target| target > counts[&number])
                    .map(|target| (number, target - counts[&number]))
            })
            .collect();
        result.replenishment_required = potions <= policy.replenish_at_or_below;
        if potions == 0 {
            result.action = if policy.exit_on_empty {
                SurvivalAction::Exit
            } else {
                SurvivalAction::Hold
            };
            result.reason = "potions_empty";
            result.replenishment_required = true;
            return Ok(result);
        }
        let Some((life, mana)) = observation.life_ratio.zip(observation.mana_ratio) else {
            result.reason = "hud_unknown";
            return Ok(result);
        };
        if life < policy.life_potion_below || mana < policy.mana_potion_below {
            let column = policy
                .belt_columns
                .iter()
                .filter(|column| column.kind == policy.potion_kind && counts[&column.column] > 0)
                .min_by_key(|column| column.column)
                .ok_or_else(|| {
                    ConfigError::value("confirmed potion count has no nonempty column")
                })?;
            result.action = SurvivalAction::UsePotion;
            result.reason = "resource_low";
            result.potion_key = Some(column.key.clone());
            return Ok(result);
        }
        result.buffs_due = policy
            .buffs
            .iter()
            .filter(|buff| {
                self.buff_stamps.get(&buff.name).is_none_or(|cast| {
                    now >= cast.observed_at + buff.duration_seconds - buff.renew_before_seconds
                })
            })
            .map(|buff| buff.name.clone())
            .collect();
        if !result.buffs_due.is_empty() {
            if observation.location != Location::Field {
                result.reason = "buff_requires_field";
            } else if observation.monsters_clear != Some(true) {
                result.reason = "buff_unsafe";
            } else {
                result.action = SurvivalAction::Buff;
                result.reason = "buff_due";
            }
        } else if observation.location == Location::Unknown {
            result.reason = "location_unknown";
        } else if result.replenishment_required {
            result.action = SurvivalAction::Replenish;
            result.reason = "potions_low";
        } else {
            result.action = SurvivalAction::Continue;
        }
        Ok(result)
    }
}

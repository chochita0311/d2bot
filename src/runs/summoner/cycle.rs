//! 긍정 관찰만 소비하는 독립 방 순환 계획기. 현재 main의 반복 실행기와 구분한다.
use super::navigation::Wing;
use crate::common::{config::ConfigError, survival::ObservationStamp, time};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[cfg(test)]
mod tests;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandKind {
    CreateRoom,
    EnterArcane,
    PrepareField,
    SearchWing,
    FightBoss,
    InspectLoot,
    ReturnHub,
    EnterCanyon,
    ReturnAct1,
    Restock,
    ExitRoom,
}
impl CommandKind {
    pub const ALL: [Self; 11] = [
        Self::CreateRoom,
        Self::EnterArcane,
        Self::PrepareField,
        Self::SearchWing,
        Self::FightBoss,
        Self::InspectLoot,
        Self::ReturnHub,
        Self::EnterCanyon,
        Self::ReturnAct1,
        Self::Restock,
        Self::ExitRoom,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::CreateRoom => "create_room",
            Self::EnterArcane => "enter_arcane",
            Self::PrepareField => "prepare_field",
            Self::SearchWing => "search_wing",
            Self::FightBoss => "fight_boss",
            Self::InspectLoot => "inspect_loot",
            Self::ReturnHub => "return_hub",
            Self::EnterCanyon => "enter_canyon",
            Self::ReturnAct1 => "return_act1",
            Self::Restock => "restock",
            Self::ExitRoom => "exit_room",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CycleEvent {
    Unknown,
    Failed,
    CharacterSelect,
    Act1Ready,
    ArcaneReady,
    FieldReady,
    WingEmptyConfirmed,
    SummonerAlive,
    SummonerDeadConfirmed,
    LootComplete,
    HubReady,
    CanyonReady,
    RestockComplete,
}
#[derive(Clone)]
pub struct CycleLimits {
    screen_age: f64,
    room_seconds: f64,
    total_seconds: f64,
    stages: BTreeMap<CommandKind, f64>,
}
impl CycleLimits {
    pub fn new(
        screen_age: f64,
        room_seconds: f64,
        total_seconds: f64,
        stages: BTreeMap<CommandKind, f64>,
    ) -> Result<Self, ConfigError> {
        for (name, value) in [
            ("screen_age", screen_age),
            ("room_seconds", room_seconds),
            ("total_seconds", total_seconds),
        ] {
            time::positive(value, name)?;
        }
        if stages.len() != CommandKind::ALL.len()
            || CommandKind::ALL
                .iter()
                .any(|kind| !stages.contains_key(kind))
        {
            return Err(ConfigError::value(
                "explicit timeout for every cycle command required",
            ));
        }
        for (kind, value) in &stages {
            time::nonnegative(*value, kind.name())?;
            if *value == 0.0 {
                return Err(ConfigError::value(format!(
                    "{} timeout must be positive",
                    kind.name()
                )));
            }
        }
        Ok(Self {
            screen_age,
            room_seconds,
            total_seconds,
            stages,
        })
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleEvidence {
    pub generation: u64,
    pub event: CycleEvent,
    pub command_id: Option<u64>,
    #[serde(default)]
    pub keys_dropped: u64,
    #[serde(default)]
    pub keys_acquired: u64,
    #[serde(default)]
    pub reason: String,
}
pub struct CycleObservation {
    stamp: ObservationStamp,
    evidence: CycleEvidence,
}
impl CycleObservation {
    pub fn new(stamp: ObservationStamp, evidence: CycleEvidence) -> Result<Self, ConfigError> {
        if evidence.command_id == Some(0) {
            return Err(ConfigError::value("command_id must be an integer in range"));
        }
        if evidence.keys_acquired > evidence.keys_dropped {
            return Err(ConfigError::value(
                "keys_acquired must be an integer in range",
            ));
        }
        if evidence.event != CycleEvent::LootComplete
            && (evidence.keys_dropped > 0 || evidence.keys_acquired > 0)
        {
            return Err(ConfigError::value(
                "key counts require confirmed loot observation",
            ));
        }
        Ok(Self { stamp, evidence })
    }
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CycleCommand {
    pub command_id: u64,
    pub kind: CommandKind,
    pub room_id: String,
    pub wing: Option<Wing>,
    pub issued_at: f64,
    pub deadline: f64,
    pub source_sequence: u64,
    pub generation: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CycleAction {
    Request,
    Wait,
    Release,
}
#[derive(Debug, Clone, Serialize)]
pub struct CycleDirective {
    pub action: CycleAction,
    pub reason: String,
    pub command: Option<CycleCommand>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CycleOutcome {
    Normal,
    ReviewRequired,
}
#[derive(Debug, Clone, Serialize)]
pub struct CycleResult {
    pub room_id: String,
    pub room_entered: bool,
    pub outcome: CycleOutcome,
    pub reason: String,
    pub checked_wings: Vec<Wing>,
    pub boss_wing: Option<Wing>,
    pub keys_dropped: u64,
    pub keys_acquired: u64,
}
pub struct SummonerCycle {
    character_id: String,
    room_ids: Vec<String>,
    limits: CycleLimits,
    started_at: f64,
    room_started_at: f64,
    last_now: f64,
    room_index: usize,
    pending: Option<CycleCommand>,
    results: Vec<CycleResult>,
    checked_wings: Vec<Wing>,
    boss_wing: Option<Wing>,
    keys_dropped: u64,
    keys_acquired: u64,
    done: bool,
    command_id: u64,
    last_sequence: Option<u64>,
    last_observed_at: f64,
    generation: u64,
    wing_index: usize,
    loot_complete: bool,
    room_entered: bool,
    terminal_reason: String,
}
impl SummonerCycle {
    pub fn new(
        character_id: String,
        room_ids: Vec<String>,
        limits: CycleLimits,
        started_at: f64,
    ) -> Result<Self, ConfigError> {
        time::text(&character_id, "character_id")?;
        if room_ids.is_empty() {
            return Err(ConfigError::value("explicit finite room IDs required"));
        }
        for room in &room_ids {
            time::text(room, "room_id")?;
        }
        if room_ids.iter().collect::<BTreeSet<_>>().len() != room_ids.len() {
            return Err(ConfigError::value("room IDs must be unique"));
        }
        time::nonnegative(started_at, "started_at")?;
        Ok(Self {
            character_id,
            room_ids,
            limits,
            started_at,
            room_started_at: started_at,
            last_now: started_at,
            room_index: 0,
            pending: None,
            results: vec![],
            checked_wings: vec![],
            boss_wing: None,
            keys_dropped: 0,
            keys_acquired: 0,
            done: false,
            command_id: 0,
            last_sequence: None,
            last_observed_at: started_at,
            generation: 0,
            wing_index: 0,
            loot_complete: false,
            room_entered: false,
            terminal_reason: String::new(),
        })
    }
    pub fn room_id(&self) -> &str {
        &self.room_ids[self.room_index]
    }
    pub fn pending(&self) -> Option<&CycleCommand> {
        self.pending.as_ref()
    }
    pub fn results(&self) -> &[CycleResult] {
        &self.results
    }
    pub fn done(&self) -> bool {
        self.done
    }
    fn directive(action: CycleAction, reason: impl Into<String>) -> CycleDirective {
        CycleDirective {
            action,
            reason: reason.into(),
            command: None,
        }
    }
    fn record(&mut self, outcome: CycleOutcome, reason: &str) {
        self.results.push(CycleResult {
            room_id: self.room_id().into(),
            room_entered: self.room_entered,
            outcome,
            reason: reason.into(),
            checked_wings: self.checked_wings.clone(),
            boss_wing: self.boss_wing,
            keys_dropped: self.keys_dropped,
            keys_acquired: self.keys_acquired,
        });
    }
    fn halt(&mut self, reason: impl Into<String>) -> CycleDirective {
        let reason = reason.into();
        if !self.done {
            self.record(CycleOutcome::ReviewRequired, &reason);
        }
        self.done = true;
        self.pending = None;
        self.terminal_reason = reason.clone();
        Self::directive(CycleAction::Release, reason)
    }
    fn issue(
        &mut self,
        kind: CommandKind,
        observation: &CycleObservation,
        now: f64,
        wing: Option<Wing>,
    ) -> Result<CycleDirective, ConfigError> {
        self.command_id = self
            .command_id
            .checked_add(1)
            .ok_or_else(|| ConfigError::value("cycle command count exhausted"))?;
        let stage = self.limits.stages.get(&kind).ok_or_else(|| {
            ConfigError::value("explicit timeout for every cycle command required")
        })?;
        let command = CycleCommand {
            command_id: self.command_id,
            kind,
            room_id: self.room_id().into(),
            wing,
            issued_at: now,
            deadline: (now + stage)
                .min(self.room_started_at + self.limits.room_seconds)
                .min(self.started_at + self.limits.total_seconds),
            source_sequence: observation.stamp.sequence(),
            generation: observation.evidence.generation,
        };
        self.pending = Some(command.clone());
        Ok(CycleDirective {
            action: CycleAction::Request,
            reason: kind.name().into(),
            command: Some(command),
        })
    }
    pub fn step(
        &mut self,
        observation: Option<&CycleObservation>,
        now: f64,
        focused: bool,
        stopped: bool,
    ) -> Result<CycleDirective, ConfigError> {
        time::nonnegative(now, "now")?;
        if self.done {
            return Ok(Self::directive(
                CycleAction::Release,
                if self.terminal_reason.is_empty() {
                    "complete"
                } else {
                    &self.terminal_reason
                },
            ));
        }
        if now < self.last_now {
            return Ok(self.halt("clock_regressed"));
        }
        self.last_now = now;
        if stopped || !focused {
            return Ok(self.halt(if stopped { "stopped" } else { "focus_lost" }));
        }
        if now >= self.started_at + self.limits.total_seconds {
            return Ok(self.halt("total_timeout"));
        }
        if now >= self.room_started_at + self.limits.room_seconds {
            return Ok(self.halt("room_timeout"));
        }
        if let Some(command) = self
            .pending
            .as_ref()
            .filter(|command| now >= command.deadline)
        {
            return Ok(self.halt(format!("stage_timeout:{}", command.kind.name())));
        }
        let Some(observation) = observation else {
            return Ok(Self::directive(
                if self.pending.is_some() {
                    CycleAction::Wait
                } else {
                    CycleAction::Release
                },
                "waiting_observation",
            ));
        };
        let stamp = &observation.stamp;
        let evidence = &observation.evidence;
        if stamp.character_id() != self.character_id || stamp.room_id() != self.room_id() {
            return Ok(self.halt("wrong_context"));
        }
        if !(0.0..=self.limits.screen_age).contains(&(now - stamp.observed_at())) {
            return Ok(self.halt("screen_stale"));
        }
        if self
            .last_sequence
            .is_some_and(|sequence| stamp.sequence() <= sequence)
            || stamp.observed_at() < self.last_observed_at
            || evidence.generation < self.generation
        {
            return Ok(Self::directive(CycleAction::Wait, "observation_not_newer"));
        }
        self.last_sequence = Some(stamp.sequence());
        self.last_observed_at = stamp.observed_at();
        self.generation = evidence.generation;
        if evidence.event == CycleEvent::Failed {
            return Ok(self.halt(if evidence.reason.is_empty() {
                "observed_failure"
            } else {
                &evidence.reason
            }));
        }
        let Some(command) = self.pending.clone() else {
            return if evidence.event == CycleEvent::CharacterSelect && evidence.command_id.is_none()
            {
                self.issue(CommandKind::CreateRoom, observation, now, None)
            } else {
                Ok(Self::directive(
                    CycleAction::Release,
                    "initial_select_unconfirmed",
                ))
            };
        };
        if evidence.command_id != Some(command.command_id)
            || stamp.sequence() <= command.source_sequence
            || stamp.observed_at() < command.issued_at
            || evidence.generation < command.generation
        {
            return Ok(Self::directive(CycleAction::Wait, "command_unconfirmed"));
        }
        self.advance(command, observation, now)
    }
    fn advance(
        &mut self,
        command: CycleCommand,
        observation: &CycleObservation,
        now: f64,
    ) -> Result<CycleDirective, ConfigError> {
        use {CommandKind as C, CycleEvent as E};
        let event = observation.evidence.event;
        let following = match (command.kind, event) {
            (C::CreateRoom, E::Act1Ready) => Some(C::EnterArcane),
            (C::EnterArcane, E::ArcaneReady) => Some(C::PrepareField),
            (C::ReturnAct1, E::Act1Ready) => Some(C::Restock),
            (C::Restock, E::RestockComplete) => Some(C::ExitRoom),
            _ => None,
        };
        if let Some(following) = following {
            if command.kind == C::CreateRoom {
                self.room_entered = true;
            }
            return self.issue(following, observation, now, None);
        }
        match (command.kind, event) {
            (C::PrepareField, E::FieldReady) => self.issue(
                C::SearchWing,
                observation,
                now,
                Some(Wing::ALL[self.wing_index]),
            ),
            (C::SearchWing, E::SummonerAlive) => {
                self.boss_wing = command.wing;
                self.issue(C::FightBoss, observation, now, command.wing)
            }
            (C::SearchWing, E::WingEmptyConfirmed) => {
                self.checked_wings.push(
                    command
                        .wing
                        .ok_or_else(|| ConfigError::value("cycle search wing missing"))?,
                );
                self.issue(C::ReturnHub, observation, now, command.wing)
            }
            (C::FightBoss, E::SummonerDeadConfirmed) => {
                self.issue(C::InspectLoot, observation, now, command.wing)
            }
            (C::InspectLoot, E::LootComplete) => {
                self.keys_dropped = observation.evidence.keys_dropped;
                self.keys_acquired = observation.evidence.keys_acquired;
                if self.keys_acquired != self.keys_dropped {
                    return Ok(self.halt("key_pickup_incomplete"));
                }
                self.loot_complete = true;
                self.issue(C::EnterCanyon, observation, now, command.wing)
            }
            (C::EnterCanyon, E::CanyonReady) => self.issue(C::ReturnAct1, observation, now, None),
            (C::ReturnHub, E::HubReady) => {
                self.wing_index += 1;
                if self.wing_index == Wing::ALL.len() {
                    Ok(self.halt("summoner_not_found"))
                } else {
                    self.issue(
                        C::SearchWing,
                        observation,
                        now,
                        Some(Wing::ALL[self.wing_index]),
                    )
                }
            }
            (C::ExitRoom, E::CharacterSelect) => {
                self.record(CycleOutcome::Normal, "act1_restock_and_exit_confirmed");
                if self.room_index + 1 == self.room_ids.len() {
                    self.done = true;
                    self.pending = None;
                    return Ok(Self::directive(CycleAction::Release, "complete"));
                }
                self.room_index += 1;
                self.room_started_at = now;
                self.checked_wings.clear();
                self.boss_wing = None;
                self.keys_dropped = 0;
                self.keys_acquired = 0;
                self.wing_index = 0;
                self.loot_complete = false;
                self.room_entered = false;
                self.issue(C::CreateRoom, observation, now, None)
            }
            _ => Ok(Self::directive(
                CycleAction::Wait,
                "postcondition_unconfirmed",
            )),
        }
    }
}

//! 같은 현재 발판의 순서 있는 시체 쌍. 개별 생존 관찰은 선택적인 연결 근거다.
use super::{appearance_vision::SummonerAppearance, objective_vision::ObjectiveReading};
use crate::common::{
    config::ConfigError,
    encounters::{LifeState, LifeWitness, SceneContext, WitnessProvenance, life_state},
    frame::{PixelRect, Size},
    survival::ObservationStamp,
};
use serde::Serialize;
use std::sync::Arc;
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SummonerLifeReading {
    pub state: LifeState,
    pub generation: u64,
    pub sequence: u64,
    pub acquired_at: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relative_ground: Option<[f64; 2]>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub corpse: Option<CorpseLink>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CorpseLink {
    pub linked_alive_sequence: Option<u64>,
    pub first_dead_sequence: u64,
    pub confirmation_kind: Option<&'static str>,
}
struct PositionedWitness {
    witness: LifeWitness,
    position: [f64; 2],
}
pub struct SummonerLifeTracker {
    character: String,
    room: String,
    generation: u64,
    alive: Option<PositionedWitness>,
    dead: Option<PositionedWitness>,
    anchor_kind: Option<String>,
    last_stamp: Option<(u64, f64)>,
}
impl SummonerLifeTracker {
    pub fn new(character: impl Into<String>, room: impl Into<String>) -> Result<Self, ConfigError> {
        let [character, room] = [character.into(), room.into()];
        ObservationStamp::new(&character, &room, 0, 0.0)?;
        Ok(Self {
            character,
            room,
            generation: 0,
            alive: None,
            dead: None,
            anchor_kind: None,
            last_stamp: None,
        })
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn moved(&mut self) -> Result<(), ConfigError> {
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| ConfigError::value("life generation exhausted"))?;
        self.alive = None;
        self.dead = None;
        self.anchor_kind = None;
        Ok(())
    }
    pub fn observe(
        &mut self,
        alive: Option<&SummonerAppearance>,
        corpse: Option<&SummonerAppearance>,
        platform: Option<&ObjectiveReading>,
        sequence: u64,
        acquired_at: f64,
        now: f64,
    ) -> Result<SummonerLifeReading, ConfigError> {
        let mut result = SummonerLifeReading {
            state: LifeState::Unknown,
            generation: self.generation,
            sequence,
            acquired_at,
            asset_id: None,
            relative_ground: None,
            corpse: None,
        };
        let age = now - acquired_at;
        let anchor = platform.and_then(|p| p.ground_point);
        if !(0.0..0.25).contains(&age)
            || anchor.is_none()
            || platform.is_none()
            || (alive.is_some() && corpse.is_some())
        {
            self.dead = None;
            return Ok(result);
        }
        if self
            .last_stamp
            .is_some_and(|(s, t)| sequence <= s || acquired_at <= t)
        {
            self.dead = None;
            return Ok(result);
        }
        let Some(platform) = platform else {
            return Ok(result);
        };
        let Some(anchor) = anchor else {
            return Ok(result);
        };
        self.last_stamp = Some((sequence, acquired_at));
        if self.anchor_kind.as_deref() != Some(&platform.evidence_kind) {
            self.alive = None;
            self.dead = None;
        }
        self.anchor_kind = Some(platform.evidence_kind.clone());
        let context = Arc::new(SceneContext::new(
            ObservationStamp::new(&self.character, &self.room, sequence, acquired_at)?,
            "arcane-sanctuary",
            self.generation,
            Size::new(1922, 1140).map_err(|e| ConfigError::value(e.to_string()))?,
            PixelRect::new(24, 55, 1900, 900).map_err(|e| ConfigError::value(e.to_string()))?,
        )?);
        let target = format!("{}:summoner", self.room);
        if let Some(alive) = alive {
            let position = std::array::from_fn(|a| alive.ground[a] - anchor[a]);
            let witness = LifeWitness::new(
                Arc::clone(&context),
                target,
                "summoner",
                LifeState::Alive,
                WitnessProvenance::LinkedObservation,
                None,
            )?;
            result.state = life_state(&context, Some(&witness), now, 0.25, None, None)?;
            self.alive = Some(PositionedWitness { witness, position });
            self.dead = None;
            result.asset_id = Some(alive.asset_id.clone());
            result.relative_ground = Some(position);
            return Ok(result);
        }
        let Some(corpse) = corpse else {
            self.dead = None;
            return Ok(result);
        };
        let position = std::array::from_fn(|a| corpse.ground[a] - anchor[a]);
        let linked = if let Some(alive) = &self.alive {
            if context.same_scene(alive.witness.context())
                && alive.witness.context().fresh(now, 12.0)?
                && (position[0] - alive.position[0]).hypot(position[1] - alive.position[1]) <= 120.0
            {
                Some(alive.witness.context().stamp().sequence())
            } else {
                None
            }
        } else {
            None
        };
        let witness = LifeWitness::new(
            Arc::clone(&context),
            target,
            "summoner",
            LifeState::Dead,
            WitnessProvenance::LinkedObservation,
            linked,
        )?;
        if let Some(prior) = &self.dead
            && (!context.same_scene(prior.witness.context())
                || !prior.witness.context().fresh(now, 12.0)?
                || (position[0] - prior.position[0]).hypot(position[1] - prior.position[1]) > 24.0)
        {
            self.dead = None;
        }
        if self.dead.is_some() {
            result.state = LifeState::Dead;
        } else {
            self.dead = Some(PositionedWitness { witness, position });
        }
        if let Some(first) = &self.dead {
            result.corpse = Some(CorpseLink {
                linked_alive_sequence: linked,
                first_dead_sequence: first.witness.context().stamp().sequence(),
                confirmation_kind: (result.state == LifeState::Dead)
                    .then_some("current_platform_corpse_pair"),
            });
        }
        result.asset_id = Some(corpse.asset_id.clone());
        result.relative_ground = Some(position);
        Ok(result)
    }
}

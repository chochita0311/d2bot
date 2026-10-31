//! 외부 입력을 받기 전에 확인하는 Summoner 실행 조건.
use super::navigation::Wing;
use crate::{
    actions::town_loop::Difficulty,
    common::{
        casting::CastingEstimate,
        config::{BotConfig, ConfigError},
        survival::CharacterSurvivalPolicy,
    },
};
use std::{path::PathBuf, sync::Arc};
/// 일반 이동의 반응만 조절한다. 보스 전투/정지 방어는 원래 범위를 쓴다.
pub fn navigation_reaction_radius(config: &BotConfig) -> Result<[f64; 2], ConfigError> {
    let value = config
        .section("run_profiles")
        .and_then(|v| v.get("summoner"))
        .and_then(|v| v.get("hunting"))
        .and_then(|v| v.get("combat_reaction_radius"));
    let Some(value) = value else {
        return Ok([240.0, 144.0]);
    };
    let radius: [f64; 2] = serde_json::from_value(value.clone())
        .map_err(|_| ConfigError::value("summoner combat_reaction_radius requires [rx, ry]"))?;
    if radius
        .iter()
        .zip([300.0, 180.0])
        .any(|(v, max)| !v.is_finite() || *v <= 0.0 || *v > max)
    {
        return Err(ConfigError::value(
            "summoner combat_reaction_radius must be positive and within [300, 180]",
        ));
    }
    Ok(radius)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomapPreference {
    On,
    Preserve,
}
impl AutomapPreference {
    pub fn name(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Preserve => "preserve",
        }
    }
}
#[derive(Debug, Clone)]
pub struct SummonerOptions {
    pub live: bool,
    pub resume_arcane: bool,
    pub casts: u8,
    pub difficulty: Difficulty,
    pub output: PathBuf,
    pub resume_town: bool,
    pub resume_labels_off: bool,
    pub segments: u8,
    pub continuous: bool,
    pub field_seconds: u8,
    pub wing: Wing,
    pub return_from: Option<Wing>,
    pub engage_monsters: bool,
    pub continue_navigation: Option<PathBuf>,
    pub all_wings: bool,
    pub finish_room: bool,
    pub automap: AutomapPreference,
    pub automatic: bool,
    pub character_key: Option<String>,
}
impl SummonerOptions {
    pub fn new(output: PathBuf, difficulty: Difficulty) -> Self {
        Self {
            live: false,
            resume_arcane: false,
            casts: 1,
            difficulty,
            output,
            resume_town: false,
            resume_labels_off: false,
            segments: 1,
            continuous: false,
            field_seconds: 20,
            wing: Wing::North,
            return_from: None,
            engage_monsters: true,
            continue_navigation: None,
            all_wings: false,
            finish_room: false,
            automap: AutomapPreference::On,
            automatic: true,
            character_key: None,
        }
    }
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !(1..=12).contains(&self.casts) {
            return Err(ConfigError::value("north casts must be one to twelve"));
        }
        if self.resume_labels_off && !self.resume_arcane {
            return Err(ConfigError::value(
                "explicit labels OFF assertion requires current Arcane resumption",
            ));
        }
        if !(1..=4).contains(&self.segments) {
            return Err(ConfigError::value(
                "supervised north segments must be one to four",
            ));
        }
        if !(1..=60).contains(&self.field_seconds) {
            return Err(ConfigError::value("field seconds must be one to sixty"));
        }
        if self.continue_navigation.is_some() && !(self.continuous && self.resume_arcane) {
            return Err(ConfigError::value(
                "navigation continuation requires current-field continuous mode",
            ));
        }
        if self.all_wings
            && (!self.continuous || self.wing != Wing::North || self.return_from.is_some())
        {
            return Err(ConfigError::value(
                "all-wing search requires continuous north outbound start",
            ));
        }
        if self.finish_room && (!self.continuous || self.all_wings || self.wing != Wing::North) {
            return Err(ConfigError::value(
                "north-room finish requires continuous north mode without all-wings",
            ));
        }
        if !self.automatic {
            return Err(ConfigError::value(
                "normal Summoner execution uses automatic screen observations; use the separate probe for supervised diagnostics",
            ));
        }
        Ok(())
    }
    pub fn validate_cycles(&self, cycles: u8) -> Result<(), ConfigError> {
        if !(1..=5).contains(&cycles) {
            return Err(ConfigError::value("north room cycles must be one to five"));
        }
        if !self.continuous || self.all_wings || self.wing != Wing::North {
            return Err(ConfigError::value(
                "new-room north tests require continuous north without all-wings",
            ));
        }
        Ok(())
    }
    pub fn selected(&self, config: &BotConfig) -> Result<SelectedRun, ConfigError> {
        self.validate()?;
        let key = self
            .character_key
            .as_deref()
            .filter(|v| !v.is_empty())
            .or_else(|| config.active_character_key())
            .ok_or_else(|| ConfigError::value("selected character profile required"))?;
        let profile = config
            .section("characters")
            .and_then(|v| v.get(key))
            .ok_or_else(|| ConfigError::value("selected character profile required"))?;
        if profile["survival"].is_null() {
            return Err(ConfigError::value(
                "selected character survival policy required",
            ));
        }
        let policy = Arc::new(CharacterSurvivalPolicy::from_value(&profile["survival"])?);
        let estimate = config.casting_estimate_for(Some(key))?;
        if estimate.action != "estimate" || estimate.frames.is_none() {
            return Err(ConfigError::value(
                "configured class/FCR reference required",
            ));
        }
        let character_class = profile["character_class"]
            .as_str()
            .ok_or_else(|| ConfigError::value("configured class/FCR reference required"))?
            .to_owned();
        Ok(SelectedRun {
            character_key: key.into(),
            character_class,
            policy,
            estimate,
        })
    }
    pub fn dry_message(&self) -> String {
        let mode = if self.continuous {
            format!("{} seconds held-F2 wide steering", self.field_seconds)
        } else {
            format!("{} north casts", self.casts)
        };
        format!(
            "DRY: entry -> automatic buff -> {} segment(s) of {mode}; no game input",
            self.segments
        )
    }
}
pub struct SelectedRun {
    pub character_key: String,
    pub character_class: String,
    pub policy: Arc<CharacterSurvivalPolicy>,
    pub estimate: CastingEstimate,
}
impl SelectedRun {
    pub fn reference_seconds(&self) -> Result<f64, ConfigError> {
        let frames = self
            .estimate
            .frames
            .as_ref()
            .and_then(serde_json::Number::as_f64)
            .filter(|v| v.is_finite())
            .ok_or_else(|| ConfigError::value("configured class/FCR reference required"))?;
        Ok(frames / 25.0)
    }
}

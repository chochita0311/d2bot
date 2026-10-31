use super::{FieldControlRuntime, FieldRuntimeOptions};
use crate::common::buffing::{AutomaticBuffRenewal, BuffRenewalLimits, PauseSchedule};
use crate::common::config::{BotConfig, ConfigError};
use crate::common::error::ControlError;
use crate::common::field_control::{
    AdaptiveTeleportCadence, DryFieldInput, FieldControlLoop, FieldInputBackend, FieldLimits,
    HeldFieldInput,
};
use crate::common::survival::{CharacterSurvivalPolicy, CharacterSurvivalState};
use crate::common::time::MonotonicClock;
use serde_json::Value;
use std::sync::Arc;

fn profile<'a>(config: &'a BotConfig, key: &str) -> Result<&'a Value, ConfigError> {
    config
        .section("characters")
        .and_then(|profiles| profiles.get(key))
        .ok_or_else(|| ConfigError::new("KeyError", format!("'{key}'")))
}

/// Defaults to dry input even when the config's legacy dry_run flag is false.
pub fn create_field_control(
    config: &BotConfig,
    character_id: &str,
    room_id: &str,
    limits: FieldLimits,
    clock: Arc<dyn MonotonicClock>,
) -> Result<FieldControlLoop<DryFieldInput>, ControlError> {
    create_field_control_with_backend(
        config,
        character_id,
        room_id,
        limits,
        clock,
        Arc::new(DryFieldInput::default()),
    )
}
pub fn create_field_control_with_backend<B: FieldInputBackend>(
    config: &BotConfig,
    character_id: &str,
    room_id: &str,
    limits: FieldLimits,
    clock: Arc<dyn MonotonicClock>,
    backend: Arc<B>,
) -> Result<FieldControlLoop<B>, ControlError> {
    let profile = profile(config, character_id)?;
    let estimate = config.casting_estimate_for(Some(character_id))?;
    let cadence = AdaptiveTeleportCadence::new(&estimate, limits.arrival_seconds())?;
    let actions = &profile["actions"];
    if actions["movement_travel_mode"] != "hold" {
        return Err(ConfigError::value(
            "this field controller requires an explicit held travel profile",
        )
        .into());
    }
    let inputs = HeldFieldInput::new(
        backend,
        actions["movement_skill_key"].as_str().unwrap_or(""),
        actions["primary_attack_skill_key"].as_str().unwrap_or(""),
        Some(clock),
        None,
        0.0,
    )?;
    let survival = if profile["survival"].is_null() {
        None
    } else {
        Some(Arc::new(CharacterSurvivalPolicy::from_value(
            &profile["survival"],
        )?))
    };
    Ok(FieldControlLoop::new(
        CharacterSurvivalState::new(character_id, room_id, survival)?,
        inputs,
        limits,
        cadence,
    )?)
}
pub fn create_field_runtime(
    config: &BotConfig,
    character_id: &str,
    room_id: &str,
    limits: FieldLimits,
    buff_limits: BuffRenewalLimits,
    options: FieldRuntimeOptions,
) -> Result<FieldControlRuntime<DryFieldInput>, ControlError> {
    create_field_runtime_with_backend(
        config,
        character_id,
        room_id,
        limits,
        buff_limits,
        Arc::new(DryFieldInput::default()),
        options,
    )
}
pub fn create_field_runtime_with_backend<B: FieldInputBackend>(
    config: &BotConfig,
    character_id: &str,
    room_id: &str,
    limits: FieldLimits,
    buff_limits: BuffRenewalLimits,
    backend: Arc<B>,
    options: FieldRuntimeOptions,
) -> Result<FieldControlRuntime<B>, ControlError> {
    let control = create_field_control_with_backend(
        config,
        character_id,
        room_id,
        limits,
        options.clock.clone(),
        backend,
    )?;
    let actions = &profile(config, character_id)?["actions"];
    let order = actions["pre_run_buff_order"]
        .as_array()
        .ok_or_else(|| ConfigError::value("Explicit non-empty buff order required"))?
        .iter()
        .map(|token| {
            token
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| ConfigError::value("Explicit non-empty buff order required"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let pauses = actions["buff_action_pause_seconds"]
        .as_object()
        .ok_or_else(|| ConfigError::value("buff pause overrides must be an object"))?
        .iter()
        .map(|(key, value)| {
            Ok((
                key.clone(),
                value
                    .as_f64()
                    .ok_or_else(|| ConfigError::value("normalized buff pause must be numeric"))?,
            ))
        })
        .collect::<Result<_, ConfigError>>()?;
    let buffs = AutomaticBuffRenewal::new(
        control.state(),
        order,
        PauseSchedule::new(pauses, 0.4)?,
        buff_limits,
        limits.ages(),
    )?;
    FieldControlRuntime::new(control, Some(buffs), options)
}

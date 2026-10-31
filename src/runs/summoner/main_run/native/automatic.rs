use super::{FieldWorld, RecordedInput};
use crate::{
    common::{
        automap_vision::AutomapObserver,
        buffing::PauseSchedule,
        conditional_loot::LabelsState,
        error::ControlError,
        field_control::FieldInputSession,
        field_preparation::{
            AutomaticBuffEnvironment, BuffDispatchOptions, BuffDispatchResult, BuffSafetyFrame,
            BuffSiteGuard, BuffTrace, GuardPreparationError, PreparationChecks,
            PreparationEnvironment, dispatch_buffs,
        },
        frame::BgrFrame,
        survival_vision::{HudReading, resource_guard},
    },
    platform::{CaptureBackend, InputSystem},
    runs::{
        RunError,
        summoner::{
            continuation::{NEAR_GROUND_RADIUS, near_ground},
            encounter_execution::EncounterEvidence,
            field_defense::clear_buff_site,
        },
    },
};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
pub struct BuffRecipe {
    pub order: Vec<String>,
    pub pauses: PauseSchedule,
    pub movement_key: String,
    pub attack_key: String,
    pub reference_seconds: f64,
    pub character_key: String,
    pub engage_monsters: bool,
}
impl BuffRecipe {
    pub fn from_config(
        config: &crate::common::config::BotConfig,
        selected: &crate::runs::summoner::options::SelectedRun,
        engage_monsters: bool,
    ) -> Result<Self, crate::common::config::ConfigError> {
        use crate::common::config::ConfigError;
        let actions = &config
            .section("characters")
            .and_then(|v| v.get(&selected.character_key))
            .ok_or_else(|| ConfigError::value("selected character profile required"))?["actions"];
        let order = actions
            .get("pre_run_buff_order")
            .map(|v| serde_json::from_value::<Vec<String>>(v.clone()))
            .transpose()
            .map_err(|e| ConfigError::value(e.to_string()))?
            .unwrap_or_default();
        let mut pauses = std::collections::BTreeMap::new();
        if let Some(values) = actions["buff_action_pause_seconds"].as_object() {
            for (key, value) in values {
                // 설정 경계가 정리한 선택적 값만 사용하며 누락 값은 기존 0.4초를 따른다.
                if let Some(seconds) = value.as_f64() {
                    pauses.insert(key.to_lowercase(), seconds);
                }
            }
        }
        Ok(Self {
            order: order.into_iter().map(|key| key.to_lowercase()).collect(),
            pauses: PauseSchedule::new(pauses, 0.4)?,
            movement_key: actions["movement_skill_key"]
                .as_str()
                .unwrap_or_default()
                .into(),
            attack_key: actions["primary_attack_skill_key"]
                .as_str()
                .unwrap_or_default()
                .into(),
            reference_seconds: selected.reference_seconds()?,
            character_key: selected.character_key.clone(),
            engage_monsters,
        })
    }
}
pub fn observe_runtime<S: CaptureBackend + InputSystem>(
    world: &mut FieldWorld<'_, S>,
    area: &mut AutomapObserver,
    character: &str,
    evidence: &mut Vec<EncounterEvidence>,
) -> Result<Value, RunError> {
    if PreparationEnvironment::stopped(world) || !PreparationEnvironment::focused(world)? {
        return Err(RunError::Runtime(
            "runtime safety interrupted or unfocused".into(),
        ));
    }
    let acquired = PreparationEnvironment::clock(world);
    let frame = PreparationEnvironment::capture(world)?;
    let reading = PreparationEnvironment::hud(world, &frame)?;
    let blocked = resource_guard(&reading, &world.loot.policy);
    if blocked.is_some() || world.loot.core.panels().check(&frame)? {
        return Err(RunError::Runtime(blocked.unwrap_or("panel_open").into()));
    }
    world
        .loot
        .camera
        .with(|camera| camera.prepare_frame(&frame, acquired, &reading, true))?;
    let player = world
        .loot
        .camera
        .with(|camera| camera.observe(&frame, acquired))?;
    let point = player
        .point
        .ok_or_else(|| RunError::Runtime(player.reason))?;
    let candidates = world.hazards.observe(
        frame.view(),
        point,
        acquired,
        PreparationEnvironment::clock(world),
        None,
    )?;
    for item in candidates {
        if item.ground().is_none()
            || near_ground(item.ground(), point.map(f64::from), NEAR_GROUND_RADIUS)?
        {
            return Err(RunError::Runtime("buff_nearby_hostile".into()));
        }
    }
    if !area.visible(frame.view(), "arcane_sanctuary")? {
        evidence.push(EncounterEvidence {
            filename: "runtime-title-unverified.png".into(),
            frame,
        });
        return Err(RunError::Runtime("current Arcane title unverified".into()));
    }
    if PreparationEnvironment::clock(world) - acquired >= 0.25 {
        return Err(RunError::Runtime("screen_stale".into()));
    }
    Ok(
        json!({"source":"runtime_screen","character_id":character,"area":"arcane_sanctuary","observed_at":PreparationEnvironment::wall_clock(world),"captured_at":acquired,"panels_closed":true,"belt_closed":true,"nearby_candidates":0,"safety_scope":"current_nearby_candidates_and_stable_ratios","player_ground":point,"life_ratio":reading.life.ratio(),"mana_ratio":reading.mana.ratio(),"item_labels_on":world.controller.labels_state()==LabelsState::On}),
    )
}
pub struct AutomaticField<'a, 'b, S: CaptureBackend + InputSystem> {
    pub world: &'a mut FieldWorld<'b, S>,
    pub area: &'a mut AutomapObserver,
    pub recipe: &'a BuffRecipe,
    pub evidence: &'a mut Vec<EncounterEvidence>,
    pub retained_inputs: &'a mut Vec<FieldInputSession<RecordedInput<S>>>,
    pub trace_number: usize,
}
struct Checks<'a> {
    guard: &'a mut BuffSiteGuard,
}
impl<S: CaptureBackend + InputSystem> PreparationChecks<FieldWorld<'_, S>> for Checks<'_> {
    fn check_frame(
        &mut self,
        world: &mut FieldWorld<'_, S>,
        frame: &Arc<BgrFrame>,
        at: f64,
        hud: &HudReading,
    ) -> Result<Option<String>, RunError> {
        let policy = world.loot.policy.clone();
        Ok(self
            .guard
            .wait_check(
                world,
                &policy,
                Some(BuffSafetyFrame {
                    frame,
                    acquired: Some(at),
                    reading: Some(hud),
                }),
            )?
            .map(str::to_owned))
    }
}
impl<S: CaptureBackend + InputSystem> AutomaticBuffEnvironment for AutomaticField<'_, '_, S> {
    type Error = RunError;
    type Guard = BuffSiteGuard;
    fn clock(&self) -> f64 {
        PreparationEnvironment::clock(self.world)
    }
    fn stopped(&self) -> bool {
        PreparationEnvironment::stopped(self.world)
    }
    fn create_guard(&mut self) -> Result<BuffSiteGuard, RunError> {
        Ok(BuffSiteGuard::new(None))
    }
    fn prepare(
        &mut self,
        guard: &mut BuffSiteGuard,
    ) -> Result<Value, GuardPreparationError<RunError>> {
        let policy = self.world.loot.policy.clone();
        guard.prepare(self.world, &policy)
    }
    fn clear_hostiles(&mut self) -> Result<(), RunError> {
        if !self.recipe.engage_monsters {
            return Err(RunError::Runtime(
                "buff-site unsafe while combat disabled".into(),
            ));
        }
        if !self.retained_inputs.is_empty() {
            return Err(RunError::Runtime(
                "previous field input session cleanup incomplete".into(),
            ));
        }
        let policy = self.world.loot.policy.clone();
        let mut inputs = self.world.loot.core.input_session(
            &self.recipe.movement_key,
            &self.recipe.attack_key,
            Some(self.recipe.reference_seconds),
            0.0,
        )?;
        let result = clear_buff_site(&mut inputs, self.world, &policy, self.evidence);
        let cleanup = inputs
            .finish(Duration::from_secs(1))
            .map_err(RunError::from);
        if cleanup.is_err() {
            self.retained_inputs.push(inputs);
        }
        let result = match (result, cleanup) {
            (Ok(v), Ok(())) => v,
            (Err(e), Ok(())) | (Ok(_), Err(e)) => return Err(e),
            (Err(e), Err(c)) => return Err(e.with_cleanup(c)),
        };
        if result.attack_dispatches > 0 {
            self.world.controller.after_combat(
                &mut self.world.loot,
                None,
                "buff_site_combat_ended_scan",
                None,
            )?;
        }
        Ok(())
    }
    fn observe(&mut self) -> Result<Value, RunError> {
        observe_runtime(
            self.world,
            self.area,
            &self.recipe.character_key,
            self.evidence,
        )
    }
    fn dispatch(&mut self, guard: &mut BuffSiteGuard) -> Result<BuffDispatchResult, RunError> {
        let mut trace = vec![];
        let channel = self.world.loot.core.evidence_channel();
        let streaming = channel.is_some();
        let number = self.trace_number;
        let mut save = move |trace: BuffTrace| -> Result<(), ControlError> {
            let index = trace.metadata["index"]
                .as_u64()
                .ok_or_else(|| ControlError::Runtime("buff trace step index required".into()))?;
            let channel = channel
                .as_ref()
                .ok_or_else(|| ControlError::Runtime("buff evidence channel missing".into()))?;
            channel.drain(&mut vec![EncounterEvidence {
                filename: format!("automatic-buff-{number:02}-step-{index:02}.png"),
                frame: trace.frame,
            }]);
            Ok(())
        };
        let policy = self.world.loot.policy.clone();
        let result = dispatch_buffs(
            self.world,
            &policy,
            &self.recipe.order,
            &self.recipe.pauses,
            &mut Checks { guard },
            BuffDispatchOptions {
                trace_frames: if streaming { None } else { Some(&mut trace) },
                trace_sink: if streaming { Some(&mut save) } else { None },
                observation_wait: 2.0,
                sequence_timeout: 12.0,
                trace_before_dispatch: true,
            },
        )?;
        for t in trace {
            let index = t.metadata["index"]
                .as_u64()
                .ok_or_else(|| ControlError::Runtime("buff trace step index required".into()))?;
            self.evidence.push(EncounterEvidence {
                filename: format!(
                    "automatic-buff-{:02}-step-{index:02}.png",
                    self.trace_number
                ),
                frame: t.frame,
            });
        }
        Ok(result)
    }
}

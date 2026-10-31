use super::*;
use crate::{
    actions::{desktop::DesktopActionBackend, town_loop::TownVision},
    common::{
        conditional_loot::LootLimits,
        controller::ControllerFieldInput,
        field_preparation::ensure_automap_on,
        hotkeys::{HotkeyAction, HotkeyBinding},
    },
    platform::windows::focus_window,
    runs::summoner::{
        camera_ground::ReviewedCameraLayout,
        encounter_execution::EncounterEvidence,
        main_run::{
            InitialField,
            entry::{ActionEntry, enter_arcane},
        },
    },
};
use serde_json::json;
impl WindowsSummonerRuntime {
    pub(super) fn initialize_native(
        &mut self,
        options: &SummonerOptions,
        report: &mut Value,
        evidence: &mut Vec<EncounterEvidence>,
    ) -> Result<InitialField, RunError> {
        if self.initialized {
            return Err(missing("fresh Summoner runtime"));
        }
        self.initialized = true;
        self.prepare_live()?;
        if !options.resume_arcane {
            report["timing"]["entry_started_at"] = json!(self.context.clock.now());
            let settings = self.settings.clone();
            let root = self.context.root.clone();
            let clock = self.context.clock.clone();
            let mut entry = ActionEntry::new(
                &mut self.entry,
                TownVision::load(&root)?,
                self.context.entry_directory.clone().unwrap_or_else(|| {
                    PathBuf::from("recordings/summoner/evidence").join(format!(
                        "{}-town-loop",
                        crate::platform::windows::local_timestamp()
                    ))
                }),
                move |_| Ok(DesktopActionBackend::windows(settings, &root)?.with_clock(clock)),
            );
            enter_arcane(
                &mut entry,
                options.difficulty,
                options.resume_town,
                &self.stop,
                |text| (self.context.emit)(text),
            )?;
            report["entry"] = json!("arcane_hub_observed");
            report["timing"]["arcane_arrived_at"] = json!(self.context.clock.now());
        }
        let system = Windows::new()?;
        self.pending_capture = Some(ScreenCapture::new(
            system,
            self.settings.clone(),
            &self.context.root,
        )?);
        let window = self
            .pending_capture
            .as_ref()
            .ok_or_else(|| missing("pending capture"))?
            .configured_window()?;
        let Some(window) = window else {
            self.register_stop_hotkeys()?;
            self.prepare_records(report)?;
            return Err(ControlError::Runtime("focus_lost".into()).into());
        };
        if !focus_window(window.id)? {
            return Err(RunError::Runtime(
                "Could not bring the Diablo window to the foreground from the control panel."
                    .into(),
            ));
        }
        let window = Arc::new(WindowGuard::new(
            Arc::new(system),
            self.settings.clone(),
            window,
        ));
        let stop = self.stop.clone();
        let guard = window.clone();
        let controller = ControllerFieldInput::new(
            Arc::new(system),
            self.gate.clone(),
            window.window().id,
            self.context.clock.clone(),
        )
        .with_guard(move || Ok(!stop.is_set() && guard.focused()?));
        let input = Arc::new(RecordedInput::new(
            controller,
            self.context.clock.clone(),
            Arc::new(FieldCore::<Windows>::wall_now),
        ));
        let capture = self
            .pending_capture
            .take()
            .ok_or_else(|| missing("pending capture"))?;
        self.field.core = Some(FieldCore::new(
            capture,
            FieldServices {
                hud: self.hud.take().ok_or_else(|| missing("HUD observer"))?,
                guards: FieldGuards {
                    clock: self.context.clock.clone(),
                    stop: self.stop.clone(),
                    window: window.clone(),
                },
                input: input.clone(),
                panels: self.panels.clone(),
                emit: self.context.emit.clone(),
            },
        ));
        self.register_stop_hotkeys()?;
        self.prepare_records(report)?;
        self.evidence_stream = Some(EvidenceStream::new(options.output.clone())?);
        let mut world = self.field.world(LootContext::Navigation)?;
        report["item_label_preparation"] = world.controller.prepare(
            &mut world.loot,
            !options.resume_arcane,
            options.resume_labels_off.then_some(false),
        )?;
        if let Some(map) = self.field.map.as_mut() {
            let mut screen = MapScreen {
                map,
                panels: self.panels.clone(),
            };
            let core = self
                .field
                .core
                .as_mut()
                .ok_or_else(|| missing("field capture"))?;
            report["automap"] = serde_json::to_value(ensure_automap_on(
                core,
                &self.field.policy,
                &mut screen,
                None,
                true,
                3.0,
            )?)?;
        } else {
            report["automap"] =
                json!({"status":"preserved_for_diagnostic","visibility":"unverified"});
        }
        self.camera_started = true;
        let acquired = self.context.clock.now();
        let core = self.field.core_mut()?;
        let frame = core.grab()?;
        let reading = core.read_hud(&frame.frame)?;
        let focused = core.guards().window.focused()?;
        let layout = ReviewedCameraLayout::load(
            self.context
                .root
                .join("assets/ui/layouts/remastered-1922x1140/camera-ground.json"),
        )?;
        self.field.camera.with(|camera| {
            camera.confirm_runtime_viewport(
                &frame.frame,
                &reading,
                focused,
                acquired,
                self.context.clock.now(),
                &layout,
            )
        })?;
        if let Some(geometry) = self.field.core()?.guards().window.geometry()? {
            report["frame_geometry"] = geometry.as_value();
        }
        self.area = Some(AutomapObserver::load(
            self.context.root.join("assets/ui/automap"),
        )?);
        report["primary_set"] =
            json!({"status":"main_weapon_start_precondition","inventory_input":false});
        let initial = self.automatic(0, evidence)?;
        report["preparation_safety"] = initial["safety"].clone();
        report["initial_safety"] = initial["safety"].clone();
        report["buff"] = initial["buff"].clone();
        report["automatic_buff"] = initial.clone();
        report["camera_projection"] = json!({"source":"shared_layout_and_current_runtime_viewport","maximum_occlusion_seconds":null});
        (self.context.emit)(
            "AUTOMATIC_BUFF_READY: balanced configured sequence; no inventory or supervisor input; timer uses actual cast requests",
        );
        Ok(InitialField {
            proof: initial["observation"].clone(),
            buff_expires_at: self
                .field
                .deadline
                .ok_or_else(|| missing("buff deadline"))?,
        })
    }
    fn register_stop_hotkeys(&mut self) -> Result<(), RunError> {
        let hotkeys = self
            .context
            .config
            .section("hotkeys")
            .ok_or_else(|| missing("hotkeys"))?;
        let mut keys = vec![];
        for key in [
            hotkeys["stop"].as_str().unwrap_or("f9"),
            hotkeys["pause"].as_str().unwrap_or("f8"),
            "f10",
        ] {
            let key = key.trim().to_lowercase();
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        let bindings = keys
            .into_iter()
            .map(|key| HotkeyBinding::parse(&key, HotkeyAction::Stop))
            .collect::<Result<Vec<_>, _>>()?;
        let stop = self.stop.clone();
        let gate = self.gate.clone();
        self.hotkeys = Some(HotkeyListener::start(
            bindings,
            Arc::new(move |_| {
                stop.request_stop();
                gate.stop();
                Ok(())
            }),
        )?);
        Ok(())
    }
    fn prepare_records(&mut self, report: &mut Value) -> Result<(), RunError> {
        self.backend_ready = true;
        self.field.recovery = Some(EmergencyResourceRecovery::default());
        report["resource_recoveries"] = json!([]);
        let ignored = self
            .context
            .config
            .section("shared_loot")
            .and_then(|v| v["ignore_labels"].as_array())
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        self.field.scanner = Some(LootScanner::new(&self.context.config, &self.context.root)?);
        self.field.loot = Some(ConditionalLootController::new(
            self.field.policy.clone(),
            self.recipe.movement_key.clone(),
            ignored,
            LootLimits::new(3.0, 12, 2, self.recipe.reference_seconds)?,
        ));
        report["loot_scans"] = json!([]);
        report["item_label_policy"] = json!("off_while_moving_on_for_post_combat_scan");
        Ok(())
    }
    pub(super) fn automatic(
        &mut self,
        trace_number: usize,
        evidence: &mut Vec<EncounterEvidence>,
    ) -> Result<Value, RunError> {
        self.check_inputs()?;
        if let Some(stream) = &self.evidence_stream {
            let channel = stream.channel("");
            self.field.core_mut()?.set_navigation_evidence(channel);
        }
        let policy = self.field.policy.clone();
        let mut world = self.field.world(LootContext::Navigation)?;
        let mut env = AutomaticField {
            world: &mut world,
            area: self.area.as_mut().ok_or_else(|| missing("area observer"))?,
            recipe: &self.recipe,
            evidence,
            retained_inputs: &mut self.retained_inputs,
            trace_number,
        };
        let record = self.driver.execute(&mut env, &self.recipe.order, &policy)?;
        self.field.deadline = Some(
            record["preparation_at"]
                .as_f64()
                .filter(|v| v.is_finite())
                .ok_or_else(|| missing("buff deadline"))?,
        );
        Ok(record)
    }
}

use super::*;
use serde_json::json;
#[cfg(test)]
mod tests;
struct State<'a, R, O, F> {
    runtime: &'a mut R,
    output: &'a mut O,
    emit: &'a mut F,
    report: Value,
    evidence: Vec<EncounterEvidence>,
    buff: Option<f64>,
}
fn initial_progress() -> Value {
    json!({"steps":[],"position":[0.0,0.0],"breadcrumbs":[[0.0,0.0]]})
}
pub(super) fn failure_message(error: &RunError) -> String {
    use crate::{
        actions::ActionError,
        common::error::{ControlError, ObservationError},
    };
    match error {
        RunError::Control(ControlError::Configuration(e))
        | RunError::Observation(ObservationError::Configuration(e))
        | RunError::Action(ActionError::Configuration(e)) => e.message().into(),
        RunError::Action(ActionError::Control(ControlError::Configuration(e))) => {
            e.message().into()
        }
        _ => error.to_string(),
    }
}
fn append(report: &mut Value, key: &str, item: Value) -> Result<(), RunError> {
    report[key]
        .as_array_mut()
        .ok_or_else(|| RunError::Runtime(format!("run report {key} must be an array")))?
        .push(item);
    Ok(())
}
impl<R: SummonerRuntime, O: RunOutput, F: FnMut(&str)> State<'_, R, O, F> {
    fn resolve_encounter(&mut self, buff: f64, recheck: bool) -> Result<bool, RunError> {
        let mut frames = Vec::new();
        let outcome = self.runtime.encounter(buff, &mut frames);
        if recheck {
            for frame in &mut frames {
                frame.filename = format!("recheck-{}", frame.filename);
            }
        }
        self.evidence.extend(frames);
        let mut encounter = outcome?;
        if recheck {
            self.report["encounter_before_recheck"] = std::mem::take(&mut self.report["encounter"]);
            if let Some(records) = encounter["evidence"].as_array_mut() {
                for record in records {
                    if let Some(name) = record["file"].as_str() {
                        record["file"] = json!(format!("recheck-{name}"));
                    }
                }
            }
        }
        self.report["encounter"] = encounter;
        if self.report["encounter"]["kill_confirmed"] != true
            || self.report["encounter"]["loot_complete"] != true
        {
            return Ok(false);
        }
        self.report["canyon_entry"] = self.runtime.enter_canyon(buff, &mut self.evidence)?;
        self.report["room_finish"] = self
            .runtime
            .finish_room(Destination::Canyon, &mut self.evidence)?;
        self.report["status"] = json!("north_room_closed");
        Ok(true)
    }
    fn pause(&mut self) -> Result<(), RunError> {
        self.runtime.snapshot(&mut self.report)?;
        self.report["input_events"] = self.runtime.input_events()?;
        self.report["evidence_write_status"] = json!("pending");
        self.output.save_report(&self.report)?;
        self.runtime
            .pause(&mut self.report, &mut self.buff, &mut self.evidence)
    }
    fn run(&mut self, options: &SummonerOptions, selected: &SelectedRun) -> Result<(), RunError> {
        let initial =
            self.runtime
                .initialize(options, selected, &mut self.report, &mut self.evidence)?;
        let mut proof = initial.proof;
        self.buff = Some(initial.buff_expires_at);
        let (mut segment, mut route_return, mut transition) = (0, options.return_from, None);
        let (mut phase_number, mut wing, mut renewed) = (0, options.wing, false);
        let mut progress = initial_progress();
        self.report["buff_renewals"] = json!([]);
        self.report["phases"] = json!([]);
        while segment < options.segments {
            if segment > 0 && options.continuous {
                (self.emit)(
                    "CONTINUE_CURRENT_FIELD: immediate next interval; fresh HUD/ground/focus and original buff deadline required",
                );
                append(
                    &mut self.report,
                    "continuations",
                    json!({"source":"same_process_transition","segment":segment,"at":self.runtime.clock()}),
                )?;
            } else if segment > 0 {
                proof = self.runtime.observe_runtime(&mut self.evidence)?;
                append(&mut self.report, "continuations", proof.clone())?;
            }
            let buff = self
                .buff
                .ok_or_else(|| RunError::Runtime("buff deadline missing".into()))?;
            let prefix = format!(
                "phase-{phase_number:02}-{}-{}-{segment:02}-",
                wing.name(),
                if route_return.is_none() {
                    "outbound"
                } else {
                    "return"
                }
            );
            let request = NavigationInterval {
                evidence_prefix: &prefix,
                options,
                selected,
                proof: &proof,
                wing,
                return_from: route_return,
                buff_expires_at: buff,
                continue_current: segment > 0 && !renewed,
                transition: transition.as_ref(),
            };
            if options.continuous {
                let mut frames = vec![];
                self.report["navigation"] = progress.clone();
                let result = self.runtime.navigate(request, &mut progress, &mut frames);
                self.report["navigation"] = result
                    .as_ref()
                    .map_or_else(|_| progress.clone(), Clone::clone);
                let files = frames
                    .iter()
                    .map(|e| format!("{prefix}{}", e.filename))
                    .chain(
                        self.report["navigation"]["streamed_evidence_files"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .map(|name| format!("{prefix}{name}")),
                    )
                    .collect::<Vec<_>>();
                let last = files
                    .iter()
                    .rev()
                    .find(|name| name.ends_with("-after.png"))
                    .cloned();
                self.report["navigation"]["evidence_files"] = json!(files);
                self.report["navigation"]["final_frame"] = json!(last);
                self.evidence
                    .extend(frames.into_iter().map(|e| EncounterEvidence {
                        filename: format!("{prefix}{}", e.filename),
                        frame: e.frame,
                    }));
                result?;
                transition = None;
                renewed = false;
            } else {
                match self
                    .runtime
                    .navigate(request, &mut progress, &mut self.evidence)
                {
                    Ok(v) => self.report["navigation"] = v,
                    Err(e) => {
                        // Python의 얕은 progress 복사에서 공유하던 두 목록은 실패 보고서에도 갱신된다.
                        if self.report.get("navigation").is_some() {
                            for key in ["steps", "breadcrumbs"] {
                                self.report["navigation"][key] = progress[key].clone();
                            }
                        }
                        return Err(e);
                    }
                }
            }
            self.report["status"] = self.report["navigation"]["status"].clone();
            if self.report["navigation"]["continuous_status"] == "nova_target_requires_review" {
                return Err(RunError::Runtime("near_hostile_requires_review".into()));
            }
            progress = self.report["navigation"].clone();
            progress
                .as_object_mut()
                .ok_or_else(|| RunError::Runtime("navigation report must be an object".into()))?
                .remove("status");
            if options.continuous
                && self.report["navigation"]["continuous_status"] == "buff_renewal_due"
            {
                let rows = self.report["buff_renewals"]
                    .as_array()
                    .ok_or_else(|| RunError::Runtime("buff renewal report missing".into()))?;
                if rows.len() >= 8 {
                    return Err(RunError::Runtime(
                        "automatic buff renewal limit reached".into(),
                    ));
                }
                let index = rows.len();
                append(
                    &mut self.report,
                    "buff_renewals",
                    json!({"wing":wing,"return_from":route_return,"previous_preparation_at":self.buff}),
                )?;
                let record = self.runtime.renew(&mut self.report, &mut self.evidence)?;
                let target = self.report["buff_renewals"][index]
                    .as_object_mut()
                    .ok_or_else(|| RunError::Runtime("buff renewal record missing".into()))?;
                target.extend(
                    record
                        .as_object()
                        .ok_or_else(|| {
                            RunError::Runtime("automatic renewal must be an object".into())
                        })?
                        .clone(),
                );
                proof = record["observation"].clone();
                self.buff = Some(
                    record["preparation_at"]
                        .as_f64()
                        .ok_or_else(|| RunError::Runtime("buff deadline missing".into()))?,
                );
                renewed = true;
                (self.emit)(
                    "BUFF_RENEWED_AUTOMATIC: fresh nearby/ratio observations and balanced sequence; same route retained",
                );
                continue;
            }
            if options.continuous
                && self.report["status"] == "wing_end_observed"
                && route_return.is_none()
            {
                self.phase()?;
                (self.emit)(
                    "WING_END_CONFIRMED: same-room central return begins from current observed terminal",
                );
                transition = Some(self.report["navigation"].clone());
                route_return = Some(wing);
                progress = initial_progress();
                self.runtime.reset_route_memory()?;
                segment = 0;
                phase_number += 1;
                continue;
            }
            if self.report["status"] == "hub_arrived" {
                self.completed()?;
                (self.emit)(
                    "CENTRAL_WAYPOINT_ARRIVED: current waypoint and player proximity/settling verified; room retained",
                );
                if options.finish_room {
                    self.phase()?;
                    self.report["room_finish"] = self
                        .runtime
                        .finish_room(Destination::Arcane, &mut self.evidence)?;
                    self.report["status"] = json!("north_room_closed");
                    break;
                }
                if options.all_wings {
                    self.phase()?;
                    let next = match wing {
                        Wing::North => Some(Wing::East),
                        Wing::East => Some(Wing::South),
                        Wing::South => Some(Wing::West),
                        Wing::West => None,
                    };
                    let Some(next) = next else {
                        self.report["status"] = json!("all_wings_empty_review");
                        (self.emit)(
                            "ALL_WINGS_EMPTY_REVIEW: four positive chest endings; Summoner result unresolved",
                        );
                        self.pause()?;
                        break;
                    };
                    transition = Some(self.report["navigation"].clone());
                    wing = next;
                    (self.emit)(&format!(
                        "NEXT_WING: {} begins from current settled central waypoint",
                        wing.name()
                    ));
                    route_return = None;
                    segment = 0;
                    progress = initial_progress();
                    self.runtime.reset_route_memory()?;
                    continue;
                }
                break;
            }
            if options.continuous
                && matches!(
                    self.report["status"].as_str(),
                    Some("field_paused" | "wing_end_observed" | "summoner_platform_observed")
                )
            {
                if self.report["status"] == "summoner_platform_observed" {
                    (self.emit)(
                        "SUMMONER_PLATFORM: current platform landmarks confirmed; remaining wing search cancelled; kill/loot unconfirmed",
                    );
                    if options.engage_monsters && self.resolve_encounter(buff, false)? {
                        break;
                    }
                }
                (self.emit)(
                    "FIELD_PAUSED: inputs released; current room retained for navigation correction",
                );
                (self.emit)(
                    "PAUSED_SURVIVAL_GUARD: HUD monitoring remains active; actual low resources or sustained HUD loss trigger safety exit",
                );
                self.pause()?;
                if self.report["status"] == "summoner_platform_observed"
                    && self.report["encounter_recheck_requested"] == true
                    && !self.runtime.stopped()
                    && self.runtime.focused()?
                {
                    self.report["encounter_recheck_requested"] = json!(false);
                    self.report["encounter_recheck_used"] = json!(true);
                    (self.emit)(
                        "SUMMONER_RECHECK: current corpse candidate after defense; verify fresh ordered corpse pair, then journal/red portal/Canyon waypoint",
                    );
                    let active_buff = self
                        .buff
                        .ok_or_else(|| RunError::Runtime("recheck buff deadline missing".into()))?;
                    self.resolve_encounter(active_buff, true)?;
                }
                break;
            }
            if self.report["status"] != "north_segment_observed" {
                return Err(RunError::Runtime("north landing not verified".into()));
            }
            self.completed()?;
            segment += 1;
        }
        if options.continuous
            && matches!(
                self.report["status"].as_str(),
                Some("north_segment_observed" | "hub_arrived")
            )
        {
            (self.emit)(
                "FIELD_INTERVAL_FINISHED: development movement finished; HUD monitoring remains active until operator handoff",
            );
            self.pause()?;
        }
        Ok(())
    }
    fn completed(&mut self) -> Result<(), RunError> {
        let n = self.report["segments_completed"]
            .as_u64()
            .ok_or_else(|| RunError::Runtime("completed segment count missing".into()))?;
        self.report["segments_completed"] = json!(n + 1);
        Ok(())
    }
    fn phase(&mut self) -> Result<(), RunError> {
        let navigation = self.report["navigation"].clone();
        append(&mut self.report, "phases", navigation)
    }
    fn failure(&mut self, options: &SummonerOptions, error: &RunError) -> Result<(), RunError> {
        self.report["status"] = json!(if self.runtime.stopped() {
            "interrupted"
        } else {
            "failed"
        });
        let mut failure = failure_message(error);
        append(&mut self.report, "errors", json!(failure))?;
        (self.emit)(&format!("SUMMONER_STOP: {failure}"));
        let immediate = |failure: &str| {
            matches!(
                failure,
                "resource_low"
                    | "paused_hud_unverified"
                    | "near_hostile_requires_review"
                    | "current Arcane title unverified"
                    | "mercenary_unverified"
            )
        };
        if failure == "mercenary_unverified" {
            self.report["operator_review_required"] = json!("mercenary_portrait_unverified");
            self.report["field_retained"] = json!(true);
            (self.emit)(
                "MERCENARY_UNVERIFIED: current portrait absent or obscured; request safe exit before operator review",
            );
        }
        if options.continuous && !immediate(&failure) {
            self.report["field_retained"] = json!(true);
            (self.emit)("FIELD_PAUSED: inputs released; no room exit during navigation training");
            let prepared = ["initial_safety", "preparation_safety"]
                .iter()
                .any(|key| self.report.get(*key).is_some_and(|v| !v.is_null()));
            if prepared
                && self.runtime.has_capture()
                && !self.runtime.stopped()
                && self.runtime.focused()?
            {
                (self.emit)(
                    "PAUSED_SURVIVAL_GUARD: preparation/navigation failure; continue HUD monitoring until operator handoff or survival risk",
                );
                if let Err(error) = self.pause() {
                    if !matches!(
                        &error,
                        RunError::Runtime(_)
                            | RunError::Control(ControlError::Runtime(_))
                            | RunError::Action(crate::actions::ActionError::Runtime(_))
                    ) {
                        return Err(error);
                    }
                    failure = failure_message(&error);
                    append(
                        &mut self.report,
                        "errors",
                        json!(format!("paused guard: {failure}")),
                    )?;
                }
            }
        }
        if (!options.continuous || immediate(&failure))
            && self.runtime.has_capture()
            && self.runtime.has_backend()
            && !self.runtime.stopped()
            && self.runtime.focused()?
        {
            self.report["safe_exit_attempt"] = json!({});
            match self
                .runtime
                .safe_exit(&mut self.report["safe_exit_attempt"], &mut self.evidence)
            {
                Ok(()) => {
                    self.report["safe_exit"] = json!("character_select_observed");
                    self.report["field_retained"] = json!(false);
                }
                Err(error) => {
                    self.report["safe_exit"] = json!("unverified");
                    append(
                        &mut self.report,
                        "errors",
                        json!(format!("exit: {}", failure_message(&error))),
                    )?;
                }
            }
        }
        Ok(())
    }
}
pub fn execute_prepared(
    runtime: &mut impl SummonerRuntime,
    output: &mut impl RunOutput,
    options: &SummonerOptions,
    selected: &SelectedRun,
    mut emit: impl FnMut(&str),
) -> Result<i32, RunError> {
    let report = json!({"status":"preparing","errors":[],"acceptance_room":false,"resume_arcane":options.resume_arcane,"resume_town":options.resume_town,"mercenary_monitor":{"enabled":selected.character_key=="flash"&&!options.resume_arcane,"scope":"current_portrait_only","same_room_recovery":options.resume_arcane},"timing":{"preparation_started_at":runtime.clock()},"segments_requested":options.segments,"segments_completed":0,"continuations":[],"continuous":options.continuous,"field_seconds":if options.continuous{json!(options.field_seconds)}else{Value::Null},"wing":options.wing,"return_from":options.return_from,"engage_monsters":options.engage_monsters,"all_wings":options.all_wings,"finish_north_room":options.finish_room,"automap_preference":options.automap.name(),"automatic":options.automatic,"character_key":selected.character_key,"main_weapon_start_required":true});
    let mut state = State {
        runtime,
        output,
        emit: &mut emit,
        report,
        evidence: vec![],
        buff: None,
    };
    let mut pending = None;
    if let Err(error) = state.run(options, selected)
        && let Err(handling) = state.failure(options, &error)
    {
        let recorded = append(
            &mut state.report,
            "errors",
            json!(format!("failure handler: {}", failure_message(&handling))),
        );
        pending = Some(error.with_cleanup(handling));
        if let Err(recording) = recorded {
            merge_error(&mut pending, recording);
        }
    }
    state.runtime.stop();
    let errors = state
        .runtime
        .cleanup(&mut state.report, &mut state.evidence);
    for e in errors {
        append(&mut state.report, "errors", json!(e))?;
    }
    match state.runtime.input_events() {
        Ok(events) => state.report["input_events"] = events,
        Err(error) => {
            append(
                &mut state.report,
                "errors",
                json!(format!("input events: {}", failure_message(&error))),
            )?;
            merge_error(&mut pending, error);
        }
    }
    state.report["evidence_write_status"] = json!("in_progress");
    if let Err(error) = state.output.save_report(&state.report) {
        append(
            &mut state.report,
            "errors",
            json!(format!("report write: {}", failure_message(&error))),
        )?;
        merge_error(&mut pending, error);
    }
    let mut images_complete = state.report["diagnostic_evidence_stream"]["skipped_files"]
        .as_array()
        .is_none_or(Vec::is_empty)
        && state.report["diagnostic_evidence_stream"]["errors"]
            .as_array()
            .is_none_or(Vec::is_empty);
    for item in &state.evidence {
        if let Err(error) = state.output.save_frame(item) {
            images_complete = false;
            append(
                &mut state.report,
                "errors",
                json!(format!(
                    "evidence {}: {}",
                    item.filename,
                    failure_message(&error)
                )),
            )?;
            merge_error(&mut pending, error);
        }
    }
    if images_complete {
        state.report["evidence_write_status"] = json!("complete");
    } else {
        state.report["evidence_write_status"] = json!("partial");
    }
    if let Err(error) = state.output.save_report(&state.report) {
        merge_error(&mut pending, error);
    }
    if let Some(error) = pending {
        return Err(error);
    }
    (state.emit)(&format!(
        "SUMMONER_RESULT: {} ({})",
        state.report["status"].as_str().unwrap_or("failed"),
        state.output.report_path().display()
    ));
    Ok(
        if matches!(
            state.report["status"].as_str(),
            Some("north_segment_observed" | "hub_arrived" | "north_room_closed")
        ) && state.report["errors"].as_array().is_some_and(Vec::is_empty)
        {
            0
        } else {
            2
        },
    )
}
fn merge_error(pending: &mut Option<RunError>, error: RunError) {
    *pending = Some(if let Some(primary) = pending.take() {
        primary.with_cleanup(error)
    } else {
        error
    });
}

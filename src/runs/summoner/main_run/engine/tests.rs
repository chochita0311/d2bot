use super::*;
use crate::{
    actions::town_loop::Difficulty,
    common::{
        config::BotConfig,
        error::ObservationError,
        frame::{BgrFrame, Size},
    },
    test_field_world::{compare, frame_index},
};
use std::{
    cell::RefCell,
    error::Error,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};
type TestResult = Result<(), Box<dyn Error>>;
struct Data {
    case: Value,
    now: f64,
    buff_index: usize,
    nav_index: usize,
    stopped: bool,
    focused: bool,
    capture: bool,
    backend: bool,
    camera: bool,
    events: Vec<Value>,
    emits: Vec<String>,
}
#[derive(Clone)]
struct World(Rc<RefCell<Data>>);
impl World {
    fn now(&self) -> f64 {
        self.0.borrow().now
    }
    fn case(&self) -> Value {
        self.0.borrow().case.clone()
    }
    fn event(&self, v: Value) {
        self.0.borrow_mut().events.push(v);
    }
    fn emit(&self, s: &str) {
        self.0.borrow_mut().emits.push(s.into());
    }
    fn advance(&self, s: f64) {
        self.0.borrow_mut().now += s;
    }
    fn buff(&self) -> Result<Value, RunError> {
        let i = self.0.borrow().buff_index;
        self.0.borrow_mut().buff_index += 1;
        self.event(json!(["buff", i, self.now()]));
        let c = self.case();
        let rows = c["buffs"].as_array();
        let row = rows
            .and_then(|rows| rows.get(i.min(rows.len().saturating_sub(1))))
            .cloned()
            .unwrap_or(json!({}));
        self.advance(row["delay"].as_f64().unwrap_or(0.2));
        if let Some(e) = row["error"].as_str() {
            return Err(RunError::Runtime(e.into()));
        }
        let deadline = row["deadline"].as_f64().unwrap_or(100.0 + i as f64 * 20.0);
        Ok(
            json!({"observation":{"source":"runtime_screen","iteration":i},"safety":{"iteration":i},"buff":{"status":"dispatched_unconfirmed"},"deadlines":[deadline+5.0],"preparation_at":deadline}),
        )
    }
}
fn frame(index: usize) -> Result<Arc<BgrFrame>, RunError> {
    Ok(Arc::new(
        BgrFrame::from_vec(
            Size::new(1, 1).map_err(ObservationError::from)?,
            vec![index as u8, (index >> 8) as u8, (index >> 16) as u8],
        )
        .map_err(ObservationError::from)?,
    ))
}
impl SummonerRuntime for World {
    fn clock(&self) -> f64 {
        self.now()
    }
    fn stopped(&self) -> bool {
        self.0.borrow().stopped
    }
    fn stop(&self) {
        self.0.borrow_mut().stopped = true;
    }
    fn focused(&self) -> Result<bool, ControlError> {
        Ok(self.0.borrow().focused)
    }
    fn has_capture(&self) -> bool {
        self.0.borrow().capture
    }
    fn has_backend(&self) -> bool {
        self.0.borrow().backend
    }
    fn input_events(&self) -> Result<Value, RunError> {
        Ok(json!([]))
    }
    fn snapshot(&mut self, report: &mut Value) -> Result<(), RunError> {
        if let Some(rows) = self.case().get("pending_recoveries") {
            report["resource_recoveries"] = rows.clone();
        }
        Ok(())
    }
    fn initialize(
        &mut self,
        o: &SummonerOptions,
        _: &SelectedRun,
        r: &mut Value,
        _: &mut Vec<EncounterEvidence>,
    ) -> Result<InitialField, RunError> {
        let c = self.case();
        if !o.resume_arcane {
            r["timing"]["entry_started_at"] = json!(self.now());
            self.event(json!(["entry", o.difficulty.name(), o.resume_town]));
            self.advance(c["entry_delay"].as_f64().unwrap_or(0.05));
            if let Some(e) = c["entry_error"].as_str() {
                return Err(RunError::Runtime(e.into()));
            }
            r["entry"] = json!("arcane_hub_observed");
            r["timing"]["arcane_arrived_at"] = json!(self.now());
        }
        {
            let mut s = self.0.borrow_mut();
            s.capture = true;
            s.backend = true;
            s.camera = true;
        }
        r["resource_recoveries"] = json!([]);
        r["loot_scans"] = json!([]);
        r["item_label_policy"] = json!("off_while_moving_on_for_post_combat_scan");
        r["item_label_preparation"] = json!({"status":"test_labels_off"});
        r["automap"] = match o.automap {
            super::super::super::options::AutomapPreference::On => json!({"status":"test_map_on"}),
            _ => json!({"status":"preserved_for_diagnostic","visibility":"unverified"}),
        };
        r["primary_set"] =
            json!({"status":"main_weapon_start_precondition","inventory_input":false});
        let initial = self.buff()?;
        r["preparation_safety"] = initial["safety"].clone();
        r["initial_safety"] = initial["safety"].clone();
        r["automatic_buff"] = initial.clone();
        r["buff"] = initial["buff"].clone();
        r["camera_projection"] = json!({"source":"shared_layout_and_current_runtime_viewport","maximum_occlusion_seconds":null});
        self.emit("AUTOMATIC_BUFF_READY: balanced configured sequence; no inventory or supervisor input; timer uses actual cast requests");
        Ok(InitialField {
            proof: initial["observation"].clone(),
            buff_expires_at: initial["preparation_at"]
                .as_f64()
                .ok_or_else(|| RunError::Runtime("deadline".into()))?,
        })
    }
    fn observe_runtime(&mut self, _: &mut Vec<EncounterEvidence>) -> Result<Value, RunError> {
        Ok(
            json!({"source":"runtime_screen","character_id":"flash","area":"arcane_sanctuary","observed_at":1000.0+self.now(),"captured_at":self.now(),"panels_closed":true,"belt_closed":true,"nearby_candidates":0,"safety_scope":"current_nearby_candidates_and_stable_ratios","player_ground":[962,578],"life_ratio":0.9,"mana_ratio":0.9,"item_labels_on":false}),
        )
    }
    fn navigate(
        &mut self,
        request: NavigationInterval<'_>,
        p: &mut Value,
        evidence: &mut Vec<EncounterEvidence>,
    ) -> Result<Value, RunError> {
        let i = self.0.borrow().nav_index;
        self.0.borrow_mut().nav_index += 1;
        self.event(json!([
            "navigate",
            request.options.continuous,
            i,
            if request.options.continuous {
                request.wing
            } else {
                Wing::North
            },
            if request.options.continuous {
                request.return_from
            } else {
                None
            },
            request.options.continuous && request.continue_current,
            request.transition,
            request.buff_expires_at,
            p
        ]));
        let c = self.case();
        let rows = c
            .get("navigation")
            .cloned()
            .unwrap_or(json!([{"status":"north_segment_observed"},{"status":"hub_arrived"}]));
        let rows = rows
            .as_array()
            .ok_or_else(|| RunError::Runtime("navigation rows".into()))?;
        let row = rows
            .get(i.min(rows.len().saturating_sub(1)))
            .ok_or_else(|| RunError::Runtime("navigation row".into()))?;
        self.advance(row["delay"].as_f64().unwrap_or(0.1));
        for name in row["evidence"].as_array().into_iter().flatten() {
            evidence.push(EncounterEvidence {
                filename: name
                    .as_str()
                    .ok_or_else(|| RunError::Runtime("name".into()))?
                    .into(),
                frame: frame(i)?,
            });
        }
        if let Some(partial) = row["partial"].as_object() {
            p.as_object_mut()
                .ok_or_else(|| RunError::Runtime("progress".into()))?
                .extend(partial.clone());
        }
        for (key, list) in [
            ("append_step", "steps"),
            ("append_breadcrumb", "breadcrumbs"),
        ] {
            if let Some(v) = row.get(key) {
                p[list]
                    .as_array_mut()
                    .ok_or_else(|| RunError::Runtime("list".into()))?
                    .push(v.clone());
            }
        }
        {
            let mut s = self.0.borrow_mut();
            if row["stop"] == true {
                s.stopped = true;
            }
            if row["unfocus"] == true {
                s.focused = false;
            }
        }
        if let Some(e) = row["error"].as_str() {
            return Err(RunError::Runtime(e.into()));
        }
        if let Some(result) = row.get("result") {
            return Ok(result.clone());
        }
        let mut result = p.clone();
        result["status"] = row
            .get("status")
            .cloned()
            .unwrap_or(json!("north_segment_observed"));
        Ok(result)
    }
    fn renew(&mut self, _: &mut Value, _: &mut Vec<EncounterEvidence>) -> Result<Value, RunError> {
        self.buff()
    }
    fn reset_route_memory(&mut self) -> Result<(), RunError> {
        self.event(json!(["reset_graph"]));
        Ok(())
    }
    fn encounter(&mut self, b: f64, _: &mut Vec<EncounterEvidence>) -> Result<Value, RunError> {
        self.event(json!(["encounter", b]));
        let c = self.case();
        if let Some(e) = c["encounter_error"].as_str() {
            return Err(RunError::Runtime(e.into()));
        }
        let index = self
            .0
            .borrow()
            .events
            .iter()
            .filter(|event| event[0] == "encounter")
            .count()
            .saturating_sub(1);
        if let Some(encounter) = c["encounters"].as_array().and_then(|rows| rows.get(index)) {
            return Ok(encounter.clone());
        }
        Ok(c.get("encounter")
            .cloned()
            .unwrap_or(json!({"kill_confirmed":true,"loot_complete":true})))
    }
    fn enter_canyon(&mut self, b: f64, _: &mut Vec<EncounterEvidence>) -> Result<Value, RunError> {
        self.event(json!(["canyon", b]));
        let c = self.case();
        if let Some(e) = c["canyon_error"].as_str() {
            return Err(RunError::Runtime(e.into()));
        }
        Ok(json!({"status":"canyon_waypoint_observed"}))
    }
    fn finish_room(
        &mut self,
        d: Destination,
        _: &mut Vec<EncounterEvidence>,
    ) -> Result<Value, RunError> {
        self.event(json!(["finish", d]));
        let c = self.case();
        if let Some(e) = c["finish_error"].as_str() {
            return Err(RunError::Runtime(e.into()));
        }
        Ok(json!({"status":"act1_then_character_select_observed"}))
    }
    fn pause(
        &mut self,
        r: &mut Value,
        b: &mut Option<f64>,
        _: &mut Vec<EncounterEvidence>,
    ) -> Result<(), RunError> {
        self.event(json!(["pause", b]));
        r["paused_survival_guard"] = json!({"status":"test_monitor"});
        let c = self.case();
        if c["pause_recheck"] == true {
            r["encounter_recheck_requested"] = json!(true);
        }
        if c["pause_renew"] == true {
            let record = self.buff()?;
            *b = record["preparation_at"].as_f64();
            r["buff_renewals"]
                .as_array_mut()
                .ok_or_else(|| RunError::Runtime("renewals".into()))?
                .push(record);
        }
        if c["pause_stop"] == true {
            self.0.borrow_mut().stopped = true;
        }
        if c["pause_config_error"] == true {
            if c["pause_corrupt_errors"] == true {
                r["errors"] = Value::Null;
            }
            return Err(crate::common::config::ConfigError::value("invalid pause evidence").into());
        }
        if let Some(e) = c["pause_error"].as_str() {
            return Err(RunError::Runtime(e.into()));
        }
        Ok(())
    }
    fn safe_exit(&mut self, r: &mut Value, _: &mut Vec<EncounterEvidence>) -> Result<(), RunError> {
        self.event(json!(["safe_exit"]));
        r["attempted"] = json!(true);
        if let Some(e) = self.case()["exit_error"].as_str() {
            return Err(RunError::Runtime(e.into()));
        }
        Ok(())
    }
    fn cleanup(&mut self, r: &mut Value, evidence: &mut Vec<EncounterEvidence>) -> Vec<String> {
        if let Some(stream) = self.case().get("diagnostic_evidence_stream") {
            r["diagnostic_evidence_stream"] = stream.clone();
        }
        if self.has_backend() {
            r["loot_scans"] = json!([]);
            r["item_labels_final"] = json!("off");
            if self.case()["loot_evidence"] == true {
                r["loot_hud_reacquisition"] = json!({"reason":"test_hud"});
                r["loot_landing_observation"] = json!({"reason":"test_landing"});
                for (name, i) in [
                    ("loot-hud-unverified-last.png", 1001),
                    ("loot-landing-last.png", 1002),
                ] {
                    match frame(i) {
                        Ok(frame) => evidence.push(EncounterEvidence {
                            filename: name.into(),
                            frame,
                        }),
                        Err(e) => return vec![e.to_string()],
                    }
                }
            }
        }
        if self.0.borrow().camera {
            if !r["camera_projection"].is_object() {
                r["camera_projection"] = json!({});
            }
            let d = r["camera_projection"].as_object_mut();
            if let Some(d) = d {
                d.extend(json!({"calibrations":0,"projected_observations":0,"final_calibrated_center":[962,578],"final_authorized":true,"reset_events":[]}).as_object().cloned().unwrap_or_default());
            }
        }
        if self.has_capture() {
            self.event(json!(["close"]));
        }
        self.case()["cleanup_errors"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    }
}
struct Output {
    world: World,
    path: PathBuf,
}
impl RunOutput for Output {
    fn report_path(&self) -> &Path {
        &self.path
    }
    fn save_report(&mut self, r: &Value) -> Result<(), RunError> {
        self.world.event(json!(["report", r]));
        let n = self
            .world
            .0
            .borrow()
            .events
            .iter()
            .filter(|e| e[0] == "report")
            .count() as u64;
        if self.world.case()["report_error_at"].as_u64() == Some(n) {
            return Err(RunError::Runtime("report write failed".into()));
        }
        Ok(())
    }
    fn save_frame(&mut self, e: &EncounterEvidence) -> Result<(), RunError> {
        assert!(
            self.world.stopped(),
            "all input must stop before image writing"
        );
        self.world
            .event(json!(["image", e.filename, frame_index(&e.frame)]));
        if self.world.case()["image_error"] == e.filename {
            return Err(RunError::Runtime("image write failed".into()));
        }
        Ok(())
    }
}
fn options(c: &Value) -> Result<SummonerOptions, Box<dyn Error>> {
    let mut o = SummonerOptions::new(PathBuf::from("out"), Difficulty::Hell);
    o.live = true;
    o.continuous = true;
    o.segments = 2;
    o.finish_room = true;
    o.character_key = Some("flash".into());
    let v = &c["options"];
    for (key, target) in [
        ("live", &mut o.live),
        ("resume_arcane", &mut o.resume_arcane),
        ("resume_town", &mut o.resume_town),
        ("resume_labels_off", &mut o.resume_labels_off),
        ("continuous", &mut o.continuous),
        ("engage_monsters", &mut o.engage_monsters),
        ("all_wings", &mut o.all_wings),
        ("finish_room", &mut o.finish_room),
        ("automatic", &mut o.automatic),
    ] {
        if let Some(x) = v[key].as_bool() {
            *target = x;
        }
    }
    for (key, target) in [
        ("casts", &mut o.casts),
        ("segments", &mut o.segments),
        ("field_seconds", &mut o.field_seconds),
    ] {
        if let Some(n) = v[key].as_u64() {
            *target = u8::try_from(n)?;
        }
    }
    if let Some(s) = v["character_key"].as_str() {
        o.character_key = Some(s.into());
    }
    if v["automap"] == "preserve" {
        o.automap = super::super::super::options::AutomapPreference::Preserve;
    }
    if v["continue_navigation"].is_string() {
        o.continue_navigation = Some(PathBuf::from(
            v["continue_navigation"].as_str().ok_or("continuation")?,
        ));
    }
    if let Some(s) = v["wing"].as_str() {
        o.wing = serde_json::from_value(json!(s))?;
    }
    if let Some(s) = v["return_from"].as_str() {
        o.return_from = Some(serde_json::from_value(json!(s))?);
    }
    Ok(o)
}
fn exercise(c: &Value, config: &BotConfig) -> Result<Value, Box<dyn Error>> {
    let world = World(Rc::new(RefCell::new(Data {
        case: c.clone(),
        now: 10.0,
        buff_index: 0,
        nav_index: 0,
        stopped: false,
        focused: true,
        capture: false,
        backend: false,
        camera: false,
        events: vec![],
        emits: vec![],
    })));
    let mut runtime = world.clone();
    let outpath = Path::new("out").join("report.json");
    let mut output = Output {
        world: world.clone(),
        path: outpath,
    };
    let o = options(c)?;
    let result = match o.selected(config) {
        Ok(selected) => {
            if o.live {
                execute_prepared(&mut runtime, &mut output, &o, &selected, |s| world.emit(s))
            } else {
                world.emit(&o.dry_message());
                Ok(0)
            }
        }
        Err(e) => Err(e.into()),
    };
    let (result, error) = match result {
        Ok(code) => (json!(code), Value::Null),
        Err(e) => (
            Value::Null,
            json!({"kind":if matches!(e,RunError::Control(ControlError::Configuration(_))){"ValueError"}else{"RuntimeError"},"message":failure_message(&e)}),
        ),
    };
    let s = world.0.borrow();
    Ok(json!({"result":result,"error":error,"events":s.events,"emits":s.emits,"now":s.now}))
}
#[test]
fn corpse_recheck_after_defense_enters_canyon_without_central_return() -> TestResult {
    let config = BotConfig::load(Path::new(env!("CARGO_MANIFEST_DIR")).join("config"))?;
    let case = json!({"navigation":[{"status":"summoner_platform_observed"}],"pause_recheck":true,"pause_renew":true,
        "encounters":[{"status":"kill_unconfirmed","kill_confirmed":false},{"status":"kill_confirmed","kill_confirmed":true,"loot_complete":true}]});
    let result = exercise(&case, &config)?;
    assert!(result["error"].is_null(), "{result}");
    let events = result["events"].as_array().ok_or("events")?;
    let encounters = events
        .iter()
        .filter(|event| event[0] == "encounter")
        .collect::<Vec<_>>();
    assert_eq!(encounters.len(), 2);
    assert_eq!(encounters[1][1], 120.0);
    assert_eq!(
        events.iter().filter(|event| event[0] == "canyon").count(),
        1
    );
    assert!(!events.iter().any(|event| event[0] == "reset_graph"));
    let report = events
        .iter()
        .rev()
        .find(|event| event[0] == "report")
        .ok_or("report")?;
    assert_eq!(report[1]["status"], "north_room_closed");
    assert_eq!(report[1]["encounter_recheck_used"], true);
    assert_eq!(
        report[1]["encounter_before_recheck"]["kill_confirmed"],
        false
    );
    let mut unresolved = case;
    unresolved["encounters"][1]["kill_confirmed"] = json!(false);
    let result = exercise(&unresolved, &config)?;
    assert!(
        !result["events"]
            .as_array()
            .ok_or("events")?
            .iter()
            .any(|event| event[0] == "canyon")
    );
    Ok(())
}

#[test]
fn current_main_orchestration_matches_source_reports_and_calls() -> TestResult {
    let config = BotConfig::load(Path::new(env!("CARGO_MANIFEST_DIR")).join("config"))?;
    let mut cases: Value = serde_json::from_str(
        r#"[
 {},{"options":{"finish_room":false}},{"options":{"segments":1}},{"options":{"continuous":false,"finish_room":false},"navigation":[{"status":"north_segment_observed"}]},
 {"entry_error":"entry failed"},{"buffs":[{"error":"resource_low"}]},{"navigation":[{"error":"terrain_uncertain"}]},
 {"navigation":[{"error":"resource_low"}]},{"navigation":[{"error":"paused_hud_unverified"}]},{"navigation":[{"error":"near_hostile_requires_review"}]},{"navigation":[{"error":"current Arcane title unverified"}]},{"navigation":[{"error":"mercenary_unverified"}]},
 {"navigation":[{"error":"terrain_uncertain","stop":true}]},{"navigation":[{"error":"terrain_uncertain","unfocus":true}]},
 {"options":{"finish_room":false},"pause_error":"resource_low"},{"navigation":[{"error":"terrain_uncertain"}],"pause_error":"resource_low"},
 {"navigation":[{"result":{"status":"field_paused","continuous_status":"nova_target_requires_review"}}]},
 {"navigation":[{"result":{"status":"field_paused","continuous_status":"buff_renewal_due"}},{"status":"hub_arrived"}]},
 {"navigation":[{"result":{"status":"field_paused","continuous_status":"buff_renewal_due"}}],"buffs":[{}, {"error":"current Arcane title unverified"}]},
 {"navigation":[{"result":{"status":"field_paused","continuous_status":"buff_renewal_due"}}]},
 {"navigation":[{"status":"wing_end_observed","evidence":["001-after.png"]},{"status":"hub_arrived"}]},
 {"options":{"all_wings":true,"finish_room":false},"navigation":[{"status":"wing_end_observed"},{"status":"hub_arrived"},{"status":"wing_end_observed"},{"status":"hub_arrived"},{"status":"wing_end_observed"},{"status":"hub_arrived"},{"status":"wing_end_observed"},{"status":"hub_arrived"}]},
 {"navigation":[{"status":"summoner_platform_observed"}]},{"options":{"engage_monsters":false},"navigation":[{"status":"summoner_platform_observed"}]},
 {"navigation":[{"status":"summoner_platform_observed"}],"encounter":{"kill_confirmed":false,"loot_complete":true}},
 {"navigation":[{"status":"summoner_platform_observed"}],"encounter":{"kill_confirmed":true,"loot_complete":false}},
 {"navigation":[{"status":"summoner_platform_observed"}],"encounter_error":"encounter uncertain"},
 {"navigation":[{"status":"summoner_platform_observed"}],"canyon_error":"portal uncertain"},
 {"finish_error":"waypoint uncertain"},{"options":{"continuous":false,"finish_room":false},"navigation":[{"status":"north_failed"}]},
 {"navigation":[{"error":"resource_low"}],"exit_error":"menu uncertain"},
 {"options":{"casts":0}},{"options":{"segments":0}},{"options":{"field_seconds":0}},{"options":{"resume_labels_off":true}},{"options":{"automatic":false}},{"options":{"character_key":"absent"}},
 {"options":{"live":false}},{"options":{"resume_arcane":true,"resume_labels_off":true}},
 {"options":{"automap":"preserve"}},{"loot_evidence":true},
 {"navigation":[{"error":"uncertain","evidence":["002-after.png"],"partial":{"continuous_status":"partial"}}]},
 {"options":{"continuous":false,"finish_room":false},"navigation":[{"status":"north_segment_observed","append_step":{"index":0}},{"error":"landing uncertain","append_step":{"index":1},"append_breadcrumb":[10,20],"partial":{"position":[10,20]}}]},
 {"options":{"finish_room":false},"pause_renew":true},{"options":{"finish_room":false},"pause_stop":true},
 {"options":{"finish_room":false},"pending_recoveries":[{"status":"reconciled","source":"test_current_record"}]}
 ]"#,
    )?;
    let expected =
        crate::test_support::legacy_expectation("main-execution-oracle", &json!({"cases":cases}))?;
    for (i, c) in cases.as_array_mut().ok_or("cases")?.iter_mut().enumerate() {
        compare(
            &exercise(c, &config)?,
            &expected[i],
            &format!("main case[{i}]"),
        );
    }
    Ok(())
}

#[test]
fn report_failures_still_stop_close_and_attempt_remaining_evidence() -> TestResult {
    let config = BotConfig::load(Path::new(env!("CARGO_MANIFEST_DIR")).join("config"))?;
    for extra in [
        json!({"report_error_at":1}),
        json!({"report_error_at":2}),
        json!({"image_error":"phase-00-north-outbound-00-one-after.png"}),
    ] {
        let mut case = json!({"navigation":[{"status":"hub_arrived","evidence":["one-after.png","two-after.png"]}]});
        case.as_object_mut()
            .ok_or("case")?
            .extend(extra.as_object().ok_or("extra")?.clone());
        let got = exercise(&case, &config)?;
        assert!(!got["error"].is_null());
        let events = got["events"].as_array().ok_or("events")?;
        let closed = events
            .iter()
            .position(|e| e[0] == "close")
            .ok_or("closed")?;
        let images = events
            .iter()
            .enumerate()
            .filter(|(_, e)| e[0] == "image")
            .collect::<Vec<_>>();
        assert_eq!(images.len(), 2);
        assert!(images.iter().all(|(i, _)| *i > closed));
        if case.get("image_error").is_some() {
            let report = events
                .iter()
                .rev()
                .find(|e| e[0] == "report")
                .ok_or("report")?;
            assert_eq!(report[1]["evidence_write_status"], "partial");
        }
    }
    Ok(())
}
#[test]
fn streamed_navigation_names_and_partial_evidence_survive_final_report() -> TestResult {
    let config = BotConfig::load(Path::new(env!("CARGO_MANIFEST_DIR")).join("config"))?;
    for skipped in [
        json!([]),
        json!(["phase-00-north-outbound-00-optional-before.png"]),
    ] {
        let got = exercise(
            &json!({
                "navigation":[{"result":{"status":"hub_arrived","streamed_evidence_files":["one-after.png"]}}],
                "diagnostic_evidence_stream":{"queued":1,"written":1,"skipped_files":skipped,"errors":[]}
            }),
            &config,
        )?;
        assert!(got["error"].is_null());
        let events = got["events"].as_array().ok_or("events")?;
        assert!(!events.iter().any(|e| e[0] == "image"));
        let report = &events
            .iter()
            .rev()
            .find(|e| e[0] == "report")
            .ok_or("report")?[1];
        assert_eq!(
            report["navigation"]["evidence_files"],
            json!(["phase-00-north-outbound-00-one-after.png"])
        );
        assert_eq!(
            report["navigation"]["final_frame"],
            "phase-00-north-outbound-00-one-after.png"
        );
        assert_eq!(
            report["diagnostic_evidence_stream"]["skipped_files"],
            skipped
        );
        assert_eq!(
            report["evidence_write_status"],
            if skipped.as_array().is_some_and(Vec::is_empty) {
                "complete"
            } else {
                "partial"
            }
        );
    }
    Ok(())
}
#[test]
fn unexpected_failure_handler_error_keeps_both_causes_and_runs_cleanup() -> TestResult {
    let config = BotConfig::load(Path::new(env!("CARGO_MANIFEST_DIR")).join("config"))?;
    let got = exercise(
        &json!({"navigation":[{"error":"terrain uncertain"}],"pause_config_error":true}),
        &config,
    )?;
    let message = got["error"]["message"].as_str().ok_or("message")?;
    assert!(message.contains("terrain uncertain"));
    assert!(message.contains("invalid pause evidence"));
    assert!(
        got["events"]
            .as_array()
            .ok_or("events")?
            .iter()
            .any(|e| e[0] == "close")
    );
    let got = exercise(
        &json!({"navigation":[{"error":"terrain uncertain"}],"pause_config_error":true,"pause_corrupt_errors":true}),
        &config,
    )?;
    let message = got["error"]["message"].as_str().ok_or("message")?;
    assert!(
        message.contains("terrain uncertain")
            && message.contains("invalid pause evidence")
            && message.contains("run report errors must be an array")
    );
    assert!(
        got["events"]
            .as_array()
            .ok_or("events")?
            .iter()
            .any(|e| e[0] == "close")
    );
    let got = exercise(
        &json!({"cleanup_errors":["hotkey cleanup: test failure","capture cleanup: test failure"]}),
        &config,
    )?;
    assert_eq!(got["result"], 2);
    let report = got["events"]
        .as_array()
        .ok_or("events")?
        .iter()
        .rev()
        .find(|e| e[0] == "report")
        .ok_or("report")?;
    assert_eq!(report[1]["errors"].as_array().ok_or("errors")?.len(), 2);
    Ok(())
}

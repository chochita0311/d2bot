use super::*;
use crate::{
    actions::town_loop::Difficulty,
    common::time::SystemClock,
    runs::summoner::{
        encounter_execution::EncounterEvidence,
        main_run::{RunOutput, SummonerRuntime, execute_prepared},
    },
};
use serde_json::json;
use std::{error::Error, path::Path, sync::Mutex};
type TestResult = Result<(), Box<dyn Error>>;
#[derive(Default)]
struct MemoryOutput {
    report: Value,
    frames: Vec<String>,
}
impl RunOutput for MemoryOutput {
    fn report_path(&self) -> &Path {
        Path::new("test-memory/report.json")
    }
    fn save_report(&mut self, report: &Value) -> Result<(), RunError> {
        self.report = report.clone();
        Ok(())
    }
    fn save_frame(&mut self, evidence: &EncounterEvidence) -> Result<(), RunError> {
        self.frames.push(evidence.filename.clone());
        Ok(())
    }
}
fn context(
    root: &Path,
    stop: Option<Cancellation>,
    emit: FieldEmitter,
) -> Result<RuntimeContext, RunError> {
    Ok(RuntimeContext {
        root: root.into(),
        config: BotConfig::load(root.join("config"))?,
        clock: Arc::new(SystemClock::default()),
        stop,
        emit,
        entry_directory: Some(root.join("target/tmp/native-entry-uncreated")),
    })
}
#[test]
fn dry_whole_runtime_loads_current_assets_without_effects_or_output() -> TestResult {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let events = Arc::new(Mutex::new(vec![]));
    let messages = events.clone();
    let emit: FieldEmitter = Arc::new(move |message| {
        if let Ok(mut messages) = messages.lock() {
            messages.push(message.to_owned());
        }
    });
    for continuous in [false, true] {
        let mut options = SummonerOptions::new(
            root.join("target/tmp/native-run-uncreated"),
            Difficulty::Hell,
        );
        options.character_key = Some("flash".into());
        options.continuous = continuous;
        assert!(!options.output.exists());
        assert!(!root.join("target/tmp/native-entry-uncreated").exists());
        assert_eq!(
            super::super::execute_once(context(root, None, emit.clone())?, &options)?,
            0
        );
        assert!(!options.output.exists());
        assert!(!root.join("target/tmp/native-entry-uncreated").exists());
    }
    let messages = events.lock().map_err(|_| "test messages poisoned")?;
    assert_eq!(messages.len(), 2);
    assert!(messages[0].contains("1 north casts; no game input"));
    assert!(messages[1].contains("20 seconds held-F2 wide steering; no game input"));
    Ok(())
}
#[test]
fn pre_stopped_native_owner_cleans_partial_initialization_and_keeps_report_boundary() -> TestResult
{
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for continuous in [false, true] {
        let external = Cancellation::default();
        external.cancel();
        let mut options = SummonerOptions::new(
            root.join("target/tmp/native-stopped-uncreated"),
            Difficulty::Hell,
        );
        options.character_key = Some("flash".into());
        options.live = true;
        options.continuous = continuous;
        let context = context(root, Some(external), Arc::new(|_| {}))?;
        let selected = options.selected(&context.config)?;
        let mut runtime = WindowsSummonerRuntime::load(context, &options, &selected)?;
        runtime.prepare_live()?;
        assert!(!runtime.has_capture());
        assert!(!runtime.has_backend());
        assert!(runtime.continuous.is_none());
        assert!(runtime.hotkeys.is_none());
        let mut output = MemoryOutput::default();
        assert_eq!(
            execute_prepared(&mut runtime, &mut output, &options, &selected, |_| {})?,
            2
        );
        assert_eq!(output.report["errors"], json!(["Arcane entry interrupted"]));
        assert_eq!(output.report["evidence_write_status"], "complete");
        assert_eq!(output.report["input_events"], json!([]));
        assert!(output.frames.is_empty());
        for absent in [
            "camera_projection",
            "resource_recoveries",
            "loot_scans",
            "frame_geometry",
        ] {
            assert!(output.report.get(absent).is_none(), "{absent}");
        }
        assert!(runtime.stopped());
        assert!(runtime.gate.stopped());
        assert!(!runtime.camera_started);
        assert!(!runtime.has_capture());
        assert!(!runtime.has_backend());
        assert!(runtime.hotkeys.is_none());
        assert!(!options.output.exists());
        assert!(!root.join("target/tmp/native-entry-uncreated").exists());
    }
    Ok(())
}
#[test]
fn dry_room_cycles_keep_source_validation_bypass_without_assets_or_output() -> TestResult {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let messages = Arc::new(Mutex::new(vec![]));
    let sink = messages.clone();
    let emit: FieldEmitter = Arc::new(move |message| {
        if let Ok(mut rows) = sink.lock() {
            rows.push(message.to_owned());
        }
    });
    let mut context = context(root, None, emit)?;
    context.root = root.join("target/tmp/nonexistent-dry-assets");
    assert!(!context.root.exists());
    let mut options = SummonerOptions::new(
        root.join("target/tmp/native-dry-cycles-uncreated"),
        Difficulty::Hell,
    );
    options.character_key = Some("not-a-profile".into());
    options.continuous = true;
    options.automatic = false;
    for cycles in [1, 5] {
        assert_eq!(
            super::super::execute(context.clone(), &options, Some(cycles))?,
            0
        );
    }
    assert!(super::super::execute(context, &options, Some(0)).is_err());
    assert!(!options.output.exists());
    let rows = messages.lock().map_err(|_| "test messages poisoned")?;
    assert_eq!(rows.len(), 2);
    assert!(rows[0].ends_with("1 north room(s)"));
    assert!(rows[1].ends_with("5 north room(s)"));
    Ok(())
}

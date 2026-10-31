//! 짧은 감독 시험의 CLI/파일 연결. 자동 실행과 감독 확인을 서로 대체하지 않는다.
mod buffs;
pub mod hold;
mod native;
pub mod options;
use super::{ToolError, invalid};
use crate::{
    common::{
        buffing::PauseSchedule,
        config::BotConfig,
        controller::InputGate,
        field_control::{FieldAction, HeldFieldInput},
        field_preparation::{self, BuffTrace, NoPreparationChecks, ProbeSignals, ProbeWait},
        hotkeys::{HotkeyAction, HotkeyBinding},
        survival::CharacterSurvivalPolicy,
        time::{MonotonicClock, SystemClock},
        vision_ops::codec,
        workers::Cancellation,
    },
    platform::windows::{ConsoleInterrupt, HotkeyListener, StdinLines},
    runs::{RunError, summoner::navigation_execution},
};
use native::NativeProbe;
use options::Options;
use serde_json::{Value, json};
use std::{collections::BTreeMap, ffi::OsString, io::Write, path::Path, sync::Arc, time::Duration};
fn emit(output: &mut impl Write, text: &str) -> Result<(), ToolError> {
    writeln!(output, "{text}")?;
    output.flush()?;
    Ok(())
}
fn failure(message: impl Into<String>) -> RunError {
    RunError::Runtime(message.into())
}
struct Signals<'a>(&'a StdinLines);
impl ProbeSignals<RunError> for Signals<'_> {
    fn try_read(&mut self) -> Result<Option<Value>, RunError> {
        self.0
            .try_read()?
            .map(|line| {
                let value: Value = serde_json::from_str(&line)?;
                if !value.is_object() {
                    return Err(failure("probe signal must be an object"));
                }
                Ok(value)
            })
            .transpose()
    }
}
fn errors(report: &mut Value) -> &mut Value {
    &mut report["errors"]
}
fn append_error(report: &mut Value, error: impl ToString) {
    if let Some(errors) = errors(report).as_array_mut() {
        let value = json!(error.to_string());
        if !errors.contains(&value) {
            errors.push(value);
        }
    }
}
fn close_reader(reader: &mut Option<StdinLines>, report: &mut Value) {
    if let Some(reader) = reader {
        loop {
            if let Err(error) = reader.close(Duration::from_secs(1)) {
                append_error(report, error);
            }
            if reader.is_finished() {
                break;
            }
        }
    }
}
fn wait_signal(
    env: &mut NativeProbe,
    reader: &StdinLines,
    policy: &CharacterSurvivalPolicy,
    deadline: f64,
    phase: &str,
    accept: impl FnMut(&mut NativeProbe, &Value) -> Result<(), crate::common::error::ControlError>,
) -> Result<Value, RunError> {
    field_preparation::wait_probe_signal(
        env,
        &mut Signals(reader),
        policy,
        ProbeWait {
            deadline,
            phase,
            retry_unverified: false,
            retry_stale: false,
        },
        accept,
        &mut NoPreparationChecks,
        |_| {},
    )
}
fn save_traces(
    output: &Path,
    name: &str,
    frames: Option<Vec<BuffTrace>>,
    report: &mut Value,
) -> Result<(), ToolError> {
    if let Some(frames) = frames {
        let mut rows = vec![];
        for frame in frames {
            let index = frame.metadata["index"]
                .as_u64()
                .ok_or_else(|| invalid("trace index required"))?;
            let filename = format!(
                "{}-step-{index:02}.png",
                if name == "buff_trace" {
                    "buff"
                } else {
                    "renewal"
                }
            );
            codec::save_png(output.join(&filename), &frame.frame)?;
            let mut metadata = frame.metadata;
            metadata["frame"] = json!(filename);
            rows.push(metadata);
        }
        report[name] = json!(rows);
    }
    Ok(())
}
fn profile<'a>(config: &'a BotConfig, options: &Options) -> Result<&'a Value, ToolError> {
    config
        .section("characters")
        .and_then(|c| c.get(&options.character))
        .ok_or_else(|| invalid(format!("unknown character '{}'", options.character)))
}

pub fn run(
    input: impl IntoIterator<Item = OsString>,
    output: &mut impl Write,
) -> Result<u8, ToolError> {
    let Some(options) = options::parse(input)? else {
        output.write_all(options::HELP.as_bytes())?;
        return Ok(0);
    };
    let root = crate::app::runtime_root()?;
    let config = super::load_config(if options.config_explicit {
        options.config.clone()
    } else {
        root.join("config")
    })?;
    let profile = profile(&config, &options)?;
    if profile["survival"].is_null() {
        return Err(
            crate::common::cli::UsageError("character survival policy required".into()).into(),
        );
    }
    let policy = CharacterSurvivalPolicy::from_value(&profile["survival"])?;
    let mut planner = options
        .step_after_buff
        .as_ref()
        .map(|_| navigation_execution::load_arcane_step_planner(&root, false, "sorceress"))
        .transpose()?;
    let reference = if planner.is_some() {
        let estimate = config.casting_estimate_for(Some(&options.character))?;
        if estimate.action != "estimate" {
            return Err(crate::common::cli::UsageError(
                "step requires configured class/FCR reference".into(),
            )
            .into());
        }
        Some(
            estimate
                .frames
                .and_then(|n| n.as_f64())
                .filter(|n| n.is_finite())
                .ok_or_else(|| invalid("step requires configured class/FCR reference"))?
                / 25.0,
        )
    } else {
        None
    };
    let actions = &profile["actions"];
    let order: Vec<String> = serde_json::from_value(actions["pre_run_buff_order"].clone())?;
    let raw_pauses: BTreeMap<String, f64> =
        serde_json::from_value(actions["buff_action_pause_seconds"].clone())?;
    let pauses = PauseSchedule::new(raw_pauses, 0.4)?;
    let movement = actions["movement_skill_key"]
        .as_str()
        .ok_or_else(|| invalid("movement key required"))?;
    let attack = actions["primary_attack_skill_key"]
        .as_str()
        .ok_or_else(|| invalid("attack key required"))?;
    let clock: Arc<dyn MonotonicClock> = Arc::new(SystemClock::default());
    let stop = Cancellation::default();
    let gate = InputGate::default();
    let mut env = NativeProbe::load(
        &root,
        &config,
        options.live,
        clock.clone(),
        stop.clone(),
        gate.clone(),
    )?;
    let inputs = HeldFieldInput::new(
        env.input.clone(),
        movement,
        attack,
        Some(clock.clone()),
        None,
        0.0,
    )?;
    let interrupt_stop = stop.clone();
    let interrupt_gate = gate.clone();
    let mut console = ConsoleInterrupt::with_callback(Arc::new(move || {
        interrupt_stop.cancel();
        interrupt_gate.stop();
        Ok(())
    }))?;
    let mut hotkeys = None;
    let mut initial_reader = None;
    let mut signals = None;
    let mut trace = options.buff_trace.then(Vec::new);
    let mut renewal_trace = (options.buff_trace && options.renew_once).then(Vec::new);
    let mut created = false;
    let mut report = json!({"status":"preparation_error","errors":[]});
    let result = (|| -> Result<(), ToolError> {
        let raw = if options.permit_stdin {
            emit(
                output,
                "READY: awaiting one fresh supervised permit JSON line; no input sent",
            )?;
            initial_reader = Some(StdinLines::start(1)?);
            loop {
                if stop.is_cancelled() {
                    return Err(invalid("permit input interrupted"));
                }
                if let Some(line) = initial_reader
                    .as_ref()
                    .ok_or_else(|| invalid("stdin reader required"))?
                    .try_read()
                    .map_err(|error| {
                        if error.to_string() == "probe signal exceeds 4096 characters" {
                            ToolError::Usage(crate::common::cli::UsageError(
                                "permit exceeds 4096 characters".into(),
                            ))
                        } else {
                            ToolError::from(error)
                        }
                    })?
                {
                    break line;
                }
                stop.wait(Duration::from_millis(20))?;
            }
        } else {
            let text = std::fs::read_to_string(
                options
                    .permit
                    .as_ref()
                    .ok_or_else(|| invalid("permit required"))?,
            )?;
            text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned()
        };
        close_reader(&mut initial_reader, &mut report);
        if report["errors"].as_array().is_some_and(|e| !e.is_empty()) {
            return Err(invalid(format!(
                "permit reader cleanup failed: {}",
                report["errors"]
            )));
        }
        let permit: Value = serde_json::from_str(&raw)?;
        if !permit.is_object() {
            return Err(invalid("permit must be an object"));
        }
        field_preparation::validate_permit(
            &permit,
            &options.character,
            &policy,
            native::wall_now(),
            options.action == "buff",
        )
        .map_err(|e| crate::common::cli::UsageError(e.to_string()))?;
        std::fs::create_dir_all(
            options
                .output
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )?;
        std::fs::create_dir(&options.output)?;
        created = true;
        let hotkeys_config = config
            .section("hotkeys")
            .ok_or_else(|| invalid("hotkeys required"))?;
        let mut bindings = vec![];
        let mut keys = vec![];
        for name in ["stop", "pause"] {
            let key = hotkeys_config[name]
                .as_str()
                .ok_or_else(|| invalid("hotkey required"))?;
            if !keys.contains(&key.to_ascii_lowercase()) {
                bindings.push(HotkeyBinding::parse(key, HotkeyAction::Stop)?);
                keys.push(key.to_ascii_lowercase());
            }
        }
        let event_stop = stop.clone();
        let event_gate = gate.clone();
        hotkeys = Some(HotkeyListener::start(
            bindings,
            Arc::new(move |_| {
                event_stop.cancel();
                event_gate.stop();
                Ok(())
            }),
        )?);
        codec::save_png(options.output.join("before.png"), env.grab()?.as_ref())?;
        field_preparation::validate_permit(
            &permit,
            &options.character,
            &policy,
            native::wall_now(),
            options.action == "buff",
        )?;
        if options.action == "buff" {
            buffs::run_buffs(
                &mut env,
                buffs::BuffRun {
                    options: &options,
                    policy: &policy,
                    order: &order,
                    pauses: &pauses,
                    movement,
                    planner: &mut planner,
                    reference,
                    trace: &mut trace,
                    renewal_trace: &mut renewal_trace,
                    signals: &mut signals,
                },
                output,
                &mut report,
            )?;
        } else {
            let wall = native::wall_now();
            let remaining = policy
                .buffs()
                .iter()
                .map(|b| {
                    permit["buff_casts"][b.name()]
                        .as_f64()
                        .map(|cast| cast + b.duration_seconds() - b.renew_before_seconds() - wall)
                })
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| invalid("buff cast time required"))?
                .into_iter()
                .reduce(f64::min)
                .ok_or_else(|| invalid("no timed buff rule"))?;
            let point = options
                .aim
                .as_ref()
                .ok_or_else(|| invalid("aim required"))?;
            let coordinate = |n: &serde_json::Number| {
                n.as_f64()
                    .filter(|n| n.is_finite())
                    .ok_or_else(|| invalid("aim exceeds runtime coordinate range"))
            };
            let aim = env.to_desktop([coordinate(&point[0])?, coordinate(&point[1])?])?;
            let window = env.window.clone();
            report = hold::hold_probe(
                &mut env,
                &policy,
                hold::HoldControl {
                    inputs: inputs.clone(),
                    clock: clock.clone(),
                    stop: stop.clone(),
                    focused: Arc::new(move || window.focused()),
                },
                hold::HoldRequest {
                    action: if options.action == "travel" {
                        FieldAction::Travel
                    } else {
                        FieldAction::Combat
                    },
                    seconds: options.seconds,
                    aim,
                    buff_deadline: clock.now() + remaining,
                },
            )?;
        }
        codec::save_png(options.output.join("after.png"), env.grab()?.as_ref())?;
        Ok(())
    })();
    if created && let Err(error) = &result {
        report["status"] = json!("probe_error");
        append_error(&mut report, error);
    }
    stop.cancel();
    gate.stop();
    if let Err(error) = inputs.close() {
        append_error(&mut report, error);
    }
    if let Some(listener) = &mut hotkeys
        && let Err(error) = listener.stop(Duration::from_secs(1))
    {
        append_error(&mut report, error);
    }
    close_reader(&mut initial_reader, &mut report);
    close_reader(&mut signals, &mut report);
    if let Err(error) = env.close() {
        append_error(&mut report, error);
    }
    if let Err(error) = console.close() {
        append_error(&mut report, error);
    }
    if !created {
        return match result {
            Err(error) => {
                if report["errors"].as_array().is_some_and(|e| !e.is_empty()) {
                    match error {
                        ToolError::Usage(error) => Err(crate::common::cli::UsageError(format!(
                            "{error}; cleanup errors: {}",
                            report["errors"]
                        ))
                        .into()),
                        error => Err(invalid(format!(
                            "{error}; cleanup errors: {}",
                            report["errors"]
                        ))),
                    }
                } else {
                    Err(error)
                }
            }
            Ok(()) => Err(invalid("probe output was not created")),
        };
    }
    report["mode"] = json!(if options.live {
        "live_supervised_probe"
    } else {
        "dry_probe"
    });
    report["action"] = json!(options.action);
    report["seconds"] = json!(options.seconds);
    report["input_events"] = env.input.events()?;
    save_traces(&options.output, "buff_trace", trace, &mut report)?;
    save_traces(&options.output, "renewal_trace", renewal_trace, &mut report)?;
    if options.action != "buff"
        && !report["input_events"].as_array().is_some_and(|a| {
            a.iter()
                .any(|e| e["action"] == "key_down" && e["sent"] == true)
        })
    {
        report["status"] = json!(format!(
            "no_input_{}",
            report["status"].as_str().unwrap_or("probe_error")
        ));
    }
    std::fs::write(
        options.output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    serde_json::to_writer(&mut *output, &report)?;
    writeln!(output)?;
    Ok(
        if report["errors"].as_array().is_some_and(Vec::is_empty)
            && matches!(
                report["status"].as_str(),
                Some(
                    "duration_complete"
                        | "dispatched_unconfirmed"
                        | "renewed_unconfirmed"
                        | "step_arrival_observed"
                )
            )
        {
            0
        } else {
            2
        },
    )
}

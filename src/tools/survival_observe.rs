use super::{ToolError, invalid};
use crate::common::{
    capture::{CaptureSettings, ScreenCapture},
    cli::{self, Kind, OptionSpec as Opt, UsageError},
    survival::{CharacterSurvivalPolicy, CharacterSurvivalState, ObservationStamp},
    survival_vision::{HudObserverOptions, SurvivalHudObserver},
    time::{MonotonicClock, SystemClock},
    vision_ops::codec,
};
use crate::platform::{InputSystem, windows::Windows};
use serde_json::{Value, json};
use std::{ffi::OsString, io::Write, path::Path, sync::Arc};

pub const HELP: &str = "Usage: d2bot survival-observe (--live | --image PATH) [--frames 1..5] [--assets PATH] [--config PATH] [--character KEY --room ROOM --age-limit SECONDS] [--output JSON]\nInput-free survival HUD observation; no game inputs.\n";
const OPTIONS: &[Opt] = &[
    Opt::new("--live", Kind::Flag).group(1),
    Opt::new("--image", Kind::Value).group(1),
    Opt::new("--frames", Kind::Range(1, 5)),
    Opt::new("--assets", Kind::Value),
    Opt::new("--config", Kind::Value),
    Opt::new("--character", Kind::Value),
    Opt::new("--room", Kind::Value),
    Opt::new("--age-limit", Kind::Float),
    Opt::new("--output", Kind::Value),
];
pub fn run(
    input: impl IntoIterator<Item = OsString>,
    output: &mut impl Write,
) -> Result<u8, ToolError> {
    let args = cli::parse(input, OPTIONS, &[1])?;
    if args.help {
        output.write_all(HELP.as_bytes())?;
        return Ok(0);
    }
    let character = args.text("--character")?.filter(|s| !s.is_empty());
    let room = args.text("--room")?.filter(|s| !s.is_empty());
    if character.is_some() != room.is_some() {
        return Err(UsageError("--character and --room must be provided together".into()).into());
    }
    let age = args.float("--age-limit");
    if character.is_some() && !age.is_some_and(|n| n.is_finite() && n > 0.0) {
        return Err(UsageError(
            "Policy requests require an explicit positive finite --age-limit".into(),
        )
        .into());
    }
    let runtime_root = crate::app::runtime_root()?;
    let observer = SurvivalHudObserver::load(
        args.path("--assets")
            .unwrap_or_else(|| runtime_root.join("assets/ui/survival")),
        HudObserverOptions::default(),
    )?;
    let config = if character.is_some() || args.has("--live") {
        Some(super::load_config(
            args.path("--config")
                .unwrap_or_else(|| runtime_root.join("config")),
        )?)
    } else {
        None
    };
    let policy = if let Some(character) = character {
        let raw = config
            .as_ref()
            .and_then(|c| c.section("characters"))
            .and_then(|v| v.get(character))
            .map(|v| &v["survival"])
            .filter(|v| !v.is_null())
            .ok_or_else(|| {
                ToolError::Usage(UsageError(
                    "Explicit character with a configured survival policy required".into(),
                ))
            })?;
        Some(Arc::new(CharacterSurvivalPolicy::from_value(raw)?))
    } else {
        None
    };
    let mut capture = if args.has("--live") {
        let config = config
            .as_ref()
            .ok_or_else(|| invalid("live configuration required"))?;
        let mut settings = CaptureSettings::from_config(config)?;
        settings.window_title = Some("Diablo II: Resurrected".into());
        settings.window_title_mode = "exact".into();
        settings.capture_backend = "window".into();
        Some(ScreenCapture::new(
            Windows::new()?,
            settings,
            &runtime_root,
        )?)
    } else {
        None
    };
    let clock = SystemClock::default();
    let result = (|| {
        let mut reports = Vec::new();
        for sequence in 0..args.small("--frames").unwrap_or(1) {
            if let Some(capture) = &capture {
                let window = capture.configured_window()?.ok_or_else(|| {
                    invalid("Visible, non-minimized game window required for observation")
                })?;
                if Windows::new()?.minimized(window.id)? {
                    return Err(invalid(
                        "Visible, non-minimized game window required for observation",
                    ));
                }
            }
            let started = clock.now();
            let frame = if let Some(capture) = &mut capture {
                capture.grab()?.frame
            } else {
                Arc::new(
                    codec::load_color(
                        &args
                            .path("--image")
                            .ok_or_else(|| invalid("image required"))?,
                    )
                    .map_err(|e| invalid(format!("Could not read observation image: {e}")))?,
                )
            };
            let acquired = clock.now();
            let reading = observer.observe(frame.view())?;
            let finished = clock.now();
            let mut report = serde_json::to_value(&reading)?;
            let fields = json!({"source":if capture.is_some(){"live_window"}else{"offline_image"},"frame_size":[frame.size().width(),frame.size().height()],"sequence":sequence,"capture_started_at":started,"acquired_at":acquired,"processed_at":finished,"capture_seconds":acquired-started,"processing_seconds":finished-acquired,"life_ratio":reading.life.ratio(),"mana_ratio":reading.mana.ratio()});
            report
                .as_object_mut()
                .ok_or_else(|| invalid("HUD report must be object"))?
                .extend(
                    fields
                        .as_object()
                        .ok_or_else(|| invalid("timing report must be object"))?
                        .clone(),
                );
            if let (Some(policy), Some(character), Some(room), Some(age)) =
                (&policy, character, room, age)
            {
                let mut state = CharacterSurvivalState::new(character, room, Some(policy.clone()))?;
                let stamp = ObservationStamp::new(character, room, sequence as u64, started)?;
                let contents = reading.belt_contents(policy);
                if contents.is_some() {
                    state.observe_belt(&stamp, contents.as_ref());
                }
                let safety = reading.safety_observation(stamp)?;
                let decision = state.decide(&safety, finished, age, age)?;
                report["policy_character"] = json!(character);
                report["room"] = json!(room);
                report["belt_contents"] = contents.map_or(Value::Null, |c| {
                    json!(
                        c.into_iter()
                            .map(|(n, b)| (n.to_string(), json!([b.kind, b.count])))
                            .collect::<serde_json::Map<_, _>>()
                    )
                });
                report["decision"] = serde_json::to_value(decision)?;
            }
            reports.push(report);
        }
        Ok(json!({"mode":"observation_only","inputs_sent":0,"reports":reports}))
    })();
    let cleanup = capture
        .as_mut()
        .map_or(Ok(()), |c| c.close().map_err(ToolError::from));
    let result: Value = match (result, cleanup) {
        (Ok(result), Ok(())) => result,
        (Err(error), Ok(())) | (Ok(_), Err(error)) => return Err(error),
        (Err(error), Err(cleanup)) => {
            return Err(invalid(format!("{error}; capture close failed: {cleanup}")));
        }
    };
    if let Some(path) = args.path("--output") {
        std::fs::create_dir_all(
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )?;
        let mut file = std::fs::File::create(path)?;
        serde_json::to_writer_pretty(&mut file, &result)?;
        writeln!(file)?;
        file.flush()?;
    }
    serde_json::to_writer(&mut *output, &result)?;
    writeln!(output)?;
    Ok(0)
}

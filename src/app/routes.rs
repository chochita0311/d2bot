use super::{
    arguments::{MainOptions, Route},
    execution,
};
use crate::{
    actions::{
        desktop::DesktopActionBackend,
        session::{ActionSession, SessionOutcome, SessionStatus},
        town_loop::{Destination, Difficulty, TownOptions},
    },
    common::{
        capture::CaptureSettings, config::BotConfig, controller::InputGate, time::SystemClock,
    },
    core::{PreviewConfig, PreviewFrames, native::NativePreview},
    platform::{
        CaptureBackend,
        windows::{Windows, local_timestamp},
    },
    runs::{
        RunError,
        summoner::{
            self,
            main_run::native::{self, RuntimeContext},
        },
    },
};
use std::{io::Write, path::PathBuf, sync::Arc, time::Duration};
pub fn run(options: MainOptions, output: &mut (impl Write + Send)) -> Result<u8, RunError> {
    if options.route() == Route::Windows {
        for window in Windows::new()?.windows()? {
            let t = window.target;
            writeln!(
                output,
                "{} | left={} top={} width={} height={}",
                window.title,
                t.left,
                t.top,
                t.size().width(),
                t.size().height()
            )?;
        }
        return Ok(0);
    }
    let root = super::runtime_root()?;
    let config_path = if options.config_explicit {
        options.config.clone()
    } else {
        root.join("config")
    };
    let config = BotConfig::load(config_path)?;
    if options.route() != Route::Gui {
        for warning in config.warnings() {
            writeln!(output, "warning: {warning}")?;
        }
        output.flush()?;
    }
    match options.route() {
        Route::Gui => {
            crate::ui::host::run(crate::ui::native::runtime(config, root)?)?;
            Ok(0)
        }
        Route::Preview => {
            let settings = PreviewConfig::from_config(&config, &root)?;
            let frames = PreviewFrames::default();
            let display = settings.overlay.then(|| frames.clone());
            execution::run(output, display, Some(130), move |stop, emit| {
                let controller_emit = emit.clone();
                let controller_emit = Arc::new(move |message: &str| {
                    controller_emit(&format!(
                        "{} | INFO | diablo2.controller | {message}",
                        crate::platform::windows::local_log_timestamp()
                    ))
                });
                let gate = InputGate::default();
                let mut env = NativePreview::load(
                    &config,
                    &root,
                    stop.clone(),
                    gate.clone(),
                    controller_emit,
                    settings.log_level,
                )?;
                crate::core::run(
                    &mut env,
                    &settings,
                    &stop,
                    &gate,
                    &frames,
                    |logger, message| {
                        emit(&format!(
                            "{} | INFO | {logger} | {message}",
                            crate::platform::windows::local_log_timestamp()
                        ))
                    },
                )
            })
        }
        Route::Summoner => {
            let directory = options.output.clone().unwrap_or_else(|| {
                PathBuf::from("recordings/summoner/evidence")
                    .join(format!("{}-main", local_timestamp()))
            });
            let request = options
                .summoner_options(directory)
                .map_err(|e| RunError::Runtime(e.to_string()))?;
            execution::run(output, None, None, move |stop, emit| {
                let context = RuntimeContext {
                    root,
                    config,
                    clock: Arc::new(SystemClock::default()),
                    stop: Some(stop),
                    emit,
                    entry_directory: None,
                };
                let code = native::execute(context, &request, options.north_cycles)?;
                u8::try_from(code)
                    .map_err(|_| RunError::Runtime("Summoner exit status exceeds range".into()))
            })
        }
        Route::Town => execution::run(output, None, None, move |stop, emit| {
            let vision = summoner::town_vision(&root)?;
            let mut settings = CaptureSettings::from_config(&config)?;
            settings.window_title = Some("Diablo II: Resurrected".into());
            settings.window_title_mode = "exact".into();
            settings.capture_backend = "window".into();
            let difficulty = match options.difficulty.as_str() {
                "normal" => Difficulty::Normal,
                "nightmare" => Difficulty::Nightmare,
                _ => Difficulty::Hell,
            };
            let request = TownOptions {
                repeat_count: if options.arcane_entry {
                    1
                } else {
                    options.town_loop.unwrap_or(1)
                },
                difficulty,
                destination: if options.arcane_entry || options.town_destination == "arcane" {
                    Destination::Arcane
                } else {
                    Destination::Town
                },
                arrival_only: options.arcane_entry,
                ..Default::default()
            };
            let directory = root
                .join("recordings/summoner/evidence")
                .join(format!("{}-town-loop", local_timestamp()));
            let mut session = ActionSession::new("Run lifecycle");
            session.start_town(vision, request, false, directory, move |_| {
                DesktopActionBackend::windows(settings, &root)
            })?;
            while session.is_running() {
                if stop.is_cancelled() {
                    session.request_stop();
                }
                for event in session.events().drain() {
                    emit(&format!("{}: {}", event.level.name(), event.message));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            session.stop(Duration::ZERO)?;
            for event in session.events().drain() {
                emit(&format!("{}: {}", event.level.name(), event.message));
            }
            match session.status()? {
                SessionStatus::Finished(Ok(SessionOutcome::Completed)) => Ok(0),
                SessionStatus::Finished(Err(error)) => {
                    emit(&format!("error: {error}"));
                    Ok(2)
                }
                _ => Ok(2),
            }
        }),
        Route::Windows => Ok(0),
    }
}

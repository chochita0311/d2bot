//! 실제 제어판을 그대로 실행하고 자체 자원 수치를2초 간격으로 stdout에 기록한다.
//! 게임 입력은 실제 GUI 액션을 시작한 경우에만 해당 세션이 전송한다.
//! config 경로 뒤 --resume-arcane은 현재 필드의8초 감독 제어만 실행한다.
//! --return-north는 기존 복귀/마감 runtime만 실행하며 새 방을 만들지 않는다.
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use d2bot::{
        actions::recording::RecordingSession,
        common::config::BotConfig,
        platform::windows::resources::own_resources,
        ui::{
            host::{ControlPanel, native_options},
            native::{self, NativeActionBackend},
            view,
        },
    };
    use std::{
        path::PathBuf,
        time::{Duration, Instant},
    };
    struct ProbedPanel {
        panel: ControlPanel<NativeActionBackend, RecordingSession>,
        started: Instant,
        sampled: Instant,
    }
    impl eframe::App for ProbedPanel {
        fn ui(&mut self, ui: &mut eframe::egui::Ui, frame: &mut eframe::Frame) {
            eframe::App::ui(&mut self.panel, ui, frame);
            if self.sampled.elapsed() >= Duration::from_secs(2) {
                self.sampled = Instant::now();
                match own_resources() {
                    Ok(resources) => println!(
                        "{}",
                        serde_json::json!({"elapsed_seconds":self.started.elapsed().as_secs_f64(),"resources":resources})
                    ),
                    Err(error) => eprintln!("resource read: {error}"),
                }
            }
        }
    }
    let root = std::env::current_dir()?;
    let mut args = std::env::args_os().skip(1);
    let config = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("config"));
    let mode = match args.next() {
        Some(flag) if flag == "--resume-arcane" => 1,
        Some(flag) if flag == "--return-north" => 2,
        None => 0,
        _ => {
            return Err(
                "expected optional --resume-arcane or --return-north after config path".into(),
            );
        }
    };
    if args.next().is_some() {
        return Err("unexpected probe argument".into());
    }
    let config = BotConfig::load(config)?;
    let recorder = RecordingSession::windows(&config, root.clone())?;
    let mut effects = native::effects();
    if mode != 0 {
        effects.summoner = Box::new(move |session, root, request| {
            let returning = mode == 2;
            let output = root.join("recordings/summoner/evidence").join(format!(
                "probe-{}-{}",
                if returning { "return" } else { "resume" },
                d2bot::platform::windows::local_timestamp()
            ));
            session.start_with(
                request,
                move || Ok(output),
                move |mut work| {
                    work.options.resume_arcane = true;
                    work.options.resume_labels_off = true;
                    work.options.field_seconds = if returning {60} else {8};
                    work.options.segments = if returning {4} else {1};
                    work.options.finish_room = returning;
                    work.options.return_from = returning.then_some(d2bot::runs::summoner::navigation::Wing::North);
                    if returning { (work.emit)("SUPERVISED_RETURN_ONLY: current north return -> central waypoint -> Act1 -> save/exit; no fresh room"); }
                    let context = d2bot::runs::summoner::main_run::native::RuntimeContext {
                        root,
                        config: work.config,
                        clock: std::sync::Arc::new(d2bot::common::time::SystemClock::default()),
                        stop: Some(work.stop),
                        emit: work.emit,
                        entry_directory: None,
                    };
                    d2bot::runs::summoner::main_run::native::execute(context, &work.options, None)
                },
            )
        });
    }
    let runtime = d2bot::ui::runtime::PanelRuntime::new(config, root, recorder, effects)?;
    let panel = ControlPanel::new(runtime);
    eframe::run_native(
        if mode == 2 {
            "Diablo2 Helper — resource probe (return north)"
        } else if mode == 1 {
            "Diablo2 Helper — resource probe (resume)"
        } else {
            "Diablo2 Helper — resource probe"
        },
        native_options()?,
        Box::new(move |context| {
            view::configure(&context.egui_ctx);
            Ok(Box::new(ProbedPanel {
                panel,
                started: Instant::now(),
                sampled: Instant::now(),
            }))
        }),
    )?;
    println!(
        "{}",
        serde_json::json!({"phase":"native_gui_dropped","resources":own_resources()?})
    );
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Windows resource counters required");
}

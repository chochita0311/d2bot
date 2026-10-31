//! 입력을 보내지 않고 동일 계산/캡처를 GPU GUI 유무에 따라 분리 계측한다.
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use d2bot::{
        common::vision_ops::codec,
        platform::{
            CaptureBackend,
            windows::{Windows, resources::own_resources},
        },
        runs::summoner::{
            objective_vision::ArcaneObjectiveObserver, scene_vision::ArcaneHazardObserver,
            terminal_vision::ArcaneTerminalObserver,
        },
    };
    use std::{
        hint::black_box,
        path::PathBuf,
        thread::JoinHandle,
        time::{Duration, Instant},
    };
    type Failure = Box<dyn std::error::Error + Send + Sync>;
    type Worker = JoinHandle<Result<(), Failure>>;
    fn workload(mode: &str) -> Result<(), Failure> {
        let initial = own_resources()?;
        println!(
            "{}",
            serde_json::json!({"phase":"start","mode":mode,"resources":initial})
        );
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut backend = Windows::new()?;
        let target = if mode == "capture" {
            Some(
                backend
                    .windows()?
                    .into_iter()
                    .find(|w| w.title == "Diablo II: Resurrected")
                    .ok_or("game window unavailable")?
                    .target,
            )
        } else {
            None
        };
        let frame = codec::load_color(root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/navigation/north-step-before.png"))?;
        let region = root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
        {
            let mut terminal = ArcaneTerminalObserver::load(region.join("terminal-patterns.json"))?;
            let objective = ArcaneObjectiveObserver::load(&region)?;
            let mut hazards = if mode == "hazards" {
                Some(ArcaneHazardObserver::load(region.join("manifest.json"))?)
            } else {
                None
            };
            for i in 0usize..80 {
                if let Some(hazards) = &mut hazards {
                    black_box(hazards.observe(
                        frame.view(),
                        [962, 580],
                        1.0,
                        1.0,
                        Some([1446, 211]),
                    )?);
                } else if let Some(target) = target {
                    black_box(backend.capture_screen(target)?);
                } else {
                    black_box(terminal.observe(frame.view())?);
                    black_box(objective.observe(frame.view())?);
                }
                if i == 0 || (i + 1).is_multiple_of(10) {
                    let usage = own_resources()?;
                    println!("{}", serde_json::json!({"iteration":i+1,"resources":usage}));
                    if usage.private_bytes.saturating_sub(initial.private_bytes) > 768 * 1024 * 1024
                    {
                        break;
                    }
                }
            }
        }
        drop(frame);
        println!(
            "{}",
            serde_json::json!({"phase":"models_frame_dropped","resources":own_resources()?})
        );
        Ok(())
    }
    let gui = std::env::args().any(|s| s == "--gui");
    let cycles = if std::env::args().any(|s| s == "--repeat") {
        3
    } else {
        1
    };
    let mode = if std::env::args().any(|s| s == "--capture") {
        "capture"
    } else if std::env::args().any(|s| s == "--hazards") {
        "hazards"
    } else {
        "vision"
    };
    if !gui {
        for _ in 0..cycles {
            workload(mode).map_err(|e| e as Box<dyn std::error::Error>)?;
        }
        return Ok(());
    }
    struct App {
        mode: &'static str,
        started: Instant,
        worker: Option<Worker>,
        launched: bool,
        cycles: usize,
        completed: usize,
    }
    impl eframe::App for App {
        fn ui(&mut self, ui: &mut eframe::egui::Ui, _: &mut eframe::Frame) {
            ui.label("Resource isolation; no game input. Closes automatically.");
            if !self.launched
                && self.completed < self.cycles
                && self.started.elapsed() >= Duration::from_secs(1)
            {
                self.launched = true;
                let mode = self.mode;
                self.worker = Some(std::thread::spawn(move || workload(mode)));
            }
            if self.worker.as_ref().is_some_and(|w| w.is_finished())
                && let Some(worker) = self.worker.take()
            {
                match worker.join() {
                    Ok(Ok(())) => {}
                    other => eprintln!("isolation failed: {other:?}"),
                }
                self.completed += 1;
                self.launched = false;
                match own_resources() {
                    Ok(r) => println!(
                        "{}",
                        serde_json::json!({"phase":"worker_joined","cycle":self.completed,"resources":r})
                    ),
                    Err(e) => eprintln!("{e}"),
                }
            }
            if self.completed == self.cycles
                && self.worker.is_none()
                && self.started.elapsed() >= Duration::from_secs(8)
            {
                ui.ctx()
                    .send_viewport_cmd(eframe::egui::ViewportCommand::Close);
            }
            ui.ctx().request_repaint_after(Duration::from_millis(50));
        }
    }
    eframe::run_native(
        "Resource isolation",
        eframe::NativeOptions::default(),
        Box::new(move |_| {
            Ok(Box::new(App {
                mode,
                started: Instant::now(),
                worker: None,
                launched: false,
                cycles,
                completed: 0,
            }))
        }),
    )?;
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Windows required");
}

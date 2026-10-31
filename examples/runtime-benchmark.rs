//! 개발용 고정 화면 성능 측정. 게임 입력은 보내지 않으며 캡처는 명시한 경우만 수행한다.
#[path = "runtime-benchmark/control.rs"]
mod control;
use d2bot::{
    common::{
        capture::{CaptureSettings, ScreenCapture},
        frame::{Image, PixelRect, Size},
        frame_geometry::{FrameGeometry, PhysicalTarget},
        recording::{Mpeg4Writer, OutputMode},
        scene_motion::{BackgroundMotionObserver, LandmarkMask, MotionLimits, MotionView},
        survival_vision::{HudObserverOptions, SurvivalHudObserver},
        vision_ops::{codec, color, resize, template},
    },
    runs::summoner::{navigation::PathContext, navigation_execution::load_arcane_step_planner},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{error::Error, hint::black_box, path::PathBuf, time::Instant};

const SAMPLES: usize = 30;
const WARMUPS: usize = 3;
fn measure(
    mut operation: impl FnMut() -> Result<(), Box<dyn Error>>,
) -> Result<Value, Box<dyn Error>> {
    for _ in 0..WARMUPS {
        operation()?;
    }
    let mut times = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let start = Instant::now();
        operation()?;
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    Ok(
        json!({"samples":SAMPLES,"warmups":WARMUPS,"p50_ms":times[(SAMPLES-1)/2],"p95_ms":times[(SAMPLES*95).div_ceil(100)-1]}),
    )
}
fn hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
fn main() -> Result<(), Box<dyn Error>> {
    let _compute = d2bot::common::compute::ComputeLease::acquire()?;
    let mut args = std::env::args_os().skip(1);
    let snapshot = PathBuf::from(args.next().ok_or("snapshot PNG required")?);
    let output = PathBuf::from(args.next().ok_or("new output directory required")?);
    let flags = args.collect::<Vec<_>>();
    let capture_enabled = flags.iter().any(|arg| arg == "--capture");
    let record_during = flags.iter().any(|arg| arg == "--record-during");
    let control_packet = flags.iter().any(|arg| arg == "--control-packet");
    if flags
        .iter()
        .any(|arg| arg != "--capture" && arg != "--record-during" && arg != "--control-packet")
    {
        return Err("unknown benchmark option".into());
    }
    std::fs::create_dir(&output)?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/navigation");
    let paths = [
        snapshot,
        source.join("north-step-before.png"),
        source.join("north-step-after.png"),
    ];
    let frames = paths
        .iter()
        .map(codec::load_color)
        .collect::<Result<Vec<_>, _>>()?;
    let current = &frames[0];
    let size = Size::new(1922, 1140)?;
    if frames.iter().any(|frame| frame.size() != size) {
        return Err("1922x1140 inputs required".into());
    }
    let hud = SurvivalHudObserver::load(
        root.join("assets/ui/survival"),
        HudObserverOptions {
            allow_high_orb_fallback: true,
            allow_partial_orb_fallback: true,
            observe_mercenary: true,
        },
    )?;
    let (ground, path) = load_arcane_step_planner(&root, true, "sorceress")?;
    let panels = d2bot::runs::summoner::main_run::native::PanelObservers::load(&root)?;
    let current_owned = std::sync::Arc::new(current.clone());
    let camera_panels = panels.clone();
    let mut camera = d2bot::runs::summoner::camera_ground::CalibratedCameraGroundObserver::new(
        ground,
        move |frame| camera_panels.check(frame),
        d2bot::runs::summoner::camera_ground::CameraLimits {
            persist_reviewed_layout: true,
            ..Default::default()
        },
    )?;
    let reading = hud.observe(current.view())?;
    let layout = d2bot::runs::summoner::camera_ground::ReviewedCameraLayout::load(
        root.join("assets/ui/layouts/remastered-1922x1140/camera-ground.json"),
    )?;
    camera.confirm_runtime_viewport(&current_owned, &reading, true, 0.0, 0.0, &layout)?;
    let mut hazards = d2bot::runs::summoner::scene_vision::ArcaneHazardObserver::load(
        root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140/manifest.json"),
    )?;
    let region = root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
    let mut terminal = d2bot::runs::summoner::terminal_vision::ArcaneTerminalObserver::load(
        region.join("terminal-patterns.json"),
    )?;
    let objective =
        d2bot::runs::summoner::objective_vision::ArcaneObjectiveObserver::load(&region)?;
    let mut hub = d2bot::runs::summoner::scene_vision::ArcaneHubObserver::load(
        region.join("hub-patterns.json"),
    )?;
    let mut town = d2bot::actions::town_loop::TownVision::load(&root)?;
    let hover =
        d2bot::runs::summoner::scene_vision::ArcaneHoverObserver::load(root.join("assets"))?;
    let geometry = FrameGeometry::create(PhysicalTarget::new(0, 0, 1922, 1140)?, None, None)?;
    let floor = Image::<bool, 1>::from_vec(Size::new(240, 142)?, vec![false; 240 * 142])?;
    let mut motion = BackgroundMotionObserver::new(
        MotionLimits::new(12, 0.65, 3.0, 0.5)?,
        Some(LandmarkMask {
            viewport: PixelRect::new(120, 180, 1720, 890)?,
            actor_exclusion: [260, 230, 260, 120],
        }),
    );
    let observe_path = || {
        path.observe(
            frames[1].view(),
            PathContext {
                sequence: 1,
                generation: 0,
                captured_at: 1.0,
                frame_size: size,
                viewport: PixelRect::new(24, 55, 1900, 900)?,
                anchor: [960.0, 580.0],
            },
        )
    };
    let mut motion_result = Value::Null;
    let mut observe_motion = || -> Result<(), Box<dyn Error>> {
        motion.reset();
        for (index, frame) in frames[1..].iter().enumerate() {
            motion_result = serde_json::to_value(motion.observe(
                frame.view(),
                MotionView {
                    sequence: index as u64 + 1,
                    captured_at: index as f64 * 0.1,
                    frame_size: size,
                    viewport: PixelRect::full(size),
                    anchor: [960.0, 580.0],
                    cell_pixels: 8,
                    floor: &floor,
                    excluded_pixels: None,
                },
                None,
            )?)?;
        }
        Ok(())
    };
    // 실제 GUI와 같은 process/pool에서 encoder와 판독이 경쟁하는 경우를 분리한다.
    #[cfg(windows)]
    let mut competing = if record_during {
        d2bot::platform::windows::enable_dpi_awareness()?;
        let config = d2bot::common::config::BotConfig::from_value(
            json!({"capture":{"fps":8,"window_title":"Diablo II: Resurrected","window_title_mode":"exact","capture_backend":"screen"}}),
        )?;
        let mut session =
            d2bot::actions::recording::RecordingSession::windows(&config, root.clone())?;
        session.start_to(output.join("concurrent.avi"))?;
        Some((session, Instant::now()))
    } else {
        None
    };
    #[cfg(not(windows))]
    if record_during {
        return Err("concurrent native recording requires Windows".into());
    }
    let mut stages = serde_json::Map::new();
    stages.insert(
        "native_normalize_gray".into(),
        measure(|| {
            black_box(color::gray(
                resize::normalize(&geometry, current)?.as_ref().view(),
            )?);
            Ok(())
        })?,
    );
    let gray = color::gray(current.view())?;
    let world = gray.view().crop(PixelRect::new(24, 55, 1900, 900)?)?;
    let journal = codec::load_gray(region.join("templates/objective/journal.png"))?;
    for (name, method) in [
        ("journal_correlation", template::MatchMethod::Correlation),
        (
            "journal_normalized",
            template::MatchMethod::NormalizedCoefficient,
        ),
    ] {
        stages.insert(
            name.into(),
            measure(|| {
                black_box(template::match_template(world, journal.view(), method)?);
                Ok(())
            })?,
        );
    }
    stages.insert(
        "hsv".into(),
        measure(|| {
            black_box(color::hsv(current.view())?);
            Ok(())
        })?,
    );
    stages.insert(
        "label_mask".into(),
        measure(|| {
            black_box(d2bot::common::label_occlusion::label_occlusion_mask(
                current.view(),
                false,
            )?);
            Ok(())
        })?,
    );
    stages.insert(
        "town_normalize".into(),
        measure(|| {
            use d2bot::actions::town_loop::TownSight;
            town.set_frame(current_owned.clone());
            town.normalize()?;
            town.clear_frame();
            Ok(())
        })?,
    );
    stages.insert(
        "preview_scale_0_7".into(),
        measure(|| {
            black_box(resize::scale_linear(current.view(), 0.7)?);
            Ok(())
        })?,
    );
    stages.insert(
        "hud".into(),
        measure(|| {
            black_box(hud.observe(current.view())?);
            Ok(())
        })?,
    );
    stages.insert(
        "path".into(),
        measure(|| {
            black_box(observe_path()?);
            Ok(())
        })?,
    );
    stages.insert("orb_pair".into(), measure(&mut observe_motion)?);
    stages.insert(
        "panels".into(),
        measure(|| {
            black_box(panels.check(&std::sync::Arc::new(current.clone()))?);
            Ok(())
        })?,
    );
    stages.insert(
        "camera_ground".into(),
        measure(|| {
            camera.prepare_frame(&current_owned, 1.0, &reading, true)?;
            black_box(camera.observe(&current_owned, 1.0)?);
            Ok(())
        })?,
    );
    stages.insert(
        "near_hazards".into(),
        measure(|| {
            black_box(hazards.observe(current.view(), [962, 580], 1.0, 1.0, None)?);
            Ok(())
        })?,
    );
    stages.insert(
        "hover_route_aim".into(),
        measure(|| {
            black_box(hover.observe(current.view(), [1446, 211])?);
            Ok(())
        })?,
    );
    stages.insert(
        "near_hazards_route_aim".into(),
        measure(|| {
            black_box(hazards.observe(current.view(), [962, 580], 1.0, 1.0, Some([1446, 211]))?);
            Ok(())
        })?,
    );
    stages.insert(
        "terminal".into(),
        measure(|| {
            black_box(terminal.observe(current.view())?);
            Ok(())
        })?,
    );
    stages.insert(
        "objective".into(),
        measure(|| {
            black_box(objective.observe(current.view())?);
            Ok(())
        })?,
    );
    stages.insert(
        "hub".into(),
        measure(|| {
            black_box(hub.observe(&current_owned)?);
            Ok(())
        })?,
    );
    stages.insert(
        "legacy_hub".into(),
        measure(|| {
            use d2bot::actions::town_loop::TownSight;
            town.set_frame(current_owned.clone());
            town.normalize()?;
            black_box(town.find("world_arcane_waypoint")?);
            town.clear_frame();
            Ok(())
        })?,
    );
    stages.insert(
        "stored_packet".into(),
        measure(|| {
            black_box(color::gray(
                resize::normalize(&geometry, current)?.as_ref().view(),
            )?);
            black_box(hud.observe(current.view())?);
            black_box(observe_path()?);
            observe_motion()?;
            Ok(())
        })?,
    );
    let control_timings = if control_packet {
        control::measure(&root, current, &hud)?
    } else {
        Value::Null
    };
    let mut concurrent_recording = Value::Null;
    #[cfg(windows)]
    if let Some((session, started)) = &mut competing {
        let was_running = session.is_running();
        let stopping = Instant::now();
        session.request_stop();
        session.finish(std::time::Duration::from_secs(3))?;
        let stop_and_join_ms = stopping.elapsed().as_secs_f64() * 1000.0;
        let events = session.drain_events();
        if !was_running
            || events
                .iter()
                .any(|event| matches!(event.level, d2bot::actions::EventLevel::Error))
        {
            return Err(format!("concurrent recording ended early or failed: {events:?}").into());
        }
        concurrent_recording = json!({"elapsed_ms":started.elapsed().as_secs_f64()*1000.0,"stop_and_join_ms":stop_and_join_ms,"bytes":std::fs::metadata(output.join("concurrent.avi"))?.len(),"running_after_finish":session.is_running()});
    }
    let video = output.join("static.avi");
    let mut writer = Mpeg4Writer::open(&video, size, 4, OutputMode::CreateNew)?;
    stages.insert(
        "encode_write".into(),
        measure(|| {
            writer.write(current)?;
            Ok(())
        })?,
    );
    let finish_started = Instant::now();
    writer.finish()?;
    let finish_ms = finish_started.elapsed().as_secs_f64() * 1000.0;
    drop(writer);
    let mut native = Value::Null;
    let mut live_recording = Value::Null;
    #[cfg(windows)]
    if capture_enabled {
        use d2bot::platform::windows::{Windows, enable_dpi_awareness};
        enable_dpi_awareness()?;
        let settings = CaptureSettings {
            window_title: Some("Diablo II: Resurrected".into()),
            window_title_mode: "exact".into(),
            capture_backend: "screen".into(),
            ..Default::default()
        };
        let mut capture = ScreenCapture::new(Windows::new()?, settings, &root)?;
        native = measure(|| {
            black_box(capture.grab()?);
            Ok(())
        })?;
        let packet = capture.grab()?;
        native["target"] = packet.target.as_value();
        capture.close()?;
        let config = d2bot::common::config::BotConfig::from_value(
            json!({"capture":{"fps":8,"window_title":"Diablo II: Resurrected","window_title_mode":"exact","capture_backend":"screen"}}),
        )?;
        let mut session =
            d2bot::actions::recording::RecordingSession::windows(&config, root.clone())?;
        let path = output.join("live.avi");
        let started = Instant::now();
        session.start_to(path.clone())?;
        std::thread::sleep(std::time::Duration::from_secs(8));
        let stopping = Instant::now();
        session.request_stop();
        let request_ms = stopping.elapsed().as_secs_f64() * 1000.0;
        session.finish(std::time::Duration::from_secs(3))?;
        let events = session.drain_events();
        if events
            .iter()
            .any(|event| matches!(event.level, d2bot::actions::EventLevel::Error))
        {
            return Err(format!("recording events: {events:?}").into());
        }
        live_recording = json!({"elapsed_ms":started.elapsed().as_secs_f64()*1000.0,"request_ms":request_ms,"stop_and_join_ms":stopping.elapsed().as_secs_f64()*1000.0,"bytes":std::fs::metadata(path)?.len(),"running_after_finish":session.is_running()});
    }
    let view = observe_path()?;
    let result = json!({"implementation":"Rust release static CRT","inputs":paths.iter().zip(&frames).map(|(path,frame)|json!({"path":path,"bgr_sha256":hash(frame.bytes())})).collect::<Vec<_>>(),"width":1922,"height":1140,"samples":SAMPLES,"warmups":WARMUPS,"percentile":"nearest-rank","stages":stages,"control_packet":control_timings,"capture_screen":native,"live_recording":live_recording,"concurrent_recording":concurrent_recording,"gray_sha256":hash(gray.bytes()),"hud":hud.observe(current.view())?,"path_floor_sha256":hash(view.floor().to_owned()?.bytes()),"motion":motion_result,"recording":{"frames":SAMPLES+WARMUPS,"fps":8,"bytes":std::fs::metadata(video)?.len(),"finish_ms":finish_ms},"inputs_sent":0});
    std::fs::write(
        output.join("result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}

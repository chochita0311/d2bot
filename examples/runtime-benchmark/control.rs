//! 같은 불변 source의 native 준비/판독만 비교한다. 화면 읽기·입력 시간은 포함하지 않는다.
use d2bot::{
    common::{
        frame::BgrFrame,
        survival_vision::SurvivalHudObserver,
        time::{MonotonicClock, SystemClock},
        workers::CooperativeStop,
    },
    runs::summoner::{
        camera_ground::{
            CalibratedCameraGroundObserver, CameraGroundHandle, CameraLimits, ReviewedCameraLayout,
        },
        continuous_navigation::{
            ContinuousObservers, ContinuousVision, NativeContinuousVision, PlanningRequest,
        },
        main_run::native::PanelObservers,
        navigation::Wing,
        navigation_execution::load_arcane_step_planner,
        objective_vision::ArcaneObjectiveObserver,
        scene_vision::{
            ArcaneHazardHandle, ArcaneHazardObserver, ArcaneHoverObserver, ArcaneTargetObserver,
        },
        terminal_vision::ArcaneTerminalObserver,
    },
};
use serde_json::{Value, json};
use std::{
    error::Error,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
fn stats(mut values: Vec<f64>) -> Value {
    values.sort_by(f64::total_cmp);
    json!({"p50_ms":values[14],"p95_ms":values[28],"p99_ms":values[29],"max_ms":values[29],"samples":30,"warmups":3})
}
pub fn measure(
    root: &Path,
    frame: &BgrFrame,
    hud: &SurvivalHudObserver,
) -> Result<Value, Box<dyn Error>> {
    let mut cases = serde_json::Map::new();
    for (name, parallel, broad) in [
        ("serial", false, false),
        ("parallel", true, false),
        ("serial_with_broad", false, true),
        ("parallel_with_broad", true, true),
    ] {
        cases.insert(name.into(), run(root, frame, hud, parallel, broad)?);
    }
    Ok(
        json!({"boundary":"after immutable frame allocation to current near/hover reading; excludes capture/input", "inputs_sent":0,"cases":cases}),
    )
}
fn run(
    root: &Path,
    frame: &BgrFrame,
    hud: &SurvivalHudObserver,
    parallel: bool,
    broad: bool,
) -> Result<Value, Box<dyn Error>> {
    let clock = Arc::new(SystemClock::default());
    let panels = PanelObservers::load(root)?;
    let camera_panels = panels.clone();
    let (ground, path) = load_arcane_step_planner(root, true, "sorceress")?;
    let mut camera = CalibratedCameraGroundObserver::new(
        ground,
        move |image| camera_panels.check(image),
        CameraLimits {
            persist_reviewed_layout: true,
            ..Default::default()
        },
    )?;
    let image = Arc::new(frame.clone());
    let acquired = clock.now();
    let reading = hud.observe(image.view())?;
    camera.confirm_runtime_viewport(
        &image,
        &reading,
        true,
        acquired,
        clock.now(),
        &ReviewedCameraLayout::load(
            root.join("assets/ui/layouts/remastered-1922x1140/camera-ground.json"),
        )?,
    )?;
    drop(image);
    let region = root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
    let mut vision = NativeContinuousVision::new(
        ContinuousObservers {
            panels: Some(panels),
            camera: CameraGroundHandle::new(camera),
            path,
            hazards: ArcaneHazardHandle::new(ArcaneHazardObserver::load(
                region.join("manifest.json"),
            )?),
            hover: Arc::new(ArcaneHoverObserver::load(root.join("assets"))?),
            target: Some(Arc::new(ArcaneTargetObserver::load(&region)?)),
            hub: None,
            graph: None,
            terrain: None,
            terminal: if broad {
                Some(ArcaneTerminalObserver::load(
                    region.join("terminal-patterns.json"),
                )?)
            } else {
                None
            },
            objective: if broad {
                Some(Arc::new(ArcaneObjectiveObserver::load(&region)?))
            } else {
                None
            },
        },
        clock.clone(),
        CooperativeStop::default(),
    )?;
    vision.initialize(0.32, None, None)?;
    let mut weak = Vec::new();
    let result = (|| -> Result<Value, Box<dyn Error>> {
        let mut near = vec![];
        let mut hover = vec![];
        for index in 0..33 {
            let image = Arc::new(frame.clone());
            weak.push(Arc::downgrade(&image));
            let acquired = clock.now();
            let started = Instant::now();
            if parallel {
                vision.begin_frame(&image)?;
            }
            let reading = hud.observe(image.view())?;
            vision.prepare(&image, acquired, &reading, true)?;
            let ground = vision.ground(&image, acquired)?;
            let player = ground.point.ok_or("benchmark current ground unavailable")?;
            let mut threats =
                vision.near_threats(&image, player, acquired, Some([1446, 211]), true)?;
            let near_ms = started.elapsed().as_secs_f64() * 1000.0;
            vision.finish_threats(&mut threats)?;
            if index >= 3 {
                near.push(near_ms);
                hover.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            if broad {
                vision.local_field_phase(true);
                let mut jobs = vision
                    .start_plan(
                        &image,
                        PlanningRequest {
                            wing: Wing::North,
                            acquired,
                            position: [0.0; 2],
                            excluded: &[],
                            ground: &ground,
                            outbound: true,
                        },
                    )?
                    .jobs;
                let _ = vision.objective(&mut jobs)?;
                let _ = vision.plan(&mut jobs)?;
                let _ = vision.terminal(&mut jobs)?;
            }
        }
        Ok(
            json!({"near":stats(near),"hover":stats(hover),"parallel_panels":parallel,"broad_scans":broad}),
        )
    })();
    let cleanup = vision.finish(Duration::from_secs(3));
    drop(vision);
    if weak.iter().any(|image| image.upgrade().is_some()) {
        return Err("benchmark retained a consumed frame".into());
    }
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error.into()),
        (Err(error), Err(cleanup)) => Err(format!("{error}; cleanup: {cleanup}").into()),
    }
}

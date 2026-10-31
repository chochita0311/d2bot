//! 저장 화면의 목표·외형·접근 바닥·종점 재확인을 실제 Rust 로더로 비교한다. 캡처/입력은 없다.
use d2bot::{
    common::{frame::PixelRect, vision_ops::codec},
    runs::summoner::{
        appearance_vision::SummonerAppearanceObserver,
        navigation::{ChooseOptions, PathContext, Wing},
        navigation_execution::load_arcane_step_planner,
        objective_vision::ArcaneObjectiveObserver,
        terminal_vision::ArcaneTerminalObserver,
    },
};
use serde_json::json;
use std::{error::Error, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let root =
        PathBuf::from(args.next().ok_or("project or package root required")?).canonicalize()?;
    let path = PathBuf::from(args.next().ok_or("native frame required")?);
    let mut coordinate = || -> Result<f64, Box<dyn Error>> {
        let value = args.next().ok_or("current player x and y required")?;
        let value: f64 = value.to_str().ok_or("coordinate must be UTF-8")?.parse()?;
        if !value.is_finite() {
            return Err("finite player coordinate required".into());
        }
        Ok(value)
    };
    let player = [coordinate()?, coordinate()?];
    let reference = args.next().map(PathBuf::from);
    if args.next().is_some() {
        return Err("usage: summoner-frame-diagnostics ROOT FRAME X Y [TERMINAL_REFERENCE]".into());
    }
    let owner = root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
    let frame = codec::load_color(&path)?;
    let mut terminal = ArcaneTerminalObserver::load(owner.join("terminal-patterns.json"))?;
    let previous = reference
        .map(|p| -> Result<_, Box<dyn Error>> {
            Ok(terminal.observe(codec::load_color(p)?.view())?)
        })
        .transpose()?
        .flatten();
    let terminal_full = terminal.observe(frame.view())?;
    let terminal_static = previous
        .as_ref()
        .map(|r| terminal.confirm_static_parts(frame.view(), r))
        .transpose()?
        .flatten();
    let terminal_relocalized = previous
        .as_ref()
        .map(|r| terminal.confirm_relocalized_parts(frame.view(), r))
        .transpose()?
        .flatten();
    let objective = ArcaneObjectiveObserver::load(&owner)?;
    let platform = objective.observe(frame.view())?;
    let appearance = SummonerAppearanceObserver::load(owner.join("summoner-encounter.json"))?;
    let alive = appearance.observe(frame.view(), platform.as_ref())?;
    let corpse = appearance.observe_corpse(frame.view(), platform.as_ref())?;
    let goal = alive
        .as_ref()
        .map(|r| r.ground)
        .or_else(|| platform.as_ref().and_then(|p| p.ground_point));
    let (_, mut floor) = load_arcane_step_planner(&root, true, "sorceress")?;
    if let Some(mut geometry) = floor.teleport() {
        geometry.preferred_distance = Some(geometry.maximum_distance);
        floor.set_teleport(Some(geometry))?;
    }
    let approach = goal
        .map(|goal| -> Result<_, Box<dyn Error>> {
            let view = floor.observe(
                frame.view(),
                PathContext {
                    sequence: 0,
                    generation: 0,
                    captured_at: 0.0,
                    frame_size: frame.size(),
                    viewport: PixelRect::new(120, 180, 1720, 890)?,
                    anchor: player,
                },
            )?;
            let choice = floor.choose(
                &view,
                Wing::North,
                ChooseOptions {
                    destination: Some([goal[0] - player[0], goal[1] - player[1]]),
                    ..Default::default()
                },
            )?;
            let distance = choice
                .aim
                .map(|p| (f64::from(p[0]) - goal[0]).hypot(f64::from(p[1]) - goal[1]));
            Ok(json!({"goal":goal,"choice":choice,"goal_distance":distance,"within_100px":distance.is_some_and(|d|d<=100.0)}))
        })
        .transpose()?;
    println!(
        "{}",
        json!({"inputs_sent":0,"captures":0,"platform":platform,"alive":alive,"corpse":corpse,"approach":approach,"terminal_reference":previous,"terminal_full":terminal_full,"terminal_static":terminal_static,"terminal_relocalized":terminal_relocalized})
    );
    Ok(())
}

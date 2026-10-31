//! 패키지의 실제 runtime 로더를 입력/캡처 없이 초기화해 누락·지문·형식을 확인한다.
use d2bot::{
    actions::{
        gem_summing::GemVision, loot_pickup::LootScanner, run_lifecycle::RoomTemplates,
        town_loop::TownVision,
    },
    common::{
        automap_vision::AutomapObserver,
        config::BotConfig,
        frame_geometry::ReferenceLayout,
        survival_vision::{HudObserverOptions, SurvivalHudObserver},
        vision_ops::codec,
    },
    runs::summoner::{
        appearance_vision::SummonerAppearanceObserver,
        camera_ground::ReviewedCameraLayout,
        canyon_finish::{CanyonApproachObserver, RedPortalObserver},
        main_run::native::PanelObservers,
        navigation_execution::load_arcane_step_planner,
        objective_vision::ArcaneObjectiveObserver,
        scene_vision::{
            ArcaneHazardObserver, ArcaneHoverObserver, ArcaneHubObserver, ArcaneTargetObserver,
        },
        terminal_vision::ArcaneTerminalObserver,
        terrain_patterns::ArcaneTerrainPatterns,
    },
};
use std::{
    error::Error,
    hint::black_box,
    path::{Path, PathBuf},
};

fn config_assets(value: &serde_json::Value, root: &Path) -> Result<usize, Box<dyn Error>> {
    let mut count = 0;
    match value {
        serde_json::Value::String(path) if path.replace('\\', "/").starts_with("assets/") => {
            let path = root.join(path);
            if !path.is_file() {
                return Err(format!("Configured asset missing: {}", path.display()).into());
            }
            if path.extension().is_some_and(|s| s != "json") {
                black_box(codec::load_color(path)?);
            }
            count += 1;
        }
        serde_json::Value::Array(items) => {
            for item in items {
                count += config_assets(item, root)?;
            }
        }
        serde_json::Value::Object(items) => {
            for item in items.values() {
                count += config_assets(item, root)?;
            }
        }
        _ => {}
    }
    Ok(count)
}
fn main() -> Result<(), Box<dyn Error>> {
    let root =
        PathBuf::from(std::env::args_os().nth(1).ok_or("package root required")?).canonicalize()?;
    let mut config = BotConfig::load(root.join("config"))?;
    let configured = config_assets(&config.snapshot(), &root)?;
    black_box(ReferenceLayout::load(
        root.join("assets/ui/layouts/remastered-1922x1140/viewport.json"),
    )?);
    black_box(ReviewedCameraLayout::load(root.join(
        "assets/ui/layouts/remastered-1922x1140/camera-ground.json",
    ))?);
    black_box(GemVision::load(&root)?);
    black_box(RoomTemplates::load(&root)?);
    black_box(TownVision::load(&root)?);
    black_box(PanelObservers::load(&root)?);
    black_box(AutomapObserver::load(root.join("assets/ui/automap"))?);
    black_box(SurvivalHudObserver::load(
        root.join("assets/ui/survival"),
        HudObserverOptions {
            allow_high_orb_fallback: true,
            allow_partial_orb_fallback: true,
            observe_mercenary: true,
        },
    )?);
    let characters = config
        .section("characters")
        .and_then(|v| v.as_object())
        .ok_or("characters required")?
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    for key in &characters {
        config.apply_character_selection(Some(key))?;
        let mut scanner = LootScanner::new(&config, &root)?;
        black_box(scanner.candidates()?);
    }
    black_box(load_arcane_step_planner(&root, true, "sorceress")?);
    let region = root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
    black_box(ArcaneHazardObserver::load(region.join("manifest.json"))?);
    black_box(ArcaneHoverObserver::load(root.join("assets"))?);
    black_box(ArcaneTargetObserver::load(&region)?);
    black_box(ArcaneHubObserver::load(region.join("hub-patterns.json"))?);
    black_box(ArcaneTerrainPatterns::load(
        region.join("navigation-patterns.json"),
    )?);
    black_box(ArcaneTerminalObserver::load(
        region.join("terminal-patterns.json"),
    )?);
    black_box(ArcaneObjectiveObserver::load(&region)?);
    black_box(SummonerAppearanceObserver::load(
        region.join("summoner-encounter.json"),
    )?);
    black_box(CanyonApproachObserver::load(&root)?);
    black_box(RedPortalObserver::load(&region)?);
    println!(
        "{}",
        serde_json::json!({"runtime_assets":"initialized","characters":characters.len(),"configured_asset_paths":configured,"inputs_sent":0,"captures":0})
    );
    Ok(())
}

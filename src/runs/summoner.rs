pub mod appearance_vision;
pub mod camera_ground;
pub mod canyon_finish;
pub mod continuation;
pub mod continuous_navigation;
pub mod cycle;
pub mod definition;
pub mod encounter_execution;
pub mod field_defense;
pub mod gui_session;
pub mod journal_vision;
pub mod legacy_segment;
pub mod life_vision;
pub mod main_run;
pub mod navigation;
pub mod navigation_execution;
pub mod objective_vision;
pub mod options;
pub mod room_finish;
pub mod route_graph;
pub mod scene_vision;
pub mod search;
pub mod search_runtime;
pub mod steering;
pub mod supervised_renewal;
pub mod terminal_vision;
pub mod terrain_patterns;
pub use definition::{
    SUMMONER_PAYLOAD_STATES, SUMMONER_PROFILE_ID, SUMMONER_ROUTE_SEGMENTS, SUMMONER_SHARED_PORTS,
    SummonerRunContext, SummonerRunOrchestrator, build_summoner_definition,
    build_summoner_orchestrator, iter_summoner_payload_states, resolve_summoner_run,
    summarize_summoner_payload,
};
pub use gui_session::{SummonerEvent, SummonerGuiSession};

/// 마을 실행에는 현재 native 화면의 완전한 중앙 근거만 연결한다.
pub fn town_vision(
    root: impl AsRef<std::path::Path>,
) -> Result<crate::actions::town_loop::TownVision, crate::actions::ActionError> {
    let root = root.as_ref();
    let mut hub = scene_vision::ArcaneHubObserver::load(
        root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140/hub-patterns.json"),
    )?;
    let mut town = crate::actions::town_loop::TownVision::load(root)?;
    town.set_native_waypoint_observer(move |frame| {
        Ok(hub.observe(frame)?.map(|point| point.map(f64::from)))
    });
    Ok(town)
}

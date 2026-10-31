//! 현재 엔진의 공개 설명. 계획된 경로와 실행 가능한 북쪽 경로를 구분한다.
use crate::{
    common::config::BotConfig,
    runs::{
        RunError,
        base::{RunDefinition, RunPayloadState, RunPort, RunRouteSegment},
    },
};
use serde_json::Value;
pub const SUMMONER_PROFILE_ID: &str = "summoner";
pub type SummonerRunOrchestrator = super::gui_session::SummonerGuiSession;
pub struct SummonerRunContext<'a> {
    pub profile_id: &'static str,
    pub definition: RunDefinition<'a>,
}
impl SummonerRunContext<'_> {
    pub fn profile(&self) -> &Value {
        self.definition.profile
    }
}
pub const SUMMONER_PAYLOAD_STATES: &[RunPayloadState] = &[
    RunPayloadState {
        key: "room_created",
        description: "The room is created with the character already selected by the user.",
        success_signal: "Town is observed and input is available.",
        next_action: "Enter the Arcane Sanctuary through the town waypoint.",
    },
    RunPayloadState {
        key: "payload_started",
        description: "The current main_run engine observes the hub and prepares the configured character.",
        success_signal: "Hub and automap are observed; the user supplies the main-weapon precondition and the balanced buff sequence is dispatched. Buff effects remain not_independently_verified.",
        next_action: "Activate the north route.",
    },
    RunPayloadState {
        key: "route_piece_active",
        description: "Continuous north movement yields to observed combat, hazards, and buff renewal.",
        success_signal: "Observed route progress remains valid.",
        next_action: "Continue until a terminal, encounter, interruption, or navigation limit.",
    },
    RunPayloadState {
        key: "summoner_encountered",
        description: "Observed Summoner-platform evidence enters the encounter controller.",
        success_signal: "The encounter result is recorded separately from route progress.",
        next_action: "Complete the encounter and loot observation or report an incomplete attempt.",
    },
    RunPayloadState {
        key: "payload_complete",
        description: "The north attempt is ready to return through the central waypoint.",
        success_signal: "The room completion observations are satisfied.",
        next_action: "Return to Act 1 and save/exit.",
    },
    RunPayloadState {
        key: "room_exit_complete",
        description: "The character selection screen is observed after exiting through Act 1.",
        success_signal: "The latest runtime reports north_room_closed.",
        next_action: "Start the next requested fresh north attempt, up to five in total.",
    },
];
pub const SUMMONER_ROUTE_SEGMENTS: &[RunRouteSegment] = &[
    RunRouteSegment {
        key: "north_go",
        label: "north way go",
        direction: "go",
        status: "ready",
        notes: &[
            "Implemented by main_run and continuous_navigation; fresh north attempts are bounded from one to five.",
            "Ready describes the executable route; live acceptance remains separate from code availability.",
        ],
    },
    RunRouteSegment {
        key: "north_return",
        label: "north way return",
        direction: "return",
        status: "ready",
        notes: &[
            "The latest main_run returns through the central waypoint to Act 1 and verifies character select.",
        ],
    },
    RunRouteSegment {
        key: "east_go",
        label: "east way go",
        direction: "go",
        status: "planned",
        notes: &[],
    },
    RunRouteSegment {
        key: "east_return",
        label: "east way return",
        direction: "return",
        status: "planned",
        notes: &[],
    },
    RunRouteSegment {
        key: "south_go",
        label: "south way go",
        direction: "go",
        status: "planned",
        notes: &[],
    },
    RunRouteSegment {
        key: "south_return",
        label: "south way return",
        direction: "return",
        status: "planned",
        notes: &[],
    },
    RunRouteSegment {
        key: "west_go",
        label: "west way go",
        direction: "go",
        status: "planned",
        notes: &[],
    },
    RunRouteSegment {
        key: "west_return",
        label: "west way return",
        direction: "return",
        status: "planned",
        notes: &[],
    },
];
pub const SUMMONER_SHARED_PORTS: &[RunPort] = &[
    RunPort {
        key: "make_room",
        kind: "wrapper",
        description: "Create a fresh room for the selected character and difficulty.",
        status: "ready",
        reusable: true,
        notes: &[],
    },
    RunPort {
        key: "arcane_entry",
        kind: "payload",
        description: "Observe town travel and Arcane Sanctuary waypoint arrival.",
        status: "ready",
        reusable: true,
        notes: &[],
    },
    RunPort {
        key: "buff_before_run",
        kind: "payload",
        description: "Dispatch the balanced initial buff sequence and automatically renew it through the current runtime.",
        status: "ready",
        reusable: true,
        notes: &[
            "The main-weapon state is a user precondition; buff effects remain not_independently_verified.",
        ],
    },
    RunPort {
        key: "hunting",
        kind: "payload",
        description: "Handle observed nearby monsters and the Summoner encounter.",
        status: "partial",
        reusable: true,
        notes: &[
            "An encounter controller exists; completed encounter and boss-death acceptance remain unverified.",
        ],
    },
    RunPort {
        key: "looting",
        kind: "payload",
        description: "Record encounter loot observations and outcomes.",
        status: "partial",
        reusable: true,
        notes: &[
            "Loot observation and conditional pickup are implemented; completed pickup acceptance remains unverified.",
        ],
    },
    RunPort {
        key: "escape_plan",
        kind: "payload",
        description: "Yield or leave on observed danger and interruption.",
        status: "ready",
        reusable: true,
        notes: &[],
    },
    RunPort {
        key: "exit_room",
        kind: "wrapper",
        description: "Return through Act 1 and observe character select.",
        status: "ready",
        reusable: true,
        notes: &[],
    },
    RunPort {
        key: "continuous_runtime",
        kind: "runtime",
        description: "Run current main_run navigation in a cancellable GUI worker.",
        status: "ready",
        reusable: true,
        notes: &[],
    },
];
pub fn build_summoner_definition(config: &BotConfig) -> Result<RunDefinition<'_>, RunError> {
    let profile = config
        .section("run_profiles")
        .and_then(|v| v.get(SUMMONER_PROFILE_ID))
        .filter(|v| !v.is_null())
        .ok_or_else(|| {
            RunError::Runtime(
                "Summoner Run requires the 'summoner' profile in run_profiles.".into(),
            )
        })?;
    Ok(RunDefinition {
        profile_id: SUMMONER_PROFILE_ID,
        profile,
        payload_states: SUMMONER_PAYLOAD_STATES,
        route_segments: SUMMONER_ROUTE_SEGMENTS,
        decision_ports: &[],
        shared_ports: SUMMONER_SHARED_PORTS,
        metadata: [
            ("organizer", "src/runs/summoner/main_run.rs"),
            ("gui_adapter", "src/runs/summoner/gui_session.rs"),
            ("route_owner", "src/runs/summoner/continuous_navigation.rs"),
            (
                "current_stage",
                "fresh room -> Arcane hub -> automatic north attempt -> Act 1 -> character select",
            ),
        ]
        .into(),
    })
}
pub fn resolve_summoner_run(config: &BotConfig) -> Result<SummonerRunContext<'_>, RunError> {
    Ok(SummonerRunContext {
        profile_id: SUMMONER_PROFILE_ID,
        definition: build_summoner_definition(config)?,
    })
}
pub fn build_summoner_orchestrator(config: BotConfig) -> SummonerRunOrchestrator {
    SummonerRunOrchestrator::new(config)
}
pub fn iter_summoner_payload_states() -> &'static [RunPayloadState] {
    SUMMONER_PAYLOAD_STATES
}
pub fn summarize_summoner_payload(
    context: &SummonerRunContext<'_>,
) -> Result<Vec<String>, RunError> {
    Ok(context.definition.summarize()?)
}

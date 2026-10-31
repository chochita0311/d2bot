//! 방금 관찰한 같은 경로의 인계 조건. 과거 보고서만으로 이동·도착을 승인하지 않는다.
mod saved_scene;
use super::{navigation::Wing, scene_vision::HubEvidence};
use crate::{
    common::{config::ConfigError, time},
    runs::RunError,
};
pub use saved_scene::{SavedRouteFlow, restore_route_flow};
use serde::Serialize;
use serde_json::Value;

fn finite(value: &Value) -> Option<f64> {
    value.as_f64().filter(|n| n.is_finite())
}
fn last_row(progress: &Value) -> &Value {
    progress["continuous_frames"]
        .as_array()
        .and_then(|rows| rows.last())
        .unwrap_or(&Value::Null)
}
fn route_matches(progress: &Value, wing: Wing, return_from: Option<Wing>) -> bool {
    progress["wing"].as_str() == Some(wing.name())
        && match return_from {
            Some(from) => progress["return_from"].as_str() == Some(from.name()),
            None => progress["return_from"].is_null(),
        }
}
pub fn validate_live_continuation(
    progress: &Value,
    wing: Wing,
    return_from: Option<Wing>,
    now: f64,
) -> Result<(), RunError> {
    let previous = finite(&last_row(progress)["processed_at"]);
    let finished = finite(
        progress
            .get("interval_finished_at")
            .unwrap_or(&last_row(progress)["processed_at"]),
    );
    if progress["continuous_status"] == "interval_complete"
        && route_matches(progress, wing, return_from)
        && previous.zip(finished).is_some_and(|(previous, finished)| {
            finished >= previous && (0.0..0.5).contains(&(now - finished))
        })
    {
        Ok(())
    } else {
        Err(RunError::Runtime(
            "immediate same-process field continuation required".into(),
        ))
    }
}
pub fn validate_terminal_return(
    previous: &Value,
    wing: Wing,
    return_from: Option<Wing>,
    now: f64,
) -> Result<(), RunError> {
    let last = last_row(previous);
    let terminal = &previous["terminal"];
    let kind = terminal["kind"].as_str();
    let status = previous["continuous_status"].as_str();
    let tracked = status == Some("recent_terminal_layout_and_player_settled")
        && matches!(kind, Some("nonboss_chest_trio" | "nonboss_chest_platform"))
        && matches!(
            terminal["provenance"].as_str(),
            Some(
                "current_static_two_parts"
                    | "current_relocalized_two_parts"
                    | "current_motion_settled_two_parts"
            )
        )
        && terminal["current_parts"].as_array().is_some_and(|parts| {
            parts.len() == 3
                && parts.iter().all(Value::is_boolean)
                && parts.iter().filter(|p| **p == true).count() >= 2
        })
        && finite(&terminal["reference_observed_at"])
            .zip(finite(&last["acquired_at"]))
            .is_some_and(|(reference, observed)| (0.0..=2.0).contains(&(observed - reference)));
    let direct = matches!(
        (kind, status),
        (
            Some("nonboss_chest_trio"),
            Some("current_chest_trio_and_player_settled")
        ) | (
            Some("nonboss_chest_platform"),
            Some("current_terminal_landmarks_and_player_settled")
        )
    );
    if previous["status"] == "wing_end_observed"
        && (tracked || direct)
        && route_matches(previous, wing, None)
        && return_from == Some(wing)
        && last["action"] == "terminal_arrival_hold"
        && finite(&last["processed_at"]).is_some_and(|at| (0.0..0.5).contains(&(now - at)))
    {
        Ok(())
    } else {
        Err(RunError::Runtime(
            "immediate observed terminal-to-hub transition required".into(),
        ))
    }
}
pub fn validate_hub_departure(
    previous: &Value,
    wing: Wing,
    return_from: Option<Wing>,
    now: f64,
) -> Result<(), RunError> {
    let next = match previous["wing"].as_str() {
        Some("north") => Some((Wing::North, Wing::East)),
        Some("east") => Some((Wing::East, Wing::South)),
        Some("south") => Some((Wing::South, Wing::West)),
        _ => None,
    };
    let last = last_row(previous);
    if previous["status"] == "hub_arrived"
        && previous["continuous_status"] == "central_waypoint_and_player_settled"
        && next.is_some_and(|(from, to)| route_matches(previous, from, Some(from)) && wing == to)
        && return_from.is_none()
        && last["action"] == "central_arrival_hold"
        && finite(&last["processed_at"]).is_some_and(|at| (0.0..0.5).contains(&(now - at)))
    {
        Ok(())
    } else {
        Err(RunError::Runtime(
            "immediate observed hub-to-next-wing transition required".into(),
        ))
    }
}
#[derive(Debug, Serialize)]
pub struct ArrivalDwell {
    minimum_seconds: f64,
    since: Option<f64>,
    stable: usize,
}
impl ArrivalDwell {
    pub fn new(reference_seconds: f64) -> Result<Self, ConfigError> {
        time::positive(reference_seconds, "arrival reference")?;
        if reference_seconds >= 1.5 {
            return Err(ConfigError::value(
                "arrival reference must fit the observation window",
            ));
        }
        Ok(Self {
            minimum_seconds: reference_seconds,
            since: None,
            stable: 0,
        })
    }
    pub fn reset(&mut self) {
        self.since = None;
        self.stable = 0;
    }
    pub fn since(&self) -> Option<f64> {
        self.since
    }
    pub fn update(
        &mut self,
        near: bool,
        displacement: Option<[f64; 2]>,
        now: f64,
    ) -> Result<bool, ConfigError> {
        time::nonnegative(now, "arrival time")?;
        if !near {
            self.reset();
            return Ok(false);
        }
        let quiet = displacement.is_some_and(|[x, y]| x.hypot(y) < 2.0);
        if self.since.is_none()
            || displacement.is_none()
            || displacement.is_some_and(|[x, y]| x.hypot(y) >= 2.0)
        {
            self.since = Some(now);
            self.stable = 0;
        }
        if quiet {
            self.stable += 1;
        }
        Ok(self.stable >= 2
            && self
                .since
                .is_some_and(|since| now - since >= self.minimum_seconds))
    }
}
#[derive(Debug, Clone, Copy, Serialize, serde::Deserialize)]
struct HubOrigin {
    relative: [f64; 2],
    at: f64,
}
#[derive(Debug, Clone, Copy, Serialize, serde::Deserialize)]
struct HubLanding {
    relative: [f64; 2],
    at: f64,
    sequence: u64,
}
#[derive(Default, Serialize, serde::Deserialize)]
pub struct HubLandingTracker {
    #[serde(
        rename = "hub_approach_origin",
        skip_serializing_if = "Option::is_none"
    )]
    origin: Option<HubOrigin>,
    #[serde(
        rename = "hub_approach_landing",
        skip_serializing_if = "Option::is_none"
    )]
    landing: Option<HubLanding>,
}
impl HubLandingTracker {
    pub fn begin(&mut self, hub: [i32; 2], player: [i32; 2], at: f64) -> Result<(), ConfigError> {
        time::nonnegative(at, "hub origin time")?;
        self.origin = Some(HubOrigin {
            relative: relative(hub, player),
            at,
        });
        Ok(())
    }
    pub fn update(
        &mut self,
        hub: [i32; 2],
        player: [i32; 2],
        evidence: Option<&HubEvidence>,
        displacement: Option<[f64; 2]>,
        acquired: f64,
        sequence: u64,
    ) -> bool {
        let relative = relative(hub, player);
        let valid = self.origin.is_some_and(|origin| {
            acquired - origin.at > 0.0
                && acquired - origin.at < 1.5
                && matches!(evidence, Some(HubEvidence::Current { .. }))
                && displacement.is_some()
                && !displacement.is_some_and(|[x, y]| x.hypot(y) >= 2.0)
                && (relative[0] - origin.relative[0]).hypot(relative[1] - origin.relative[1])
                    >= 64.0
        });
        if !valid {
            self.landing = None;
            return false;
        }
        let previous = self.landing;
        self.landing = Some(HubLanding {
            relative,
            at: acquired,
            sequence,
        });
        previous.is_some_and(|before| {
            before.sequence.checked_add(1) == Some(sequence)
                && acquired - before.at > 0.0
                && acquired - before.at < 0.25
                && (relative[0] - before.relative[0]).hypot(relative[1] - before.relative[1]) < 2.0
        })
    }
}
fn relative(hub: [i32; 2], player: [i32; 2]) -> [f64; 2] {
    std::array::from_fn(|i| f64::from(hub[i]) - f64::from(player[i]))
}
pub const NEAR_GROUND_RADIUS: [f64; 2] = [300.0, 180.0];
pub fn near_ground(
    point: Option<[f64; 2]>,
    player: [f64; 2],
    radius: [f64; 2],
) -> Result<bool, RunError> {
    let Some(point) = point else {
        return Ok(false);
    };
    if radius.contains(&0.0) {
        return Err(RunError::Runtime("division by zero".into()));
    }
    Ok(((point[0] - player[0]) / radius[0]).hypot((point[1] - player[1]) / radius[1]) <= 1.0)
}

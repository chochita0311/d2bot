pub mod base;
pub mod summoner;
pub use base::{RunDefinition, RunPayloadState, RunPort, RunRouteSegment};
pub use summoner::{
    SUMMONER_PAYLOAD_STATES, SUMMONER_PROFILE_ID, SUMMONER_ROUTE_SEGMENTS, SUMMONER_SHARED_PORTS,
    SummonerRunContext, SummonerRunOrchestrator, build_summoner_definition,
    build_summoner_orchestrator, iter_summoner_payload_states, resolve_summoner_run,
    summarize_summoner_payload,
};

use crate::common::error::{ControlError, ObservationError};
use std::{error::Error, fmt};
#[derive(Debug)]
pub enum RunError {
    Action(crate::actions::ActionError),
    Control(ControlError),
    Observation(ObservationError),
    Runtime(String),
    Context {
        message: &'static str,
        source: Box<RunError>,
    },
    Cleanup {
        primary: Box<RunError>,
        cleanup: Box<RunError>,
    },
}
impl RunError {
    pub fn with_cleanup(self, cleanup: RunError) -> Self {
        Self::Cleanup {
            primary: Box::new(self),
            cleanup: Box::new(cleanup),
        }
    }
    pub fn context(self, message: &'static str) -> Self {
        Self::Context {
            message,
            source: Box::new(self),
        }
    }
}
impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Action(error) => error.fmt(f),
            Self::Control(error) => error.fmt(f),
            Self::Observation(error) => error.fmt(f),
            Self::Runtime(message) => f.write_str(message),
            Self::Context { message, .. } => f.write_str(message),
            Self::Cleanup { primary, cleanup } => {
                write!(f, "{primary}; cleanup also failed: {cleanup}")
            }
        }
    }
}
impl Error for RunError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Action(error) => Some(error),
            Self::Control(error) => Some(error),
            Self::Observation(error) => Some(error),
            Self::Runtime(_) => None,
            Self::Context { source, .. } => Some(source.as_ref()),
            Self::Cleanup { primary, .. } => Some(primary.as_ref()),
        }
    }
}
impl From<crate::actions::ActionError> for RunError {
    fn from(error: crate::actions::ActionError) -> Self {
        Self::Action(error)
    }
}
impl From<std::io::Error> for RunError {
    fn from(error: std::io::Error) -> Self {
        Self::Observation(ObservationError::Io(error))
    }
}
impl From<serde_json::Error> for RunError {
    fn from(error: serde_json::Error) -> Self {
        Self::Observation(ObservationError::Json(error))
    }
}
impl From<ControlError> for RunError {
    fn from(error: ControlError) -> Self {
        Self::Control(error)
    }
}
impl From<ObservationError> for RunError {
    fn from(error: ObservationError) -> Self {
        Self::Observation(error)
    }
}
impl From<crate::common::config::ConfigError> for RunError {
    fn from(error: crate::common::config::ConfigError) -> Self {
        Self::Control(error.into())
    }
}

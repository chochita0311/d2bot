//! Independent, screen-observed actions. Recognition is separate from dispatch.
#[cfg(windows)]
pub mod desktop;
pub mod gem_summing;
pub mod loot_pickup;
pub mod recording;
pub mod run_lifecycle;
pub mod runtime;
pub mod session;
pub mod town_loop;

use crate::common::{
    config::ConfigError,
    error::{ControlError, ObservationError},
    frame::{FrameError, ImageView},
    vision_ops::{
        VisionError,
        template::{self, MatchMethod},
    },
};
use serde::Serialize;
use std::{error::Error, fmt};

#[derive(Debug)]
pub enum ActionError {
    Configuration(ConfigError),
    Control(ControlError),
    Observation(ObservationError),
    Vision(VisionError),
    Io(std::io::Error),
    Json(serde_json::Error),
    Runtime(String),
}
impl ActionError {
    pub fn runtime(message: impl Into<String>) -> Self {
        Self::Runtime(message.into())
    }
}
impl fmt::Display for ActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(e) => e.fmt(f),
            Self::Control(e) => e.fmt(f),
            Self::Observation(e) => e.fmt(f),
            Self::Vision(e) => e.fmt(f),
            Self::Io(e) => e.fmt(f),
            Self::Json(e) => e.fmt(f),
            Self::Runtime(message) => f.write_str(message),
        }
    }
}
impl Error for ActionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Configuration(e) => Some(e),
            Self::Control(e) => Some(e),
            Self::Observation(e) => Some(e),
            Self::Vision(e) => Some(e),
            Self::Io(e) => Some(e),
            Self::Json(e) => Some(e),
            Self::Runtime(_) => None,
        }
    }
}
impl From<ConfigError> for ActionError {
    fn from(e: ConfigError) -> Self {
        Self::Configuration(e)
    }
}
impl From<ControlError> for ActionError {
    fn from(e: ControlError) -> Self {
        Self::Control(e)
    }
}
impl From<ObservationError> for ActionError {
    fn from(e: ObservationError) -> Self {
        Self::Observation(e)
    }
}
impl From<VisionError> for ActionError {
    fn from(e: VisionError) -> Self {
        Self::Vision(e)
    }
}
impl From<FrameError> for ActionError {
    fn from(e: FrameError) -> Self {
        Self::Vision(e.into())
    }
}
impl From<std::io::Error> for ActionError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<serde_json::Error> for ActionError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EventLevel {
    Info,
    Warning,
    Error,
}
impl EventLevel {
    pub fn name(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActionEvent {
    pub level: EventLevel,
    pub message: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MatchResult {
    pub top_left: [usize; 2],
    pub width: usize,
    pub height: usize,
    pub score: f64,
}

pub fn locate<const C: usize>(
    frame: ImageView<'_, u8, C>,
    image: ImageView<'_, u8, C>,
    threshold: f64,
) -> Result<Option<MatchResult>, ActionError> {
    if !threshold.is_finite() {
        return Err(ActionError::runtime("finite template threshold required"));
    }
    let scores = template::match_template(frame, image, MatchMethod::NormalizedCoefficient)?;
    let Some(peak) = template::maximum(&scores) else {
        return Ok(None);
    };
    if f64::from(peak.score) < threshold {
        return Ok(None);
    }
    Ok(Some(MatchResult {
        top_left: peak.location,
        width: image.size().width(),
        height: image.size().height(),
        score: f64::from(peak.score),
    }))
}

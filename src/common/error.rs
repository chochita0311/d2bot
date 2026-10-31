use super::config::ConfigError;
use std::{error::Error, fmt};

/// 자산·설정 오류를 정상적인 미검출과 구분한다.
#[derive(Debug)]
pub enum ObservationError {
    Configuration(ConfigError),
    Vision(super::vision_ops::VisionError),
    Io(std::io::Error),
    Json(serde_json::Error),
    Invalid(String),
}
impl ObservationError {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
}
impl fmt::Display for ObservationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(e) => e.fmt(f),
            Self::Vision(e) => e.fmt(f),
            Self::Io(e) => e.fmt(f),
            Self::Json(e) => e.fmt(f),
            Self::Invalid(message) => f.write_str(message),
        }
    }
}
impl Error for ObservationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Configuration(e) => Some(e),
            Self::Vision(e) => Some(e),
            Self::Io(e) => Some(e),
            Self::Json(e) => Some(e),
            Self::Invalid(_) => None,
        }
    }
}
impl From<ConfigError> for ObservationError {
    fn from(e: ConfigError) -> Self {
        Self::Configuration(e)
    }
}
impl From<super::vision_ops::VisionError> for ObservationError {
    fn from(e: super::vision_ops::VisionError) -> Self {
        Self::Vision(e)
    }
}
impl From<super::frame::FrameError> for ObservationError {
    fn from(e: super::frame::FrameError) -> Self {
        Self::Vision(e.into())
    }
}
impl From<std::io::Error> for ObservationError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<serde_json::Error> for ObservationError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

/// Drop cannot report failures; an unexpected backend panic must not cause a
/// second unwind to abort the process while another worker error is unwinding.
pub(crate) fn best_effort_cleanup(call: impl FnOnce() -> Result<(), ControlError>) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(call));
}

#[derive(Debug)]
pub enum ControlError {
    Configuration(ConfigError),
    Backend(String),
    Runtime(String),
}

impl fmt::Display for ControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(error) => error.fmt(f),
            Self::Backend(message) | Self::Runtime(message) => f.write_str(message),
        }
    }
}
impl Error for ControlError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Configuration(error) => Some(error),
            _ => None,
        }
    }
}
impl From<ConfigError> for ControlError {
    fn from(error: ConfigError) -> Self {
        Self::Configuration(error)
    }
}

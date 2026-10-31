//! 기존 개발 도구의 명령/파일 경계. 실제 정책과 판독은 공통 도메인에 위임한다.
pub mod arcane_navigation_replay;
pub mod field_input_probe;
pub mod field_replay;
pub mod survival_observe;
pub mod town_loop_vision;
use crate::{common::cli::UsageError, runs::RunError};
use std::{
    fmt,
    io::{Read, Write},
    path::Path,
};
fn load_config(path: impl AsRef<Path>) -> Result<crate::common::config::BotConfig, ToolError> {
    let config = crate::common::config::BotConfig::load(path)?;
    if !config.warnings().is_empty() {
        let mut output = std::io::stderr().lock();
        for warning in config.warnings() {
            writeln!(output, "warning: {warning}")?;
        }
        output.flush()?;
    }
    Ok(config)
}

#[derive(Debug)]
pub enum ToolError {
    Usage(UsageError),
    Runtime(RunError),
}
impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(e) => e.fmt(f),
            Self::Runtime(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for ToolError {}
impl From<UsageError> for ToolError {
    fn from(e: UsageError) -> Self {
        Self::Usage(e)
    }
}
impl From<RunError> for ToolError {
    fn from(e: RunError) -> Self {
        Self::Runtime(e)
    }
}
macro_rules! runtime_error {
    ($($kind:ty),* $(,)?)=>{$(impl From<$kind> for ToolError {fn from(e:$kind)->Self {Self::Runtime(e.into())}})*};
}
runtime_error!(
    std::io::Error,
    serde_json::Error,
    crate::common::config::ConfigError,
    crate::common::error::ControlError,
    crate::common::error::ObservationError,
    crate::actions::ActionError
);
impl From<crate::common::frame::FrameError> for ToolError {
    fn from(e: crate::common::frame::FrameError) -> Self {
        crate::common::error::ObservationError::from(e).into()
    }
}
impl From<crate::common::vision_ops::VisionError> for ToolError {
    fn from(e: crate::common::vision_ops::VisionError) -> Self {
        crate::common::error::ObservationError::from(e).into()
    }
}
pub(crate) fn invalid(message: impl Into<String>) -> ToolError {
    ToolError::Runtime(RunError::Runtime(message.into()))
}
pub(crate) fn json_file(path: &Path, limit: u64) -> Result<serde_json::Value, ToolError> {
    let file = std::fs::File::open(path)?;
    if file.metadata()?.len() > limit {
        return Err(UsageError("JSON input must be at most 1 MiB".into()).into());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(UsageError("JSON input must be at most 1 MiB".into()).into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}

//! Validation at monotonic-time boundaries; wall-clock timestamps stay separate.

use super::config::ConfigError;
use std::time::Instant;

pub trait MonotonicClock: Send + Sync {
    fn now(&self) -> f64;
}

#[derive(Debug)]
pub struct SystemClock {
    origin: Instant,
}

impl Default for SystemClock {
    fn default() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl MonotonicClock for SystemClock {
    fn now(&self) -> f64 {
        self.origin.elapsed().as_secs_f64()
    }
}

pub(crate) fn nonnegative(value: f64, name: &str) -> Result<f64, ConfigError> {
    if !value.is_finite() {
        Err(ConfigError::value(format!(
            "{name} must be a finite number"
        )))
    } else if value < 0.0 {
        Err(ConfigError::value(format!("{name} is out of range")))
    } else {
        Ok(value)
    }
}

pub(crate) fn positive(value: f64, name: &str) -> Result<f64, ConfigError> {
    nonnegative(value, name)?;
    if value == 0.0 {
        Err(ConfigError::value(format!("{name} must be positive")))
    } else {
        Ok(value)
    }
}

pub(crate) fn text(value: &str, name: &str) -> Result<(), ConfigError> {
    if value.trim().is_empty() {
        Err(ConfigError::value(format!(
            "{name} must be a non-empty string"
        )))
    } else {
        Ok(())
    }
}

//! Actor-owned movement state; explicit cleanup retains failed releases for retry.
use super::{error::ControlError, field_control::FieldInputBackend};
use std::sync::Arc;

#[derive(Debug, Clone, Copy)]
pub enum MovementIntent {
    Travel,
    Reposition,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovementMode {
    Hold,
    Press,
}
impl MovementMode {
    pub fn resolve(raw: &str, intent: MovementIntent) -> Self {
        match raw.trim().to_lowercase().as_str() {
            "hold" => Self::Hold,
            "press" => Self::Press,
            _ => match intent {
                MovementIntent::Travel => Self::Hold,
                MovementIntent::Reposition => Self::Press,
            },
        }
    }
}
pub struct MovementExecutionState<B: FieldInputBackend> {
    backend: Arc<B>,
    key: String,
    held: bool,
    stopped: bool,
}
impl<B: FieldInputBackend> MovementExecutionState<B> {
    pub fn new(backend: Arc<B>, key: String) -> Self {
        Self {
            backend,
            key,
            held: false,
            stopped: false,
        }
    }
    pub fn held(&self) -> bool {
        self.held
    }
    pub fn stopped(&self) -> bool {
        self.stopped
    }
    pub fn apply(&mut self, mode: MovementMode) -> Result<String, ControlError> {
        if self.stopped {
            return Ok("with movement stopped.".into());
        }
        if self.key.is_empty() {
            return Err(ControlError::Runtime(
                "Movement intent requires movement_skill_key.".into(),
            ));
        }
        if mode == MovementMode::Hold {
            if !self.held {
                self.held = true;
                if let Err(error) = self.backend.key_down(&self.key) {
                    return Err(match self.release(true) {
                        Ok(()) => error,
                        Err(cleanup) => ControlError::Runtime(format!(
                            "{error}; movement release failed: {cleanup}"
                        )),
                    });
                }
            }
            Ok(format!(
                "with movement_skill_key '{}' still held.",
                self.key
            ))
        } else {
            self.release(false)?;
            self.backend.press(&self.key)?;
            Ok(format!("and cast movement_skill_key '{}'.", self.key))
        }
    }
    pub fn release(&mut self, stop: bool) -> Result<(), ControlError> {
        self.stopped |= stop;
        if self.held && !self.key.is_empty() {
            self.backend.key_up(&self.key)?;
            self.held = false;
        }
        Ok(())
    }
}
impl<B: FieldInputBackend> Drop for MovementExecutionState<B> {
    fn drop(&mut self) {
        super::error::best_effort_cleanup(|| self.release(true));
    }
}

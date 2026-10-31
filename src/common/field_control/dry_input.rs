use super::{DesktopPoint, FieldInputBackend};
use crate::common::error::ControlError;
use std::collections::VecDeque;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DryInputEvent {
    Down(String),
    Up(String),
    Press(String),
    Move(DesktopPoint),
}
#[derive(Default)]
struct State {
    events: VecDeque<DryInputEvent>,
    pointer: Option<DesktopPoint>,
}
#[derive(Default)]
pub struct DryFieldInput(Mutex<State>);
impl DryFieldInput {
    fn state(&self) -> Result<std::sync::MutexGuard<'_, State>, ControlError> {
        self.0
            .lock()
            .map_err(|_| ControlError::Runtime("dry input lock poisoned".into()))
    }
    fn event(&self, event: DryInputEvent) -> Result<(), ControlError> {
        let mut state = self.state()?;
        if state.events.len() == 1000 {
            state.events.pop_front();
        }
        state.events.push_back(event);
        Ok(())
    }
    pub fn events(&self) -> Result<Vec<DryInputEvent>, ControlError> {
        Ok(self.state()?.events.iter().cloned().collect())
    }
}
impl FieldInputBackend for DryFieldInput {
    fn is_dry_run(&self) -> bool {
        true
    }
    fn key_down(&self, key: &str) -> Result<(), ControlError> {
        self.event(DryInputEvent::Down(key.into()))
    }
    fn key_up(&self, key: &str) -> Result<(), ControlError> {
        self.event(DryInputEvent::Up(key.into()))
    }
    fn press(&self, key: &str) -> Result<(), ControlError> {
        self.event(DryInputEvent::Press(key.into()))
    }
    fn move_pointer(&self, point: DesktopPoint) -> Result<(), ControlError> {
        self.state()?.pointer = Some(point);
        self.event(DryInputEvent::Move(point))
    }
    fn supports_pointer_confirmation(&self) -> bool {
        true
    }
    fn pointer_matches(&self, point: DesktopPoint) -> Result<bool, ControlError> {
        Ok(self.state()?.pointer == Some(point))
    }
}

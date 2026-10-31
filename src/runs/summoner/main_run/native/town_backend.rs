use super::{FieldCore, FieldGuards, RecordedInput};
use crate::{
    actions::{
        ActionError,
        runtime::{ActionBackend, FocusOutcome},
    },
    common::{
        controller::normalize_pointer, field_control::FieldInputBackend,
        frame_geometry::PhysicalTarget, realtime::CapturedFrame, workers::Cancellation,
    },
    platform::{CaptureBackend, InputEvent, InputSystem},
    runs::summoner::navigation_execution::StepEnvironment,
};
use rand::{Rng, SeedableRng, rngs::StdRng};
use std::{sync::Arc, time::Duration};
/// 마을 마감은 기존 action 시간/페이로드를 사용한다. 캡처와 단축키는 메인 실행기가 계속 소유한다.
pub struct BorrowedTownBackend<'a, S: CaptureBackend + InputSystem> {
    core: &'a mut FieldCore<S>,
    guards: FieldGuards<S>,
    system: Arc<S>,
    input: Arc<RecordedInput<S>>,
    random: StdRng,
    town: bool,
}
impl<'a, S: CaptureBackend + InputSystem> BorrowedTownBackend<'a, S> {
    pub fn new(core: &'a mut FieldCore<S>) -> Result<Self, ActionError> {
        let guards = core.guards();
        let system = guards.window.system();
        let input = core.input();
        let random = StdRng::try_from_os_rng().map_err(|e| {
            ActionError::runtime(format!("action random initialization failed: {e}"))
        })?;
        Ok(Self {
            core,
            guards,
            system,
            input,
            random,
            town: false,
        })
    }
    fn send(&self, event: InputEvent) -> Result<(), ActionError> {
        if self.system.send(event)? != 1 {
            return Err(ActionError::runtime(
                "SendInput did not insert the requested event",
            ));
        }
        Ok(())
    }
    fn guard(&self) -> Result<(), ActionError> {
        if self.guards.stopped() || !self.guards.focused()? {
            return Err(ActionError::runtime(
                "north-room finish interrupted or unfocused",
            ));
        }
        Ok(())
    }
}
impl<S: CaptureBackend + InputSystem> ActionBackend for BorrowedTownBackend<'_, S> {
    fn capture(&mut self) -> Result<CapturedFrame, ActionError> {
        Ok(self.core.grab()?)
    }
    fn target(&self) -> PhysicalTarget {
        self.guards.window.target()
    }
    fn geometry_is_current(&self) -> Result<bool, ActionError> {
        Ok(self.guards.window.focused()?)
    }
    fn frame_to_desktop(&self, p: [f64; 2]) -> Result<[f64; 2], ActionError> {
        Ok(self.guards.window.to_desktop(p)?.map(|n| n as f64))
    }
    fn desktop_to_frame(&self, p: [f64; 2]) -> Result<[i64; 2], ActionError> {
        Ok(self.guards.window.from_desktop_float(p)?.map(i64::from))
    }
    fn focus_configured(&mut self) -> Result<FocusOutcome, ActionError> {
        Ok(if self.guards.focused()? {
            FocusOutcome::Focused(self.target())
        } else {
            FocusOutcome::Failed
        })
    }
    fn pointer(&self) -> Result<[i64; 2], ActionError> {
        Ok(self.system.pointer()?.map(i64::from))
    }
    fn move_desktop(&mut self, p: [i64; 2]) -> Result<(), ActionError> {
        self.guard()?;
        let convert =
            |n| i32::try_from(n).map_err(|_| ActionError::runtime("pointer exceeds desktop range"));
        let p = [convert(p[0])?, convert(p[1])?];
        let [dx, dy] = normalize_pointer(self.system.desktop()?, p)?;
        self.guard()?;
        self.send(InputEvent::Mouse {
            dx,
            dy,
            flags: 0x8000 | 0x4000 | 0x0001,
        })
    }
    fn key(&mut self, k: &str, up: bool) -> Result<(), ActionError> {
        if !up {
            self.guard()?;
        }
        if up {
            self.input.controller().key_up(k)?
        } else {
            self.input.controller().key_down(k)?
        };
        Ok(())
    }
    fn click(&mut self, b: &str) -> Result<(), ActionError> {
        self.guard()?;
        Ok(self.input.controller().click_without_pause(b)?)
    }
    fn mouse_button(&mut self, b: &str, up: bool) -> Result<(), ActionError> {
        if !up {
            self.guard()?;
        }
        Ok(self.input.controller().mouse_button(b, up)?)
    }
    fn wall_now(&self) -> f64 {
        FieldCore::<S>::wall_now()
    }
    fn monotonic_now(&self) -> f64 {
        self.guards.clock.now()
    }
    fn sleep(&mut self, d: Duration) -> Result<(), ActionError> {
        std::thread::sleep(d);
        Ok(())
    }
    fn wait(&mut self, d: Duration) -> Result<(), ActionError> {
        self.guards.stop.wait(d)?;
        Ok(())
    }
    fn start_town_guard(&mut self) -> Result<(), ActionError> {
        self.guard()?;
        self.town = true;
        Ok(())
    }
    fn town_guard(&self) -> Result<(), ActionError> {
        self.guard()
    }
    fn town_geometry(&self) -> Result<(usize, PhysicalTarget), ActionError> {
        if !self.town {
            return Err(ActionError::runtime("town geometry is unavailable"));
        }
        self.guard()?;
        Ok((self.guards.window.window().id.value(), self.target()))
    }
    fn random_integer(&mut self, low: i64, high: i64) -> Result<i64, ActionError> {
        if low > high {
            return Err(ActionError::runtime("empty action integer range"));
        }
        Ok(self.random.random_range(low..=high))
    }
    fn random_float(&mut self, low: f64, high: f64) -> Result<f64, ActionError> {
        if !low.is_finite() || !high.is_finite() || low > high || !(high - low).is_finite() {
            return Err(ActionError::runtime("invalid action random range"));
        }
        Ok(if low == high {
            low
        } else {
            self.random.random_range(low..=high)
        })
    }
    fn bind_stop(&mut self, _: Cancellation) -> Result<(), ActionError> {
        Err(ActionError::runtime(
            "borrowed field hotkeys are owned by the main run",
        ))
    }
    fn unbind_stop(&mut self) -> Result<(), ActionError> {
        Ok(())
    }
    fn close_capture(&mut self) -> Result<(), ActionError> {
        Ok(())
    }
}

//! 독립 탐색의 같은 화면 증거를 공통 생존 actor와 연결한다.
use super::{
    navigation::{PathContext, PathView},
    search::{
        ArcaneSearch, HoverKind, HoverObject, SearchAction, SearchActionKind, SearchObservation,
        Terminal, terminal_action,
    },
};
use crate::{
    common::{
        buffing::BuffConfirmation,
        capture::ScreenCapture,
        config::ConfigError,
        error::{ControlError, best_effort_cleanup},
        field_control::{FieldAction, FieldInputBackend, FieldObservation},
        field_runtime::FieldControlRuntime,
        frame::{BgrFrame, PixelRect},
        frame_geometry::{CurrentGeometry, FrameGeometry, PhysicalTarget},
        realtime::CapturedFrame,
        scene_motion::CameraMotion,
        survival::ObservationStamp,
        survival_vision::{HudEvidence, HudReading},
    },
    platform::CaptureBackend,
    runs::RunError,
};
use std::{sync::Arc, time::Duration};
#[cfg(test)]
mod tests;
pub struct SceneHover {
    pub kind: HoverKind,
    pub bounds: PixelRect,
}
pub struct ArcaneScene {
    field: FieldObservation,
    terminal: Option<Terminal>,
    hub_ready: bool,
    hover: Option<SceneHover>,
    buff_confirmation: Option<BuffConfirmation>,
    player_ground: Option<[i32; 2]>,
}
impl ArcaneScene {
    pub fn new(
        field: FieldObservation,
        terminal: Option<Terminal>,
        hub_ready: bool,
        hover: Option<SceneHover>,
        buff_confirmation: Option<BuffConfirmation>,
        player_ground: Option<[i32; 2]>,
    ) -> Result<Self, ConfigError> {
        if field.has_navigation_aims() {
            return Err(ConfigError::value(
                "perceptual search owns movement; held route aims must be absent",
            ));
        }
        if player_ground.is_some_and(|p| p.iter().any(|n| *n < 0)) {
            return Err(ConfigError::value(
                "same-frame native player ground required",
            ));
        }
        Ok(Self {
            field,
            terminal,
            hub_ready,
            hover,
            buff_confirmation,
            player_ground,
        })
    }
}
pub trait SearchCapture {
    fn current_target(&self) -> Result<PhysicalTarget, RunError>;
    fn target(&self) -> PhysicalTarget;
    fn grab(&mut self) -> Result<CapturedFrame, RunError>;
    fn client_box(&self) -> Result<Option<PixelRect>, RunError>;
    fn close(&mut self) -> Result<(), RunError>;
    fn to_desktop(
        &self,
        geometry: Option<&FrameGeometry>,
        point: [i32; 2],
    ) -> Result<[i64; 2], RunError> {
        if let Some(geometry) = geometry {
            if !geometry.is_current(CurrentGeometry {
                target: self.current_target()?,
                client: self.client_box()?,
            }) {
                return Err(RunError::Runtime(
                    "window geometry changed after capture".into(),
                ));
            }
            return geometry
                .to_desktop(point.map(f64::from))
                .map_err(|e| RunError::Runtime(e.to_string()));
        }
        let target = self.target();
        Ok([
            i64::from(target.left) + i64::from(point[0]),
            i64::from(target.top) + i64::from(point[1]),
        ])
    }
}
impl<B: CaptureBackend> SearchCapture for ScreenCapture<B> {
    fn current_target(&self) -> Result<PhysicalTarget, RunError> {
        Ok(ScreenCapture::current_target(self)?)
    }
    fn target(&self) -> PhysicalTarget {
        ScreenCapture::target(self)
    }
    fn grab(&mut self) -> Result<CapturedFrame, RunError> {
        Ok(ScreenCapture::grab(self)?)
    }
    fn client_box(&self) -> Result<Option<PixelRect>, RunError> {
        Ok(Some(self.current_client_box()?))
    }
    fn close(&mut self) -> Result<(), RunError> {
        Ok(ScreenCapture::close(self)?)
    }
}
/// 의미 판독은 주입한 reader가 소유한다. 탐색은 추측한 적/종료 신호를 만들지 않는다.
pub trait SearchPerception {
    fn next_stamp(&mut self) -> Result<ObservationStamp, RunError>;
    fn scene(
        &mut self,
        frame: &Arc<BgrFrame>,
        stamp: &ObservationStamp,
    ) -> Result<ArcaneScene, RunError>;
    fn hud(&mut self, frame: &Arc<BgrFrame>) -> Result<HudReading, RunError>;
    fn motion(&mut self, frame: &Arc<BgrFrame>, view: &PathView) -> Result<CameraMotion, RunError>;
}
pub struct SearchRuntimeOptions {
    pub viewport: PixelRect,
    pub anchor: [f64; 2],
}
pub struct ArcaneSearchRuntime<B: FieldInputBackend, C: SearchCapture, P: SearchPerception> {
    search: ArcaneSearch,
    field: FieldControlRuntime<B>,
    capture: C,
    perception: P,
    options: SearchRuntimeOptions,
    target: Option<PhysicalTarget>,
    closed: bool,
    cleanup_complete: bool,
}
impl<B: FieldInputBackend, C: SearchCapture, P: SearchPerception> ArcaneSearchRuntime<B, C, P> {
    pub fn new(
        search: ArcaneSearch,
        field: FieldControlRuntime<B>,
        capture: C,
        perception: P,
        options: SearchRuntimeOptions,
    ) -> Result<Self, ConfigError> {
        if search.limits().screen_age != field.handle().screen_age() {
            return Err(ConfigError::value(
                "search and shared control must agree on frame freshness",
            ));
        }
        Ok(Self {
            search,
            field,
            capture,
            perception,
            options,
            target: None,
            closed: false,
            cleanup_complete: false,
        })
    }
    pub fn search(&self) -> &ArcaneSearch {
        &self.search
    }
    pub fn start(&mut self) -> Result<(), RunError> {
        Ok(self.field.start()?)
    }
    fn window(&self, expected: Option<PhysicalTarget>) -> Result<PhysicalTarget, RunError> {
        let target = self.capture.current_target()?;
        if expected.is_some_and(|value| value != target) {
            return Err(RunError::Runtime(
                "window rectangle changed during search".into(),
            ));
        }
        Ok(target)
    }
    fn terminal(&mut self, action: &SearchAction) -> Result<(), RunError> {
        self.field.handle().release_inputs()?;
        if action.kind == SearchActionKind::Review {
            self.field.stop(Duration::from_secs(1))?;
        }
        Ok(())
    }
    pub fn step(&mut self) -> Result<SearchAction, RunError> {
        if self.closed {
            return Ok(self.search.fail("runtime_closed"));
        }
        let result = self.step_current();
        match result {
            Ok(action) => Ok(action),
            Err(error) => {
                self.search.fail("observer_or_input_error");
                match self.close() {
                    Ok(()) => Err(error),
                    Err(cleanup) => Err(error.with_cleanup(cleanup)),
                }
            }
        }
    }
    fn step_current(&mut self) -> Result<SearchAction, RunError> {
        let handle = self.field.handle();
        let clock = handle.clock();
        if !handle.is_dry_run() && !handle.running() {
            return Err(RunError::Runtime(
                "live search requires the independent field watchdog".into(),
            ));
        }
        let status = self
            .search
            .tick(clock.now(), handle.status()?.stopped, handle.focused()?)?;
        if terminal_action(&status) {
            self.terminal(&status)?;
            return Ok(status);
        }
        let before = self.window(self.target)?;
        self.target = Some(before);
        let captured_at = clock.now();
        let generation = self.search.expected_generation().unwrap_or(0);
        let packet = self.capture.grab()?;
        self.window(Some(before))?;
        let expected_size = packet
            .geometry
            .as_ref()
            .map_or(before.size(), |g| g.layout().size());
        if self.capture.target() != before || packet.frame.size() != expected_size {
            return Err(RunError::Runtime(
                "capture and current window differ".into(),
            ));
        }
        let next = self.perception.next_stamp()?;
        let stamp = ObservationStamp::new(
            next.character_id(),
            next.room_id(),
            next.sequence(),
            captured_at,
        )?;
        let scene = self.perception.scene(&packet.frame, &stamp)?;
        if scene.field.safety().stamp() != &stamp {
            return Err(
                ConfigError::value("scene must use the captured frame's exact stamp").into(),
            );
        }
        let reading = self.perception.hud(&packet.frame)?;
        if !handle.publish_hud(
            HudEvidence {
                stamp: stamp.clone(),
                reading,
            },
            scene.field,
            scene.buff_confirmation,
        )? {
            let status = self.search.tick(clock.now(), false, true)?;
            if status.kind == SearchActionKind::Review {
                self.field.stop(Duration::from_secs(1))?;
                return Ok(status);
            }
            return Ok(self
                .search
                .action(SearchActionKind::Hold, "shared_observation_rejected"));
        }
        let directive = self.field.pump_current()?;
        if directive.action == FieldAction::Exit {
            self.field.stop(Duration::from_secs(1))?;
            return Ok(self.search.fail("shared_safety_exit"));
        }
        if !self.field.faults()?.is_empty() {
            return Err(RunError::Runtime("shared field worker failed".into()));
        }
        let ground = scene.player_ground;
        if ground.is_none() && !handle.is_dry_run() {
            return Ok(self
                .search
                .action(SearchActionKind::Hold, "player_ground_unverified"));
        }
        let view = self.search.observer().observe(
            packet.frame.view(),
            PathContext {
                sequence: stamp.sequence(),
                generation,
                captured_at,
                frame_size: packet.frame.size(),
                viewport: self.options.viewport,
                anchor: ground.map_or(self.options.anchor, |p| p.map(f64::from)),
            },
        )?;
        let motion = self.perception.motion(&packet.frame, &view)?;
        let observation = SearchObservation {
            view,
            motion,
            movement_allowed: self.field.navigation_allowed(&stamp)?,
            terminal: scene.terminal,
            hub_ready: scene.hub_ready,
            hover: scene.hover.map(|h| HoverObject {
                kind: h.kind,
                bounds: [
                    h.bounds.left() as f64,
                    h.bounds.top() as f64,
                    h.bounds.right() as f64,
                    h.bounds.bottom() as f64,
                ],
            }),
        };
        let action = self.search.step(
            &observation,
            clock.now(),
            handle.status()?.stopped,
            handle.focused()?,
        )?;
        if terminal_action(&action) {
            self.terminal(&action)?;
        }
        if matches!(action.kind, SearchActionKind::Aim | SearchActionKind::Cast) {
            self.window(Some(before))?;
            let point = action
                .aim
                .ok_or_else(|| ConfigError::value("search request aim missing"))?;
            let aim = self.capture.to_desktop(packet.geometry.as_deref(), point)?;
            if self
                .field
                .dispatch_navigation(&stamp, aim, action.kind == SearchActionKind::Cast)?
            {
                if !self.search.acknowledge(&action, clock.now(), true)? {
                    return Ok(self.search.fail("navigation_ack_rejected"));
                }
            } else {
                self.search.defer(&action);
                return Ok(self
                    .search
                    .action(SearchActionKind::Hold, "shared_control_preempted"));
            }
        }
        Ok(action)
    }
    pub fn run(
        &mut self,
        mut wait: impl FnMut(f64) -> Result<bool, RunError>,
        mut on_action: impl FnMut(&SearchAction) -> Result<(), RunError>,
    ) -> Result<SearchAction, RunError> {
        while !self.closed {
            let action = self.step()?;
            on_action(&action)?;
            if terminal_action(&action) {
                if action.kind == SearchActionKind::Review {
                    self.field.stop(Duration::from_secs(1))?;
                }
                return Ok(action);
            }
            if wait(0.02)? {
                self.field.stop(Duration::from_secs(1))?;
                return Ok(self.search.fail("interrupted"));
            }
        }
        Ok(self.search.fail("runtime_closed"))
    }
    pub fn close(&mut self) -> Result<(), RunError> {
        self.close_with_timeout(Duration::from_secs(1))
    }
    fn close_with_timeout(&mut self, timeout: Duration) -> Result<(), RunError> {
        self.closed = true;
        if self.cleanup_complete {
            return Ok(());
        }
        let stop = self.field.stop(timeout).map_err(RunError::from);
        let capture = self.capture.close();
        let result = match (stop, capture) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(e), Ok(())) | (Ok(()), Err(e)) => Err(e),
            (Err(e), Err(c)) => Err(e.with_cleanup(c)),
        };
        self.cleanup_complete = result.is_ok();
        result
    }
}
impl<B: FieldInputBackend, C: SearchCapture, P: SearchPerception> Drop
    for ArcaneSearchRuntime<B, C, P>
{
    fn drop(&mut self) {
        best_effort_cleanup(|| {
            self.close_with_timeout(Duration::from_millis(100))
                .map_err(|e| ControlError::Runtime(e.to_string()))
        });
    }
}

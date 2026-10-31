use super::super::evidence_stream::EvidenceChannel;
use super::{RecordedInput, WindowGuard};
use crate::{
    actions::{ActionError, town_loop::TownVision},
    common::{
        buffing_vision::WeaponSetObserver,
        capture::{LatestCapture, ScreenCapture},
        emergency_recovery::RecoveryEnvironment,
        error::{ControlError, ObservationError},
        field_control::{DesktopPoint, FieldInputBackend, FieldInputSession, InputSessionOptions},
        field_preparation::PreparationEnvironment,
        frame::BgrFrame,
        survival_vision::{HudReading, SlotReading, SurvivalHudObserver},
        time::MonotonicClock,
        workers::{CooperativeStop, OwnedObserver},
    },
    platform::{CaptureBackend, InputSystem},
    runs::{
        RunError,
        summoner::{
            canyon_finish::CanyonEnvironment,
            navigation_execution::{LandingEnvironment, StepEnvironment},
            room_finish::ExitEnvironment,
        },
    },
};
use std::{
    path::Path,
    sync::{Arc, Weak},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub type FieldEmitter = Arc<dyn Fn(&str) + Send + Sync>;
#[cfg(test)]
mod tests;
fn panel_error(e: ActionError) -> ObservationError {
    match e {
        ActionError::Configuration(e) | ActionError::Control(ControlError::Configuration(e)) => {
            e.into()
        }
        ActionError::Observation(e) => e,
        ActionError::Vision(e) => e.into(),
        ActionError::Io(e) => e.into(),
        ActionError::Json(e) => e.into(),
        e => ObservationError::invalid(e.to_string()),
    }
}
struct Panels {
    weapon: WeaponSetObserver,
    town: TownVision,
    last: Option<(Weak<BgrFrame>, bool)>,
}
#[derive(Clone)]
pub struct PanelObservers(OwnedObserver<Panels>);
impl PanelObservers {
    pub fn load(root: impl AsRef<Path>) -> Result<Self, RunError> {
        let root = root.as_ref();
        Ok(Self(OwnedObserver::new(
            Panels {
                weapon: WeaponSetObserver::load(root.join("assets/ui/weapon-set"))?,
                town: super::super::super::town_vision(root)?,
                last: None,
            },
            "runtime panels",
        )))
    }
    pub fn check(&self, frame: &Arc<BgrFrame>) -> Result<bool, ObservationError> {
        self.0.with(|p| {
            // 불변인 동일 Arc의 픽셀 판독만 재사용한다. 새 캡처/다른 할당은 반드시 다시 읽는다.
            if let Some((previous, open)) = &p.last
                && previous.ptr_eq(&Arc::downgrade(frame))
            {
                return Ok(*open);
            }
            p.last = None;
            if p.weapon.observe(frame.view())?.is_some() {
                p.last = Some((Arc::downgrade(frame), true));
                return Ok(true);
            }
            let open = p.town.panels_open(frame).map_err(panel_error)?;
            p.last = Some((Arc::downgrade(frame), open));
            Ok(open)
        })
    }
    pub fn with_town<T>(
        &self,
        call: impl FnOnce(&mut TownVision) -> Result<T, RunError>,
    ) -> Result<T, RunError> {
        self.0.with_error(|p| {
            p.last = None;
            let result = call(&mut p.town);
            p.town.clear_frame();
            result
        })
    }
}
pub struct FieldGuards<S: CaptureBackend + InputSystem> {
    pub clock: Arc<dyn MonotonicClock>,
    pub stop: CooperativeStop,
    pub window: Arc<WindowGuard<S>>,
}
impl<S: CaptureBackend + InputSystem> Clone for FieldGuards<S> {
    fn clone(&self) -> Self {
        Self {
            clock: self.clock.clone(),
            stop: self.stop.clone(),
            window: self.window.clone(),
        }
    }
}
impl<S: CaptureBackend + InputSystem> StepEnvironment for FieldGuards<S> {
    fn clock(&self) -> f64 {
        self.clock.now()
    }
    fn stopped(&self) -> bool {
        self.stop.is_set()
    }
    fn focused(&self) -> Result<bool, ControlError> {
        self.window.focused()
    }
    fn wait(&self, seconds: f64) -> Result<(), ControlError> {
        let duration = Duration::try_from_secs_f64(seconds)
            .map_err(|_| ControlError::Runtime("field wait exceeds runtime range".into()))?;
        self.stop.wait(duration).map(|_| ())
    }
}
pub struct FieldServices<S: CaptureBackend + InputSystem> {
    pub hud: SurvivalHudObserver,
    pub guards: FieldGuards<S>,
    pub input: Arc<RecordedInput<S>>,
    pub panels: PanelObservers,
    pub emit: FieldEmitter,
}
/// 캡처/현재 HUD는 한 실행 스레드가 소유한다. 감시 작업에는 불변 guard와 입력 소유자만 공유한다.
pub struct FieldCore<S: CaptureBackend + InputSystem> {
    capture: Option<ScreenCapture<S>>,
    latest: Option<LatestCapture>,
    capture_source: Option<(u64, f64, f64)>,
    capture_method: Option<&'static str>,
    navigation_evidence: Option<EvidenceChannel>,
    services: FieldServices<S>,
}
impl<S: CaptureBackend + InputSystem> FieldCore<S> {
    pub fn new(capture: ScreenCapture<S>, services: FieldServices<S>) -> Self {
        Self {
            capture: Some(capture),
            latest: None,
            capture_source: None,
            capture_method: None,
            navigation_evidence: None,
            services,
        }
    }
    pub fn guards(&self) -> FieldGuards<S> {
        self.services.guards.clone()
    }
    pub fn input(&self) -> Arc<RecordedInput<S>> {
        self.services.input.clone()
    }
    pub fn panels(&self) -> PanelObservers {
        self.services.panels.clone()
    }
    pub fn emit(&self, text: &str) {
        (self.services.emit)(text);
    }
    pub fn grab(&mut self) -> Result<crate::common::realtime::CapturedFrame, ControlError> {
        self.grab_frame(false)
    }
    pub fn grab_latest(&mut self) -> Result<crate::common::realtime::CapturedFrame, ControlError> {
        self.grab_frame(true)
    }
    pub fn capture_source(&self) -> Option<(u64, f64, f64)> {
        self.capture_source
    }
    pub fn capture_method(&self) -> Option<&'static str> {
        self.capture_method
    }
    pub(super) fn set_navigation_evidence(&mut self, channel: EvidenceChannel) {
        self.navigation_evidence = Some(channel);
    }
    pub(super) fn evidence_channel(&self) -> Option<EvidenceChannel> {
        self.navigation_evidence.clone()
    }
    pub(super) fn drain_navigation_evidence(
        &mut self,
        frames: &mut Vec<crate::runs::summoner::encounter_execution::EncounterEvidence>,
    ) -> Vec<String> {
        self.navigation_evidence
            .as_ref()
            .map_or_else(Vec::new, |channel| channel.drain(frames))
    }
    pub fn enable_latest_capture(&mut self) -> Result<(), ControlError> {
        if self.latest.is_none() {
            let mut capture = self
                .capture
                .take()
                .ok_or_else(|| ControlError::Runtime("field capture owner missing".into()))?;
            capture.prefer_focused_screen();
            self.latest = Some(LatestCapture::new(
                capture,
                self.services.guards.clock.clone(),
                self.services.guards.stop.clone(),
            )?);
        }
        Ok(())
    }
    fn grab_frame(
        &mut self,
        latest: bool,
    ) -> Result<crate::common::realtime::CapturedFrame, ControlError> {
        let started = self.services.guards.clock.now();
        self.capture_source = None;
        self.capture_method = None;
        let frame = if let Some(capture) = &self.latest {
            let source = capture.grab((!latest).then_some(started))?;
            self.capture_source = Some((source.sequence, source.started_at, source.completed_at));
            self.capture_method = source.method;
            source.captured
        } else {
            let frame = self
                .capture
                .as_mut()
                .ok_or_else(|| ControlError::Runtime("field capture owner missing".into()))?
                .grab()?;
            self.capture_source = Some((0, started, self.services.guards.clock.now()));
            self.capture_method = self
                .capture
                .as_ref()
                .and_then(crate::common::realtime::ScreenSource::capture_method);
            frame
        };
        self.services.guards.window.admit(&frame)?;
        Ok(frame)
    }
    pub fn read_hud(&self, frame: &Arc<BgrFrame>) -> Result<HudReading, RunError> {
        Ok(self.services.hud.observe(frame.view())?)
    }
    pub fn close_capture(&mut self) -> Result<(), RunError> {
        if let Some(capture) = &mut self.latest {
            capture.finish(Duration::from_secs(2))?;
        }
        self.latest = None;
        self.capture_source = None;
        self.capture_method = None;
        if let Some(capture) = &mut self.capture {
            capture.close()?;
        }
        Ok(())
    }
    pub fn input_session(
        &self,
        movement: &str,
        attack: &str,
        reference: Option<f64>,
        aim_settle_seconds: f64,
    ) -> Result<FieldInputSession<RecordedInput<S>>, RunError> {
        let guards = self.guards();
        let window = guards.window.clone();
        Ok(FieldInputSession::new(
            self.input(),
            movement,
            attack,
            InputSessionOptions {
                clock: guards.clock,
                stop: guards.stop,
                focused: Arc::new(move || window.focused()),
                repeat_seconds: reference,
                aim_settle_seconds,
            },
        )?)
    }
    pub fn wall_now() -> f64 {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(v) => v.as_secs_f64(),
            Err(e) => -e.duration().as_secs_f64(),
        }
    }
    pub fn to_desktop(&self, p: [f64; 2]) -> Result<DesktopPoint, ControlError> {
        self.services.guards.window.to_desktop(p)
    }
}
impl<S: CaptureBackend + InputSystem> PreparationEnvironment for FieldCore<S> {
    type Error = RunError;
    fn clock(&self) -> f64 {
        self.services.guards.clock.now()
    }
    fn wall_clock(&self) -> f64 {
        Self::wall_now()
    }
    fn stopped(&self) -> bool {
        self.services.guards.stop.is_set()
    }
    fn focused(&self) -> Result<bool, RunError> {
        Ok(self.services.guards.window.focused()?)
    }
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError> {
        Ok(self.grab()?.frame)
    }
    fn hud(&mut self, frame: &Arc<BgrFrame>) -> Result<HudReading, RunError> {
        self.read_hud(frame)
    }
    fn press(&mut self, k: &str) -> Result<(), RunError> {
        Ok(self.services.input.press(k)?)
    }
    fn wait(&mut self, seconds: f64) -> Result<(), RunError> {
        Ok(self.services.guards.wait(seconds)?)
    }
}
impl<S: CaptureBackend + InputSystem> RecoveryEnvironment for FieldCore<S> {
    fn closed_belt_slots(
        &mut self,
        frame: &Arc<BgrFrame>,
        reading: &HudReading,
    ) -> Result<Vec<SlotReading>, RunError> {
        Ok(self.services.hud.closed_belt_slots(frame.view(), reading)?)
    }
}
impl<S: CaptureBackend + InputSystem> StepEnvironment for FieldCore<S> {
    fn clock(&self) -> f64 {
        self.services.guards.clock.now()
    }
    fn stopped(&self) -> bool {
        self.services.guards.stop.is_set()
    }
    fn focused(&self) -> Result<bool, ControlError> {
        self.services.guards.window.focused()
    }
    fn wait(&self, seconds: f64) -> Result<(), ControlError> {
        self.services.guards.wait(seconds)
    }
}
impl<S: CaptureBackend + InputSystem> LandingEnvironment for FieldCore<S> {
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError> {
        Ok(self.grab()?.frame)
    }
    fn hud(&mut self, frame: &Arc<BgrFrame>) -> Result<HudReading, RunError> {
        self.read_hud(frame)
    }
}
impl<S: CaptureBackend + InputSystem> CanyonEnvironment for FieldCore<S> {
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError> {
        Ok(self.grab()?.frame)
    }
    fn hud(&mut self, frame: &Arc<BgrFrame>) -> Result<HudReading, RunError> {
        self.read_hud(frame)
    }
    fn frame_to_desktop(&self, p: [f64; 2]) -> Result<DesktopPoint, ControlError> {
        self.to_desktop(p)
    }
    fn emit(&mut self, text: &str) {
        FieldCore::emit(self, text);
    }
}
impl<S: CaptureBackend + InputSystem> ExitEnvironment for FieldCore<S> {
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError> {
        Ok(self.grab()?.frame)
    }
    fn frame_to_desktop(&self, p: [f64; 2]) -> Result<DesktopPoint, ControlError> {
        self.to_desktop(p)
    }
}

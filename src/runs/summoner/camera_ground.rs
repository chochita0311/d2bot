//! 현재 프레임의 몸 근거와 닫힌 viewport 확인에 한정한 카메라 위치 보정.
mod handle;
use super::scene_vision::{FlashGroundObserver, GroundReading};
use crate::common::{
    buffing::BeltVisibility,
    error::ObservationError,
    frame::{BgrFrame, Size},
    survival_vision::HudReading,
};
pub use handle::CameraGroundHandle;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    path::Path,
    sync::{Arc, Weak},
};

pub trait GroundObserver: Send {
    fn observe(
        &mut self,
        frame: &Arc<BgrFrame>,
        captured_at: Option<f64>,
    ) -> Result<GroundReading, ObservationError>;
}
impl GroundObserver for FlashGroundObserver {
    fn observe(
        &mut self,
        frame: &Arc<BgrFrame>,
        captured_at: Option<f64>,
    ) -> Result<GroundReading, ObservationError> {
        FlashGroundObserver::observe(self, frame.view(), captured_at)
    }
}
/// 다른 캐릭터는 소서리스 외형을 재사용하지 않고 검토한 카메라 경로만 소비한다.
pub struct CameraOnlyGround;
impl GroundObserver for CameraOnlyGround {
    fn observe(
        &mut self,
        _frame: &Arc<BgrFrame>,
        _captured_at: Option<f64>,
    ) -> Result<GroundReading, ObservationError> {
        Ok(GroundReading::unknown("ground_unverified"))
    }
}
#[derive(Debug, Clone, Copy)]
pub struct CameraLimits {
    pub maximum_occlusion: f64,
    pub maximum_spread: f64,
    pub maximum_shift: f64,
    pub persist_reviewed_layout: bool,
}
impl Default for CameraLimits {
    fn default() -> Self {
        Self {
            maximum_occlusion: 2.0,
            maximum_spread: 12.0,
            maximum_shift: 24.0,
            persist_reviewed_layout: false,
        }
    }
}
#[derive(Debug, Deserialize)]
pub struct CameraViewportProof {
    pub source: String,
    pub panels_closed: bool,
    pub observed_at: f64,
    #[serde(default)]
    pub camera_layout_verified: bool,
}
#[derive(Debug, Clone, Deserialize)]
pub struct ReviewedCameraLayout {
    schema: u32,
    frame_size: [usize; 2],
    body_observations: [BodyObservation; 3],
}
#[derive(Debug, Clone, Deserialize)]
struct BodyObservation {
    point: [i32; 2],
}
impl ReviewedCameraLayout {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ObservationError> {
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }
}
struct FrameContext {
    frame: Weak<BgrFrame>,
    at: f64,
    valid: bool,
}
type PanelObserver = Box<dyn FnMut(&Arc<BgrFrame>) -> Result<bool, ObservationError> + Send>;
pub struct CalibratedCameraGroundObserver {
    ground: Box<dyn GroundObserver>,
    panels_open: PanelObserver,
    limits: CameraLimits,
    authorized: bool,
    context: Option<FrameContext>,
    samples: VecDeque<(f64, [i32; 2])>,
    center: Option<[i32; 2]>,
    last_strong_at: Option<f64>,
    last_at: Option<f64>,
    viewport_anchor: Option<[i32; 2]>,
    reviewed_anchor: Option<[i32; 2]>,
    calibrated_at: Option<f64>,
    projections: u64,
    calibrations: u64,
    seed_started: Option<f64>,
    reference_kind: &'static str,
    reviewed_projection_active: bool,
    reset_events: VecDeque<serde_json::Value>,
}
#[derive(Serialize)]
pub struct CameraDiagnostics<'a> {
    pub authorized: bool,
    pub samples: &'a VecDeque<(f64, [i32; 2])>,
    pub center: Option<[i32; 2]>,
    pub last_strong_at: Option<f64>,
    pub last_at: Option<f64>,
    pub viewport_anchor: Option<[i32; 2]>,
    pub reviewed_anchor: Option<[i32; 2]>,
    pub calibrated_at: Option<f64>,
    pub projections: u64,
    pub calibrations: u64,
    pub seed_started: Option<f64>,
    pub reference_kind: &'static str,
    pub reviewed_projection_active: bool,
    pub reset_events: &'a VecDeque<serde_json::Value>,
}
fn distance(a: [i32; 2], b: [i32; 2]) -> f64 {
    (f64::from(a[0]) - f64::from(b[0])).hypot(f64::from(a[1]) - f64::from(b[1]))
}
fn median(points: [[i32; 2]; 3]) -> [i32; 2] {
    let axis = |axis| {
        let mut values = points.map(|p| p[axis]);
        values.sort_unstable();
        values[1]
    };
    [axis(0), axis(1)]
}
impl CalibratedCameraGroundObserver {
    pub fn new(
        ground: Box<dyn GroundObserver>,
        panels_open: impl FnMut(&Arc<BgrFrame>) -> Result<bool, ObservationError> + Send + 'static,
        limits: CameraLimits,
    ) -> Result<Self, ObservationError> {
        if [
            limits.maximum_occlusion,
            limits.maximum_spread,
            limits.maximum_shift,
        ]
        .into_iter()
        .any(|v| !v.is_finite() || v <= 0.0)
        {
            return Err(ObservationError::invalid(
                "positive finite camera limits required",
            ));
        }
        Ok(Self {
            ground,
            panels_open: Box::new(panels_open),
            limits,
            authorized: false,
            context: None,
            samples: VecDeque::with_capacity(3),
            center: None,
            last_strong_at: None,
            last_at: None,
            viewport_anchor: None,
            reviewed_anchor: None,
            calibrated_at: None,
            projections: 0,
            calibrations: 0,
            seed_started: None,
            reference_kind: "current_body",
            reviewed_projection_active: false,
            reset_events: VecDeque::with_capacity(32),
        })
    }
    pub fn diagnostics(&self) -> CameraDiagnostics<'_> {
        CameraDiagnostics {
            authorized: self.authorized,
            samples: &self.samples,
            center: self.center,
            last_strong_at: self.last_strong_at,
            last_at: self.last_at,
            viewport_anchor: self.viewport_anchor,
            reviewed_anchor: self.reviewed_anchor,
            calibrated_at: self.calibrated_at,
            projections: self.projections,
            calibrations: self.calibrations,
            seed_started: self.seed_started,
            reference_kind: self.reference_kind,
            reviewed_projection_active: self.reviewed_projection_active,
            reset_events: &self.reset_events,
        }
    }
    pub fn reset(&mut self, reason: &str, captured_at: Option<f64>, preserve_reviewed: bool) {
        let retained = preserve_reviewed
            && self.reviewed_projection_active
            && self.limits.persist_reviewed_layout;
        self.samples.clear();
        self.center = if retained { self.viewport_anchor } else { None };
        self.last_strong_at = None;
        self.calibrated_at = None;
        self.seed_started = None;
        self.reviewed_projection_active = retained;
        if self.reset_events.len() == 32 {
            self.reset_events.pop_front();
        }
        self.reset_events.push_back(serde_json::json!({"reason":reason,"captured_at":captured_at,"reviewed_reference_retained":retained}));
    }
    fn seed_reviewed_layout(
        &mut self,
        layout: &ReviewedCameraLayout,
    ) -> Result<(), ObservationError> {
        if layout.schema != 1 || layout.frame_size != [1922, 1140] {
            return Err(ObservationError::invalid(
                "reviewed native three-body camera calibration required",
            ));
        }
        let points = layout.body_observations.each_ref().map(|p| p.point);
        let center = median(points);
        if !(24..1900).contains(&center[0]) || !(55..900).contains(&center[1]) {
            return Err(ObservationError::invalid(
                "camera reference outside world viewport",
            ));
        }
        if points
            .into_iter()
            .any(|p| distance(p, center) > self.limits.maximum_spread)
        {
            return Err(ObservationError::invalid(
                "camera reference body samples disagree",
            ));
        }
        self.center = Some(center);
        self.viewport_anchor = Some(center);
        self.reviewed_anchor = Some(center);
        self.reference_kind = "reviewed_layout";
        self.reviewed_projection_active = self.limits.persist_reviewed_layout;
        Ok(())
    }
    pub fn confirm_closed_viewport(
        &mut self,
        proof: &CameraViewportProof,
        now: f64,
        layout: Option<&ReviewedCameraLayout>,
    ) -> Result<(), ObservationError> {
        if proof.source != "supervised_visual"
            || !proof.panels_closed
            || !proof.observed_at.is_finite()
            || !(0.0..=15.0).contains(&(now - proof.observed_at))
        {
            return Err(ObservationError::invalid(
                "fresh supervised closed-panel viewport proof required",
            ));
        }
        self.reset("new_supervised_confirmation", None, false);
        self.viewport_anchor = None;
        self.reviewed_anchor = None;
        self.authorized = true;
        self.context = None;
        self.last_at = None;
        self.reference_kind = "current_body";
        if let Some(layout) = layout {
            if !proof.camera_layout_verified {
                return Err(ObservationError::invalid(
                    "fresh current camera-layout confirmation required",
                ));
            }
            self.seed_reviewed_layout(layout)?;
        }
        Ok(())
    }
    pub fn confirm_runtime_viewport(
        &mut self,
        frame: &Arc<BgrFrame>,
        hud: &HudReading,
        focused: bool,
        captured_at: f64,
        now: f64,
        layout: &ReviewedCameraLayout,
    ) -> Result<(), ObservationError> {
        if !focused
            || frame.size() != Size::new(1922, 1140)?
            || !(0.0..0.25).contains(&(now - captured_at))
            || (self.panels_open)(frame)?
            || hud.belt_visibility != BeltVisibility::Closed
            || hud.life.ratio().is_none()
            || hud.mana.ratio().is_none()
        {
            return Err(ObservationError::invalid(
                "current closed native runtime viewport required",
            ));
        }
        self.reset("new_runtime_viewport", None, false);
        self.authorized = true;
        self.context = None;
        self.last_at = None;
        self.seed_reviewed_layout(layout)?;
        self.prepare_frame(frame, captured_at, hud, focused)
    }
    pub fn prepare_frame(
        &mut self,
        frame: &Arc<BgrFrame>,
        captured_at: f64,
        hud: &HudReading,
        focused: bool,
    ) -> Result<(), ObservationError> {
        let native = frame.size() == Size::new(1922, 1140)?;
        let mut layout_valid = self.authorized && focused && native;
        if layout_valid && (self.panels_open)(frame)? {
            self.authorized = false;
            layout_valid = false;
        }
        if !focused || !native {
            self.authorized = false;
        }
        let valid = layout_valid
            && hud.belt_visibility == BeltVisibility::Closed
            && hud.life.ratio().is_some()
            && hud.mana.ratio().is_some();
        if !valid {
            self.reset(
                if layout_valid {
                    "resource_observation_unverified"
                } else {
                    "viewport_unverified"
                },
                Some(captured_at),
                layout_valid,
            );
        } else if self.center.is_some()
            && self.last_strong_at.is_none()
            && self.seed_started.is_none()
        {
            self.seed_started = Some(captured_at);
        }
        // Weak는 픽셀을 보관하지 않으면서 해제 뒤 주소 재사용에 의한 오인을 막는다.
        self.context = Some(FrameContext {
            frame: Arc::downgrade(frame),
            at: captured_at,
            valid,
        });
        Ok(())
    }
    pub fn observe(
        &mut self,
        frame: &Arc<BgrFrame>,
        captured_at: f64,
    ) -> Result<GroundReading, ObservationError> {
        if !captured_at.is_finite() || captured_at < 0.0 {
            return Err(ObservationError::invalid(
                "finite camera capture time required",
            ));
        }
        if self.last_at.is_some_and(|last| captured_at < last) {
            self.reset("camera_time_reversed", Some(captured_at), false);
            self.last_at = Some(captured_at);
            return Ok(GroundReading::unknown("camera_time_reversed"));
        }
        self.last_at = Some(captured_at);
        let mut reading = self.ground.observe(frame, Some(captured_at))?;
        let context = self.context.as_ref().filter(|context| {
            context.frame.ptr_eq(&Arc::downgrade(frame)) && context.at == captured_at
        });
        if context.is_none_or(|c| !c.valid) {
            let preserve = self.authorized && context.is_some_and(|c| !c.valid);
            self.reset(
                "current_frame_context_unverified",
                Some(captured_at),
                preserve,
            );
            return Ok(reading);
        }
        if let (Some(point), Some(anchor)) = (reading.point, self.viewport_anchor)
            && matches!(
                reading.reason.as_str(),
                "flash_es_and_body_shape" | "flash_body_shape_and_es"
            )
            && distance(point, anchor) > self.limits.maximum_shift
        {
            reading = GroundReading::unknown("ground_unverified");
        }
        if let Some(point) = reading.point
            && reading.reason != "flash_marker_tracked"
        {
            if self
                .viewport_anchor
                .is_some_and(|anchor| distance(point, anchor) > self.limits.maximum_shift)
            {
                self.reset("camera_actor_displaced", Some(captured_at), false);
                if let Some(event) = self
                    .reset_events
                    .back_mut()
                    .and_then(serde_json::Value::as_object_mut)
                {
                    event.insert("candidate_point".into(), serde_json::json!(point));
                    event.insert("candidate_reason".into(), serde_json::json!(reading.reason));
                    event.insert("candidate_score".into(), serde_json::json!(reading.score));
                    event.insert(
                        "viewport_anchor".into(),
                        serde_json::json!(self.viewport_anchor),
                    );
                }
                return Ok(GroundReading::unknown("camera_actor_displaced"));
            }
            if self
                .samples
                .back()
                .is_some_and(|(at, _)| captured_at - at > 0.5)
            {
                self.samples.clear();
            }
            if self.samples.back().is_none_or(|(at, _)| captured_at > *at) {
                if self.samples.len() == 3 {
                    self.samples.pop_front();
                }
                self.samples.push_back((captured_at, point));
            }
            self.last_strong_at = Some(captured_at);
            if self.samples.len() == 3 && self.samples[2].0 - self.samples[0].0 >= 0.15 {
                let points = [self.samples[0].1, self.samples[1].1, self.samples[2].1];
                let center = median(points);
                if points
                    .into_iter()
                    .all(|p| distance(p, center) <= self.limits.maximum_spread)
                    && self.center.is_none()
                {
                    self.center = Some(center);
                    self.calibrated_at = Some(captured_at);
                    self.viewport_anchor = Some(center);
                    self.calibrations = self.calibrations.checked_add(1).ok_or_else(|| {
                        ObservationError::invalid("camera calibration counter exhausted")
                    })?;
                    if self.limits.persist_reviewed_layout
                        && self.reviewed_anchor.is_some_and(|anchor| {
                            distance(center, anchor) <= self.limits.maximum_spread
                        })
                    {
                        self.reviewed_projection_active = true;
                    }
                }
            }
            return Ok(reading);
        }
        let reference_at = self.last_strong_at.or(self.seed_started);
        if !self.reviewed_projection_active
            && self.center.is_some()
            && reference_at.is_none_or(|at| captured_at - at > self.limits.maximum_occlusion)
        {
            self.reset("camera_reference_expired", Some(captured_at), false);
        }
        if reading.point.is_some() || reading.reason != "ground_unverified" || self.center.is_none()
        {
            return Ok(reading);
        }
        let reference_at = reference_at
            .ok_or_else(|| ObservationError::invalid("camera projection reference time missing"))?;
        self.projections = self
            .projections
            .checked_add(1)
            .ok_or_else(|| ObservationError::invalid("camera projection counter exhausted"))?;
        Ok(GroundReading {
            point: self.center,
            reason: "camera_ground_projection".into(),
            score: None,
            calibration: Some(
                serde_json::json!({"calibrated_at":self.calibrated_at,"last_body_at":self.last_strong_at,"occluded_seconds":captured_at-reference_at,"panel_basis":"supervised_closed_and_controlled_inputs","reference_kind":self.reference_kind,"current_reviewed_viewport":self.reviewed_projection_active}),
            ),
        })
    }
}

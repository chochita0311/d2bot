use super::*;
use crate::{
    common::{
        buffing::BeltVisibility,
        scene_motion::{BackgroundMotionObserver, CameraMotion, LandmarkMask, MotionLimits},
        survival::CharacterSurvivalPolicy,
        survival_vision::{HudReading, ResourceReading, resource_guard},
    },
    runs::RunError,
};
use serde::Serialize;

pub trait LandingEnvironment: StepEnvironment {
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError>;
    fn hud(&mut self, frame: &Arc<BgrFrame>) -> Result<HudReading, RunError>;
}
pub struct LandingRequest<'a> {
    pub frame: &'a Arc<BgrFrame>,
    pub view: &'a PathView,
    pub choice: &'a PathChoice,
    pub policy: &'a CharacterSurvivalPolicy,
    pub reference_seconds: f64,
    pub expires_at: Option<f64>,
}
#[derive(Debug, Serialize)]
pub struct LandingSample {
    pub elapsed: f64,
    pub processing_seconds: f64,
    pub ground: Option<[i32; 2]>,
    pub ground_reason: String,
    pub motion: CameraMotion,
    pub resources: Option<&'static str>,
    pub life: ResourceReading,
    pub mana: ResourceReading,
    pub belt_visibility: BeltVisibility,
}
#[derive(Debug, Serialize)]
pub struct LandingResult {
    pub status: &'static str,
    pub displacement: [f64; 2],
    pub observations: Vec<LandingSample>,
}
trait LandingMotion {
    fn observe(
        &mut self,
        frame: &Arc<BgrFrame>,
        view: &PathView,
        hint: Option<[f64; 2]>,
    ) -> Result<CameraMotion, RunError>;
}
struct NativeMotion(BackgroundMotionObserver);
impl LandingMotion for NativeMotion {
    fn observe(
        &mut self,
        frame: &Arc<BgrFrame>,
        view: &PathView,
        hint: Option<[f64; 2]>,
    ) -> Result<CameraMotion, RunError> {
        let masks = PathMotionMasks::new(view)?;
        Ok(self.0.observe(frame.view(), masks.view(), hint)?)
    }
}
pub fn observe_step_landing(
    env: &mut impl LandingEnvironment,
    ground: &mut dyn GroundObserver,
    path: &ArcanePathObserver,
    request: LandingRequest<'_>,
) -> Result<LandingResult, RunError> {
    let mut motion = NativeMotion(BackgroundMotionObserver::new(
        MotionLimits::new(12, 0.65, 3.0, 0.5)?,
        Some(LandmarkMask {
            viewport: PixelRect::new(120, 180, 1720, 890).map_err(ObservationError::from)?,
            actor_exclusion: [160, 230, 160, 75],
        }),
    ));
    observe_with_motion(env, ground, path, request, &mut motion)
}
fn observe_with_motion(
    env: &mut impl LandingEnvironment,
    ground: &mut dyn GroundObserver,
    path: &ArcanePathObserver,
    request: LandingRequest<'_>,
    motion: &mut impl LandingMotion,
) -> Result<LandingResult, RunError> {
    if !(0.0..1.5).contains(&request.reference_seconds) || request.reference_seconds == 0.0 {
        return Err(RunError::Runtime(
            "configured cast reference must fit the landing observation window".into(),
        ));
    }
    let aim = request
        .choice
        .aim
        .ok_or_else(|| RunError::Runtime("landing observation requires a planned aim".into()))?;
    motion.observe(request.frame, request.view, None)?;
    let started = env.clock();
    let mut result = LandingResult {
        status: "arrival_unverified",
        displacement: [0.0; 2],
        observations: Vec::new(),
    };
    let desired =
        std::array::from_fn::<_, 2, _>(|i| f64::from(aim[i]) - request.view.context.anchor[i]);
    let desired_length = desired[0].hypot(desired[1]);
    let mut stable = 0;
    for sequence in 2..34 {
        if request
            .expires_at
            .is_some_and(|expiry| env.clock() >= expiry)
        {
            result.status = "segment_time_limit";
            break;
        }
        if env.stopped() || env.clock() - started >= 1.5 {
            break;
        }
        if !env.focused()? {
            result.status = "focus_lost";
            break;
        }
        let acquired = env.clock();
        let current = env.capture()?;
        let reading = env.hud(&current)?;
        let blocked = resource_guard(&reading, request.policy);
        if blocked == Some("resource_low") {
            result.status = "resource_low";
            break;
        }
        let position = ground.observe(&current, Some(acquired))?;
        let current_view = path.observe(
            current.view(),
            PathContext {
                sequence,
                generation: request.view.context.generation,
                captured_at: acquired,
                frame_size: current.size(),
                viewport: request.view.context.viewport,
                anchor: position
                    .point
                    .map(|p| p.map(f64::from))
                    .unwrap_or(request.view.context.anchor),
            },
        )?;
        let remaining = std::array::from_fn(|i| desired[i] - result.displacement[i]);
        let measured = motion.observe(&current, &current_view, Some(remaining))?;
        let processed = env.clock();
        let elapsed = processed - started;
        let age = processed - acquired;
        let displacement = measured.displacement;
        result.observations.push(LandingSample {
            elapsed,
            processing_seconds: age,
            ground: position.point,
            ground_reason: position.reason,
            motion: measured,
            resources: blocked,
            life: reading.life,
            mana: reading.mana,
            belt_visibility: reading.belt_visibility,
        });
        if request.expires_at.is_some_and(|expiry| processed >= expiry) {
            result.status = "segment_time_limit";
            break;
        }
        if !(0.0..0.25).contains(&age) {
            result.status = "screen_stale";
            break;
        }
        if let Some(delta) = displacement {
            result.displacement = std::array::from_fn(|i| result.displacement[i] + delta[i]);
        }
        if displacement.is_none() || blocked.is_some() {
            stable = 0;
        } else if let Some([dx, dy]) = displacement {
            stable = if dx.hypot(dy) < 2.0 { stable + 1 } else { 0 };
            let [x, y] = result.displacement;
            let distance = x.hypot(y);
            let aligned = desired_length > 0.0
                && distance > 0.0
                && (x * desired[0] + y * desired[1]) / distance / desired_length >= 0.8;
            if stable >= 2
                && position.point.is_some()
                && elapsed >= request.reference_seconds
                && (12.0..=2.0 * desired_length + 25.0).contains(&distance)
                && aligned
                && env.focused()?
                && !env.stopped()
            {
                result.status = "arrival_observed";
                break;
            }
        }
        env.wait(0.02)?;
    }
    Ok(result)
}
#[cfg(test)]
mod tests;

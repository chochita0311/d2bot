use super::*;
use crate::{
    actions::town_loop::{TownSight, TownVision},
    common::time::MonotonicClock,
    runs::summoner::{
        camera_ground::GroundObserver,
        navigation::{ArcanePathObserver, ChooseOptions, PathView, Wing},
        navigation_execution::{
            LandingRequest, PlanOptions, observe_step_landing, plan_arcane_step,
        },
        scene_vision::{ArcaneHazardHandle, ArcaneHoverObserver},
    },
};

/// 단발 이동은 현재 바닥 관찰기를 그대로 사용하며 연속 이동의 프레임 준비를 덧붙이지 않는다.
pub struct NativeLegacyVision {
    pub ground: Box<dyn GroundObserver>,
    pub path: ArcanePathObserver,
    pub hub: Option<TownVision>,
    pub hazards: Option<ArcaneHazardHandle>,
    pub hover: Option<Arc<ArcaneHoverObserver>>,
    pub clock: Arc<dyn MonotonicClock>,
}
impl LegacyVision for NativeLegacyVision {
    type View = PathView;
    fn has_hazards(&self) -> bool {
        self.hazards.is_some()
    }
    fn has_hover(&self) -> bool {
        self.hover.is_some()
    }
    fn hub(&mut self, frame: &Arc<BgrFrame>) -> Result<Option<[f64; 2]>, RunError> {
        let Some(hub) = self.hub.as_mut() else {
            return Ok(None);
        };
        hub.set_frame(frame.clone());
        hub.normalize()?;
        Ok(hub.find("world_arcane_waypoint")?.map(|p| {
            [
                p.x * frame.size().width() as f64 / 1267.0,
                p.y * frame.size().height() as f64 / 753.0,
            ]
        }))
    }
    fn ground(&mut self, frame: &Arc<BgrFrame>, acquired: f64) -> Result<GroundReading, RunError> {
        Ok(self.ground.observe(frame, Some(acquired))?)
    }
    fn plan(
        &mut self,
        frame: &Arc<BgrFrame>,
        request: LegacyPlanRequest<'_>,
    ) -> Result<CurrentPlan<PathView>, RunError> {
        let plan = plan_arcane_step(
            frame,
            Wing::North,
            self.ground.as_mut(),
            &self.path,
            request.acquired,
            PlanOptions {
                hub_point: request.hub,
                choose: ChooseOptions {
                    position: request.position,
                    visited: request.visited,
                    ..Default::default()
                },
                ..Default::default()
            },
            self.clock.as_ref(),
        )?;
        Ok(CurrentPlan {
            view: plan.view,
            choice: plan.choice,
            ground: plan.ground,
        })
    }
    fn hazards(
        &mut self,
        frame: &Arc<BgrFrame>,
        player: [i32; 2],
        acquired: f64,
        now: f64,
        aim: Option<[i32; 2]>,
    ) -> Result<Vec<VisualCandidate>, RunError> {
        Ok(self
            .hazards
            .as_mut()
            .ok_or_else(|| RunError::Runtime("hazard observer missing".into()))?
            .observe(frame.view(), player, acquired, now, aim)?)
    }
    fn hover(
        &mut self,
        frame: &Arc<BgrFrame>,
        aim: [i32; 2],
    ) -> Result<Vec<HoverCandidate>, RunError> {
        Ok(self
            .hover
            .as_ref()
            .ok_or_else(|| RunError::Runtime("hover observer missing".into()))?
            .observe(frame.view(), aim)?)
    }
    fn landing(
        &mut self,
        env: &mut impl LandingEnvironment,
        request: LegacyLandingRequest<'_, PathView>,
    ) -> Result<LandingResult, RunError> {
        observe_step_landing(
            env,
            self.ground.as_mut(),
            &self.path,
            LandingRequest {
                frame: request.frame,
                view: request.view,
                choice: request.choice,
                policy: request.policy,
                reference_seconds: request.reference_seconds,
                expires_at: Some(request.expires_at),
            },
        )
    }
}

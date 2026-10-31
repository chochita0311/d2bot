//! 현재 발판의 병렬 관찰과 한정된 접근/노바 실행.
mod execution;
use super::{
    appearance_vision::{SummonerAppearance, SummonerAppearanceObserver},
    camera_ground::CameraGroundHandle,
    navigation::{ArcanePathObserver, ChooseOptions, PathChoice, PathContext, PathView, Wing},
    objective_vision::{ArcaneObjectiveObserver, ObjectiveReading},
    scene_vision::GroundReading,
};
use crate::{
    common::{
        error::ObservationError,
        frame::{BgrFrame, PixelRect},
        survival_vision::HudReading,
        workers::{CooperativeStop, ObservationPool, ObservationTask},
    },
    runs::RunError,
};
pub use execution::{
    EncounterEnvironment, EncounterEvidence, EncounterReport, EncounterRequest,
    run_platform_encounter,
};
use std::{sync::Arc, time::Duration};

pub struct EncounterFrame<P> {
    pub player: GroundReading,
    pub platform: Option<ObjectiveReading>,
    pub target: Option<SummonerAppearance>,
    pub corpse: Option<SummonerAppearance>,
    pub path: Option<P>,
}
pub trait EncounterVision {
    type Path;
    fn prepares_ground(&self) -> bool;
    fn prepare(
        &mut self,
        frame: &Arc<BgrFrame>,
        acquired: f64,
        reading: &HudReading,
        focused: bool,
    ) -> Result<(), RunError>;
    fn observe(
        &mut self,
        frame: &Arc<BgrFrame>,
        sequence: u64,
        acquired: f64,
        may_approach: bool,
        observe_corpse: bool,
    ) -> Result<EncounterFrame<Self::Path>, RunError>;
    fn choose(&mut self, path: Self::Path, destination: [f64; 2]) -> Result<PathChoice, RunError>;
    fn finish(&mut self, timeout: Duration) -> Result<(), RunError>;
}
pub struct EncounterPathTask {
    task: ObservationTask<Result<PathView, ObservationError>>,
    path: Arc<ArcanePathObserver>,
}
/// 호출자가 소유하여 종료가 늦어질 때도 pool과 camera를 보존한다.
pub struct NativeEncounterVision {
    camera: CameraGroundHandle,
    path: Arc<ArcanePathObserver>,
    objective: Arc<ArcaneObjectiveObserver>,
    appearance: Arc<SummonerAppearanceObserver>,
    pool: ObservationPool,
}
impl NativeEncounterVision {
    pub fn new(
        camera: CameraGroundHandle,
        mut path: Arc<ArcanePathObserver>,
        objective: Arc<ArcaneObjectiveObserver>,
        appearance: Arc<SummonerAppearanceObserver>,
        stop: CooperativeStop,
    ) -> Result<Self, RunError> {
        if let Some(mut geometry) = path.teleport() {
            // 목표 접근에서 짧은 탐색 선호가 실제 목표 바닥을 제외하지 않게 한다.
            geometry.preferred_distance = Some(geometry.maximum_distance);
            Arc::make_mut(&mut path).set_teleport(Some(geometry))?;
        }
        Ok(Self {
            camera,
            path,
            objective,
            appearance,
            pool: ObservationPool::new(3, 3, stop)?,
        })
    }
}
impl EncounterVision for NativeEncounterVision {
    type Path = EncounterPathTask;
    fn prepares_ground(&self) -> bool {
        true
    }
    fn prepare(
        &mut self,
        frame: &Arc<BgrFrame>,
        acquired: f64,
        reading: &HudReading,
        focused: bool,
    ) -> Result<(), RunError> {
        Ok(self
            .camera
            .with(|camera| camera.prepare_frame(frame, acquired, reading, focused))?)
    }
    fn observe(
        &mut self,
        frame: &Arc<BgrFrame>,
        sequence: u64,
        acquired: f64,
        may_approach: bool,
        observe_corpse: bool,
    ) -> Result<EncounterFrame<Self::Path>, RunError> {
        let camera = self.camera.clone();
        let player_frame = frame.clone();
        let player = self
            .pool
            .submit(move || camera.with(|camera| camera.observe(&player_frame, acquired)))?;
        let platform = self.objective.observe(frame.view())?;
        let appearance = self.appearance.clone();
        let target_frame = frame.clone();
        let target_platform = platform.clone();
        let target = self
            .pool
            .submit(move || appearance.observe(target_frame.view(), target_platform.as_ref()))?;
        let corpse = if observe_corpse {
            let appearance = self.appearance.clone();
            let corpse_frame = frame.clone();
            let corpse_platform = platform.clone();
            Some(self.pool.submit(move || {
                appearance.observe_corpse(corpse_frame.view(), corpse_platform.as_ref())
            })?)
        } else {
            None
        };
        let player = player.join()??;
        let path = if let Some(anchor) = player.point.filter(|_| platform.is_some() && may_approach)
        {
            let observer = self.path.clone();
            let path_frame = frame.clone();
            Some(EncounterPathTask {
                task: self.pool.submit(move || {
                    observer.observe(
                        path_frame.view(),
                        PathContext {
                            sequence,
                            generation: 0,
                            captured_at: acquired,
                            frame_size: path_frame.size(),
                            viewport: PixelRect::new(120, 180, 1720, 890)?,
                            anchor: anchor.map(f64::from),
                        },
                    )
                })?,
                path: self.path.clone(),
            })
        } else {
            None
        };
        let target = target.join()??;
        let corpse = corpse
            .map(ObservationTask::join)
            .transpose()?
            .transpose()?
            .flatten();
        Ok(EncounterFrame {
            player,
            platform,
            target,
            corpse,
            path,
        })
    }
    fn choose(&mut self, path: Self::Path, destination: [f64; 2]) -> Result<PathChoice, RunError> {
        let view = path.task.join()??;
        Ok(path.path.choose(
            &view,
            Wing::North,
            ChooseOptions {
                destination: Some(destination),
                ..ChooseOptions::default()
            },
        )?)
    }
    fn finish(&mut self, timeout: Duration) -> Result<(), RunError> {
        Ok(self.pool.finish(timeout)?)
    }
}

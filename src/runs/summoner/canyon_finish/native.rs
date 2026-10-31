use super::*;
use crate::{
    actions::town_loop::{TownSight, TownVision},
    common::{
        frame::{BgrFrame, PixelRect},
        survival_vision::HudReading,
    },
    runs::{
        RunError,
        summoner::{
            camera_ground::CameraGroundHandle, journal_vision::HorazonJournalObserver,
            navigation::PathContext,
        },
    },
};
use std::{path::Path, sync::Arc};
pub struct NativeCanyonVision {
    pub camera: CameraGroundHandle,
    pub path: ArcanePathObserver,
    pub town: TownVision,
    journal: HorazonJournalObserver,
    steps: CanyonApproachObserver,
    portal: RedPortalObserver,
}
impl NativeCanyonVision {
    pub fn new(
        root: impl AsRef<Path>,
        camera: CameraGroundHandle,
        path: ArcanePathObserver,
    ) -> Result<Self, RunError> {
        let root = root.as_ref();
        let owner = root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
        Ok(Self {
            camera,
            path,
            town: super::super::town_vision(root)?,
            journal: HorazonJournalObserver::load(owner.join("journal-exit.json"))?,
            steps: CanyonApproachObserver::load(root)?,
            portal: RedPortalObserver::load(owner)?,
        })
    }
}
impl CanyonVision for NativeCanyonVision {
    fn town_frame(&mut self, frame: &Arc<BgrFrame>) -> Result<(), RunError> {
        self.town.set_frame(frame.clone());
        self.town.normalize()?;
        Ok(())
    }
    fn find(&mut self, name: &str) -> Result<bool, RunError> {
        Ok(self.town.find(name)?.is_some())
    }
    fn steps(&mut self, frame: &Arc<BgrFrame>) -> Result<Option<[f64; 2]>, RunError> {
        Ok(self.steps.observe(frame.view())?)
    }
    fn portal(&mut self, frame: &Arc<BgrFrame>) -> Result<Option<[f64; 2]>, RunError> {
        Ok(self.portal.observe(frame.view())?.map(|p| p.map(f64::from)))
    }
    fn journal(&mut self, frame: &Arc<BgrFrame>) -> Result<Option<[f64; 2]>, RunError> {
        Ok(self.journal.observe(frame.view())?)
    }
    fn prepares_ground(&self) -> bool {
        true
    }
    fn prepare(
        &mut self,
        frame: &Arc<BgrFrame>,
        at: f64,
        hud: &HudReading,
        focused: bool,
    ) -> Result<(), RunError> {
        Ok(self
            .camera
            .with(|camera| camera.prepare_frame(frame, at, hud, focused))?)
    }
    fn ground(&mut self, frame: &Arc<BgrFrame>, at: f64) -> Result<Option<[f64; 2]>, RunError> {
        Ok(self
            .camera
            .with(|camera| camera.observe(frame, at))?
            .point
            .map(|p| p.map(f64::from)))
    }
    fn landing(
        &mut self,
        frame: &Arc<BgrFrame>,
        sequence: u64,
        at: f64,
        player: [f64; 2],
        journal: [f64; 2],
    ) -> Result<Option<[f64; 2]>, RunError> {
        let view = self.path.observe(
            frame.view(),
            PathContext {
                sequence,
                generation: 0,
                captured_at: at,
                frame_size: frame.size(),
                viewport: PixelRect::new(120, 180, 1720, 890).map_err(ObservationError::from)?,
                anchor: player,
            },
        )?;
        Ok(journal_landing(&self.path, &view, journal, player)?.map(|p| p.map(f64::from)))
    }
}

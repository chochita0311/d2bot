use super::{CalibratedCameraGroundObserver, GroundObserver};
use crate::common::{error::ObservationError, workers::OwnedObserver};
use crate::{common::frame::BgrFrame, runs::summoner::scene_vision::GroundReading};
use std::sync::Arc;
#[derive(Clone)]
pub struct CameraGroundHandle(OwnedObserver<CalibratedCameraGroundObserver>);
impl CameraGroundHandle {
    pub fn new(camera: CalibratedCameraGroundObserver) -> Self {
        Self(OwnedObserver::new(camera, "camera"))
    }
    pub fn with<T>(
        &self,
        call: impl FnOnce(&mut CalibratedCameraGroundObserver) -> Result<T, ObservationError>,
    ) -> Result<T, ObservationError> {
        self.0.with(call)
    }
}
impl GroundObserver for CameraGroundHandle {
    fn observe(
        &mut self,
        frame: &Arc<BgrFrame>,
        at: Option<f64>,
    ) -> Result<GroundReading, ObservationError> {
        let at =
            at.ok_or_else(|| ObservationError::invalid("current camera capture time required"))?;
        self.with(|camera| camera.observe(frame, at))
    }
}

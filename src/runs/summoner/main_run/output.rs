use super::*;
use crate::common::vision_ops::codec;
use std::{
    fs,
    path::{Component, Path, PathBuf},
};
pub trait RunOutput {
    fn report_path(&self) -> &Path;
    fn save_report(&mut self, report: &Value) -> Result<(), RunError>;
    fn save_frame(&mut self, evidence: &EncounterEvidence) -> Result<(), RunError>;
}
pub struct RunFiles {
    directory: PathBuf,
    report: PathBuf,
}
impl RunFiles {
    pub fn create(directory: impl AsRef<Path>) -> Result<Self, RunError> {
        let directory = directory.as_ref();
        if let Some(parent) = directory.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir(directory)?;
        Ok(Self {
            directory: directory.into(),
            report: directory.join("report.json"),
        })
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
}
impl RunOutput for RunFiles {
    fn report_path(&self) -> &Path {
        &self.report
    }
    fn save_report(&mut self, report: &Value) -> Result<(), RunError> {
        let pending = self.directory.join("report.json.pending");
        fs::write(&pending, serde_json::to_vec_pretty(report)?)?;
        fs::rename(pending, &self.report)?;
        Ok(())
    }
    fn save_frame(&mut self, evidence: &EncounterEvidence) -> Result<(), RunError> {
        save_frame_file(&self.directory, evidence)
    }
}
pub(super) fn save_frame_file(
    directory: &Path,
    evidence: &EncounterEvidence,
) -> Result<(), RunError> {
    let mut parts = Path::new(&evidence.filename).components();
    if !matches!(parts.next(), Some(Component::Normal(_))) || parts.next().is_some() {
        return Err(RunError::Runtime(
            "evidence filename must remain in its run directory".into(),
        ));
    }
    codec::save_png(directory.join(&evidence.filename), &evidence.frame)
        .map_err(|e| crate::common::error::ObservationError::from(e).into())
}

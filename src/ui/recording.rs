//! 녹화 구현의 소유권만 제어판에 노출하고 코덱은 녹화 모듈에 남긴다.
use crate::{actions::ActionEvent, common::config::BotConfig, runs::RunError};
use std::{path::PathBuf, time::Duration};
pub trait RecordingPort: 'static {
    fn configure(&mut self, config: &BotConfig) -> Result<(), RunError>;
    fn start(&mut self) -> Result<PathBuf, RunError>;
    fn request_stop(&self);
    /// 실제 캡처·파일 마감이 끝날 때까지 true다.
    fn is_running(&self) -> bool;
    fn drain_events(&self) -> Vec<ActionEvent>;
    /// GUI는 is_running=false인 소유자만 ZERO로 회수한다.
    fn finish(&mut self, timeout: Duration) -> Result<(), RunError>;
}
impl RecordingPort for crate::actions::recording::RecordingSession {
    fn configure(&mut self, config: &BotConfig) -> Result<(), RunError> {
        self.configure(config)
    }
    fn start(&mut self) -> Result<PathBuf, RunError> {
        self.start()
    }
    fn request_stop(&self) {
        self.request_stop();
    }
    fn is_running(&self) -> bool {
        self.is_running()
    }
    fn drain_events(&self) -> Vec<ActionEvent> {
        self.drain_events()
    }
    fn finish(&mut self, timeout: Duration) -> Result<(), RunError> {
        self.finish(timeout)
    }
}

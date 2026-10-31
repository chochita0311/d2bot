//! 현재 Summoner 준비·이동·전투·마감의 순서와 보고서 소유.
mod cycles;
mod engine;
pub mod entry;
mod evidence_stream;
pub mod native;
mod output;
use super::{
    encounter_execution::EncounterEvidence,
    navigation::Wing,
    options::{SelectedRun, SummonerOptions},
};
use crate::{actions::town_loop::Destination, common::error::ControlError, runs::RunError};
pub use cycles::{RoomRunner, execute_cycles};
pub use engine::execute_prepared;
pub use output::{RunFiles, RunOutput};
use serde_json::Value;
pub struct InitialField {
    pub proof: Value,
    pub buff_expires_at: f64,
}
pub struct NavigationInterval<'a> {
    pub evidence_prefix: &'a str,
    pub options: &'a SummonerOptions,
    pub selected: &'a SelectedRun,
    pub proof: &'a Value,
    pub wing: Wing,
    pub return_from: Option<Wing>,
    pub buff_expires_at: f64,
    pub continue_current: bool,
    pub transition: Option<&'a Value>,
}
/// 영상·입력·버프 소유자는 구현체에 남고, 이 경계는 원래 함수 단위 호출을 유지한다.
pub trait SummonerRuntime {
    fn clock(&self) -> f64;
    fn stopped(&self) -> bool;
    fn stop(&self);
    fn focused(&self) -> Result<bool, ControlError>;
    fn has_capture(&self) -> bool;
    fn has_backend(&self) -> bool;
    fn input_events(&self) -> Result<Value, RunError>;
    /// Python 보고서가 공유하던 변경 가능한 목록을 쓰기 직전에 값으로 반영한다.
    fn snapshot(&mut self, _report: &mut Value) -> Result<(), RunError> {
        Ok(())
    }
    fn initialize(
        &mut self,
        options: &SummonerOptions,
        selected: &SelectedRun,
        report: &mut Value,
        evidence: &mut Vec<EncounterEvidence>,
    ) -> Result<InitialField, RunError>;
    fn observe_runtime(&mut self, evidence: &mut Vec<EncounterEvidence>)
    -> Result<Value, RunError>;
    fn navigate(
        &mut self,
        interval: NavigationInterval<'_>,
        progress: &mut Value,
        evidence: &mut Vec<EncounterEvidence>,
    ) -> Result<Value, RunError>;
    fn renew(
        &mut self,
        report: &mut Value,
        evidence: &mut Vec<EncounterEvidence>,
    ) -> Result<Value, RunError>;
    fn reset_route_memory(&mut self) -> Result<(), RunError>;
    fn encounter(
        &mut self,
        buff_expires_at: f64,
        evidence: &mut Vec<EncounterEvidence>,
    ) -> Result<Value, RunError>;
    fn enter_canyon(
        &mut self,
        buff_expires_at: f64,
        evidence: &mut Vec<EncounterEvidence>,
    ) -> Result<Value, RunError>;
    fn finish_room(
        &mut self,
        destination: Destination,
        evidence: &mut Vec<EncounterEvidence>,
    ) -> Result<Value, RunError>;
    fn pause(
        &mut self,
        report: &mut Value,
        buff_expires_at: &mut Option<f64>,
        evidence: &mut Vec<EncounterEvidence>,
    ) -> Result<(), RunError>;
    fn safe_exit(
        &mut self,
        diagnostics: &mut Value,
        evidence: &mut Vec<EncounterEvidence>,
    ) -> Result<(), RunError>;
    /// 이미지 쓰기 전에 입력 해제를 포함한 모든 정리를 시도하고 실제 오류를 돌려준다.
    fn cleanup(&mut self, report: &mut Value, evidence: &mut Vec<EncounterEvidence>)
    -> Vec<String>;
}

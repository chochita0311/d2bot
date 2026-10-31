use super::*;
use crate::common::config::ConfigError;
use serde::Serialize;
use serde_json::json;
use std::path::Path;
#[cfg(test)]
mod tests;
pub trait RoomRunner {
    fn stopped(&self) -> bool;
    fn run_once(&mut self, options: &SummonerOptions) -> Result<i32, RunError>;
    fn read_report(&mut self, path: &Path) -> Result<Value, RunError>;
}
#[derive(Serialize)]
struct Summary {
    status: &'static str,
    requested: u8,
    completed: u8,
    recovery_closed: bool,
    rooms: Vec<Value>,
    acceptance_rooms: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}
/// 호출자는 live 부모 디렉터리를 새로 소유한다. 초기 복귀 방은 새 방 완료 수에서 제외한다.
pub fn execute_cycles(
    runner: &mut impl RoomRunner,
    output: &mut impl RunOutput,
    options: &SummonerOptions,
    cycles: u8,
    mut emit: impl FnMut(&str),
) -> Result<i32, RunError> {
    options.validate_cycles(cycles)?;
    if !options.live {
        emit(&format!(
            "DRY: north end -> central waypoint -> Act 1 -> save/exit -> new room; {cycles} north room(s)"
        ));
        return Ok(0);
    }
    if !options.automatic {
        return Err(
            ConfigError::value("normal Summoner runs use automatic screen observations").into(),
        );
    }
    let mut summary = Summary {
        status: "running",
        requested: cycles,
        completed: 0,
        recovery_closed: false,
        rooms: vec![],
        acceptance_rooms: 0,
        error: None,
    };
    let recovery = options.resume_arcane || options.return_from.is_some();
    let result = (|| {
        for index in 0..cycles + u8::from(recovery) {
            if runner.stopped() {
                summary.status = "interrupted";
                return Ok(2);
            }
            let recovering = recovery && index == 0;
            let room = if recovering {
                "initial-return".to_owned()
            } else {
                format!("room-{:02}", summary.completed + 1)
            };
            let mut current = options.clone();
            current.output = options.output.join(&room);
            current.finish_room = true;
            if index > 0 {
                current.resume_arcane = false;
                current.resume_town = false;
                current.return_from = None;
                current.continue_navigation = None;
                current.resume_labels_off = false;
            }
            let result = runner.run_once(&current)?;
            let report = runner.read_report(&current.output.join("report.json"))?;
            let status = report
                .get("status")
                .ok_or_else(|| ConfigError::new("KeyError", "'status'"))?
                .clone();
            summary.rooms.push(json!({"report":format!("{room}/report.json"),"status":status,"recovery":recovering}));
            if result != 0 || status != "north_room_closed" {
                summary.status = "needs_navigation_or_encounter_review";
                return Ok(2);
            }
            if recovering {
                summary.recovery_closed = true;
            } else {
                summary.completed += 1;
            }
            emit(&format!(
                "NORTH_ROOM_CYCLES: {}/{} new-room tests closed through Act 1",
                summary.completed, cycles
            ));
        }
        summary.status = "north_room_tests_finished";
        Ok(0)
    })();
    if let Err(error) = &result {
        summary.status = "failed";
        summary.error = Some(super::engine::failure_message(error));
    }
    let saved = output.save_report(&serde_json::to_value(&summary)?);
    match (result, saved) {
        (Ok(v), Ok(())) => Ok(v),
        (Err(e), Ok(())) | (Ok(_), Err(e)) => Err(e),
        (Err(e), Err(writing)) => Err(e.with_cleanup(writing)),
    }
}

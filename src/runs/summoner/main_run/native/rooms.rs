use super::{RuntimeContext, execute_once};
use crate::runs::{
    RunError,
    summoner::{
        main_run::{RoomRunner, RunFiles, execute_cycles},
        options::SummonerOptions,
    },
};
use serde_json::Value;
use std::path::Path;
struct WindowsRooms {
    context: RuntimeContext,
}
impl RoomRunner for WindowsRooms {
    fn stopped(&self) -> bool {
        self.context
            .stop
            .as_ref()
            .is_some_and(|stop| stop.is_cancelled())
    }
    fn run_once(&mut self, options: &SummonerOptions) -> Result<i32, RunError> {
        execute_once(self.context.clone(), options)
    }
    fn read_report(&mut self, path: &Path) -> Result<Value, RunError> {
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }
}
/// 반복 방도 같은 전체 중단 신호와 시계를 사용하며 각 방의 가변 상태는 새로 소유한다.
pub fn execute(
    context: RuntimeContext,
    options: &SummonerOptions,
    north_cycles: Option<u8>,
) -> Result<i32, RunError> {
    let Some(cycles) = north_cycles else {
        return execute_once(context, options);
    };
    options.validate_cycles(cycles)?;
    if !options.live {
        (context.emit)(&format!(
            "DRY: north end -> central waypoint -> Act 1 -> save/exit -> new room; {cycles} north room(s)"
        ));
        return Ok(0);
    }
    let emit = context.emit.clone();
    let mut output = RunFiles::create(options.output.clone())?;
    let mut runner = WindowsRooms { context };
    execute_cycles(&mut runner, &mut output, options, cycles, |message| {
        emit(message)
    })
}

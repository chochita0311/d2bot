//! GUI는 취소를 요청하고 결과를 읽는다. 실행과 정리는 한 작업 스레드가 끝까지 소유한다.
#[cfg(test)]
mod tests;
use super::options::{AutomapPreference, SummonerOptions};
use crate::{
    actions::town_loop::Difficulty,
    common::{
        config::{BotConfig, ConfigError},
        time::{MonotonicClock, SystemClock},
        workers::{Cancellation, WorkerGroup},
    },
    runs::RunError,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
#[derive(Debug, Clone, Serialize)]
pub struct SummonerEvent {
    pub level: String,
    pub message: String,
    pub payload: Option<Value>,
    pub priority: u8,
}
#[derive(Default)]
struct EventBuffer {
    events: VecDeque<SummonerEvent>,
    dropped: usize,
}
#[derive(Clone, Default)]
pub struct SummonerEvents(Arc<Mutex<EventBuffer>>);
impl SummonerEvents {
    fn push(&self, level: &str, message: impl Into<String>, payload: Option<Value>) {
        let mut buffer = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if buffer.events.len() == 256 {
            buffer.events.pop_front();
            buffer.dropped = buffer.dropped.saturating_add(1);
        }
        buffer.events.push_back(SummonerEvent {
            level: level.into(),
            message: message.into(),
            payload,
            priority: 1,
        });
    }
    pub fn log(&self, message: &str) {
        self.push("info", message, None);
    }
    pub fn drain(&self) -> Vec<SummonerEvent> {
        let mut buffer = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut events = vec![];
        if buffer.dropped > 0 {
            events.push(SummonerEvent {
                level: "warning".into(),
                message: format!(
                    "Summoner log omitted {} older event(s); the latest 256 were retained.",
                    buffer.dropped
                ),
                payload: None,
                priority: 1,
            });
            buffer.dropped = 0;
        }
        events.extend(buffer.events.drain(..));
        events
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SummonerResult {
    Stopped,
    Complete,
    Incomplete,
    Failed,
}
#[derive(Debug, Clone)]
pub struct SummonerStart {
    pub repeat_count: Option<u8>,
    pub difficulty: Difficulty,
    pub character_key: Option<String>,
}
impl Default for SummonerStart {
    fn default() -> Self {
        Self {
            repeat_count: None,
            difficulty: Difficulty::Hell,
            character_key: None,
        }
    }
}
pub struct SummonerWork {
    pub config: BotConfig,
    pub options: SummonerOptions,
    pub north_cycles: u8,
    pub stop: Cancellation,
    pub emit: Arc<dyn Fn(&str) + Send + Sync>,
}
pub struct SummonerGuiSession {
    config: BotConfig,
    worker: Option<WorkerGroup>,
    stop: Cancellation,
    events: SummonerEvents,
    output_path: Option<PathBuf>,
    result: Arc<Mutex<Option<SummonerResult>>>,
}
impl SummonerGuiSession {
    pub fn new(config: BotConfig) -> Self {
        Self {
            config,
            worker: None,
            stop: Cancellation::default(),
            events: SummonerEvents::default(),
            output_path: None,
            result: Arc::new(Mutex::new(None)),
        }
    }
    pub fn is_running(&self) -> bool {
        self.worker.as_ref().is_some_and(|w| !w.is_finished())
    }
    pub fn events(&self) -> SummonerEvents {
        self.events.clone()
    }
    pub fn output_path(&self) -> Option<&std::path::Path> {
        self.output_path.as_deref()
    }
    pub fn update_config(&mut self, config: BotConfig) -> Result<(), RunError> {
        if self.is_running() {
            return Err(RunError::Runtime(
                "Stop the Summoner run before changing its configuration.".into(),
            ));
        }
        self.config = config;
        Ok(())
    }
    fn validate(&self, request: &SummonerStart) -> Result<(u8, String), RunError> {
        if self.is_running() {
            return Err(RunError::Runtime("Summoner run is already running.".into()));
        }
        let count = request.repeat_count.unwrap_or(1);
        if !(1..=5).contains(&count) {
            return Err(ConfigError::value("Summoner Repeat Count must be from 1 to 5; leave it blank for 1 fresh north attempt.").into());
        }
        let character = request
            .character_key
            .as_ref()
            .filter(|key| {
                self.config
                    .section("characters")
                    .and_then(|v| v.get(*key))
                    .is_some()
            })
            .ok_or_else(|| {
                ConfigError::value(
                    "Select a configured character before starting the Summoner run.",
                )
            })?;
        Ok((count, character.clone()))
    }
    /// 모의 실행도 같은 시작·완료·중단 경계를 사용한다. 경로 생성은 검증 뒤에만 호출한다.
    pub fn start_with(
        &mut self,
        request: SummonerStart,
        output: impl FnOnce() -> Result<PathBuf, RunError>,
        work: impl FnOnce(SummonerWork) -> Result<i32, RunError> + Send + 'static,
    ) -> Result<PathBuf, RunError> {
        let (count, character) = self.validate(&request)?;
        if let Some(previous) = &mut self.worker {
            previous.finish(Duration::ZERO)?;
        }
        let output = output()?;
        let stop = Cancellation::default();
        let result = Arc::new(Mutex::new(None));
        self.stop = stop.clone();
        self.result = result.clone();
        self.output_path = Some(output.clone());
        let events = self.events.clone();
        events.push("info",format!("Summoner north run started: {count} fresh attempt(s), {}, character {character}. The matching character must already be selected in the game. Press F10 or Stop Action to stop.",request.difficulty.name()),Some(json!({"status":"running","output":output})));
        let mut options = SummonerOptions::new(output.clone(), request.difficulty);
        options.live = true;
        options.continuous = true;
        options.field_seconds = 60;
        options.segments = 4;
        options.character_key = Some(character);
        options.automap = AutomapPreference::On;
        let logs = events.clone();
        let packet = SummonerWork {
            config: self.config.clone(),
            options,
            north_cycles: count,
            stop: stop.clone(),
            emit: Arc::new(move |message| logs.log(message)),
        };
        let mut worker = WorkerGroup::new(stop, 1);
        let error_events = self.events.clone();
        let failed_output = output.clone();
        let failed_result = self.result.clone();
        if let Err(error) = worker.spawn("summoner-gui", move |token| {
            let outcome = if token.is_cancelled() {
                Ok(2)
            } else {
                (|| {
                    let _compute = crate::common::compute::ComputeLease::acquire()?;
                    work(packet)
                })()
            };
            let (status, level, message, exit_code) = match outcome {
                Err(error) => (
                    SummonerResult::Failed,
                    "error",
                    format!("Summoner run failed: {error}"),
                    None,
                ),
                Ok(code) if token.is_cancelled() => (
                    SummonerResult::Stopped,
                    "info",
                    "Summoner run stopped; unfinished attempts do not count as success.".into(),
                    Some(code),
                ),
                Ok(0) => (
                    SummonerResult::Complete,
                    "info",
                    "Summoner north attempts completed. Full Summoner acceptance remains separate."
                        .into(),
                    Some(0),
                ),
                Ok(code) => (
                    SummonerResult::Incomplete,
                    "warning",
                    "Summoner run incomplete; review its evidence before retrying.".into(),
                    Some(code),
                ),
            };
            *result.lock().unwrap_or_else(|e| e.into_inner()) = Some(status);
            let mut payload = json!({"status":status,"output":output});
            if let Some(code) = exit_code {
                payload["exit_code"] = json!(code);
            }
            events.push(level, message, Some(payload));
            Ok(())
        }) {
            *failed_result.lock().unwrap_or_else(|e| e.into_inner()) = Some(SummonerResult::Failed);
            error_events.push(
                "error",
                format!("Summoner run failed: {error}"),
                Some(json!({"status":"failed","output":failed_output})),
            );
            return Err(error.into());
        }
        self.worker = Some(worker);
        self.output_path
            .clone()
            .ok_or_else(|| RunError::Runtime("Summoner output unavailable".into()))
    }
    #[cfg(windows)]
    pub fn start(&mut self, root: PathBuf, request: SummonerStart) -> Result<PathBuf, RunError> {
        self.start_with(
            request,
            || {
                use rand::{Rng, SeedableRng, rngs::StdRng};
                let mut rng = StdRng::try_from_os_rng().map_err(|e| {
                    RunError::Runtime(format!("Summoner output random initialization failed: {e}"))
                })?;
                let suffix = rng
                    .random::<[u8; 5]>()
                    .iter()
                    .map(|v| format!("{v:02x}"))
                    .collect::<String>();
                Ok(PathBuf::from("recordings/summoner/evidence").join(format!(
                    "gui-{}-{suffix}",
                    crate::platform::windows::local_timestamp()
                )))
            },
            move |packet| {
                let clock: Arc<dyn MonotonicClock> = Arc::new(SystemClock::default());
                let context = super::main_run::native::RuntimeContext {
                    root,
                    config: packet.config,
                    clock,
                    stop: Some(packet.stop),
                    emit: packet.emit,
                    entry_directory: None,
                };
                super::main_run::native::execute(
                    context,
                    &packet.options,
                    Some(packet.north_cycles),
                )
            },
        )
    }
    pub fn request_stop(&self) {
        self.stop.cancel();
    }
    pub fn stop(&self) {
        self.request_stop();
    }
    pub fn close(&self) {
        self.request_stop();
    }
    pub fn result(&self) -> Result<Option<SummonerResult>, RunError> {
        let result = *self.result.lock().unwrap_or_else(|e| e.into_inner());
        if result.is_none()
            && let Some(worker) = self.worker.as_ref().filter(|w| w.is_finished())
        {
            let faults = worker.faults()?;
            let message = faults
                .iter()
                .map(|v| v.message.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            self.events.push(
                "error",
                format!("Summoner run failed: {message}"),
                Some(json!({"status":"failed","output":self.output_path})),
            );
            *self.result.lock().unwrap_or_else(|e| e.into_inner()) = Some(SummonerResult::Failed);
            return Ok(Some(SummonerResult::Failed));
        }
        Ok(result)
    }
    /// GUI 정지 요청과 달리 종료 소유자가 정리 완료를 확인하는 명시적 경계다.
    pub fn shutdown(&mut self, timeout: Duration) -> Result<(), RunError> {
        self.request_stop();
        if let Some(worker) = &mut self.worker {
            worker.finish(timeout)?;
        }
        self.result()?;
        Ok(())
    }
}

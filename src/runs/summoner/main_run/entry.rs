//! 기존 마을 진입 세션의 시작·이벤트·결과를 기다린 뒤 소유한 작업을 정리한다.
use crate::{
    actions::{
        ActionError, ActionEvent, EventLevel,
        run_lifecycle::{self, TownOutcome},
        runtime::{ActionBackend, ActionContext, ActionEvents, PointerStyle},
        session::{ActionSession, SessionOutcome, SessionStatus},
        town_loop::{Destination, Difficulty, TownOptions, TownVision},
    },
    common::{
        error::ControlError,
        workers::{Cancellation, CooperativeStop},
    },
    runs::RunError,
};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
pub trait EntrySession {
    fn start(&mut self, difficulty: Difficulty, resume_town: bool) -> Result<(), RunError>;
    fn running(&mut self) -> Result<bool, RunError>;
    fn has_events(&mut self) -> bool;
    fn next_event(&mut self, timeout: Duration) -> Result<Option<ActionEvent>, RunError>;
    fn request_stop(&self);
    fn last_result(&self) -> Result<Option<TownOutcome>, RunError>;
    fn finish(&mut self, timeout: Duration) -> Result<(), RunError>;
}
pub fn enter_arcane(
    session: &mut impl EntrySession,
    difficulty: Difficulty,
    resume_town: bool,
    stop: &CooperativeStop,
    mut emit: impl FnMut(&str),
) -> Result<(), RunError> {
    if stop.is_set() {
        return Err(RunError::Runtime("Arcane entry interrupted".into()));
    }
    session.start(difficulty, resume_town)?;
    let result = (|| {
        while session.running()? || session.has_events() {
            if stop.is_set() {
                session.request_stop();
            }
            if let Some(event) = session.next_event(Duration::from_millis(200))? {
                let level = match event.level {
                    EventLevel::Info => "info",
                    EventLevel::Warning => "warning",
                    EventLevel::Error => "error",
                };
                emit(&format!("{level}: {}", event.message));
            }
        }
        let result = session.last_result()?;
        if result != Some(TownOutcome::Passed) {
            let result = match result {
                None => "None",
                Some(TownOutcome::Stopped) => "stopped",
                Some(TownOutcome::Failed) => "failed",
                Some(TownOutcome::Passed) => "passed",
            };
            return Err(RunError::Runtime(format!(
                "Arcane entry did not pass: {result}"
            )));
        }
        Ok(())
    })();
    let cleanup = session.finish(Duration::from_secs(2));
    match (result, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(e), Ok(())) | (Ok(()), Err(e)) => Err(e),
        (Err(e), Err(c)) => Err(e.with_cleanup(c)),
    }
}
type Factory<B> = Box<dyn FnOnce(Cancellation) -> Result<B, ActionError> + Send>;
/// 작업을 빌리므로 join 지연에도 상위 실행기가 ActionSession 소유권을 유지한다.
pub struct ActionEntry<'a, B: ActionBackend> {
    session: &'a mut ActionSession,
    vision: Option<TownVision>,
    directory: PathBuf,
    factory: Option<Factory<B>>,
    result: Arc<Mutex<Option<TownOutcome>>>,
    events: ActionEvents,
    pending: VecDeque<ActionEvent>,
}
impl<'a, B: ActionBackend + 'static> ActionEntry<'a, B> {
    pub fn new(
        session: &'a mut ActionSession,
        vision: TownVision,
        directory: PathBuf,
        factory: impl FnOnce(Cancellation) -> Result<B, ActionError> + Send + 'static,
    ) -> Self {
        let events = session.events();
        Self {
            session,
            vision: Some(vision),
            directory,
            factory: Some(Box::new(factory)),
            result: Arc::new(Mutex::new(None)),
            events,
            pending: VecDeque::new(),
        }
    }
    fn collect_events(&mut self) {
        self.pending.extend(self.events.drain());
    }
}
impl<B: ActionBackend + 'static> EntrySession for ActionEntry<'_, B> {
    fn start(&mut self, difficulty: Difficulty, resume_town: bool) -> Result<(), RunError> {
        let mut vision = self
            .vision
            .take()
            .ok_or_else(|| RunError::Runtime("Arcane entry already started".into()))?;
        let factory = self
            .factory
            .take()
            .ok_or_else(|| RunError::Runtime("Arcane entry factory unavailable".into()))?;
        let directory = self.directory.clone();
        let result = self.result.clone();
        let options = TownOptions {
            repeat_count: 1,
            difficulty,
            destination: Destination::Arcane,
            arrival_only: true,
            reset_act2_start: difficulty == Difficulty::Hell,
            ..Default::default()
        };
        // 실제 작업 전 보정 누락을 거부하는 기존 start_town 경계를 유지한다.
        crate::actions::town_loop::TownPlanner::new(options, |name| vision.calibrated(name))?;
        self.session.start(move |token, events| {
            let work = (|| {
                let backend = match factory(token.clone()) {
                    Ok(backend) => backend,
                    Err(error) => {
                        events.push(ActionEvent {
                            level: EventLevel::Error,
                            message: format!("Run lifecycle failed: {error}"),
                        });
                        return Err(error);
                    }
                };
                let mut context = ActionContext::new(backend, token, PointerStyle::Lifecycle)
                    .with_events(events.clone());
                run_lifecycle::run_town(&mut context, &mut vision, options, resume_town, &directory)
            })();
            let outcome = work
                .as_ref()
                .map(|r| r.result)
                .unwrap_or(TownOutcome::Failed);
            *result
                .lock()
                .map_err(|_| ActionError::runtime("Arcane entry result poisoned"))? = Some(outcome);
            work.map(|report| {
                if report.result == TownOutcome::Passed {
                    SessionOutcome::Completed
                } else {
                    SessionOutcome::Interrupted
                }
            })
        })?;
        let label = match difficulty {
            Difficulty::Normal => "Normal",
            Difficulty::Nightmare => "Nightmare",
            Difficulty::Hell => "Hell",
        };
        self.events.push(ActionEvent {
            level: EventLevel::Info,
            message: format!("Run lifecycle started for 1 run(s) on {label}. Press F10 to stop."),
        });
        Ok(())
    }
    fn running(&mut self) -> Result<bool, RunError> {
        Ok(matches!(
            self.session.status()?,
            SessionStatus::Running | SessionStatus::Stopping
        ))
    }
    fn has_events(&mut self) -> bool {
        self.collect_events();
        !self.pending.is_empty()
    }
    fn next_event(&mut self, timeout: Duration) -> Result<Option<ActionEvent>, RunError> {
        let began = Instant::now();
        loop {
            self.collect_events();
            if let Some(event) = self.pending.pop_front() {
                return Ok(Some(event));
            }
            let remaining = timeout.saturating_sub(began.elapsed());
            if remaining.is_zero() {
                return Ok(None);
            }
            std::thread::sleep(remaining.min(Duration::from_millis(20)));
        }
    }
    fn request_stop(&self) {
        self.session.request_stop();
    }
    fn last_result(&self) -> Result<Option<TownOutcome>, RunError> {
        let result = *self
            .result
            .lock()
            .map_err(|_| ControlError::Runtime("Arcane entry result poisoned".into()))?;
        if result.is_none() && matches!(self.session.status()?, SessionStatus::Finished(Err(_))) {
            return Ok(Some(TownOutcome::Failed));
        }
        Ok(result)
    }
    fn finish(&mut self, timeout: Duration) -> Result<(), RunError> {
        Ok(self.session.stop(timeout)?)
    }
}
#[cfg(test)]
mod tests;

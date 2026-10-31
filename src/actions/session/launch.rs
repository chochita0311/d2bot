use super::*;
use crate::actions::{
    gem_summing::{self, GemOutcome, GemVision},
    loot_pickup::{self, LootScanner},
    run_lifecycle::{self, RoomTemplates},
    runtime::{ActionBackend, ActionContext, PointerStyle},
    town_loop::{Difficulty, TownOptions, TownPlanner, TownVision},
};
use std::path::PathBuf;

impl ActionSession {
    fn start_with_backend<B: ActionBackend + 'static>(
        &mut self,
        style: PointerStyle,
        factory: impl FnOnce(Cancellation) -> Result<B, ActionError> + Send + 'static,
        work: impl FnOnce(&mut ActionContext<B>) -> Result<SessionOutcome, ActionError> + Send + 'static,
    ) -> Result<(), ActionError> {
        let name = self.name.clone();
        self.start(move |token, events| {
            let backend = match factory(token.clone()) {
                Ok(backend) => backend,
                Err(error) => {
                    events.push(ActionEvent {
                        level: EventLevel::Error,
                        message: format!("{name} failed: {error}"),
                    });
                    return Err(error);
                }
            };
            let mut context = ActionContext::new(backend, token, style).with_events(events);
            work(&mut context)
        })
    }
    pub fn start_gems<B: ActionBackend + 'static>(
        &mut self,
        vision: Arc<GemVision>,
        factory: impl FnOnce(Cancellation) -> Result<B, ActionError> + Send + 'static,
    ) -> Result<(), ActionError> {
        self.start_with_backend(PointerStyle::Gems, factory, move |context| {
            gem_summing::run_session(vision.as_ref(), context).map(|outcome| match outcome {
                GemOutcome::Completed => SessionOutcome::Completed,
                GemOutcome::Interrupted => SessionOutcome::Interrupted,
            })
        })?;
        self.events.push(ActionEvent {
            level: EventLevel::Info,
            message: "Gem summing started. Press F10 to stop.".into(),
        });
        Ok(())
    }
    pub fn start_loot<B: ActionBackend + 'static>(
        &mut self,
        mut scanner: LootScanner,
        factory: impl FnOnce(Cancellation) -> Result<B, ActionError> + Send + 'static,
    ) -> Result<(), ActionError> {
        self.start_with_backend(PointerStyle::Loot, factory, move |context| {
            loot_pickup::run_session(&mut scanner, context)?;
            Ok(if context.cancelled() {
                SessionOutcome::Interrupted
            } else {
                SessionOutcome::Completed
            })
        })?;
        self.events.push(ActionEvent {
            level: EventLevel::Info,
            message: "Loot pickup started. Press F10 to stop.".into(),
        });
        Ok(())
    }
    pub fn start_rooms<B: ActionBackend + 'static>(
        &mut self,
        vision: TownVision,
        templates: Arc<RoomTemplates>,
        repeat: Option<u64>,
        difficulty: Difficulty,
        factory: impl FnOnce(Cancellation) -> Result<B, ActionError> + Send + 'static,
    ) -> Result<(), ActionError> {
        self.start_rooms_exact(
            vision,
            templates,
            repeat.map(serde_json::Number::from),
            difficulty,
            factory,
        )
    }
    pub fn start_rooms_exact<B: ActionBackend + 'static>(
        &mut self,
        mut vision: TownVision,
        templates: Arc<RoomTemplates>,
        repeat: Option<serde_json::Number>,
        difficulty: Difficulty,
        factory: impl FnOnce(Cancellation) -> Result<B, ActionError> + Send + 'static,
    ) -> Result<(), ActionError> {
        let started_count = repeat.as_ref().map(ToString::to_string);
        self.start_with_backend(PointerStyle::Lifecycle, factory, move |context| {
            run_lifecycle::run_rooms_exact(
                context,
                &mut vision,
                templates.as_ref(),
                repeat,
                difficulty,
            )
        })?;
        self.lifecycle_started_text(started_count, difficulty);
        Ok(())
    }
    pub fn start_town<B: ActionBackend + 'static>(
        &mut self,
        mut vision: TownVision,
        options: TownOptions,
        resume: bool,
        directory: PathBuf,
        factory: impl FnOnce(Cancellation) -> Result<B, ActionError> + Send + 'static,
    ) -> Result<(), ActionError> {
        TownPlanner::new(options, |name| vision.calibrated(name))?;
        self.start_with_backend(PointerStyle::Lifecycle, factory, move |context| {
            run_lifecycle::run_town(context, &mut vision, options, resume, &directory).map(
                |report| {
                    if report.result == run_lifecycle::TownOutcome::Passed {
                        SessionOutcome::Completed
                    } else {
                        SessionOutcome::Interrupted
                    }
                },
            )
        })?;
        self.lifecycle_started(Some(u64::from(options.repeat_count)), options.difficulty);
        Ok(())
    }
    fn lifecycle_started(&self, repeat: Option<u64>, difficulty: Difficulty) {
        self.lifecycle_started_text(repeat.map(|v| v.to_string()), difficulty);
    }
    fn lifecycle_started_text(&self, repeat: Option<String>, difficulty: Difficulty) {
        let count = repeat.map_or_else(|| "until stopped".into(), |n| format!("for {n} run(s)"));
        let label = match difficulty {
            Difficulty::Normal => "Normal",
            Difficulty::Nightmare => "Nightmare",
            Difficulty::Hell => "Hell",
        };
        self.events.push(ActionEvent {
            level: EventLevel::Info,
            message: format!("Run lifecycle started {count} on {label}. Press F10 to stop."),
        });
    }
}

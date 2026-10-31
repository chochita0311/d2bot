//! 실제 세션의 최종 소유자. UI는 의도만 전달하고 작업 중에는 취소·이벤트만 다룬다.
#[cfg(all(test, windows))]
mod tests;
use super::{
    model::{Activity, EventSource, PanelState},
    recording::RecordingPort,
    snapshot::{SnapshotEffect, SnapshotSession},
    view::Intent,
};
use crate::{
    actions::{
        ActionError, ActionEvent, EventLevel,
        gem_summing::GemVision,
        loot_pickup::LootScanner,
        run_lifecycle::RoomTemplates,
        runtime::ActionBackend,
        session::{ActionSession, SessionStatus},
        town_loop::{Difficulty, TownOptions},
    },
    common::{capture::CaptureSettings, config::BotConfig},
    runs::{
        RunError,
        summoner::{
            SummonerGuiSession,
            gui_session::{SummonerEvent, SummonerStart},
        },
    },
};
use std::{path::PathBuf, sync::Arc, time::Duration};
pub type BackendFactory<B> =
    Arc<dyn Fn(CaptureSettings, PathBuf) -> Result<B, ActionError> + Send + Sync>;
pub type SummonerLauncher =
    Box<dyn Fn(&mut SummonerGuiSession, PathBuf, SummonerStart) -> Result<PathBuf, RunError>>;
pub struct PanelEffects<B: ActionBackend> {
    pub backend: BackendFactory<B>,
    pub summoner: SummonerLauncher,
    pub snapshot: SnapshotEffect,
    pub windows: Box<dyn Fn() -> Result<Vec<String>, RunError>>,
}
#[derive(Debug, Clone)]
pub struct UiFault {
    pub title: &'static str,
    pub message: String,
}
fn message(error: RunError) -> String {
    match error {
        RunError::Control(crate::common::error::ControlError::Configuration(error)) => {
            error.message().into()
        }
        RunError::Action(ActionError::Configuration(error)) => error.message().into(),
        error => error.to_string(),
    }
}
fn fault(title: &'static str, error: impl Into<RunError>) -> UiFault {
    UiFault {
        title,
        message: message(error.into()),
    }
}
fn event(event: ActionEvent) -> SummonerEvent {
    SummonerEvent {
        level: match event.level {
            EventLevel::Info => "info",
            EventLevel::Warning => "warning",
            EventLevel::Error => "error",
        }
        .into(),
        message: event.message,
        payload: None,
        priority: 1,
    }
}
pub struct PanelRuntime<B: ActionBackend + 'static, R: RecordingPort> {
    pub state: PanelState,
    config: BotConfig,
    root: PathBuf,
    effects: PanelEffects<B>,
    recorder: R,
    gems: ActionSession,
    loot: ActionSession,
    lifecycle: ActionSession,
    summoner: SummonerGuiSession,
    gem_vision: Arc<GemVision>,
    snapshot: SnapshotSession,
    recording_started: bool,
    recording_stop_requested: bool,
    joined: [bool; 4],
}
impl<B: ActionBackend + 'static, R: RecordingPort> PanelRuntime<B, R> {
    pub fn new(
        config: BotConfig,
        root: PathBuf,
        recorder: R,
        effects: PanelEffects<B>,
    ) -> Result<Self, RunError> {
        let state = PanelState::new(&config)?;
        let gem_vision = Arc::new(GemVision::load(&root)?);
        let summoner = SummonerGuiSession::new(config.clone());
        Ok(Self {
            state,
            config,
            root,
            effects,
            recorder,
            gems: ActionSession::new("Gem summing"),
            loot: ActionSession::new("Loot pickup"),
            lifecycle: ActionSession::new("Run lifecycle"),
            summoner,
            gem_vision,
            snapshot: SnapshotSession::default(),
            recording_started: false,
            recording_stop_requested: false,
            joined: [true; 4],
        })
    }
    pub fn config(&self) -> &BotConfig {
        &self.config
    }
    fn activity(&self) -> Activity {
        Activity {
            recording: self.recorder.is_running(),
            gem: self.gems.is_running(),
            loot: self.loot.is_running(),
            lifecycle: self.lifecycle.is_running(),
            summoner: self.summoner.is_running(),
            recording_cleanup: self.recording_stop_requested && self.recorder.is_running(),
        }
    }
    fn sync_activity(&mut self) {
        self.state.activity = self.activity();
    }
    fn apply_config(&mut self) -> Result<(), RunError> {
        let title = self.state.window_title.trim();
        self.config.set_capture_source(
            if title.is_empty() {
                None
            } else {
                Some(title.into())
            },
            self.state.backend.trim(),
        )?;
        let key = self.state.selected_character().map(str::to_owned);
        self.config.apply_character_selection(key.as_deref())?;
        self.recorder.configure(&self.config)?;
        self.summoner.update_config(self.config.clone())?;
        Ok(())
    }
    pub fn refresh_windows(&mut self, select_current: bool) -> Result<(), UiFault> {
        let titles = (self.effects.windows)().map_err(|e| fault("Window Error", e))?;
        self.state.refresh_windows(titles, select_current);
        Ok(())
    }
    pub fn intent(&mut self, intent: Intent) -> Result<(), UiFault> {
        self.sync_activity();
        let title = match intent {
            Intent::Record | Intent::StopRecording => "Recording Error",
            Intent::Snapshot => "Snapshot Error",
            Intent::Lifecycle => "Run Lifecycle Error",
            Intent::Gems => "Gem Summing Error",
            Intent::Loot => "Item Looting Error",
            Intent::Summoner => "Summoner Run Error",
            _ => "Action Error",
        };
        let result = self.apply_intent(intent);
        self.sync_activity();
        if let Err(error) = result {
            let fault = fault(title, error);
            self.state.append_log("error", &fault.message);
            return Err(fault);
        }
        if matches!(
            intent,
            Intent::Gems | Intent::Loot | Intent::Lifecycle | Intent::Summoner | Intent::StopAction
        ) {
            self.state.refresh_controls();
        }
        Ok(())
    }
    fn apply_intent(&mut self, intent: Intent) -> Result<(), RunError> {
        if self.state.closing && intent != Intent::StopAction {
            return Err(RunError::Runtime("The control panel is closing.".into()));
        }
        match intent {
            Intent::WindowChanged => {
                let title = self.state.window_title.trim();
                if !title.is_empty() {
                    self.state
                        .append_log("info", &format!("Selected window: {title}"));
                }
            }
            Intent::CharacterChanged => {
                if !self.state.activity.actions()
                    && let Some(key) = self.state.selected_character().map(str::to_owned)
                {
                    self.config.apply_character_selection(Some(&key))?;
                    self.summoner.update_config(self.config.clone())?;
                    self.state.append_log(
                        "info",
                        &format!("Selected character: {}", self.state.character.trim()),
                    );
                }
            }
            Intent::RefreshWindows => {
                let titles = (self.effects.windows)()?;
                self.state.refresh_windows(titles, false);
            }
            Intent::Record => {
                if !self.state.activity.actions() {
                    self.apply_config()?;
                }
                let output = self.recorder.start()?;
                self.recording_started = true;
                self.recording_stop_requested = false;
                self.state
                    .recording_started(output.to_string_lossy().into());
            }
            Intent::StopRecording => {
                self.recorder.request_stop();
                self.recording_stop_requested = true;
            }
            Intent::Snapshot => {
                if !self.state.activity.actions() {
                    self.apply_config()?;
                }
                let mut settings = CaptureSettings::from_config(&self.config)?;
                settings.reference_layout = None;
                self.snapshot
                    .start(settings, self.root.clone(), self.effects.snapshot.clone())?;
            }
            Intent::StopAction => {
                self.request_action_stop();
                self.state.stopped_action();
            }
            Intent::Gems | Intent::Loot | Intent::Lifecycle | Intent::Summoner => {
                self.state.ensure_action_available()?;
                self.apply_config()?;
                match intent {
                    Intent::Summoner => {
                        let repeat = self.state.repeat()?;
                        let repeat=match repeat{None=>None,Some(n)=>Some(n.as_u64().filter(|n|(1..=5).contains(n)).map(|v|v as u8).ok_or_else(||crate::common::config::ConfigError::value("Summoner Repeat Count must be from 1 to 5; leave it blank for 1 fresh north attempt."))?)};
                        let request = SummonerStart {
                            repeat_count: repeat,
                            difficulty: self.difficulty().ok_or_else(|| crate::common::config::ConfigError::value("Select a valid Summoner difficulty: normal, nightmare, or hell."))?,
                            character_key: self.state.selected_character().map(str::to_owned),
                        };
                        let output = (self.effects.summoner)(
                            &mut self.summoner,
                            self.root.clone(),
                            request,
                        )?;
                        self.joined[3] = false;
                        self.state.status = "Summoner Run".into();
                        self.state.output = output.to_string_lossy().into();
                    }
                    Intent::Gems => {
                        let (settings, root, factory) = self.backend_request()?;
                        self.gems.start_gems(self.gem_vision.clone(), move |_| {
                            factory(settings, root)
                        })?;
                        self.joined[0] = false;
                        self.state.status = "Gem Summing".into();
                    }
                    Intent::Loot => {
                        let scanner = LootScanner::new(&self.config, &self.root)?;
                        let (settings, root, factory) = self.backend_request()?;
                        self.loot
                            .start_loot(scanner, move |_| factory(settings, root))?;
                        self.joined[1] = false;
                        self.state.status = "Item Looting".into();
                    }
                    Intent::Lifecycle => {
                        self.start_lifecycle()?;
                        self.joined[2] = false;
                        self.state.status = "Run Lifecycle".into();
                    }
                    _ => (),
                }
            }
        }
        Ok(())
    }
    fn difficulty(&self) -> Option<Difficulty> {
        match self.state.difficulty.trim() {
            "normal" => Some(Difficulty::Normal),
            "nightmare" => Some(Difficulty::Nightmare),
            "hell" => Some(Difficulty::Hell),
            _ => None,
        }
    }
    fn backend_request(&self) -> Result<(CaptureSettings, PathBuf, BackendFactory<B>), RunError> {
        Ok((
            CaptureSettings::from_config(&self.config)?,
            self.root.clone(),
            self.effects.backend.clone(),
        ))
    }
    fn start_lifecycle(&mut self) -> Result<(), RunError> {
        let repeat = self.state.repeat()?;
        let (settings, root, factory) = self.backend_request()?;
        if self.state.town_loop {
            let count = repeat
                .as_ref()
                .and_then(serde_json::Number::as_u64)
                .filter(|n| (1..=3).contains(n))
                .ok_or_else(|| {
                    crate::common::config::ConfigError::value(
                        "Supervised town loop requires an explicit repeat count from 1 to 3.",
                    )
                })? as u8;
            let difficulty = self
                .difficulty()
                .ok_or_else(|| crate::common::config::ConfigError::value("Unknown difficulty."))?;
            let vision = crate::runs::summoner::town_vision(&self.root)?;
            let directory = town_directory();
            self.lifecycle.start_town(
                vision,
                TownOptions {
                    repeat_count: count,
                    difficulty,
                    ..Default::default()
                },
                false,
                directory,
                move |_| factory(settings, root),
            )?;
        } else {
            let difficulty = self.difficulty().unwrap_or(Difficulty::Hell);
            let vision = crate::runs::summoner::town_vision(&self.root)?;
            let templates = Arc::new(RoomTemplates::load(&self.root)?);
            self.lifecycle
                .start_rooms_exact(vision, templates, repeat, difficulty, move |_| {
                    factory(settings, root)
                })?;
        }
        Ok(())
    }
    fn request_action_stop(&self) {
        for session in [&self.gems, &self.loot, &self.lifecycle] {
            if session.is_running() {
                session.request_stop();
            }
        }
        if self.summoner.is_running() {
            self.summoner.request_stop();
        }
    }
    pub fn request_close(&mut self) {
        if !self.state.begin_close() {
            return;
        }
        for session in [&self.gems, &self.loot, &self.lifecycle] {
            session.request_stop();
        }
        self.summoner.request_stop();
        self.recorder.request_stop();
        self.recording_stop_requested = self.recorder.is_running();
        self.snapshot.request_stop();
        self.sync_activity();
    }
    pub fn poll(&mut self) -> Result<Vec<UiFault>, RunError> {
        let mut faults = vec![];
        // UI만 drain하고 작업 스레드는 소유 큐에 추가하므로 순서를 고정할 수 있다.
        for e in self.recorder.drain_events() {
            self.state.event(EventSource::Recording, &event(e));
        }
        for (source, session) in [
            (EventSource::Lifecycle, &self.lifecycle),
            (EventSource::Gem, &self.gems),
            (EventSource::Loot, &self.loot),
        ] {
            for e in session.events().drain() {
                self.state.event(source, &event(e));
            }
        }
        self.summoner.result()?;
        for e in self.summoner.events().drain() {
            self.state.event(EventSource::Summoner, &e);
        }
        for (index, source, session) in [
            (0, EventSource::Gem, &mut self.gems),
            (1, EventSource::Loot, &mut self.loot),
            (2, EventSource::Lifecycle, &mut self.lifecycle),
        ] {
            if !self.joined[index] && !session.is_running() {
                if let SessionStatus::Finished(Err(error)) = session.status()?
                    && error.contains("worker panicked:")
                {
                    self.state.event(
                        source,
                        &SummonerEvent {
                            level: "error".into(),
                            message: error,
                            payload: None,
                            priority: 1,
                        },
                    );
                }
                session.stop(Duration::ZERO)?;
                self.joined[index] = true;
            }
        }
        if !self.joined[3] && !self.summoner.is_running() {
            self.summoner.shutdown(Duration::ZERO)?;
            self.joined[3] = true;
        }
        if self.recording_started && !self.recorder.is_running() {
            self.recorder.finish(Duration::ZERO)?;
            self.recording_started = false;
            if self.recording_stop_requested {
                if !self.state.closing {
                    self.state.recording_stopped();
                }
                self.recording_stop_requested = false;
            }
        }
        if let Some(result) = self.snapshot.take_finished()? {
            match result {
                Ok(path) => {
                    self.state.output = path.to_string_lossy().into();
                    self.state.append_log(
                        "info",
                        &format!(
                            "Snapshot saved: {}",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        ),
                    );
                }
                Err(error) => {
                    self.state
                        .append_log("error", &format!("Snapshot failed: {error}"));
                    faults.push(UiFault {
                        title: "Snapshot Error",
                        message: error,
                    });
                }
            }
        }
        self.sync_activity();
        self.state.refresh_controls();
        Ok(faults)
    }
    pub fn ready_to_close(&self) -> bool {
        self.state.can_close()
            && !self.snapshot.has_pending_work()
            && self.joined.iter().all(|v| *v)
            && !self.recording_started
    }
}
fn town_directory() -> PathBuf {
    #[cfg(windows)]
    let timestamp = crate::platform::windows::local_timestamp();
    #[cfg(not(windows))]
    let timestamp = "unsupported-platform";
    PathBuf::from("recordings/summoner/evidence").join(format!("{timestamp}-town-loop"))
}

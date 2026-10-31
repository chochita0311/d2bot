//! 작업 스레드에 UI 참조를 넘기지 않는 선택·버튼·로그 상태.
#[cfg(test)]
mod tests;
use crate::{
    common::{
        config::{BotConfig, ConfigError},
        numeric_input,
        unicode_casefold::casefold,
    },
    runs::summoner::gui_session::SummonerEvent,
};
use serde::Serialize;
use std::collections::BTreeMap;
pub const TITLE: &str = "Diablo2 Helper";
pub const INITIAL_SIZE: [f32; 2] = [1120.0, 820.0];
pub const MINIMUM_SIZE: [f32; 2] = [1020.0, 800.0];
pub const TRIM_NOTICE: &str = "Older GUI messages were trimmed from this live view.";
const PREFERRED_GAME_PATTERNS: [&str; 4] = [
    "diablo ii: resurrected",
    "diablo ii",
    "resurrected",
    "diablo2",
];
#[derive(Debug, Default, Clone, Copy, Serialize)]
pub struct Activity {
    pub recording: bool,
    pub lifecycle: bool,
    pub gem: bool,
    pub loot: bool,
    pub summoner: bool,
    pub recording_cleanup: bool,
}
impl Activity {
    pub fn actions(self) -> bool {
        self.lifecycle || self.gem || self.loot || self.summoner
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventSource {
    Recording,
    Lifecycle,
    Gem,
    Loot,
    Summoner,
}
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Controls {
    pub actions: bool,
    pub stop_action: bool,
    pub selectors: bool,
    pub record: bool,
    pub stop_record: bool,
}
pub struct PanelState {
    pub window_title: String,
    pub window_titles: Vec<String>,
    pub backend: String,
    pub character: String,
    pub characters: Vec<String>,
    character_keys: BTreeMap<String, String>,
    pub repeat_count: String,
    pub difficulty: String,
    pub town_loop: bool,
    pub status: String,
    pub output: String,
    pub closing: bool,
    pub activity: Activity,
    record_enabled: bool,
    stop_record_enabled: bool,
    visible_lines: usize,
    log: String,
    log_revision: u64,
}
impl PanelState {
    pub fn new(config: &BotConfig) -> Result<Self, ConfigError> {
        let value = config
            .section("gui")
            .ok_or_else(|| ConfigError::value("GUI configuration required"))?;
        let visible_lines =
            numeric_input::visible_lines(&value["visible_log_lines"]).ok_or_else(|| {
                ConfigError::value("visible_log_lines cannot be converted to an integer")
            })?;
        let capture = config
            .section("capture")
            .ok_or_else(|| ConfigError::value("capture settings missing"))?;
        let mut state = Self {
            window_title: capture["window_title"]
                .as_str()
                .filter(|v| !v.is_empty())
                .unwrap_or("Diablo")
                .into(),
            window_titles: vec![],
            backend: capture["capture_backend"].as_str().unwrap_or("auto").into(),
            character: String::new(),
            characters: vec![],
            character_keys: BTreeMap::new(),
            repeat_count: String::new(),
            difficulty: "hell".into(),
            town_loop: false,
            status: "Idle".into(),
            output: "No recording yet".into(),
            closing: false,
            activity: Activity::default(),
            record_enabled: true,
            stop_record_enabled: false,
            visible_lines,
            log: String::new(),
            log_revision: 0,
        };
        state.refresh_characters(config)?;
        for warning in config.warnings() {
            state.append_log("warning", &warning.to_string());
        }
        Ok(state)
    }
    pub fn refresh_characters(&mut self, config: &BotConfig) -> Result<(), ConfigError> {
        let characters = config
            .section("characters")
            .and_then(|v| v.as_object())
            .ok_or_else(|| ConfigError::value("characters configuration required"))?;
        self.characters.clear();
        self.character_keys.clear();
        for (key, profile) in characters {
            let display = profile["display_name"]
                .as_str()
                .filter(|v| !v.is_empty())
                .unwrap_or(key)
                .to_owned();
            self.characters.push(display.clone());
            self.character_keys.insert(display, key.clone());
        }
        self.character = config
            .active_character_key()
            .and_then(|key| {
                characters.get(key).map(|profile| {
                    profile["display_name"]
                        .as_str()
                        .filter(|v| !v.is_empty())
                        .unwrap_or(key)
                        .to_owned()
                })
            })
            .unwrap_or_default();
        Ok(())
    }
    pub fn selected_character(&self) -> Option<&str> {
        self.character_keys
            .get(self.character.trim())
            .map(String::as_str)
    }
    pub fn choose_window<'a>(titles: &'a [String], current: &str) -> Option<&'a str> {
        let normalized = titles
            .iter()
            .filter(|v| !v.trim().is_empty())
            .map(|v| (v.as_str(), casefold(v)))
            .collect::<Vec<_>>();
        let non_panel = normalized
            .iter()
            .filter(|(_, v)| v != &casefold(TITLE))
            .collect::<Vec<_>>();
        if !current.is_empty() {
            for (title, lowered) in &non_panel {
                if lowered == current {
                    return Some(title);
                }
            }
            for (title, lowered) in &non_panel {
                if lowered.contains(current) {
                    return Some(title);
                }
            }
        }
        for pattern in PREFERRED_GAME_PATTERNS {
            for (title, lowered) in &non_panel {
                if lowered.contains(pattern) {
                    return Some(title);
                }
            }
        }
        non_panel
            .first()
            .map(|(title, _)| *title)
            .or_else(|| normalized.first().map(|(title, _)| *title))
    }
    pub fn refresh_windows(&mut self, titles: Vec<String>, select_current: bool) {
        self.window_titles = titles;
        if self.window_titles.is_empty() {
            self.append_log("warning", "No visible windows found.");
            return;
        }
        if select_current {
            if let Some(value) =
                Self::choose_window(&self.window_titles, &casefold(self.window_title.trim()))
            {
                self.window_title = value.into();
            }
        } else if self.window_title.trim().is_empty()
            && let Some(value) = Self::choose_window(&self.window_titles, "")
        {
            self.window_title = value.into();
        }
    }
    pub fn validate_repeat(proposed: &str) -> bool {
        proposed.chars().all(numeric_input::digit)
    }
    pub fn repeat(&self) -> Result<Option<serde_json::Number>, ConfigError> {
        let value = numeric_input::strip(&self.repeat_count);
        if value.is_empty() {
            Ok(None)
        } else {
            numeric_input::integer(value)
                .map(Some)
                .ok_or_else(|| ConfigError::value("Repeat Count must be a whole number."))
        }
    }
    pub fn ensure_action_available(&self) -> Result<(), ConfigError> {
        if self.closing {
            return Err(ConfigError::new(
                "RuntimeError",
                "The control panel is closing.",
            ));
        }
        if self.activity.actions() {
            return Err(ConfigError::new(
                "RuntimeError",
                "Another game action is active. Stop it and wait for it to finish before starting a new action.",
            ));
        }
        Ok(())
    }
    pub fn controls(&self) -> Controls {
        let available = !self.activity.actions() && !self.closing;
        Controls {
            actions: available,
            stop_action: self.activity.actions() && !self.closing,
            selectors: available,
            record: self.record_enabled && !self.closing,
            stop_record: self.stop_record_enabled && !self.closing,
        }
    }
    pub fn append_log(&mut self, level: &str, message: &str) {
        self.log_revision = self.log_revision.wrapping_add(1);
        self.log
            .push_str(&format!("[{}] {message}\n", level.to_uppercase()));
        let count = self.log.bytes().filter(|v| *v == b'\n').count() + 1;
        if count > self.visible_lines {
            let remove = count - self.visible_lines;
            let mut seen = 0;
            let mut boundary = 0;
            for (index, byte) in self.log.bytes().enumerate() {
                if byte == b'\n' {
                    seen += 1;
                    if seen == remove {
                        boundary = index + 1;
                        break;
                    }
                }
            }
            self.log.drain(..boundary);
            self.log.insert_str(0, &format!("[INFO] {TRIM_NOTICE}\n"));
        }
    }
    pub fn log(&self) -> &str {
        &self.log
    }
    pub fn log_revision(&self) -> u64 {
        self.log_revision
    }
    pub fn recording_started(&mut self, output: String) {
        self.output = output;
        self.record_enabled = false;
        self.stop_record_enabled = true;
        if !self.activity.actions() {
            self.status = "Recording".into();
        }
    }
    pub fn recording_stopped(&mut self) {
        self.status = "Idle".into();
        self.record_enabled = true;
        self.stop_record_enabled = false;
    }
    pub fn stopped_action(&mut self) {
        self.status = if self.activity.actions() {
            "Stopping"
        } else {
            "Stopped"
        }
        .into();
        self.refresh_controls();
    }
    pub fn begin_close(&mut self) -> bool {
        if self.closing {
            return false;
        }
        self.closing = true;
        self.status = "Stopping".into();
        self.record_enabled = false;
        self.stop_record_enabled = false;
        true
    }
    pub fn can_close(&self) -> bool {
        self.closing
            && !self.activity.actions()
            && !self.activity.recording
            && !self.activity.recording_cleanup
    }
    pub fn refresh_controls(&mut self) {
        if self.activity.actions() || self.closing {
            return;
        }
        if self.status == "Stopping" {
            self.status = "Stopped".into();
        }
        if ["Error", "Incomplete", "Summoner complete", "Stopped"].contains(&self.status.as_str()) {
            return;
        }
        self.status = if self.activity.recording {
            "Recording"
        } else {
            "Idle"
        }
        .into();
    }
    pub fn event(&mut self, source: EventSource, event: &SummonerEvent) {
        self.append_log(&event.level, &event.message);
        if event.level == "error" && source != EventSource::Gem {
            self.status = "Error".into();
            if source == EventSource::Recording {
                self.record_enabled = true;
                self.stop_record_enabled = false;
            }
        }
        if source == EventSource::Summoner
            && let Some(payload) = &event.payload
        {
            let status = payload["status"].as_str().unwrap_or("");
            if !(status == "running" && self.status == "Stopping")
                && let Some(value) = match status {
                    "running" => Some("Summoner Run"),
                    "complete" => Some("Summoner complete"),
                    "incomplete" => Some("Incomplete"),
                    "failed" => Some("Error"),
                    "stopped" => Some("Stopped"),
                    _ => None,
                }
            {
                self.status = value.into();
            }
            if let Some(output) = payload["output"].as_str().filter(|v| !v.is_empty()) {
                self.output = output.into();
            }
        }
    }
}

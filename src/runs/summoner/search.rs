//! 현재 바닥과 상대 이동으로 네 날개를 잇는 독립적인 단발 입력 요청 상태기계.
use super::navigation::{ArcanePathObserver, ChooseOptions, PathView, Wing};
use crate::{
    common::{config::ConfigError, scene_motion::CameraMotion, time},
    runs::RunError,
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
#[cfg(test)]
mod tests;
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SearchLimits {
    pub screen_age: f64,
    pub landing_seconds: f64,
    pub wing_seconds: f64,
    pub return_seconds: f64,
    pub total_seconds: f64,
    pub minimum_movement: f64,
    pub breadcrumb_radius: f64,
    pub maximum_casts: u64,
    pub maximum_hover_attempts: u64,
    pub settled_frames: u64,
    pub settled_motion_pixels: f64,
}
impl SearchLimits {
    pub fn validate(&self) -> Result<(), ConfigError> {
        for (name, value) in [
            ("screen_age", self.screen_age),
            ("landing_seconds", self.landing_seconds),
            ("wing_seconds", self.wing_seconds),
            ("return_seconds", self.return_seconds),
            ("total_seconds", self.total_seconds),
            ("minimum_movement", self.minimum_movement),
            ("breadcrumb_radius", self.breadcrumb_radius),
            ("settled_motion_pixels", self.settled_motion_pixels),
        ] {
            time::positive(value, name)?;
        }
        for (name, value, minimum) in [
            ("maximum_casts", self.maximum_casts, 1),
            ("maximum_hover_attempts", self.maximum_hover_attempts, 1),
            ("settled_frames", self.settled_frames, 2),
        ] {
            if value < minimum {
                return Err(ConfigError::value(format!(
                    "{name} must be an integer in range"
                )));
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Terminal {
    NonbossEnd,
    SummonerPresent,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HoverKind {
    Chest,
    Coffin,
    Shrine,
    Teleporter,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HoverObject {
    pub kind: HoverKind,
    pub bounds: [f64; 4],
}
pub struct SearchObservation {
    pub view: PathView,
    pub motion: CameraMotion,
    pub movement_allowed: bool,
    pub terminal: Option<Terminal>,
    pub hub_ready: bool,
    pub hover: Option<HoverObject>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchActionKind {
    Aim,
    Cast,
    Hold,
    Review,
    Summoner,
    WingStart,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchAction {
    pub kind: SearchActionKind,
    pub reason: String,
    pub wing: Wing,
    pub aim: Option<[i32; 2]>,
    pub request_id: Option<u64>,
    pub sequence: Option<u64>,
    pub generation: Option<u64>,
    pub issued_at: Option<f64>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchPhase {
    Outbound,
    Return,
    Review,
    Summoner,
}
pub struct ArcaneSearch {
    observer: ArcanePathObserver,
    limits: SearchLimits,
    reference: f64,
    started_at: f64,
    stage_at: f64,
    last_now: f64,
    interval: f64,
    wing_index: usize,
    phase: SearchPhase,
    position: [f64; 2],
    breadcrumbs: Vec<[f64; 2]>,
    return_points: VecDeque<[f64; 2]>,
    checked_wings: Vec<Wing>,
    last_view: Option<PathView>,
    expected_generation: Option<u64>,
    pending: Option<SearchAction>,
    cursor: Option<([i32; 2], u64, f64)>,
    landing: Option<(f64, [f64; 2], u64)>,
    hover_attempts: u64,
    cast_count: u64,
    serial: u64,
    landing_stable: u64,
    review_reason: Option<String>,
}
impl ArcaneSearch {
    pub fn new(
        observer: ArcanePathObserver,
        limits: SearchLimits,
        reference_seconds: f64,
        started_at: f64,
    ) -> Result<Self, ConfigError> {
        limits.validate()?;
        time::nonnegative(reference_seconds, "reference_seconds")?;
        time::nonnegative(started_at, "started_at")?;
        if reference_seconds == 0.0 || reference_seconds >= limits.landing_seconds {
            return Err(ConfigError::value(
                "positive casting reference below landing timeout required",
            ));
        }
        Ok(Self {
            observer,
            limits,
            reference: reference_seconds,
            started_at,
            stage_at: started_at,
            last_now: started_at,
            interval: reference_seconds,
            wing_index: 0,
            phase: SearchPhase::Outbound,
            position: [0.0; 2],
            breadcrumbs: vec![[0.0; 2]],
            return_points: VecDeque::new(),
            checked_wings: vec![],
            last_view: None,
            expected_generation: None,
            pending: None,
            cursor: None,
            landing: None,
            hover_attempts: 0,
            cast_count: 0,
            serial: 0,
            landing_stable: 0,
            review_reason: None,
        })
    }
    pub fn wing(&self) -> Wing {
        Wing::ALL[self.wing_index]
    }
    pub fn phase(&self) -> SearchPhase {
        self.phase
    }
    pub fn expected_generation(&self) -> Option<u64> {
        self.expected_generation
    }
    pub fn limits(&self) -> SearchLimits {
        self.limits
    }
    pub fn observer(&self) -> &ArcanePathObserver {
        &self.observer
    }
    pub fn last_view(&self) -> Option<&PathView> {
        self.last_view.as_ref()
    }
    pub fn action(&self, kind: SearchActionKind, reason: impl Into<String>) -> SearchAction {
        SearchAction {
            kind,
            reason: reason.into(),
            wing: self.wing(),
            aim: None,
            request_id: None,
            sequence: None,
            generation: None,
            issued_at: None,
        }
    }
    pub fn fail(&mut self, reason: impl Into<String>) -> SearchAction {
        let reason = self
            .review_reason
            .get_or_insert_with(|| reason.into())
            .clone();
        self.phase = SearchPhase::Review;
        self.pending = None;
        self.cursor = None;
        self.landing = None;
        self.action(SearchActionKind::Review, reason)
    }
    pub fn tick(
        &mut self,
        now: f64,
        interrupted: bool,
        focused: bool,
    ) -> Result<SearchAction, ConfigError> {
        time::nonnegative(now, "now")?;
        if now < self.last_now {
            return Ok(self.fail("clock_regressed"));
        }
        self.last_now = now;
        if self.phase == SearchPhase::Review {
            return Ok(self.action(
                SearchActionKind::Review,
                self.review_reason.clone().unwrap_or_default(),
            ));
        }
        if self.phase == SearchPhase::Summoner {
            return Ok(self.action(SearchActionKind::Summoner, "handoff_combat"));
        }
        if interrupted || !focused {
            return Ok(self.fail("interrupted_or_unfocused"));
        }
        if now >= self.started_at + self.limits.total_seconds {
            return Ok(self.fail("search_timeout"));
        }
        let returning = self.phase == SearchPhase::Return;
        if now
            >= self.stage_at
                + if returning {
                    self.limits.return_seconds
                } else {
                    self.limits.wing_seconds
                }
        {
            return Ok(self.fail(if returning {
                "return_timeout"
            } else {
                "wing_timeout"
            }));
        }
        if self
            .landing
            .is_some_and(|(at, _, _)| now >= at + self.limits.landing_seconds)
        {
            return Ok(self.fail("landing_unconfirmed"));
        }
        if self
            .pending
            .as_ref()
            .and_then(|r| r.issued_at)
            .is_some_and(|at| now >= at + self.limits.screen_age)
        {
            return Ok(self.fail("input_ack_timeout"));
        }
        Ok(self.action(
            SearchActionKind::Hold,
            if self
                .last_view
                .as_ref()
                .is_some_and(|v| now - v.context.captured_at > self.limits.screen_age)
            {
                "fresh_screen_required"
            } else {
                "new_observation_required"
            },
        ))
    }
    pub fn acknowledge(
        &mut self,
        request: &SearchAction,
        now: f64,
        succeeded: bool,
    ) -> Result<bool, ConfigError> {
        if self.pending.as_ref() != Some(request) {
            return Ok(false);
        }
        if !succeeded
            || !request
                .issued_at
                .is_some_and(|at| at <= now && now < at + self.limits.screen_age)
        {
            self.fail("input_failed_or_late");
            return Ok(false);
        }
        self.pending = None;
        let Some(aim) = request.aim else {
            return Err(ConfigError::value("search request aim missing"));
        };
        let Some(sequence) = request.sequence else {
            return Err(ConfigError::value("search request sequence missing"));
        };
        if request.kind == SearchActionKind::Aim {
            self.cursor = Some((aim, sequence, now));
        } else {
            self.landing = Some((now, self.position, sequence));
            self.landing_stable = 0;
            self.cursor = None;
            self.cast_count = self
                .cast_count
                .checked_add(1)
                .ok_or_else(|| ConfigError::value("search cast count exhausted"))?;
            self.expected_generation = Some(
                self.expected_generation
                    .and_then(|v| v.checked_add(1))
                    .ok_or_else(|| ConfigError::value("search generation exhausted"))?,
            );
        }
        Ok(true)
    }
    pub fn defer(&mut self, request: &SearchAction) -> bool {
        if self.pending.as_ref() != Some(request) {
            false
        } else {
            self.pending = None;
            self.cursor = None;
            true
        }
    }
    fn request(
        &mut self,
        kind: SearchActionKind,
        reason: impl Into<String>,
        aim: [i32; 2],
        view: &PathView,
        now: f64,
    ) -> Result<SearchAction, ConfigError> {
        self.serial = self
            .serial
            .checked_add(1)
            .ok_or_else(|| ConfigError::value("search request count exhausted"))?;
        let action = SearchAction {
            aim: Some(aim),
            request_id: Some(self.serial),
            sequence: Some(view.context.sequence),
            generation: Some(view.context.generation),
            issued_at: Some(now),
            ..self.action(kind, reason)
        };
        self.pending = Some(action.clone());
        Ok(action)
    }
    pub fn step(
        &mut self,
        observation: &SearchObservation,
        now: f64,
        interrupted: bool,
        focused: bool,
    ) -> Result<SearchAction, RunError> {
        let status = self.tick(now, interrupted, focused)?;
        if terminal_action(&status) {
            return Ok(status);
        }
        let view = &observation.view;
        let context = view.context;
        if !(0.0..=self.limits.screen_age).contains(&(now - context.captured_at)) {
            return Ok(self.action(SearchActionKind::Hold, "screen_stale"));
        }
        let expected = *self.expected_generation.get_or_insert(context.generation);
        if context.generation < expected {
            return Ok(self.action(SearchActionKind::Hold, "frame_precedes_cast"));
        }
        if context.generation > expected {
            return Ok(self.fail("unexpected_scene_generation"));
        }
        if let Some(last) = &self.last_view {
            if context.sequence <= last.context.sequence
                || context.captured_at < last.context.captured_at
            {
                return Ok(self.action(SearchActionKind::Hold, "frame_not_newer"));
            }
            if context.frame_size != last.context.frame_size
                || context.viewport != last.context.viewport
            {
                return Ok(self.fail("window_layout_changed"));
            }
        }
        let had_view = self.last_view.is_some();
        self.last_view = Some(view.clone());
        let delta = observation.motion.displacement;
        if had_view {
            if let Some(delta) = delta {
                self.position = [self.position[0] + delta[0], self.position[1] + delta[1]];
                if self.cursor.is_some() && length(delta) > view.cell_pixels as f64 / 2.0 {
                    self.cursor = None;
                }
            } else {
                self.cursor = None;
            }
        }
        if self.pending.is_some() {
            return Ok(self.action(SearchActionKind::Hold, "input_outcome_pending"));
        }
        if observation.terminal == Some(Terminal::SummonerPresent) {
            self.phase = SearchPhase::Summoner;
            self.cursor = None;
            self.landing = None;
            return Ok(self.action(SearchActionKind::Summoner, "handoff_combat"));
        }
        if let Some((began, origin, source)) = self.landing {
            if context.sequence <= source || context.captured_at < began {
                return Ok(self.action(SearchActionKind::Hold, "frame_precedes_cast"));
            }
            self.landing_stable =
                if delta.is_some_and(|d| length(d) <= self.limits.settled_motion_pixels) {
                    self.landing_stable.saturating_add(1)
                } else {
                    0
                };
            if delta.is_none()
                || distance(self.position, origin) < self.limits.minimum_movement
                || now < began + self.interval
                || self.landing_stable < self.limits.settled_frames
            {
                return Ok(self.action(SearchActionKind::Hold, "landing_pending"));
            }
            self.interval = self.reference.max(
                self.limits
                    .landing_seconds
                    .min(0.75 * self.interval + 0.25 * (now - began)),
            );
            self.landing = None;
            self.hover_attempts = 0;
            if self.phase == SearchPhase::Outbound {
                self.breadcrumbs.push(self.position);
            }
        }
        if !observation.movement_allowed {
            self.cursor = None;
            return Ok(self.action(
                SearchActionKind::Hold,
                "survival_or_buff_preempts_navigation",
            ));
        }
        if self.phase == SearchPhase::Outbound && observation.terminal == Some(Terminal::NonbossEnd)
        {
            self.checked_wings.push(self.wing());
            self.phase = SearchPhase::Return;
            self.stage_at = now;
            self.return_points = self.breadcrumbs.iter().rev().copied().collect();
            self.cursor = None;
        }
        if self.phase == SearchPhase::Return {
            while self.return_points.front().is_some_and(|point| {
                distance(self.position, *point) <= self.limits.breadcrumb_radius
            }) {
                self.return_points.pop_front();
            }
            if self.return_points.is_empty() && observation.hub_ready {
                if self.wing_index == 3 {
                    return Ok(self.fail("all_wings_empty"));
                }
                self.wing_index += 1;
                self.phase = SearchPhase::Outbound;
                self.stage_at = now;
                self.position = [0.0; 2];
                self.breadcrumbs = vec![[0.0; 2]];
                self.cursor = None;
                return Ok(self.action(SearchActionKind::WingStart, "hub_confirmed"));
            }
            if self.return_points.is_empty() {
                self.cursor = None;
                return Ok(self.action(SearchActionKind::Hold, "hub_arrival_unconfirmed"));
            }
        }
        if self.cast_count >= self.limits.maximum_casts {
            return Ok(self.fail("cast_budget_exhausted"));
        }
        if let Some((aim, source, aimed_at)) = self.cursor.filter(|_| observation.hover.is_none()) {
            if context.sequence > source && context.captured_at >= aimed_at {
                let x = ((f64::from(aim[0]) - context.viewport.left() as f64)
                    / view.cell_pixels as f64)
                    .floor();
                let y = ((f64::from(aim[1]) - context.viewport.top() as f64)
                    / view.cell_pixels as f64)
                    .floor();
                if x >= 0.0
                    && y >= 0.0
                    && view
                        .floor()
                        .row(y as usize)
                        .and_then(|row| row.get(x as usize))
                        .is_some_and(|v| *v != 0)
                {
                    return Ok(self.request(
                        SearchActionKind::Cast,
                        "fresh_floor_and_hover_clear",
                        aim,
                        view,
                        now,
                    )?);
                }
            }
            self.cursor = None;
        }
        if observation.hover.is_some() {
            self.hover_attempts = self.hover_attempts.saturating_add(1);
            self.cursor = None;
            if self.hover_attempts > self.limits.maximum_hover_attempts {
                return Ok(self.fail("hover_unresolved"));
            }
        }
        let destination = self
            .return_points
            .front()
            .copied()
            .or_else(|| (self.phase == SearchPhase::Return).then_some([0.0; 2]));
        let excluded = observation
            .hover
            .as_ref()
            .map(|h| h.bounds)
            .into_iter()
            .collect::<Vec<_>>();
        let choice = self.observer.choose(
            view,
            self.wing(),
            ChooseOptions {
                position: self.position,
                visited: if self.phase == SearchPhase::Outbound {
                    &self.breadcrumbs[..self.breadcrumbs.len() - 1]
                } else {
                    &[]
                },
                destination,
                excluded: &excluded,
                ..Default::default()
            },
        )?;
        let Some(aim) = choice.aim else {
            return Ok(self.action(SearchActionKind::Hold, choice.reason));
        };
        Ok(self.request(
            SearchActionKind::Aim,
            if observation.hover.is_some() {
                "hover_reacquire".into()
            } else {
                choice.reason
            },
            aim,
            view,
            now,
        )?)
    }
}
fn length(p: [f64; 2]) -> f64 {
    p[0].hypot(p[1])
}
fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    length([a[0] - b[0], a[1] - b[1]])
}
pub fn terminal_action(action: &SearchAction) -> bool {
    matches!(
        action.kind,
        SearchActionKind::Review | SearchActionKind::Summoner
    )
}

/// 독립 실행 경로는 False 반환과 backend 오류를 구분한다.
pub trait SearchEnvironment {
    fn clock(&self) -> f64;
    fn focused(&self) -> Result<bool, RunError>;
    fn interrupted(&self) -> bool;
    fn observe(&mut self, generation: Option<u64>) -> Result<Option<SearchObservation>, RunError>;
    fn move_pointer(&mut self, aim: [i32; 2]) -> Result<bool, RunError>;
    fn press(&mut self, key: &str) -> Result<bool, RunError>;
    fn key_up(&mut self, key: &str) -> Result<(), RunError>;
    fn wait(&mut self, seconds: f64) -> Result<bool, RunError>;
}
pub fn run_search(
    search: &mut ArcaneSearch,
    environment: &mut impl SearchEnvironment,
    movement_key: &str,
    mut on_action: impl FnMut(&SearchAction) -> Result<(), RunError>,
) -> Result<SearchAction, RunError> {
    let result = (|| {
        loop {
            let mut action = search.tick(
                environment.clock(),
                environment.interrupted(),
                environment.focused()?,
            )?;
            if !terminal_action(&action)
                && let Some(observation) = environment.observe(search.expected_generation())?
            {
                action = search.step(
                    &observation,
                    environment.clock(),
                    environment.interrupted(),
                    environment.focused()?,
                )?;
            }
            on_action(&action)?;
            if terminal_action(&action) {
                return Ok(action);
            }
            if matches!(action.kind, SearchActionKind::Aim | SearchActionKind::Cast) {
                if !input_guard(search, environment)? {
                    return Ok(search.fail("input_guard_rejected"));
                }
                let aim = action
                    .aim
                    .ok_or_else(|| ConfigError::value("search request aim missing"))?;
                if !environment.move_pointer(aim)? {
                    search.acknowledge(&action, environment.clock(), false)?;
                    return Ok(search.fail("input_move_rejected"));
                }
                if action.kind == SearchActionKind::Cast {
                    if !input_guard(search, environment)? {
                        return Ok(search.fail("input_guard_rejected"));
                    }
                    if !environment.press(movement_key)? {
                        search.acknowledge(&action, environment.clock(), false)?;
                        return Ok(search.fail("input_cast_rejected"));
                    }
                }
                if !search.acknowledge(&action, environment.clock(), true)? {
                    return Ok(search.fail("input_ack_rejected"));
                }
            }
            if environment.wait(0.02)? {
                return Ok(search.fail("interrupted"));
            }
        }
    })();
    if result.is_err() {
        search.fail("observer_or_input_error");
    }
    match (result, environment.key_up(movement_key)) {
        (Ok(v), Ok(())) => Ok(v),
        (Err(e), Ok(())) | (Ok(_), Err(e)) => Err(e),
        (Err(e), Err(c)) => Err(e.with_cleanup(c)),
    }
}
fn input_guard(search: &ArcaneSearch, env: &impl SearchEnvironment) -> Result<bool, RunError> {
    Ok(!env.interrupted()
        && env.focused()?
        && search
            .last_view()
            .is_some_and(|v| env.clock() - v.context.captured_at <= search.limits.screen_age))
}

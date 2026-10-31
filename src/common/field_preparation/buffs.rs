use super::*;
use crate::common::buffing::PauseSchedule;
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize)]
pub struct BuffDispatch {
    pub key: String,
    pub cast_request_at: f64,
    pub cast_request_monotonic: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct BuffDispatchResult {
    pub status: String,
    pub buff_dispatches: Vec<BuffDispatch>,
    pub equipment_dispatch_balanced: bool,
    pub effect_verified: bool,
    pub held_after: Vec<String>,
    pub errors: Vec<String>,
    pub trace_phase: &'static str,
}
pub struct BuffTrace {
    pub frame: Arc<BgrFrame>,
    pub metadata: Value,
}
pub type BuffTraceSink<'a> = dyn FnMut(BuffTrace) -> Result<(), ControlError> + 'a;
pub struct BuffDispatchOptions<'a> {
    pub trace_frames: Option<&'a mut Vec<BuffTrace>>,
    pub trace_sink: Option<&'a mut BuffTraceSink<'a>>,
    pub observation_wait: f64,
    pub sequence_timeout: f64,
    pub trace_before_dispatch: bool,
}
impl Default for BuffDispatchOptions<'_> {
    fn default() -> Self {
        Self {
            trace_frames: None,
            trace_sink: None,
            observation_wait: 0.75,
            sequence_timeout: 8.0,
            trace_before_dispatch: false,
        }
    }
}
struct TraceCheck<'a, 'b, E: PreparationEnvironment, H: PreparationChecks<E>> {
    checks: &'a mut H,
    frames: Option<&'a mut Vec<BuffTrace>>,
    sink: &'a mut Option<&'b mut BuffTraceSink<'b>>,
    key: &'a str,
    index: usize,
    _environment: std::marker::PhantomData<E>,
}
impl<E: PreparationEnvironment, H: PreparationChecks<E>> PreparationChecks<E>
    for TraceCheck<'_, '_, E, H>
{
    fn check_frame(
        &mut self,
        env: &mut E,
        frame: &Arc<BgrFrame>,
        acquired: f64,
        reading: &HudReading,
    ) -> Result<Option<String>, E::Error> {
        let blocked = self.checks.check_frame(env, frame, acquired, reading)?;
        if blocked.is_some() {
            return Ok(blocked);
        }
        if self.frames.is_some() || self.sink.is_some() {
            let processed = env.clock();
            let trace = BuffTrace {
                frame: frame.clone(),
                metadata: json!({"key":self.key,"index":self.index,"phase":"before_dispatch","captured_at":acquired,"processed_at":processed,"hud":reading}),
            };
            if let Some(frames) = self.frames.as_mut() {
                frames.push(trace);
            } else if let Some(sink) = self.sink.as_mut() {
                sink(trace)?;
            }
            if processed - acquired >= 0.25 {
                return Ok(Some("trace_screen_stale".into()));
            }
        }
        Ok(None)
    }
}
pub fn dispatch_buffs<E: PreparationEnvironment>(
    env: &mut E,
    policy: &CharacterSurvivalPolicy,
    order: &[String],
    pauses: &PauseSchedule,
    checks: &mut impl PreparationChecks<E>,
    mut options: BuffDispatchOptions<'_>,
) -> Result<BuffDispatchResult, E::Error> {
    if options.trace_frames.is_some() && options.trace_sink.is_some() {
        return Err(ConfigError::value("buff trace requires one output owner").into());
    }
    if options.trace_sink.is_some() && order.len() > 16 {
        return Err(ConfigError::value("buff trace requires at most sixteen steps").into());
    }
    if options
        .trace_frames
        .as_ref()
        .is_some_and(|frames| !frames.is_empty() || order.len() > 16)
    {
        return Err(ConfigError::value(
            "buff trace requires an empty buffer and at most sixteen steps",
        )
        .into());
    }
    let mut events = Vec::new();
    let mut swapped = false;
    let mut status = "dispatched_unconfirmed".to_owned();
    let began = env.clock();
    for token in order {
        if env.stopped() || !env.focused()? || env.clock() - began >= options.sequence_timeout {
            status = "interrupted".into();
            break;
        }
        let deadline =
            (began + options.sequence_timeout).min(env.clock() + options.observation_wait);
        let blocked = if options.trace_before_dispatch {
            let mut traced = TraceCheck {
                checks,
                frames: options.trace_frames.as_deref_mut(),
                sink: &mut options.trace_sink,
                key: token,
                index: events.len(),
                _environment: std::marker::PhantomData,
            };
            wait_readable_resources(
                env,
                policy,
                deadline,
                &mut traced,
                &["screen_stale", "buff_ground_unverified"],
            )?
        } else {
            // 버프 전 단계에는 외부 관찰 콜백 없이 현재 화면 검사만 연결한다.
            let mut checked = TraceCheck {
                checks,
                frames: None,
                sink: &mut None,
                key: token,
                index: events.len(),
                _environment: std::marker::PhantomData,
            };
            wait_readable_resources(env, policy, deadline, &mut checked, &[])?
        };
        if let Some(blocked) = blocked {
            status = blocked;
            break;
        }
        if let Some(blocked) = checks.before_step(env)? {
            status = blocked;
            break;
        }
        if env.stopped() || !env.focused()? || env.clock() - began >= options.sequence_timeout {
            status = "interrupted_or_stale".into();
            break;
        }
        if ["right-click", "left-click"].contains(&token.as_str()) {
            return Err(ConfigError::value(
                "this probe supports configured keyboard buff sequences only",
            )
            .into());
        }
        let wall = env.wall_clock();
        let requested = env.clock();
        env.press(token)?;
        events.push(BuffDispatch {
            key: token.clone(),
            cast_request_at: wall,
            cast_request_monotonic: requested,
        });
        if token == "w" {
            swapped = !swapped;
        }
        env.wait(pauses.seconds(token))?;
        if (options.trace_frames.is_some() || options.trace_sink.is_some())
            && !options.trace_before_dispatch
            && !env.stopped()
            && env.focused()?
        {
            if options
                .trace_frames
                .as_ref()
                .is_some_and(|frames| frames.len() >= 16)
            {
                return Err(ConfigError::value("buff trace exceeds sixteen frames").into());
            }
            let acquired = env.clock();
            let frame = env.capture()?;
            let reading = env.hud(&frame)?;
            let processed = env.clock();
            let trace = BuffTrace {
                frame,
                metadata: json!({"key":token,"index":events.len()-1,"phase":"after_dispatch","captured_at":acquired,"processed_at":processed,"hud":reading}),
            };
            if let Some(frames) = options.trace_frames.as_mut() {
                frames.push(trace);
            } else if let Some(sink) = options.trace_sink.as_mut() {
                sink(trace)?;
            }
            if processed - acquired >= 0.25 {
                status = "trace_screen_stale".into();
                break;
            }
        }
    }
    // 부분 전환 뒤 복귀를 추측하지 않고 실제 전송된 순서의 균형만 보고한다.
    if swapped && status == "dispatched_unconfirmed" {
        status = "equipment_dispatch_unbalanced".into();
    }
    Ok(BuffDispatchResult {
        status,
        buff_dispatches: events,
        equipment_dispatch_balanced: !swapped,
        effect_verified: false,
        held_after: vec![],
        errors: vec![],
        trace_phase: if options.trace_before_dispatch {
            "before_dispatch"
        } else {
            "after_dispatch"
        },
    })
}

/// 근접 적 판정만 재시도하며, 나머지 판독·입력 오류는 원인과 함께 전달한다.
pub enum GuardPreparationError<E> {
    NearbyHostile,
    Other(E),
}
pub trait AutomaticBuffEnvironment {
    type Error: From<ConfigError> + From<ControlError>;
    type Guard;
    fn clock(&self) -> f64;
    fn stopped(&self) -> bool;
    fn create_guard(&mut self) -> Result<Self::Guard, Self::Error>;
    fn prepare(
        &mut self,
        guard: &mut Self::Guard,
    ) -> Result<Value, GuardPreparationError<Self::Error>>;
    fn clear_hostiles(&mut self) -> Result<(), Self::Error>;
    fn observe(&mut self) -> Result<Value, Self::Error>;
    fn dispatch(&mut self, guard: &mut Self::Guard) -> Result<BuffDispatchResult, Self::Error>;
}
#[derive(Default)]
pub struct AutomaticBuffDriver {
    deadlines: Vec<f64>,
    failed_dispatch: Option<BuffDispatchResult>,
    weapon_state_unverified: bool,
}
impl AutomaticBuffDriver {
    pub const PREPARATION_LEAD_SECONDS: f64 = 5.0;
    pub fn deadlines(&self) -> &[f64] {
        &self.deadlines
    }
    pub fn failed_dispatch(&self) -> Option<&BuffDispatchResult> {
        self.failed_dispatch.as_ref()
    }
    pub fn weapon_state_unverified(&self) -> bool {
        self.weapon_state_unverified
    }
    pub fn preparation_at(&self) -> Option<f64> {
        self.deadlines
            .iter()
            .copied()
            .reduce(f64::min)
            .map(|deadline| deadline - Self::PREPARATION_LEAD_SECONDS)
    }
    pub fn due(&self, now: f64) -> bool {
        self.preparation_at().is_some_and(|at| now >= at)
    }
    pub fn execute<E: AutomaticBuffEnvironment>(
        &mut self,
        env: &mut E,
        order: &[String],
        policy: &CharacterSurvivalPolicy,
    ) -> Result<Value, E::Error> {
        if self.weapon_state_unverified {
            return Err(runtime("buff_weapon_state_unverified"));
        }
        self.failed_dispatch = None;
        let previous_deadline = self.deadlines.iter().copied().reduce(f64::min);
        let started = env.clock();
        if order.is_empty()
            || !order
                .iter()
                .filter(|token| token.to_lowercase() == "w")
                .count()
                .is_multiple_of(2)
        {
            return Err(ConfigError::value(
                "buff sequence must restore the starting main weapon set",
            )
            .into());
        }
        if policy.buffs().is_empty() {
            return Err(ConfigError::value("timed buff policy required").into());
        }
        let (mut guard, stability) = {
            let mut prepared = None;
            for attempt in 0..3 {
                if env.stopped() {
                    return Err(runtime("automatic buff interrupted"));
                }
                let mut guard = env.create_guard()?;
                match env.prepare(&mut guard) {
                    Ok(stability) => {
                        prepared = Some((guard, stability));
                        break;
                    }
                    Err(GuardPreparationError::NearbyHostile) if attempt < 2 => {
                        env.clear_hostiles()?
                    }
                    Err(GuardPreparationError::NearbyHostile) => {
                        return Err(runtime("buff_nearby_hostile"));
                    }
                    Err(GuardPreparationError::Other(error)) => return Err(error),
                }
            }
            prepared.ok_or_else(|| runtime::<E::Error>("automatic buff preparation incomplete"))?
        };
        let before = env.observe()?;
        // 전송 도중 오류도 일부 W가 이미 적용됐을 수 있다. 새 순서로 덮어쓰지 않는다.
        self.weapon_state_unverified = order.iter().any(|token| token.eq_ignore_ascii_case("w"));
        let dispatched = env.dispatch(&mut guard)?;
        self.weapon_state_unverified = !dispatched.equipment_dispatch_balanced;
        if dispatched.status != "dispatched_unconfirmed" || !dispatched.equipment_dispatch_balanced
        {
            let error = runtime(format!("automatic buff dispatch: {}", dispatched.status));
            self.failed_dispatch = Some(dispatched);
            return Err(error);
        }
        let mut deadlines = Vec::with_capacity(policy.buffs().len());
        for rule in policy.buffs() {
            let request = dispatched
                .buff_dispatches
                .iter()
                .rev()
                .find(|event| event.key == rule.key())
                .ok_or_else(|| {
                    runtime::<E::Error>(format!("timed buff request missing: {}", rule.name()))
                })?;
            let at = request.cast_request_monotonic;
            if !at.is_finite() || !(0.0..=env.clock()).contains(&at) {
                return Err(runtime("invalid actual buff request time"));
            }
            deadlines.push(at + rule.duration_seconds() - rule.renew_before_seconds());
        }
        let proof = env.observe()?;
        let checked_at = env.clock();
        if deadlines.iter().any(|deadline| checked_at >= *deadline) {
            return Err(runtime(
                "automatic buff deadline elapsed during preparation",
            ));
        }
        self.deadlines = deadlines;
        Ok(
            json!({"buff":dispatched,"safety":before,"observation":proof,"stability":stability,"deadlines":self.deadlines,"preparation_at":self.preparation_at(),"preparation_started_at":started,"previous_deadline":previous_deadline,"equipment_basis":"main_weapon_start_precondition_and_balanced_sequence","effect_status":"not_independently_verified"}),
        )
    }
}

use super::*;
use serde_json::Value;

/// 문자열 읽기는 실행 주체가 소유한다. 대기 루프는 현재 화면 감시를 계속한다.
pub trait ProbeSignals<E> {
    fn try_read(&mut self) -> Result<Option<Value>, E>;
}
impl<E> ProbeSignals<E> for std::sync::mpsc::Receiver<Result<Value, E>> {
    fn try_read(&mut self) -> Result<Option<Value>, E> {
        match self.try_recv() {
            Ok(item) => item.map(Some),
            Err(
                std::sync::mpsc::TryRecvError::Empty | std::sync::mpsc::TryRecvError::Disconnected,
            ) => Ok(None),
        }
    }
}
pub struct ProbeWait<'a> {
    pub deadline: f64,
    pub phase: &'a str,
    pub retry_unverified: bool,
    pub retry_stale: bool,
}
pub fn wait_probe_signal<E: PreparationEnvironment>(
    env: &mut E,
    signals: &mut impl ProbeSignals<E::Error>,
    policy: &CharacterSurvivalPolicy,
    options: ProbeWait<'_>,
    mut accept: impl FnMut(&mut E, &Value) -> Result<(), ControlError>,
    checks: &mut impl PreparationChecks<E>,
    mut emit: impl FnMut(&str),
) -> Result<Value, E::Error> {
    while !env.stopped() && env.clock() < options.deadline {
        let deadline = options.deadline.min(env.clock() + 0.75);
        let blocked = wait_readable_resources(env, policy, deadline, checks, &[])?;
        if blocked.as_deref() == Some("observation_unverified") && options.retry_unverified {
            continue;
        }
        if let Some(blocked) = blocked {
            return Err(runtime(format!("{}: {blocked}", options.phase)));
        }
        if env.stopped() || env.clock() >= options.deadline {
            break;
        }
        let Some(signal) = signals.try_read()? else {
            env.wait(0.02)?;
            continue;
        };
        if signal["phase"] != options.phase {
            return Err(ConfigError::value("unexpected supervised probe phase").into());
        }
        if let Err(error) = accept(env, &signal) {
            if let ControlError::Configuration(config) = &error
                && config.kind() == "StaleSupervision"
                && options.retry_stale
            {
                emit(&format!(
                    "AWAIT_FRESH_SUPERVISION: {}: {}; no input authorized",
                    options.phase,
                    config.message()
                ));
                continue;
            }
            return Err(error.into());
        }
        return Ok(signal);
    }
    Err(runtime(format!(
        "{}: interrupted or confirmation timeout",
        options.phase
    )))
}

//! 현재 화면·감독 확인과 입력 요청을 구분하는 필드 준비 절차.

mod buffs;
mod permit;
mod probe;
mod screen;
mod site;
pub use buffs::*;
pub use permit::*;
pub use probe::*;
pub use screen::*;
pub use site::*;

use super::{
    config::ConfigError,
    error::ControlError,
    frame::BgrFrame,
    survival::CharacterSurvivalPolicy,
    survival_vision::{HudReading, resource_guard},
};
use std::sync::Arc;

/// 호출자가 같은 시계, 취소 신호, 캡처/입력 소유자를 제공한다.
pub trait PreparationEnvironment {
    type Error: From<ConfigError> + From<ControlError>;
    fn clock(&self) -> f64;
    fn wall_clock(&self) -> f64;
    fn stopped(&self) -> bool;
    fn focused(&self) -> Result<bool, Self::Error>;
    fn capture(&mut self) -> Result<Arc<BgrFrame>, Self::Error>;
    fn hud(&mut self, frame: &Arc<BgrFrame>) -> Result<HudReading, Self::Error>;
    fn press(&mut self, key: &str) -> Result<(), Self::Error>;
    fn wait(&mut self, seconds: f64) -> Result<(), Self::Error>;
}
pub trait PreparationChecks<E: PreparationEnvironment> {
    fn observe_frame(
        &mut self,
        _env: &mut E,
        _frame: &Arc<BgrFrame>,
        _acquired: f64,
    ) -> Result<(), E::Error> {
        Ok(())
    }
    fn check_frame(
        &mut self,
        _env: &mut E,
        _frame: &Arc<BgrFrame>,
        _acquired: f64,
        _hud: &HudReading,
    ) -> Result<Option<String>, E::Error> {
        Ok(None)
    }
    fn before_step(&mut self, _env: &mut E) -> Result<Option<String>, E::Error> {
        Ok(None)
    }
}
pub struct NoPreparationChecks;
impl<E: PreparationEnvironment> PreparationChecks<E> for NoPreparationChecks {}

pub fn wait_readable_resources<E: PreparationEnvironment>(
    env: &mut E,
    policy: &CharacterSurvivalPolicy,
    deadline: f64,
    checks: &mut impl PreparationChecks<E>,
    retry_reasons: &[&str],
) -> Result<Option<String>, E::Error> {
    let mut blocked = Some("observation_unverified".to_owned());
    while env.clock() < deadline && !env.stopped() {
        if !env.focused()? {
            return Ok(Some("focus_lost".into()));
        }
        let acquired = env.clock();
        let frame = env.capture()?;
        let reading = env.hud(&frame)?;
        checks.observe_frame(env, &frame, acquired)?;
        blocked = if env.clock() - acquired >= 0.25 {
            Some("screen_stale".into())
        } else {
            resource_guard(&reading, policy).map(str::to_owned)
        };
        if blocked.is_none() {
            blocked = checks.check_frame(env, &frame, acquired, &reading)?;
        }
        if blocked.as_deref() != Some("observation_unverified")
            && !retry_reasons
                .iter()
                .any(|reason| blocked.as_deref() == Some(reason))
        {
            return Ok(blocked);
        }
        env.wait(0.02)?;
    }
    Ok(if env.stopped() {
        Some("interrupted".into())
    } else {
        blocked
    })
}
fn runtime<E: From<ControlError>>(message: impl Into<String>) -> E {
    ControlError::Runtime(message.into()).into()
}

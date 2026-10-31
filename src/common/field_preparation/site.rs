use super::*;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::VecDeque;

type RatioBounds = [[f64; 2]; 2];
pub trait BuffSiteEnvironment: PreparationEnvironment {
    /// 카메라 준비가 필요한 구현만 제공한다. 이 호출 안에서 현재 포커스를 재확인한다.
    fn prepare_ground(
        &mut self,
        _frame: &Arc<BgrFrame>,
        _acquired: f64,
        _reading: &HudReading,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    fn ground(
        &mut self,
        frame: &Arc<BgrFrame>,
        acquired: f64,
    ) -> Result<Option<[i32; 2]>, Self::Error>;
    fn hazard_grounds(
        &mut self,
        frame: &Arc<BgrFrame>,
        player: [i32; 2],
        acquired: f64,
        processed: f64,
    ) -> Result<Vec<Option<[f64; 2]>>, Self::Error>;
}
pub struct BuffSafetyFrame<'a> {
    pub frame: &'a Arc<BgrFrame>,
    pub acquired: Option<f64>,
    pub reading: Option<&'a HudReading>,
}
#[derive(Debug, Clone, Serialize)]
pub struct BuffSiteObservation {
    pub at: f64,
    pub life_ratio_bounds: [f64; 2],
    pub mana_ratio_bounds: [f64; 2],
}
#[derive(Default)]
pub struct BuffSiteGuard {
    baseline: Option<RatioBounds>,
    samples: Vec<(f64, RatioBounds)>,
    observations: VecDeque<BuffSiteObservation>,
    supervised_ground: Option<Value>,
}
impl BuffSiteGuard {
    pub fn new(supervised_ground: Option<Value>) -> Self {
        Self {
            supervised_ground,
            ..Self::default()
        }
    }
    pub fn observations(&self) -> &VecDeque<BuffSiteObservation> {
        &self.observations
    }
    pub fn baseline(&self) -> Option<RatioBounds> {
        self.baseline
    }
    pub fn samples(&self) -> &[(f64, RatioBounds)] {
        &self.samples
    }
    pub fn check<E: BuffSiteEnvironment>(
        &mut self,
        env: &mut E,
        policy: &CharacterSurvivalPolicy,
        supplied: Option<BuffSafetyFrame<'_>>,
    ) -> Result<Option<&'static str>, E::Error> {
        if env.stopped() || !env.focused()? {
            return Ok(Some("interrupted_or_unfocused"));
        }
        let (frame, acquired, reading) = if let Some(supplied) = supplied {
            let Some(acquired) = supplied.acquired else {
                return Ok(Some("screen_stale"));
            };
            let Some(reading) = supplied.reading else {
                return Ok(Some("screen_stale"));
            };
            if !(0.0..0.25).contains(&(env.clock() - acquired)) {
                return Ok(Some("screen_stale"));
            }
            (supplied.frame.clone(), acquired, reading.clone())
        } else {
            let acquired = env.clock();
            let frame = env.capture()?;
            let reading = env.hud(&frame)?;
            (frame, acquired, reading)
        };
        if let Some(blocked) = resource_guard(&reading, policy) {
            return Ok(Some(blocked));
        }
        env.prepare_ground(&frame, acquired, &reading)?;
        let mut point = env.ground(&frame, acquired)?;
        if point.is_none()
            && let Some(proof) = &self.supervised_ground
        {
            let at = proof.get("observed_at").and_then(Value::as_f64);
            let candidate = proof["player_ground"]
                .as_array()
                .filter(|values| values.len() == 2)
                .and_then(|values| values[0].as_i64().zip(values[1].as_i64()));
            if proof["source"] == "supervised_visual"
                && at.is_some_and(|at| (0.0..=15.0).contains(&(env.wall_clock() - at)))
                && let Some((x, y)) =
                    candidate.filter(|(x, y)| (24..1900).contains(x) && (55..900).contains(y))
            {
                point = Some([x as i32, y as i32]);
            }
        }
        let Some(point) = point else {
            return Ok(Some("buff_ground_unverified"));
        };
        let processed = env.clock();
        let hazards = env.hazard_grounds(&frame, point, acquired, processed)?;
        if hazards.iter().any(|ground| {
            ground.is_none_or(|ground| {
                ((ground[0] - f64::from(point[0])) / 300.0).powi(2)
                    + ((ground[1] - f64::from(point[1])) / 180.0).powi(2)
                    <= 1.0
            })
        }) {
            return Ok(Some("buff_nearby_hostile"));
        }
        if env.clock() - acquired >= 0.25 {
            return Ok(Some("screen_stale"));
        }
        let bounds =
            |resource: &super::super::survival_vision::ResourceReading| -> Option<[f64; 2]> {
                let ratio = resource.ratio()?;
                Some(
                    if resource.current().is_some() && resource.maximum().is_some() {
                        [ratio, ratio]
                    } else {
                        [ratio, 1.0]
                    },
                )
            };
        let Some(bounds) = bounds(&reading.life)
            .zip(bounds(&reading.mana))
            .map(|(life, mana)| [life, mana])
        else {
            return Ok(Some("observation_unverified"));
        };
        self.observations.push_back(BuffSiteObservation {
            at: acquired,
            life_ratio_bounds: bounds[0],
            mana_ratio_bounds: bounds[1],
        });
        while self.observations.len() > 32 {
            self.observations.pop_front();
        }
        if self.baseline.is_some()
            && self
                .samples
                .iter()
                .filter(|(at, _)| acquired - *at <= 1.0)
                .any(|(_, before)| (0..2).any(|index| bounds[index][1] < before[index][0] - 0.05))
        {
            return Ok(Some("buff_resources_unstable"));
        }
        self.samples.push((acquired, bounds));
        if self.baseline.is_some() {
            self.samples.retain(|(at, _)| acquired - *at <= 1.0);
        }
        Ok(None)
    }
    pub fn wait_check<E: BuffSiteEnvironment>(
        &mut self,
        env: &mut E,
        policy: &CharacterSurvivalPolicy,
        supplied: Option<BuffSafetyFrame<'_>>,
    ) -> Result<Option<&'static str>, E::Error> {
        if supplied.is_some() {
            return self.check(env, policy, supplied);
        }
        let deadline = env.clock() + 0.75;
        loop {
            let blocked = self.check(env, policy, None)?;
            if !matches!(
                blocked,
                Some("observation_unverified" | "buff_ground_unverified" | "screen_stale")
            ) || env.clock() >= deadline
            {
                return Ok(blocked);
            }
            env.wait(0.02)?;
        }
    }
    pub fn prepare<E: BuffSiteEnvironment>(
        &mut self,
        env: &mut E,
        policy: &CharacterSurvivalPolicy,
    ) -> Result<Value, GuardPreparationError<E::Error>> {
        let began = env.clock();
        while env.clock() - began < 8.0 {
            let blocked = self
                .check(env, policy, None)
                .map_err(GuardPreparationError::Other)?;
            match blocked {
                Some("observation_unverified" | "buff_ground_unverified" | "screen_stale") => {
                    self.samples.clear()
                }
                Some("buff_nearby_hostile") => return Err(GuardPreparationError::NearbyHostile),
                Some(reason) => return Err(GuardPreparationError::Other(runtime(reason))),
                None => {}
            }
            if let Some((first, last)) = self.samples.first().zip(self.samples.last())
                && last.0 - first.0 >= 1.0
            {
                let stable = (0..2).all(|index| {
                    let maximum = self
                        .samples
                        .iter()
                        .map(|(_, bounds)| bounds[index][0])
                        .fold(f64::NEG_INFINITY, f64::max);
                    let minimum = self
                        .samples
                        .iter()
                        .map(|(_, bounds)| bounds[index][1])
                        .fold(f64::INFINITY, f64::min);
                    maximum - minimum <= 0.05
                });
                if stable {
                    self.baseline = Some(last.1);
                    return Ok(
                        json!({"window_seconds":last.0-first.0,"maximum_confirmed_ratio_change":0.05,"ratio_bounds":last.1}),
                    );
                }
                let last = *last;
                self.samples.clear();
                self.samples.push(last);
            }
            env.wait(0.02).map_err(GuardPreparationError::Other)?;
        }
        Err(GuardPreparationError::Other(runtime(
            "buff_resource_stability_timeout",
        )))
    }
}

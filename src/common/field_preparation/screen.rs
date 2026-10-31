use super::*;
use serde::Serialize;
use serde_json::Value;

pub struct AutomapSample {
    pub visible: bool,
    pub obscured: bool,
    pub evidence: Value,
}
pub trait PreparationScreen<E: PreparationEnvironment> {
    fn panels_open(&mut self, frame: &Arc<BgrFrame>) -> Result<bool, E::Error>;
    fn automap(&mut self, frame: &Arc<BgrFrame>) -> Result<AutomapSample, E::Error>;
}
#[derive(Serialize)]
pub struct AutomapPreparation {
    pub status: &'static str,
    pub tab_sent: bool,
    pub evidence: Value,
}
pub fn ensure_automap_on<E: PreparationEnvironment>(
    env: &mut E,
    policy: &CharacterSurvivalPolicy,
    screen: &mut impl PreparationScreen<E>,
    supervised_on: Option<&Value>,
    allow_toggle: bool,
    timeout: f64,
) -> Result<AutomapPreparation, E::Error> {
    if !(0.1..=3.0).contains(&timeout) {
        return Err(ConfigError::value("bounded automap observation policy required").into());
    }
    let mut toggled = false;
    let mut missing_since = None;
    let deadline = env.clock() + timeout;
    while env.clock() < deadline {
        if env.stopped() || !env.focused()? {
            return Err(runtime("automap preparation interrupted or unfocused"));
        }
        let acquired = env.clock();
        let frame = env.capture()?;
        let blocked = resource_guard(&env.hud(&frame)?, policy);
        if blocked.is_some() || screen.panels_open(&frame)? {
            return Err(runtime(format!(
                "automap preparation: {}",
                blocked.unwrap_or("panel_open")
            )));
        }
        let sample = screen.automap(&frame)?;
        if env.clock() - acquired >= 0.25 {
            continue;
        }
        if sample.visible {
            return Ok(AutomapPreparation {
                status: "on_observed",
                tab_sent: toggled,
                evidence: sample.evidence,
            });
        }
        if !toggled && let Some(proof) = supervised_on.filter(|proof| proof["automap_on"] == true) {
            let at = proof
                .get("observed_at")
                .and_then(Value::as_f64)
                .filter(|at| at.is_finite());
            if proof["source"] != "supervised_visual"
                || proof["area"] != "arcane_sanctuary"
                || !at.is_some_and(|at| (0.0..=15.0).contains(&(env.wall_clock() - at)))
            {
                return Err(runtime("fresh supervised automap ON proof required"));
            }
            return Ok(AutomapPreparation {
                status: "on_supervised",
                tab_sent: false,
                evidence: proof.clone(),
            });
        }
        let missing = *missing_since.get_or_insert(acquired);
        if allow_toggle && !toggled && !sample.obscured && acquired - missing >= 0.15 {
            if env.stopped() || !env.focused()? {
                return Err(runtime("automap preparation interrupted or unfocused"));
            }
            env.press("tab")?;
            toggled = true;
        }
        env.wait(0.02)?;
    }
    Err(runtime("automap ON unverified after bounded preparation"))
}
pub fn enable_fresh_room_item_labels<E: PreparationEnvironment>(
    env: &mut E,
    policy: &CharacterSurvivalPolicy,
    screen: &mut impl PreparationScreen<E>,
) -> Result<Value, E::Error> {
    if env.stopped() || !env.focused()? {
        return Err(runtime("item label preparation interrupted or unfocused"));
    }
    let acquired = env.clock();
    let frame = env.capture()?;
    let blocked = resource_guard(&env.hud(&frame)?, policy);
    if blocked.is_some() || screen.panels_open(&frame)? || env.clock() - acquired >= 0.25 {
        return Err(runtime(format!(
            "item label preparation: {}",
            blocked.unwrap_or("panel_or_stale_frame")
        )));
    }
    if env.stopped() || !env.focused()? {
        return Err(runtime("item label preparation interrupted or unfocused"));
    }
    env.press("alt")?;
    Ok(
        serde_json::json!({"status":"on_requested","alt_sent":true,"basis":"fresh_room_default_off_toggle_mode","visibility":"unverified_until_drop"}),
    )
}
pub fn ensure_weapon_set<E: PreparationEnvironment>(
    env: &mut E,
    expected: u8,
    timeout: f64,
    observe: &mut impl FnMut(&Arc<BgrFrame>) -> Result<Option<u8>, E::Error>,
) -> Result<Value, E::Error> {
    if ![1, 2].contains(&expected) {
        return Err(ConfigError::value("explicit primary weapon set required").into());
    }
    fn guard<E: PreparationEnvironment>(env: &E) -> Result<(), E::Error> {
        if env.stopped() || !env.focused()? {
            Err(runtime("weapon check interrupted or unfocused"))
        } else {
            Ok(())
        }
    }
    fn read<E: PreparationEnvironment>(
        env: &mut E,
        timeout: f64,
        predicate: impl Fn(u8) -> bool,
        observe: &mut impl FnMut(&Arc<BgrFrame>) -> Result<Option<u8>, E::Error>,
    ) -> Result<u8, E::Error> {
        let deadline = env.clock() + timeout;
        while env.clock() < deadline {
            guard(env)?;
            let acquired = env.clock();
            let frame = env.capture()?;
            let number = observe(&frame)?;
            if env.clock() - acquired < 0.25
                && let Some(number) = number.filter(|number| predicate(*number))
            {
                return Ok(number);
            }
            env.wait(0.02)?;
        }
        Err(runtime("weapon set unverified"))
    }
    guard(env)?;
    env.press("i")?;
    let before = read(env, timeout, |number| [1, 2].contains(&number), observe)?;
    if before != expected {
        guard(env)?;
        env.press("w")?;
        read(env, timeout, |number| number == expected, observe)?;
    }
    guard(env)?;
    env.press("i")?;
    Ok(serde_json::json!({"before":before,"after":expected,"swapped":before!=expected}))
}

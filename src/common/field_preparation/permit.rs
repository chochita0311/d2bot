use super::*;
use serde_json::Value;

fn finite(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite())
}
fn integer(value: Option<&Value>) -> Option<i64> {
    value.and_then(Value::as_i64)
}
pub fn validate_runtime_permit(proof: &Value, now: f64) -> Result<(), ConfigError> {
    if proof["source"] != "runtime_screen"
        || proof["area"] != "arcane_sanctuary"
        || proof["panels_closed"] != true
        || proof["belt_closed"] != true
        || integer(proof.get("nearby_candidates")) != Some(0)
        || proof["safety_scope"] != "current_nearby_candidates_and_stable_ratios"
        || !finite(proof.get("observed_at")).is_some_and(|at| (0.0..0.5).contains(&(now - at)))
    {
        return Err(ConfigError::value(
            "fresh runtime field observation required",
        ));
    }
    Ok(())
}
pub fn validate_permit(
    permit: &Value,
    character: &str,
    policy: &CharacterSurvivalPolicy,
    now: f64,
    buff: bool,
) -> Result<(), ConfigError> {
    let mut required = vec!["field", "monsters_clear", "belt_closed"];
    if !buff {
        required.extend(["buff_effect_verified", "equipment_restored"]);
    }
    if permit["character_id"] != character || required.iter().any(|key| permit[*key] != true) {
        return Err(ConfigError::value(
            "explicit supervised field/buff/equipment/belt confirmations required",
        ));
    }
    let at = finite(permit.get("observed_at"))
        .filter(|at| *at <= now)
        .ok_or_else(|| {
            ConfigError::value("valid nonfuture supervised observation time required")
        })?;
    let capacity: i64 = policy
        .belt_columns()
        .iter()
        .filter(|column| column.kind() == policy.potion_kind())
        .map(|column| i64::from(column.capacity()))
        .sum();
    if !integer(permit.get("potions_remaining"))
        .is_some_and(|count| (1..=capacity).contains(&count))
    {
        return Err(ConfigError::value(
            "confirmed nonempty purple belt required",
        ));
    }
    if !buff
        && !policy.buffs().is_empty()
        && let Some(casts) = permit.get("buff_casts").filter(|casts| !casts.is_object())
    {
        let name = match casts {
            Value::Null => "NoneType",
            Value::Bool(_) => "bool",
            Value::Number(number) => {
                if number.is_i64() || number.is_u64() {
                    "int"
                } else {
                    "float"
                }
            }
            Value::String(_) => "str",
            Value::Array(_) => "list",
            Value::Object(_) => "dict",
        };
        return Err(ConfigError::new(
            "AttributeError",
            format!("'{name}' object has no attribute 'get'"),
        ));
    }
    if !buff
        && policy.buffs().iter().any(|rule| {
            !finite(
                permit
                    .get("buff_casts")
                    .and_then(|casts| casts.get(rule.name())),
            )
            .is_some_and(|cast| {
                (0.0..rule.duration_seconds() - rule.renew_before_seconds()).contains(&(now - cast))
            })
        })
    {
        return Err(ConfigError::value(
            "confirmed buff already due or missing cast time",
        ));
    }
    if now - at > 15.0 {
        return Err(ConfigError::new(
            "StaleSupervision",
            format!(
                "supervised permit is stale (age {:.3}s; limit 15s)",
                now - at
            ),
        ));
    }
    Ok(())
}
pub fn validate_travel_permit(
    permit: &Value,
    policy: &CharacterSurvivalPolicy,
    now: f64,
) -> Result<(), ConfigError> {
    if permit["source"] == "runtime_screen" {
        return validate_runtime_permit(permit, now);
    }
    if permit
        .get("monsters_clear")
        .and_then(Value::as_bool)
        .is_none()
    {
        return Err(ConfigError::value(
            "current nearby hostile presence must be explicitly observed",
        ));
    }
    let mut supervised = permit.clone();
    supervised["monsters_clear"] = Value::Bool(true);
    validate_permit(&supervised, "flash", policy, now, true)
}
pub fn confirm_initial_probe(
    signal: &Value,
    character: &str,
    last_cast_wall: f64,
    now: f64,
) -> Result<(), ConfigError> {
    let observed = finite(signal.get("observed_at"));
    if signal["character_id"] != character
        || signal["source"] != "supervised_visual"
        || [
            "effect_verified",
            "equipment_restored",
            "sequence_effects_verified",
        ]
        .iter()
        .any(|key| signal[*key] != true)
        || !observed.is_some_and(|at| last_cast_wall <= at && at <= now)
    {
        return Err(ConfigError::value(
            "fresh post-sequence supervised effect/equipment proof required",
        ));
    }
    if let Some(at) = observed
        && now - at > 15.0
    {
        return Err(ConfigError::new(
            "StaleSupervision",
            format!(
                "post-sequence proof is stale (age {:.3}s; limit 15s)",
                now - at
            ),
        ));
    }
    Ok(())
}

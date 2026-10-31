use super::ConfigError;
use serde_json::{Map, Value, json};
use std::cmp::Ordering;
use std::collections::HashSet;

fn fields<'a>(
    raw: &'a Value,
    allowed: &[&str],
    required: &[&str],
    name: &str,
) -> Result<&'a Map<String, Value>, ConfigError> {
    let raw = raw
        .as_object()
        .ok_or_else(|| ConfigError::value(format!("{name} must be an object")))?;
    if raw.keys().any(|key| !allowed.contains(&key.as_str()))
        || required.iter().any(|key| !raw.contains_key(*key))
    {
        return Err(ConfigError::value(format!(
            "{name} has missing or unsupported fields"
        )));
    }
    Ok(raw)
}

fn exact<'a>(
    raw: &'a Value,
    names: &[&str],
    name: &str,
) -> Result<&'a Map<String, Value>, ConfigError> {
    if !raw.is_object() {
        return Err(ConfigError::value(format!(
            "{name} has missing or unsupported fields"
        )));
    }
    fields(raw, names, names, name)
}

fn text<'a>(raw: &'a Value, name: &str) -> Result<&'a str, ConfigError> {
    raw.as_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ConfigError::value(format!("{name} must be a non-empty string")))
}

fn decimal_cmp(left: &str, right: &str) -> Ordering {
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

fn integer(
    raw: &Value,
    name: &str,
    minimum: u64,
    maximum: Option<u64>,
    cast: bool,
) -> Result<String, ConfigError> {
    let message = if cast {
        format!("{name} must be an integer >= {minimum}")
    } else {
        format!("{name} must be an integer in range")
    };
    let value = raw
        .as_number()
        .map(ToString::to_string)
        .ok_or_else(|| ConfigError::value(&message))?;
    let digits = value.strip_prefix('-').unwrap_or(&value);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ConfigError::value(message));
    }
    let digits = digits.trim_start_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    if (value.starts_with('-') && digits != "0")
        || decimal_cmp(digits, &minimum.to_string()).is_lt()
        || maximum.is_some_and(|maximum| decimal_cmp(digits, &maximum.to_string()).is_gt())
    {
        return Err(ConfigError::value(message));
    }
    Ok(digits.to_owned())
}

fn small_integer(raw: &Value, name: &str, minimum: u64, maximum: u64) -> Result<u64, ConfigError> {
    integer(raw, name, minimum, Some(maximum), false)?
        .parse()
        .map_err(|_| ConfigError::value(format!("{name} must be an integer in range")))
}

fn number(raw: &Value, name: &str, maximum: Option<f64>) -> Result<f64, ConfigError> {
    let value = raw
        .as_number()
        .and_then(|number| number.as_f64())
        .filter(|value| value.is_finite())
        .ok_or_else(|| ConfigError::value(format!("{name} must be a finite number")))?;
    if value < 0.0 || maximum.is_some_and(|maximum| value > maximum) {
        return Err(ConfigError::value(format!("{name} is out of range")));
    }
    Ok(value)
}

pub(crate) fn casting_profile(raw: &Value) -> Result<(), ConfigError> {
    let raw = exact(
        raw,
        &["rule_set_id", "skill_id", "form", "fcr"],
        "character casting",
    )?;
    for name in ["rule_set_id", "skill_id", "form"] {
        text(&raw[name], name)?;
    }
    if !raw["fcr"].is_null() {
        integer(&raw["fcr"], "fcr", 0, None, true)?;
    }
    Ok(())
}

pub(crate) fn casting_catalog(raw: &Value) -> Result<(), ConfigError> {
    let items = raw
        .as_array()
        .ok_or_else(|| ConfigError::value("casting_rule_sets must be a list"))?;
    let mut ids = Vec::new();
    for item in items {
        let item = exact(
            item,
            &[
                "rule_set_id",
                "ruleset_family",
                "revision",
                "source_url",
                "evidence_status",
                "rules",
            ],
            "casting rule set",
        )?;
        let rules = item["rules"]
            .as_array()
            .ok_or_else(|| ConfigError::value("rules must be a list"))?;
        let mut contexts = Vec::new();
        for rule in rules {
            let rule = exact(
                rule,
                &["character_class", "skill_id", "form", "breakpoints"],
                "casting rule",
            )?;
            let points = rule["breakpoints"]
                .as_array()
                .ok_or_else(|| ConfigError::value("breakpoints must be a list"))?;
            let mut parsed = Vec::new();
            for point in points {
                let point = exact(point, &["minimum_fcr", "frames"], "cast breakpoint")?;
                let fcr = integer(&point["minimum_fcr"], "minimum_fcr", 0, None, true)?;
                let frames = integer(&point["frames"], "frames", 1, None, true)?;
                parsed.push((fcr, frames));
            }
            let context = (
                text(&rule["character_class"], "character_class")?,
                text(&rule["skill_id"], "skill_id")?,
                text(&rule["form"], "form")?,
            );
            let first = parsed
                .first()
                .ok_or_else(|| ConfigError::value("breakpoints must contain cast breakpoints"))?;
            if first.0 != "0" {
                return Err(ConfigError::value("breakpoints must start at zero FCR"));
            }
            if parsed.windows(2).any(|pair| {
                !decimal_cmp(&pair[1].0, &pair[0].0).is_gt()
                    || !decimal_cmp(&pair[1].1, &pair[0].1).is_lt()
            }) {
                return Err(ConfigError::value(
                    "FCR must increase and full animation frames must decrease",
                ));
            }
            contexts.push(context);
        }
        for name in ["rule_set_id", "ruleset_family", "revision", "source_url"] {
            text(&item[name], name)?;
        }
        if item["evidence_status"] != "reference" {
            return Err(ConfigError::value(
                "only reference casting rules are supported",
            ));
        }
        if rules.is_empty() {
            return Err(ConfigError::value("rules must contain casting rules"));
        }
        if contexts.iter().copied().collect::<HashSet<_>>().len() != contexts.len() {
            return Err(ConfigError::value(
                "casting contexts must be unique within a rule set",
            ));
        }
        ids.push(text(&item["rule_set_id"], "rule_set_id")?);
    }
    if ids.iter().copied().collect::<HashSet<_>>().len() != ids.len() {
        return Err(ConfigError::value("casting rule-set IDs must be unique"));
    }
    Ok(())
}

pub(crate) fn survival(raw: &Value) -> Result<Value, ConfigError> {
    let required = [
        "life_potion_below",
        "mana_potion_below",
        "potion_kind",
        "replenish_at_or_below",
        "exit_on_empty",
        "belt_toggle_key",
        "belt_columns",
    ];
    let mut allowed = required.to_vec();
    allowed.extend(["buffs", "potion_pickup"]);
    let raw = fields(raw, &allowed, &required, "survival")?;
    let buffs = raw.get("buffs").cloned().unwrap_or(json!([]));
    let columns = raw["belt_columns"]
        .as_array()
        .ok_or_else(|| ConfigError::value("belt_columns and buffs must be lists"))?;
    let buffs_array = buffs
        .as_array()
        .ok_or_else(|| ConfigError::value("belt_columns and buffs must be lists"))?;
    let mut column_ids = HashSet::new();
    let mut keys = HashSet::new();
    for column in columns {
        let column = fields(
            column,
            &["column", "key", "kind", "capacity", "target_count"],
            &["column", "key", "kind", "capacity", "target_count"],
            "belt column",
        )?;
        let id = small_integer(&column["column"], "belt.column", 1, 4)?;
        let key = text(&column["key"], "belt.key")?;
        text(&column["kind"], "belt.kind")?;
        let capacity = small_integer(&column["capacity"], "belt.capacity", 1, 4)?;
        if !column["target_count"].is_null() {
            small_integer(&column["target_count"], "belt.target_count", 0, capacity)?;
        }
        column_ids.insert(id);
        keys.insert(key);
    }
    let mut buff_names = HashSet::new();
    for buff in buffs_array {
        let buff = fields(
            buff,
            &["name", "key", "duration_seconds", "renew_before_seconds"],
            &["name", "key", "duration_seconds", "renew_before_seconds"],
            "buff",
        )?;
        buff_names.insert(text(&buff["name"], "buff.name")?);
        text(&buff["key"], "buff.key")?;
        let duration = number(&buff["duration_seconds"], "buff.duration_seconds", None)?;
        let margin = number(
            &buff["renew_before_seconds"],
            "buff.renew_before_seconds",
            None,
        )?;
        if duration == 0.0 || margin >= duration {
            return Err(ConfigError::value(
                "buff renewal margin must be shorter than a positive duration",
            ));
        }
    }
    let mut pickup = raw.get("potion_pickup").cloned().unwrap_or(Value::Null);
    if !pickup.is_null() {
        let pickup_raw = fields(
            &pickup,
            &["preferred_kind", "fallback_kind", "fallback_at_or_below"],
            &["preferred_kind"],
            "potion_pickup",
        )?;
        let preferred = text(
            &pickup_raw["preferred_kind"],
            "potion_pickup.preferred_kind",
        )?;
        let kind = pickup_raw
            .get("fallback_kind")
            .cloned()
            .unwrap_or(Value::Null);
        let limit = pickup_raw
            .get("fallback_at_or_below")
            .cloned()
            .unwrap_or(Value::Null);
        if kind.is_null() != limit.is_null() {
            return Err(ConfigError::value(
                "fallback kind and count threshold must be configured together",
            ));
        }
        if !kind.is_null() {
            text(&kind, "potion_pickup.fallback_kind")?;
            integer(&limit, "potion_pickup.fallback_at_or_below", 0, None, false)?;
            if pickup_raw["preferred_kind"] == kind {
                return Err(ConfigError::value(
                    "preferred and fallback potion kinds must differ",
                ));
            }
        }
        pickup =
            json!({"preferred_kind":preferred,"fallback_kind":kind,"fallback_at_or_below":limit});
    }
    for name in ["life_potion_below", "mana_potion_below"] {
        if number(&raw[name], name, Some(1.0))? == 0.0 {
            return Err(ConfigError::value(format!(
                "{name} must be greater than zero"
            )));
        }
    }
    let potion_kind = text(&raw["potion_kind"], "potion_kind")?;
    text(&raw["belt_toggle_key"], "belt_toggle_key")?;
    if !raw["exit_on_empty"].is_boolean() {
        return Err(ConfigError::value("exit_on_empty must be a boolean"));
    }
    if columns.is_empty() {
        return Err(ConfigError::value(
            "belt_columns must contain column policies",
        ));
    }
    if column_ids.len() != columns.len() {
        return Err(ConfigError::value("belt column numbers must be unique"));
    }
    if keys.len() != columns.len() {
        return Err(ConfigError::value("belt usage keys must be unique"));
    }
    if buff_names.len() != buffs_array.len() {
        return Err(ConfigError::value("buff names must be unique"));
    }
    let potions = columns
        .iter()
        .filter(|column| column["kind"] == potion_kind)
        .collect::<Vec<_>>();
    if potions.is_empty()
        || potions
            .iter()
            .any(|column| column["target_count"].is_null())
    {
        return Err(ConfigError::value(
            "potion columns require explicit targets",
        ));
    }
    let target = potions
        .iter()
        .map(|column| small_integer(&column["target_count"], "belt.target_count", 0, 4))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .sum::<u64>();
    if target == 0 {
        return Err(ConfigError::value("potion target must be positive"));
    }
    small_integer(
        &raw["replenish_at_or_below"],
        "replenish_at_or_below",
        0,
        target,
    )?;
    if !pickup.is_null() && !pickup["fallback_at_or_below"].is_null() {
        small_integer(
            &pickup["fallback_at_or_below"],
            "potion_pickup.fallback_at_or_below",
            0,
            target,
        )?;
    }
    Ok(json!({
        "life_potion_below":raw["life_potion_below"],"mana_potion_below":raw["mana_potion_below"],"potion_kind":raw["potion_kind"],
        "replenish_at_or_below":raw["replenish_at_or_below"],"exit_on_empty":raw["exit_on_empty"],"belt_toggle_key":raw["belt_toggle_key"],
        "belt_columns":raw["belt_columns"],"buffs":buffs,"potion_pickup":pickup
    }))
}

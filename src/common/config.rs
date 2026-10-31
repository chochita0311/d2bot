//! 기존 설정의 문서 의미를 보존하고 실행 타입 변환은 소비자에게 맡긴다.

pub(crate) mod schema;

use serde_json::{Map, Value, json};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct ConfigError {
    kind: &'static str,
    message: String,
}

impl ConfigError {
    pub fn kind(&self) -> &'static str {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub(crate) fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub(crate) fn value(message: impl Into<String>) -> Self {
        Self::new("ValueError", message)
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.kind, self.message)
    }
}

impl Error for ConfigError {}

/// A repaired optional override. Runtime consumers use their existing missing-key default.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigWarning {
    /// JSON pointer into the input document, including the original token spelling.
    pub path: String,
    pub original: Value,
    pub message: &'static str,
}
impl fmt::Display for ConfigWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {} (original: {})",
            self.path, self.message, self.original
        )
    }
}

fn pointer_segment(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

impl From<std::io::Error> for ConfigError {
    fn from(error: std::io::Error) -> Self {
        let kind = match error.kind() {
            std::io::ErrorKind::NotFound => "FileNotFoundError",
            std::io::ErrorKind::PermissionDenied => "PermissionError",
            std::io::ErrorKind::InvalidData => "UnicodeDecodeError",
            _ => "OSError",
        };
        Self::new(kind, error.to_string())
    }
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::Number(number) => {
            if number.to_string().contains(['.', 'e', 'E']) {
                "float"
            } else {
                "int"
            }
        }
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Object(_) => "dict",
    }
}

fn object(value: &Value) -> Result<&Map<String, Value>, ConfigError> {
    object_method(value, "get")
}

fn object_method<'a>(
    value: &'a Value,
    method: &str,
) -> Result<&'a Map<String, Value>, ConfigError> {
    value.as_object().ok_or_else(|| {
        ConfigError::new(
            "AttributeError",
            format!("'{}' object has no attribute '{method}'", type_name(value)),
        )
    })
}

fn get(raw: &Map<String, Value>, name: &str, default: Value) -> Value {
    raw.get(name).cloned().unwrap_or(default)
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64() != Some(0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}

fn list(value: Value) -> Result<Vec<Value>, ConfigError> {
    match value {
        Value::Array(items) => Ok(items),
        Value::Object(items) => Ok(items
            .into_iter()
            .map(|(key, _)| Value::String(key))
            .collect()),
        Value::String(text) => Ok(text
            .chars()
            .map(|character| Value::String(character.to_string()))
            .collect()),
        value => Err(ConfigError::new(
            "TypeError",
            format!("'{}' object is not iterable", type_name(&value)),
        )),
    }
}

fn model(raw: &Value, defaults: Value, constructor: Option<&str>) -> Result<Value, ConfigError> {
    let raw = if constructor.is_some() {
        raw.as_object().ok_or_else(|| {
            ConfigError::new(
                "TypeError",
                format!(
                    "type object argument after ** must be a mapping, not {}",
                    type_name(raw)
                ),
            )
        })?
    } else {
        object(raw)?
    };
    let mut output = defaults
        .as_object()
        .cloned()
        .ok_or_else(|| ConfigError::value("internal model defaults must be an object"))?;
    if constructor.is_some()
        && let Some(key) = raw.keys().find(|key| !output.contains_key(*key))
    {
        return Err(ConfigError::new(
            "TypeError",
            format!("__init__() got an unexpected keyword argument '{key}'"),
        ));
    }
    for (key, value) in &mut output {
        if let Some(value_raw) = raw.get(key) {
            *value = value_raw.clone();
        }
    }
    Ok(Value::Object(output))
}

fn list_fields(value: &mut Value, names: &[&str]) -> Result<(), ConfigError> {
    let fields = value
        .as_object_mut()
        .ok_or_else(|| ConfigError::value("internal model must be an object"))?;
    for name in names {
        let converted = list(fields.get(*name).cloned().unwrap_or(Value::Null))?;
        fields.insert((*name).to_owned(), Value::Array(converted));
    }
    Ok(())
}

fn required_model(raw: &Value, defaults: Value, required: &[&str]) -> Result<Value, ConfigError> {
    let fields = raw.as_object().ok_or_else(|| {
        ConfigError::new(
            "TypeError",
            match raw {
                Value::Array(_) => "list indices must be integers or slices, not str".to_owned(),
                Value::String(_) => "string indices must be integers".to_owned(),
                value => format!("'{}' object is not subscriptable", type_name(value)),
            },
        )
    })?;
    for name in required {
        if !fields.contains_key(*name) {
            return Err(ConfigError::new("KeyError", format!("'{name}'")));
        }
    }
    model(raw, defaults, None)
}

fn template(raw: &Value) -> Result<Value, ConfigError> {
    required_model(
        raw,
        json!({"name":null,"path":null,"threshold":0.85,"action":"log","label":null,"context":"any"}),
        &["name", "path"],
    )
}

fn actions(
    raw: &Value,
    character: &str,
    warnings: &mut Vec<ConfigWarning>,
) -> Result<Value, ConfigError> {
    let mut output = model(
        raw,
        json!({
            "movement_skill_key":null,"movement_travel_mode":"hold","movement_reposition_mode":"press",
            "primary_attack_skill_key":null,"left_skill_key":null,"interact_skill_key":null,
            "buff_skill_keys":[],"pre_run_buff_order":[],"buff_action_pause_seconds":{},
            "attack_pattern":"single_skill","engagement_style":"safe"
        }),
        None,
    )?;
    list_fields(&mut output, &["buff_skill_keys", "pre_run_buff_order"])?;
    let pauses = object_method(&output["buff_action_pause_seconds"], "items")?;
    let mut converted = Map::new();
    for (key, value) in pauses {
        let seconds = match value {
            Value::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
            Value::Number(value) => value.as_f64(),
            Value::String(value) => {
                let text = value.trim();
                let bytes = text.as_bytes();
                let valid_separators = bytes.iter().enumerate().all(|(index, byte)| {
                    *byte != b'_'
                        || (index > 0
                            && index + 1 < bytes.len()
                            && bytes[index - 1].is_ascii_digit()
                            && bytes[index + 1].is_ascii_digit())
                });
                if valid_separators {
                    text.replace('_', "").parse::<f64>().ok()
                } else {
                    None
                }
            }
            _ => None,
        }
        .filter(|value| {
            value.is_finite()
                && *value >= 0.0
                && std::time::Duration::try_from_secs_f64(*value).is_ok()
        });
        let normalized = key.to_lowercase();
        if let Some(seconds) = seconds {
            converted.insert(normalized, json!(seconds));
        } else {
            // Do not let NaN/overflow reach clocks or waits. Omitting the override
            // preserves each consumer's own default instead of inventing a new gap.
            converted.remove(&normalized);
            warnings.push(ConfigWarning {
                path: format!(
                    "/characters/{}/actions/buff_action_pause_seconds/{}",
                    pointer_segment(character),
                    pointer_segment(key)
                ),
                original: value.clone(),
                message: "invalid buff pause override removed; using the consumer's default pause",
            });
        }
    }
    output["buff_action_pause_seconds"] = Value::Object(converted);
    Ok(output)
}

fn character(
    name: &str,
    raw: &Value,
    warnings: &mut Vec<ConfigWarning>,
) -> Result<Value, ConfigError> {
    let mut output = model(
        raw,
        json!({
            "display_name":name,"progression_mode":"standard","ruleset_family":"rotw","preferred_run_profile":null,
            "actions":{},"survival":null,"character_class":null,"casting":null
        }),
        None,
    )?;
    // Python과 같은 순서로 하위 정책을 읽은 뒤 캐릭터 class를 검증한다.
    output["actions"] = actions(&output["actions"], name, warnings)?;
    if !output["survival"].is_null() {
        output["survival"] = schema::survival(&output["survival"])?;
    }
    if !output["casting"].is_null() {
        schema::casting_profile(&output["casting"])?;
    }
    if !output["character_class"].is_null()
        && output["character_class"]
            .as_str()
            .is_none_or(|value| value.trim().is_empty())
    {
        return Err(ConfigError::value(
            "character_class must be a non-empty string or None",
        ));
    }
    Ok(output)
}

fn farm(name: &str, raw: &Value) -> Result<Value, ConfigError> {
    let mut output = model(
        raw,
        json!({"name":name,"goal":"watch","description":"","templates":[],"hunting":{},"loot":{},"life":{},"run_specific_rules":[]}),
        None,
    )?;
    output["templates"] = list(output["templates"].clone())?
        .iter()
        .map(template)
        .collect::<Result<Vec<_>, _>>()?
        .into();
    let combat_reaction_radius = output["hunting"].get("combat_reaction_radius").cloned();
    output["hunting"] = model(
        &output["hunting"],
        json!({
            "objective":"watch","waypoint_act":null,"waypoint_name":null,"target_monsters":[],"target_areas":[],
            "route_notes":[],"fight_style":"safe","search_timeout_seconds":60,"disengage_on_uncertainty":true
        }),
        None,
    )?;
    if let Some(radius) = combat_reaction_radius {
        output["hunting"]["combat_reaction_radius"] = radius;
    }
    list_fields(
        &mut output["hunting"],
        &["target_monsters", "target_areas", "route_notes"],
    )?;
    output["loot"] = model(
        &output["loot"],
        json!({"keep_labels":[],"ignore_labels":[],"potion_columns_reserved":2,"free_inventory_slots_min":6,"identify_before_drop":false,"pickup_gold":false}),
        None,
    )?;
    list_fields(&mut output["loot"], &["keep_labels", "ignore_labels"])?;
    output["life"] = model(
        &output["life"],
        json!({
            "use_healing_potion_below":0.65,"use_rejuvenation_below":0.35,"emergency_retreat_below":0.2,"use_mana_potion_below":0.3,
            "town_portal_on_risk":true,"stop_on_death_screen":true,"belt_restock_healing_below":4,"belt_restock_mana_below":4
        }),
        None,
    )?;
    list_fields(&mut output, &["run_specific_rules"])?;
    Ok(output)
}

fn shared_loot(raw: &Value) -> Result<Value, ConfigError> {
    let mut output = model(
        raw,
        json!({"description":"","ignore_labels":[],"fixed_items":[]}),
        None,
    )?;
    list_fields(&mut output, &["ignore_labels"])?;
    output["fixed_items"] = list(output["fixed_items"].clone())?.iter().map(|raw| {
        required_model(raw, json!({"label":null,"ground_template":null,"inventory_template":null,"threshold":0.85}), &["label"])
    }).collect::<Result<Vec<_>,_>>()?.into();
    Ok(output)
}

fn read_json(path: &Path) -> Result<Value, ConfigError> {
    let text = fs::read_to_string(path)?;
    serde_json::from_str(text.strip_prefix('\u{feff}').unwrap_or(&text))
        .map_err(|error| ConfigError::new("JSONDecodeError", error.to_string()))
}

fn json_paths(path: &Path, output: &mut Vec<PathBuf>) -> Result<(), ConfigError> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let child = entry.path();
        if child
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        {
            output.push(child.clone());
        }
        if entry.file_type()?.is_dir() {
            json_paths(&child, output)?;
        }
    }
    Ok(())
}

fn merge(base: &mut Map<String, Value>, incoming: Value) -> Result<(), ConfigError> {
    let incoming = incoming.as_object().ok_or_else(|| {
        ConfigError::new(
            "AttributeError",
            format!("'{}' object has no attribute 'items'", type_name(&incoming)),
        )
    })?;
    for (key, value) in incoming {
        if let (Some(Value::Object(current)), Value::Object(_)) = (base.get_mut(key), value) {
            merge(current, value.clone())?;
        } else {
            base.insert(key.clone(), value.clone());
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
enum FarmSource {
    Registered(String),
    Standalone(Value),
}

#[derive(Debug, Clone)]
pub struct BotConfig {
    data: Map<String, Value>,
    pub active_character: Option<String>,
    farm_source: FarmSource,
    warnings: Vec<ConfigWarning>,
}

impl BotConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let raw = if path.is_dir() {
            let mut paths = Vec::new();
            json_paths(path, &mut paths)?;
            paths.sort_by_key(|path| {
                path.components()
                    .map(|part| part.as_os_str().to_string_lossy().to_lowercase())
                    .collect::<Vec<_>>()
            });
            let mut merged = Map::new();
            for path in paths {
                merge(&mut merged, read_json(&path)?)?;
            }
            Value::Object(merged)
        } else {
            read_json(path)?
        };
        Self::from_value(raw)
    }

    pub fn from_value(raw: Value) -> Result<Self, ConfigError> {
        let raw = object(&raw)?;
        let capture = model(
            &get(raw, "capture", json!({})),
            json!({"fps":8,"monitor_index":1,"region":null,"window_title":null,"window_title_mode":"contains","follow_window":true,"capture_backend":"auto","preview_scale":0.75,"reference_layout":null}),
            Some("CaptureConfig"),
        )?;
        let recording = model(
            &get(raw, "recording", json!({})),
            json!({"enabled":false,"output_path":"recordings/session.avi","directory":"recordings","codec":"XVID"}),
            Some("RecordingConfig"),
        )?;
        let gui = model(
            &get(raw, "gui", json!({})),
            json!({"visible_log_lines":300}),
            Some("GuiConfig"),
        )?;
        let hotkeys = model(
            &get(raw, "hotkeys", json!({})),
            json!({"pause":"f8","stop":"f9"}),
            Some("HotkeyConfig"),
        )?;
        let loot = shared_loot(&get(raw, "shared_loot", json!({})))?;
        let mut characters = Map::new();
        let mut warnings = Vec::new();
        for (name, raw) in object_method(&get(raw, "characters", json!({})), "items")? {
            characters.insert(name.clone(), character(name, raw, &mut warnings)?);
        }
        let mut profiles = Map::new();
        for (name, raw) in object_method(&get(raw, "run_profiles", json!({})), "items")? {
            profiles.insert(name.clone(), farm(name, raw)?);
        }
        let legacy = get(raw, "farm", json!({}));
        let farm_source = if !truthy(&legacy) {
            match profiles.keys().next() {
                Some(name) => FarmSource::Registered(name.clone()),
                None => FarmSource::Standalone(farm("run", &json!({}))?),
            }
        } else {
            FarmSource::Standalone(farm("farm", &legacy)?)
        };
        let active_character = characters.keys().next().cloned();
        let catalog = get(raw, "casting_rule_sets", json!([]));
        schema::casting_catalog(&catalog)?;
        let data = json!({
            "dry_run":get(raw,"dry_run",json!(true)),"overlay":get(raw,"overlay",json!(true)),"log_level":get(raw,"log_level",json!("INFO")),
            "capture":capture,"recording":recording,"gui":gui,"hotkeys":hotkeys,"shared_loot":loot,"characters":characters,
            "active_character":null,"run_profiles":profiles,"farm":null,"casting_rules":{"rule_sets":catalog}
        });
        let mut result = Self {
            data: data
                .as_object()
                .cloned()
                .ok_or_else(|| ConfigError::value("internal config must be an object"))?,
            active_character,
            farm_source,
            warnings,
        };
        let first = result.active_character.clone();
        result.apply_character_selection(first.as_deref())?;
        Ok(result)
    }

    pub fn active_character_key(&self) -> Option<&str> {
        let characters = self.data.get("characters")?.as_object()?;
        self.active_character
            .as_deref()
            .filter(|key| characters.contains_key(*key))
            .or_else(|| characters.keys().next().map(String::as_str))
    }

    pub fn active_character_profile(&self) -> Option<&Value> {
        self.data
            .get("characters")?
            .get(self.active_character_key()?)
    }

    /// Casting deliberately uses the explicitly active key, without the UI selection fallback.
    pub fn active_casting_estimate(&self) -> Result<super::casting::CastingEstimate, ConfigError> {
        self.casting_estimate_for(self.active_character.as_deref())
    }
    pub fn casting_estimate_for(
        &self,
        character_key: Option<&str>,
    ) -> Result<super::casting::CastingEstimate, ConfigError> {
        let profile = character_key.and_then(|key| self.data.get("characters")?.get(key));
        let Some(profile) = profile else {
            return Ok(super::casting::CastingEstimate::character_missing());
        };
        let catalog =
            super::casting::CastingCatalog::from_value(&self.data["casting_rules"]["rule_sets"])?;
        let casting = if profile["casting"].is_null() {
            None
        } else {
            Some(super::casting::CharacterCastingProfile::from_value(
                &profile["casting"],
            )?)
        };
        let family = profile["ruleset_family"]
            .as_str()
            .ok_or_else(|| ConfigError::value("ruleset_family must be a non-empty string"))?;
        catalog.estimate(
            family,
            profile["character_class"].as_str(),
            casting.as_ref(),
        )
    }

    pub fn apply_character_selection(&mut self, key: Option<&str>) -> Result<(), ConfigError> {
        let profile = key.and_then(|key| self.data.get("characters")?.get(key));
        let Some(profile) = profile else {
            self.active_character = self.active_character_key().map(str::to_owned);
            return Ok(());
        };
        self.active_character = key.map(str::to_owned);
        let preferred = &profile["preferred_run_profile"];
        if truthy(preferred) {
            if preferred.is_array() || preferred.is_object() {
                return Err(ConfigError::new(
                    "TypeError",
                    format!("unhashable type: '{}'", type_name(preferred)),
                ));
            }
            if let Some(name) = preferred
                .as_str()
                .filter(|name| self.data["run_profiles"].get(*name).is_some())
            {
                self.farm_source = FarmSource::Registered(name.to_owned());
            }
        }
        Ok(())
    }

    pub fn farm(&self) -> &Value {
        match &self.farm_source {
            FarmSource::Registered(key) => &self.data["run_profiles"][key],
            FarmSource::Standalone(value) => value,
        }
    }

    pub fn run_profile_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.data.get_mut("run_profiles")?.get_mut(key)
    }
    /// 제어판 선택은 정규화된 설정의 해당 필드만 바꾸며 파일에는 쓰지 않는다.
    pub fn set_capture_source(
        &mut self,
        window_title: Option<String>,
        backend: &str,
    ) -> Result<(), ConfigError> {
        let capture = self
            .data
            .get_mut("capture")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| ConfigError::value("capture settings missing"))?;
        capture.insert("window_title".into(), json!(window_title));
        capture.insert(
            "capture_backend".into(),
            json!(if backend.is_empty() { "auto" } else { backend }),
        );
        Ok(())
    }

    pub fn section(&self, name: &str) -> Option<&Value> {
        // These two values are derived, never read the stored snapshot placeholders.
        match name {
            "farm" => Some(self.farm()),
            "active_character" => None,
            _ => self.data.get(name),
        }
    }

    pub fn warnings(&self) -> &[ConfigWarning] {
        &self.warnings
    }

    pub fn snapshot(&self) -> Value {
        let mut snapshot = self.data.clone();
        snapshot.insert("active_character".to_owned(), json!(self.active_character));
        snapshot.insert("farm".to_owned(), self.farm().clone());
        Value::Object(snapshot)
    }
}

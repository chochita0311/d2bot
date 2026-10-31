//! 실행 정의는 설정을 빌리고, 상태·경로·공통 기능의 설명만 소유한다.
use crate::common::config::ConfigError;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
#[derive(Debug, Clone, Serialize)]
pub struct RunPayloadState {
    pub key: &'static str,
    pub description: &'static str,
    pub success_signal: &'static str,
    pub next_action: &'static str,
}
#[derive(Debug, Clone, Serialize)]
pub struct RunPort {
    pub key: &'static str,
    pub kind: &'static str,
    pub description: &'static str,
    pub status: &'static str,
    pub reusable: bool,
    pub notes: &'static [&'static str],
}
#[derive(Debug, Clone, Serialize)]
pub struct RunRouteSegment {
    pub key: &'static str,
    pub label: &'static str,
    pub direction: &'static str,
    pub status: &'static str,
    pub notes: &'static [&'static str],
}
#[derive(Serialize)]
pub struct RunDefinition<'a> {
    pub profile_id: &'static str,
    pub profile: &'a Value,
    pub payload_states: &'static [RunPayloadState],
    pub route_segments: &'static [RunRouteSegment],
    pub decision_ports: &'static [RunPort],
    pub shared_ports: &'static [RunPort],
    pub metadata: BTreeMap<&'static str, &'static str>,
}
fn display(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Null => "None".into(),
        Value::Bool(value) => if *value { "True" } else { "False" }.into(),
        _ => value.to_string(),
    }
}
fn truth(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::String(value) => !value.is_empty(),
        Value::Number(value) => value.as_f64().is_none_or(|v| v != 0.0),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}
fn joined(value: &Value) -> Result<String, ConfigError> {
    let values = value
        .as_array()
        .ok_or_else(|| ConfigError::value("target list required"))?;
    let text = values
        .iter()
        .enumerate()
        .map(|(index, v)| {
            v.as_str().ok_or_else(|| {
                let kind = match v {
                    Value::Null => "NoneType",
                    Value::Bool(_) => "bool",
                    Value::Number(n) => {
                        if n.to_string().contains(['.', 'e', 'E']) {
                            "float"
                        } else {
                            "int"
                        }
                    }
                    Value::Array(_) => "list",
                    Value::Object(_) => "dict",
                    Value::String(_) => "str",
                };
                ConfigError::new(
                    "TypeError",
                    format!("sequence item {index}: expected str instance, {kind} found"),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?
        .join(", ");
    Ok(if text.is_empty() {
        "(not configured)".into()
    } else {
        text
    })
}
impl RunDefinition<'_> {
    pub fn summarize(&self) -> Result<Vec<String>, ConfigError> {
        let hunting = &self.profile["hunting"];
        let waypoint = if truth(&hunting["waypoint_act"]) && truth(&hunting["waypoint_name"]) {
            format!(
                "Act {} / {}",
                display(&hunting["waypoint_act"]),
                display(&hunting["waypoint_name"])
            )
        } else {
            "(not configured)".into()
        };
        let route = self
            .route_segments
            .iter()
            .filter(|v| v.status == "ready")
            .map(|v| v.label)
            .collect::<Vec<_>>()
            .join(", ");
        let ports = self
            .decision_ports
            .iter()
            .chain(self.shared_ports)
            .map(|v| v.key)
            .collect::<Vec<_>>()
            .join(", ");
        Ok(vec![
            format!("goal: {}", display(&self.profile["goal"])),
            format!("waypoint: {waypoint}"),
            format!("targets: {}", joined(&hunting["target_monsters"])?),
            format!("areas: {}", joined(&hunting["target_areas"])?),
            format!("fight style: {}", display(&hunting["fight_style"])),
            format!(
                "search timeout: {}s",
                display(&hunting["search_timeout_seconds"])
            ),
            format!(
                "ready route pieces: {}",
                if route.is_empty() {
                    "(route pieces not ready)"
                } else {
                    &route
                }
            ),
            format!(
                "shared runtime pieces: {}",
                if ports.is_empty() {
                    "(none yet)"
                } else {
                    &ports
                }
            ),
            "wrapper: room create/exit stays outside this payload".into(),
        ])
    }
}

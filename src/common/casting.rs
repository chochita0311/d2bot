//! Immutable reference rules. Estimates describe a source table, not measured timing.

use super::config::{ConfigError, schema};
use super::time;
use serde::{Deserialize, Serialize};
use serde_json::{Number, Value};
use std::cmp::Ordering;

/// A validated nonnegative JSON integer, including values larger than u64.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(transparent)]
struct ReferenceInteger(Number);

impl ReferenceInteger {
    fn digits(&self) -> String {
        let spelling = self.0.to_string();
        let digits = spelling.trim_start_matches('-').trim_start_matches('0');
        if digits.is_empty() {
            "0".into()
        } else {
            digits.into()
        }
    }

    fn compare(&self, other: &Self) -> Ordering {
        let left = self.digits();
        let right = other.digits();
        left.len().cmp(&right.len()).then_with(|| left.cmp(&right))
    }
}

#[derive(Debug, Clone, Deserialize)]
struct CastBreakpoint {
    minimum_fcr: ReferenceInteger,
    frames: ReferenceInteger,
}

#[derive(Debug, Clone, Deserialize)]
struct CastingRule {
    character_class: String,
    skill_id: String,
    form: String,
    breakpoints: Vec<CastBreakpoint>,
}

#[derive(Debug, Clone, Deserialize)]
struct CastingRuleSet {
    rule_set_id: String,
    ruleset_family: String,
    revision: String,
    source_url: String,
    evidence_status: String,
    rules: Vec<CastingRule>,
}

#[derive(Debug, Clone, Deserialize)]
struct ProfileData {
    rule_set_id: String,
    skill_id: String,
    form: String,
    fcr: Option<ReferenceInteger>,
}

#[derive(Debug, Clone)]
pub struct CharacterCastingProfile(ProfileData);

impl CharacterCastingProfile {
    pub fn from_value(raw: &Value) -> Result<Self, ConfigError> {
        schema::casting_profile(raw)?;
        // Number's Value deserializer can shorten huge decimal integers through f64.
        // Parse their JSON spelling directly so breakpoint ordering stays exact.
        serde_json::from_str(&raw.to_string())
            .map(Self)
            .map_err(|error| ConfigError::value(error.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CastingEstimate {
    pub action: &'static str,
    pub reason: &'static str,
    pub ruleset_family: Option<String>,
    pub character_class: Option<String>,
    pub skill_id: Option<String>,
    pub form: Option<String>,
    pub fcr: Option<Number>,
    pub minimum_fcr: Option<Number>,
    pub frames: Option<Number>,
    pub rule_set_id: Option<String>,
    pub revision: Option<String>,
    pub source_url: Option<String>,
    pub evidence_status: Option<String>,
}

impl CastingEstimate {
    pub(crate) fn character_missing() -> Self {
        Self {
            action: "hold",
            reason: "character_missing",
            ruleset_family: None,
            character_class: None,
            skill_id: None,
            form: None,
            fcr: None,
            minimum_fcr: None,
            frames: None,
            rule_set_id: None,
            revision: None,
            source_url: None,
            evidence_status: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CastingCatalog(Vec<CastingRuleSet>);

impl CastingCatalog {
    pub fn from_value(raw: &Value) -> Result<Self, ConfigError> {
        schema::casting_catalog(raw)?;
        serde_json::from_str(&raw.to_string())
            .map(Self)
            .map_err(|error| ConfigError::value(error.to_string()))
    }

    pub fn estimate(
        &self,
        family: &str,
        class: Option<&str>,
        casting: Option<&CharacterCastingProfile>,
    ) -> Result<CastingEstimate, ConfigError> {
        time::text(family, "ruleset_family")?;
        if let Some(class) = class {
            time::text(class, "character_class")?;
        }
        let mut result = CastingEstimate {
            action: "hold",
            reason: "casting_unconfigured",
            ruleset_family: Some(family.into()),
            character_class: class.map(Into::into),
            skill_id: None,
            form: None,
            fcr: None,
            minimum_fcr: None,
            frames: None,
            rule_set_id: None,
            revision: None,
            source_url: None,
            evidence_status: None,
        };
        let Some(CharacterCastingProfile(profile)) = casting else {
            return Ok(result);
        };
        result.rule_set_id = Some(profile.rule_set_id.clone());
        result.skill_id = Some(profile.skill_id.clone());
        result.form = Some(profile.form.clone());
        result.fcr = profile.fcr.as_ref().map(|value| value.0.clone());
        let Some(class) = class else {
            result.reason = "class_unconfigured";
            return Ok(result);
        };
        let Some(fcr) = &profile.fcr else {
            result.reason = "fcr_unconfigured";
            return Ok(result);
        };
        let Some(rule_set) = self
            .0
            .iter()
            .find(|item| item.rule_set_id == profile.rule_set_id)
        else {
            result.reason = "rule_set_unsupported";
            return Ok(result);
        };
        result.revision = Some(rule_set.revision.clone());
        result.source_url = Some(rule_set.source_url.clone());
        result.evidence_status = Some(rule_set.evidence_status.clone());
        if rule_set.ruleset_family != family {
            result.reason = "ruleset_unsupported";
            return Ok(result);
        }
        let Some(rule) = rule_set.rules.iter().find(|item| {
            item.character_class == class
                && item.skill_id == profile.skill_id
                && item.form == profile.form
        }) else {
            result.reason = "context_unsupported";
            return Ok(result);
        };
        // Validation guarantees a zero breakpoint; never round FCR through a float.
        let point = rule
            .breakpoints
            .iter()
            .rev()
            .find(|point| !point.minimum_fcr.compare(fcr).is_gt())
            .ok_or_else(|| {
                ConfigError::value("validated casting rule has no applicable breakpoint")
            })?;
        result.action = "estimate";
        result.reason = "reference_rule";
        result.minimum_fcr = Some(point.minimum_fcr.0.clone());
        result.frames = Some(point.frames.0.clone());
        Ok(result)
    }
}

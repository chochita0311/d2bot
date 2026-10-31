use super::{ToolError, invalid, json_file};
use crate::common::{
    cli::{self, Kind, OptionSpec as Opt},
    config::BotConfig,
    field_control::{
        DryFieldInput, DryInputEvent, Enemy, FieldLimits, FieldObservation, NavigationEvidence,
    },
    field_runtime::create_field_control_with_backend,
    survival::{BeltContents, BeltReading, Location, ObservationStamp, SafetyObservation},
    time::MonotonicClock,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    io::Write,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

pub const HELP: &str = "Usage: d2bot field-replay --character KEY --room ROOM --scenario JSON [--config PATH]\nBounded synthetic replay; inputs_sent is always zero.\n";
const OPTIONS: &[Opt] = &[
    Opt::new("--config", Kind::Value),
    Opt::new("--character", Kind::Value).required(),
    Opt::new("--room", Kind::Value).required(),
    Opt::new("--scenario", Kind::Value).required(),
];
pub fn run(
    input: impl IntoIterator<Item = OsString>,
    output: &mut impl Write,
) -> Result<u8, ToolError> {
    let args = cli::parse(input, OPTIONS, &[])?;
    if args.help {
        output.write_all(HELP.as_bytes())?;
        return Ok(0);
    }
    let root = crate::app::runtime_root()?;
    let config = super::load_config(args.path("--config").unwrap_or_else(|| root.join("config")))?;
    let scenario = json_file(
        &args
            .path("--scenario")
            .ok_or_else(|| invalid("scenario required"))?,
        1024 * 1024,
    )?;
    let result = replay(
        &config,
        args.text("--character")?
            .ok_or_else(|| invalid("character required"))?,
        args.text("--room")?
            .ok_or_else(|| invalid("room required"))?,
        &scenario,
    )?;
    serde_json::to_writer(&mut *output, &result)?;
    writeln!(output)?;
    Ok(0)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Limits {
    screen_age: f64,
    belt_age: f64,
    combat_seconds: f64,
    arrival_seconds: f64,
    potion_confirmation_seconds: f64,
}
#[derive(Default)]
struct ReplayClock(AtomicU64);
impl MonotonicClock for ReplayClock {
    fn now(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Acquire))
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Safety {
    life_ratio: Value,
    mana_ratio: Value,
    #[serde(default = "unknown")]
    location: Location,
    #[serde(default)]
    monsters_clear: Option<bool>,
}
fn unknown() -> Location {
    Location::Unknown
}
fn enemy_unknown() -> Enemy {
    Enemy::Unknown
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Field {
    safety: Safety,
    belt_closed: Option<bool>,
    #[serde(default = "enemy_unknown")]
    enemy: Enemy,
    #[serde(default)]
    travel_aim: Option<[i64; 2]>,
    #[serde(default)]
    reachable: Option<bool>,
    #[serde(default)]
    arrived: Option<bool>,
    #[serde(default)]
    loot_checked: bool,
    #[serde(default)]
    hunt_aim: Option<[i64; 2]>,
}
fn number(value: &Value, name: &str) -> Result<f64, ToolError> {
    value
        .as_f64()
        .filter(|n| n.is_finite() && *n >= 0.0)
        .ok_or_else(|| invalid(format!("{name} must be a finite nonnegative number")))
}
fn ratio(value: &Value) -> Result<Option<f64>, ToolError> {
    if value.is_null() {
        Ok(None)
    } else {
        Ok(Some(number(value, "resource ratio")?))
    }
}
fn required_text<'a>(event: &'a Value, key: &str) -> Result<&'a str, ToolError> {
    event[key]
        .as_str()
        .ok_or_else(|| invalid(format!("{key} must be text")))
}

pub fn replay(
    config: &BotConfig,
    character: &str,
    room: &str,
    scenario: &Value,
) -> Result<Value, ToolError> {
    if scenario["evidence"] != "synthetic" {
        return Err(invalid(
            "this replay consumes synthetic semantic events, not live visual evidence",
        ));
    }
    let limits: Limits = serde_json::from_value(scenario["limits"].clone())?;
    let limits = FieldLimits::new(
        limits.screen_age,
        limits.belt_age,
        limits.combat_seconds,
        limits.arrival_seconds,
        limits.potion_confirmation_seconds,
    )?;
    let events = scenario["events"]
        .as_array()
        .filter(|v| (1..=1000).contains(&v.len()))
        .ok_or_else(|| invalid("bounded event list required"))?;
    let backend = Arc::new(DryFieldInput::default());
    let clock = Arc::new(ReplayClock::default());
    let mut control = create_field_control_with_backend(
        config,
        character,
        room,
        limits,
        clock.clone(),
        backend.clone(),
    )?;
    let result = (|| {
        let mut results = Vec::with_capacity(events.len());
        let mut previous = -1.0;
        for event in events {
            let now = number(&event["at"], "event time")?;
            if now < previous {
                return Err(invalid("replay time cannot move backwards"));
            }
            previous = now;
            clock.0.store(now.to_bits(), Ordering::Release);
            let kind = required_text(event, "type")?;
            let stamp = if kind == "tick" {
                None
            } else {
                Some(ObservationStamp::new(
                    character,
                    room,
                    event["sequence"]
                        .as_u64()
                        .ok_or_else(|| invalid("sequence must be a nonnegative integer"))?,
                    now,
                )?)
            };
            let mut result = match kind {
                "tick" => serde_json::to_value(control.tick(now)?)?,
                _ => {
                    let stamp = stamp.ok_or_else(|| invalid("stamp required"))?;
                    match kind {
                        "belt" => {
                            let mut contents = BeltContents::new();
                            for (key, value) in event["contents"]
                                .as_object()
                                .ok_or_else(|| invalid("contents must be an object"))?
                            {
                                let key = crate::common::numeric_input::integer(key)
                                    .and_then(|n| n.as_u64())
                                    .and_then(|n| u8::try_from(n).ok())
                                    .ok_or_else(|| invalid("belt event not accepted"))?;
                                let values = value
                                    .as_array()
                                    .filter(|v| v.len() == 2)
                                    .ok_or_else(|| invalid("belt event not accepted"))?;
                                let kind = if values[0].is_null() {
                                    None
                                } else {
                                    Some(
                                        values[0]
                                            .as_str()
                                            .ok_or_else(|| invalid("belt event not accepted"))?
                                            .into(),
                                    )
                                };
                                contents.insert(
                                    key,
                                    BeltReading {
                                        kind,
                                        count: values[1]
                                            .as_i64()
                                            .ok_or_else(|| invalid("belt event not accepted"))?,
                                    },
                                );
                            }
                            if !control.state_mut().observe_belt(&stamp, Some(&contents)) {
                                return Err(invalid("belt event not accepted"));
                            }
                            json!({"action":"confirmed_belt"})
                        }
                        "buff" => {
                            let location: Location =
                                serde_json::from_value(event["location"].clone())?;
                            let clear = event
                                .get("monsters_clear")
                                .filter(|v| v.is_null() || v.is_boolean())
                                .ok_or_else(|| {
                                    invalid("monsters_clear must be boolean or unknown")
                                })?
                                .as_bool();
                            if !control.state_mut().confirm_buff(
                                &stamp,
                                required_text(event, "name")?,
                                location,
                                clear,
                            ) {
                                return Err(invalid("buff event not accepted"));
                            }
                            json!({"action":"synthetic_confirmed_buff"})
                        }
                        "potion_consumed" => {
                            let column = event["column"]
                                .as_u64()
                                .and_then(|n| u8::try_from(n).ok())
                                .ok_or_else(|| invalid("consumption event not accepted"))?;
                            if !control.acknowledge_potion(&stamp, column) {
                                return Err(invalid("consumption event not accepted"));
                            }
                            json!({"action":"confirmed_consumption"})
                        }
                        "observation" => {
                            let field: Field = serde_json::from_value(event["field"].clone())?;
                            // Option만으로 누락된 필수 belt_closed를 조용히 unknown으로 바꾸지 않는다.
                            if event["field"].get("belt_closed").is_none() {
                                return Err(invalid("belt_closed required"));
                            }
                            let safety = SafetyObservation::new(
                                stamp,
                                ratio(&field.safety.life_ratio)?,
                                ratio(&field.safety.mana_ratio)?,
                                field.safety.location,
                                field.safety.monsters_clear,
                            )?;
                            let observation = FieldObservation::new(
                                safety,
                                field.belt_closed,
                                field.enemy,
                                NavigationEvidence {
                                    travel_aim: field.travel_aim,
                                    reachable: field.reachable,
                                    arrived: field.arrived,
                                    loot_checked: field.loot_checked,
                                    hunt_aim: field.hunt_aim,
                                },
                            )?;
                            serde_json::to_value(control.observe(&observation, now, true)?)?
                        }
                        _ => return Err(invalid("unsupported replay event")),
                    }
                }
            };
            result["sequence"] = event.get("sequence").cloned().unwrap_or(Value::Null);
            result["at"] = json!(now);
            results.push(result);
        }
        Ok(results)
    })();
    let cleanup = control.stop().map_err(ToolError::from);
    let results = match (result, cleanup) {
        (Ok(result), Ok(())) => result,
        (Err(error), Ok(())) | (Ok(_), Err(error)) => return Err(error),
        (Err(error), Err(cleanup)) => {
            return Err(invalid(format!("{error}; cleanup also failed: {cleanup}")));
        }
    };
    let events = backend
        .events()?
        .into_iter()
        .map(|e| match e {
            DryInputEvent::Down(k) => json!(["down", k]),
            DryInputEvent::Up(k) => json!(["up", k]),
            DryInputEvent::Press(k) => json!(["press", k]),
            DryInputEvent::Move(p) => json!(["move", p[0], p[1]]),
        })
        .collect::<Vec<_>>();
    Ok(
        json!({"mode":"synthetic_replay","inputs_sent":0,"results":results,"dry_input_events":events}),
    )
}

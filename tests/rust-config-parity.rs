use d2bot::common::config::BotConfig;
use serde_json::{Value, json};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
mod support;
use std::sync::atomic::{AtomicU64, Ordering};

type TestResult = Result<(), Box<dyn Error>>;
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    path: PathBuf,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn Error>> {
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
        fs::create_dir_all(&root)?;
        let root = root.canonicalize()?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let path = root.join(format!(
            "rust-config-{}-{stamp}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)?;
        Ok(Self { root, path })
    }

    fn write(&self, name: &str, value: &Value) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.path.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, serde_json::to_vec(value)?)?;
        Ok(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // oracle가 종료한 뒤, 이 검사에서 만든 정확한 Cargo tmp 하위 경로만 지운다.
        if self.path.parent() == Some(self.root.as_path())
            && self
                .path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("rust-config-"))
            && let Err(error) = fs::remove_dir_all(&self.path)
        {
            eprintln!("fixture cleanup failed: {error}");
        }
    }
}

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn oracle(path: &Path, request: &Value) -> Result<Value, Box<dyn Error>> {
    support::legacy_expectation(
        "config-oracle",
        &json!({"config_path":path,"options":request}),
    )
}

fn rust(path: &Path, request: &Value) -> Value {
    let result = (|| {
        let mut config = BotConfig::load(path)?;
        if request.get("override_active").is_some() {
            config.active_character = request["override_active"].as_str().map(str::to_owned);
        }
        if let Some(selections) = request["select"].as_array() {
            for key in selections {
                config.apply_character_selection(key.as_str())?;
            }
        }
        if let Some(mutation) = request.get("mutate_run")
            && let (Some(key), Some(field)) = (mutation["key"].as_str(), mutation["field"].as_str())
            && let Some(profile) = config.run_profile_mut(key)
        {
            profile[field] = mutation["value"].clone();
        }
        let farm_is_registered = config
            .section("run_profiles")
            .and_then(Value::as_object)
            .is_some_and(|profiles| {
                profiles
                    .values()
                    .any(|profile| std::ptr::eq(profile, config.farm()))
            });
        Ok::<_, d2bot::common::config::ConfigError>(json!({
            "snapshot":config.snapshot(),
            "character_order":config.section("characters").and_then(Value::as_object).map(|values|values.keys().cloned().collect::<Vec<_>>()),
            "run_order":config.section("run_profiles").and_then(Value::as_object).map(|values|values.keys().cloned().collect::<Vec<_>>()),
            "farm_is_registered":farm_is_registered
        }))
    })();
    match result {
        Ok(result) => result,
        Err(error) => json!({"error_type":error.kind(),"error_message":error.message()}),
    }
}

fn compare(path: &Path, request: Value) -> TestResult {
    let mut actual = rust(path, &request);
    // SPEC-0031의 Rust 전용 반경은 별도 정책 검사로 확인하고 나머지는 원본과 비교한다.
    if let Some(profiles) = actual
        .get_mut("snapshot")
        .and_then(|v| v.get_mut("run_profiles"))
        .and_then(Value::as_object_mut)
    {
        for profile in profiles.values_mut() {
            if let Some(hunting) = profile["hunting"].as_object_mut() {
                hunting.remove("combat_reaction_radius");
            }
        }
    }
    if let Some(hunting) = actual
        .get_mut("snapshot")
        .and_then(|v| v.get_mut("farm"))
        .and_then(|v| v.get_mut("hunting"))
        .and_then(Value::as_object_mut)
    {
        hunting.remove("combat_reaction_radius");
    }
    assert_eq!(
        actual,
        oracle(path, &request)?,
        "config/request: {} / {request}",
        path.display()
    );
    Ok(())
}

fn policy() -> Value {
    json!({"life_potion_below":0.5,"mana_potion_below":0.1,"potion_kind":"purple","replenish_at_or_below":1,"exit_on_empty":true,"belt_toggle_key":"b",
        "belt_columns":[{"column":1,"key":"1","kind":"purple","capacity":4,"target_count":4}],
        "buffs":[{"name":"orders","key":"s","duration_seconds":150,"renew_before_seconds":10}],
        "potion_pickup":{"preferred_kind":"large","fallback_kind":"small","fallback_at_or_below":2}})
}

fn catalog() -> Value {
    json!([{"rule_set_id":"reference","ruleset_family":"resurrection","revision":"v1","source_url":"https://example.invalid/reference","evidence_status":"reference",
        "rules":[{"character_class":"sorceress","skill_id":"teleport","form":"normal","breakpoints":[{"minimum_fcr":0,"frames":13},{"minimum_fcr":105,"frames":8}]}]}])
}

#[test]
fn committed_configs_and_selection_match_python() -> TestResult {
    for path in [
        root().join("config"),
        root().join("config/characters/characters.json"),
    ] {
        for selection in [
            json!({}),
            json!({"select":["flash"]}),
            json!({"select":["flash","constantin"]}),
            json!({"select":["missing",null]}),
        ] {
            compare(&path, selection)?;
        }
    }
    Ok(())
}

#[test]
fn defaults_legacy_values_and_builders_match_python() -> TestResult {
    let fixture = Fixture::new()?;
    for value in [
        json!({}),
        json!({"ignored_root":123}),
        json!({"capture":{"fps":null,"follow_window":"legacy"},"gui":{"visible_log_lines":false}}),
        json!({"characters":{"hero":{"actions":{"buff_skill_keys":"ab","pre_run_buff_order":{"w":1,"s":2},"buff_action_pause_seconds":{"W":"0.4","w":true}}}}}),
        json!({"run_profiles":{"z":{"hunting":{"target_areas":"ab","route_notes":{"first":1}},"loot":{"keep_labels":"xy"},"run_specific_rules":{"note":1}}}}),
        json!({"shared_loot":{"ignore_labels":"ab","fixed_items":[{"label":"item","extra":"ignored"}]}}),
    ] {
        compare(&fixture.write("config.json", &value)?, json!({}))?;
    }
    Ok(())
}

#[test]
fn recursive_merge_bom_windows_order_and_replacement_match_python() -> TestResult {
    let fixture = Fixture::new()?;
    fixture.write("a/01.json",&json!({"characters":{"z":{},"a":{}},"run_profiles":{"z":{"templates":[{"name":"before","path":"before.png"}]},"a":{}},"capture":{"fps":3}}))?;
    let later=fixture.write("B.JSON",&json!({"characters":{"z":{"preferred_run_profile":"a"}},"run_profiles":{"z":{"templates":[]}},"capture":{"window_title":"Game"}}))?;
    let bytes = fs::read(&later)?;
    let mut bom = vec![0xef, 0xbb, 0xbf];
    bom.extend(bytes);
    fs::write(later, bom)?;
    compare(&fixture.path, json!({}))?;
    Ok(())
}

#[test]
fn farm_precedence_fallback_and_reference_match_python() -> TestResult {
    let fixture = Fixture::new()?;
    let value = json!({"characters":{"hero":{"preferred_run_profile":"second"},"other":{}},"run_profiles":{"first":{},"second":{}},"farm":{"name":"legacy"}});
    let path = fixture.write("config.json", &value)?;
    for request in [
        json!({}),
        json!({"select":["other"]}),
        json!({"select":["other","missing",null]}),
        json!({"override_active":"missing","select":["missing"]}),
        json!({"mutate_run":{"key":"second","field":"description","value":"shared mutation"}}),
    ] {
        compare(&path, request)?;
    }
    for farm in [
        Value::Null,
        json!(false),
        json!(0),
        json!(""),
        json!([]),
        json!({}),
        json!({"goal":"legacy"}),
    ] {
        compare(
            &fixture.write(
                "legacy.json",
                &json!({"farm":farm,"run_profiles":{"z":{},"a":{}}}),
            )?,
            json!({}),
        )?;
    }
    Ok(())
}

#[test]
fn schema_rejections_match_python() -> TestResult {
    let fixture = Fixture::new()?;
    for (name, bad) in [
        ("life_potion_below", json!(true)),
        ("mana_potion_below", json!(0)),
        ("replenish_at_or_below", json!(5)),
        ("exit_on_empty", json!(1)),
        ("belt_toggle_key", json!("")),
        ("unknown_option", json!(1)),
        ("belt_columns", json!([])),
    ] {
        let mut value = policy();
        value[name] = bad;
        compare(
            &fixture.write(
                "bad.json",
                &json!({"characters":{"hero":{"survival":value}}}),
            )?,
            json!({}),
        )?;
    }
    let mut cases = Vec::new();
    let mut value = policy();
    value["belt_columns"][0]["column"] = json!(true);
    cases.push(value);
    let mut value = policy();
    value["belt_columns"][0]["target_count"] = Value::Null;
    cases.push(value);
    let mut value = policy();
    value["buffs"][0]["duration_seconds"] = json!(0);
    cases.push(value);
    let mut value = policy();
    value["potion_pickup"]["fallback_at_or_below"] = json!(8);
    cases.push(value);
    let mut value = policy();
    value["potion_pickup"]["fallback_kind"] = Value::Null;
    cases.push(value);
    for value in cases {
        compare(
            &fixture.write(
                "bad.json",
                &json!({"characters":{"hero":{"survival":value}}}),
            )?,
            json!({}),
        )?;
    }
    for value in [
        json!({"capture":{"unknown":1}}),
        json!({"recording":{"extra":1}}),
        json!({"characters":{"hero":{"character_class":" "}}}),
        json!({"characters":{"hero":{"actions":{"buff_skill_keys":null}}}}),
    ] {
        compare(&fixture.write("bad.json", &value)?, json!({}))?;
    }
    Ok(())
}

#[test]
fn casting_validation_and_large_integer_match_python() -> TestResult {
    let fixture = Fixture::new()?;
    for fcr in [
        json!(null),
        json!(105),
        serde_json::from_str(&format!("1{}", "0".repeat(300)))?,
    ] {
        let value = json!({"casting_rule_sets":catalog(),"characters":{"hero":{"character_class":"sorceress","casting":{"rule_set_id":"reference","skill_id":"teleport","form":"normal","fcr":fcr},"survival":policy()}}});
        compare(&fixture.write("good.json", &value)?, json!({}))?;
    }
    for bad in [json!(true), json!(105.0), json!(-1), json!("105")] {
        compare(&fixture.write("bad.json",&json!({"characters":{"hero":{"casting":{"rule_set_id":"reference","skill_id":"teleport","form":"normal","fcr":bad}}}}))?,json!({}))?;
    }
    for field in ["evidence_status", "source_url", "rules"] {
        let mut value = catalog();
        value[0][field] = if field == "rules" {
            json!([])
        } else {
            json!("")
        };
        compare(
            &fixture.write("bad.json", &json!({"casting_rule_sets":value}))?,
            json!({}),
        )?;
    }
    Ok(())
}

#[test]
fn read_and_parse_failures_keep_error_category() -> TestResult {
    let fixture = Fixture::new()?;
    let path = fixture.path.join("config.json");
    for bytes in [vec![b'{'], vec![0xff]] {
        fs::write(&path, bytes)?;
        assert_eq!(
            rust(&path, &json!({}))["error_type"],
            oracle(&path, &json!({}))?["error_type"]
        );
    }
    let missing = fixture.path.join("missing.json");
    assert_eq!(
        rust(&missing, &json!({}))["error_type"],
        oracle(&missing, &json!({}))?["error_type"]
    );
    Ok(())
}

#[test]
fn invalid_optional_pauses_use_existing_defaults_and_report_repairs() -> TestResult {
    let fixture = Fixture::new()?;
    let original = json!({"NaN":"NaN","inf":"Infinity","negative":"-inf","overflow":"1e10000","unrepresentable":1e300,
        "negative_gap":-0.1,"bad":"bad","null":null,"list":[],"map":{},
        "W":0.6,"w":"nan","kept":"0.4","zero":0,"boolean":true});
    let path = fixture.write(
        "repairs.json",
        &json!({"characters":{"he/ro~":{"actions":{"buff_action_pause_seconds":original}}}}),
    )?;
    let config = BotConfig::load(path)?;
    let expected_path = fixture.write("valid.json", &json!({"characters":{"he/ro~":{"actions":{"buff_action_pause_seconds":{"kept":0.4,"zero":0,"boolean":true}}}}}))?;
    let expected = oracle(&expected_path, &json!({}))?;
    assert_eq!(config.snapshot(), expected["snapshot"]);
    assert_eq!(config.warnings().len(), 11);
    for warning in config.warnings() {
        assert!(
            warning
                .path
                .starts_with("/characters/he~1ro~0/actions/buff_action_pause_seconds/")
        );
        let key = warning
            .path
            .rsplit('/')
            .next()
            .ok_or("missing warning key")?;
        assert_eq!(warning.original, original[key]);
        assert!(warning.message.contains("default pause"));
    }
    // Finite JSON numbers can also overflow float conversion, without using nonstandard literals.
    let mut raw = json!({"characters":{"hero":{"actions":{"buff_action_pause_seconds":{}}}}});
    raw["characters"]["hero"]["actions"]["buff_action_pause_seconds"]["w"] =
        serde_json::from_str("1e10000")?;
    let config = BotConfig::from_value(raw)?;
    assert_eq!(config.warnings().len(), 1);
    assert_eq!(
        config.snapshot()["characters"]["hero"]["actions"]["buff_action_pause_seconds"],
        json!({})
    );
    assert!(
        BotConfig::load(root().join("config"))?
            .warnings()
            .is_empty()
    );
    Ok(())
}

#[test]
fn malformed_legacy_containers_and_float_strings_match_python() -> TestResult {
    let fixture = Fixture::new()?;
    for value in [
        Value::Null,
        json!([]),
        json!("bad"),
        json!({"capture":null}),
        json!({"characters":null}),
        json!({"run_profiles":false}),
        json!({"characters":{"hero":{"actions":{"buff_action_pause_seconds":null}}}}),
        json!({"characters":{"hero":{"actions":{"buff_action_pause_seconds":{"w":"1_0.5","s":" 1e1_0 "}}}}}),
        json!({"run_profiles":{"x":{"templates":[null]}}}),
    ] {
        compare(&fixture.write("legacy.json", &value)?, json!({}))?;
    }
    Ok(())
}

#[test]
fn validation_stage_order_matches_python() -> TestResult {
    let fixture = Fixture::new()?;
    let mut value = catalog();
    value[0]["rules"][0]["breakpoints"][0]["minimum_fcr"] = json!(1);
    value[0]["rules"][0]["breakpoints"][1]["frames"] = json!(true);
    compare(
        &fixture.write("ordered.json", &json!({"casting_rule_sets":value}))?,
        json!({}),
    )?;
    let first = catalog()[0].clone();
    let mut bad = first.clone();
    bad["source_url"] = json!("");
    compare(
        &fixture.write(
            "ordered.json",
            &json!({"casting_rule_sets":[first.clone(),first,bad]}),
        )?,
        json!({}),
    )?;
    let mut value = catalog();
    let first = value[0]["rules"][0].clone();
    let mut bad = first.clone();
    bad["breakpoints"][0]["frames"] = json!(true);
    value[0]["rules"] = json!([first.clone(), first, bad]);
    compare(
        &fixture.write("ordered.json", &json!({"casting_rule_sets":value}))?,
        json!({}),
    )?;
    Ok(())
}

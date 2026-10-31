use super::*;
use serde_json::{Value, json};
use std::{collections::BTreeMap, error::Error};
type TestResult = Result<(), Box<dyn Error>>;
#[test]
fn repaired_pause_warning_is_visible_without_blocking_startup() -> TestResult {
    let config = BotConfig::from_value(
        json!({"characters":{"sample":{"actions":{"buff_action_pause_seconds":{"f1":"Infinity"}}}}}),
    )?;
    let state = PanelState::new(&config)?;
    assert!(
        state
            .log()
            .contains("/characters/sample/actions/buff_action_pause_seconds/f1")
    );
    assert!(state.log().contains("default pause"));
    assert_eq!(state.status, "Idle");
    Ok(())
}
fn snapshot(panel: &PanelState) -> Value {
    json!({"window_title":panel.window_title,"character":panel.character,"characters":panel.characters,"selected":panel.selected_character(),"status":panel.status,"output":panel.output,"closing":panel.closing,"can_close":panel.can_close(),"controls":panel.controls(),"log":panel.log()})
}
fn exercise(case: &Value) -> Result<Value, Box<dyn Error>> {
    let config = BotConfig::from_value(case.get("config").cloned().unwrap_or(json!({})))?;
    let mut panel = PanelState::new(&config)?;
    let mut rows = vec![];
    for op in case["ops"].as_array().ok_or("ops")? {
        if let Some(values) = op["set"].as_object() {
            for (key, value) in values {
                let target = match key.as_str() {
                    "window_title" => &mut panel.window_title,
                    "character" => &mut panel.character,
                    "repeat_count" => &mut panel.repeat_count,
                    "status" => &mut panel.status,
                    "output" => &mut panel.output,
                    _ => return Err("unknown variable".into()),
                };
                *target = value.as_str().ok_or("string variable")?.into();
            }
        }
        if let Some(values) = op["activity"].as_object() {
            for (key, value) in values {
                let target = match key.as_str() {
                    "recording" => &mut panel.activity.recording,
                    "lifecycle" => &mut panel.activity.lifecycle,
                    "gem" => &mut panel.activity.gem,
                    "loot" => &mut panel.activity.loot,
                    "summoner" => &mut panel.activity.summoner,
                    "recording_cleanup" => &mut panel.activity.recording_cleanup,
                    _ => return Err("unknown activity".into()),
                };
                *target = value.as_bool().ok_or("running flag")?;
            }
        }
        let mut result = json!({});
        match op["op"].as_str().unwrap_or("refresh") {
            "windows" => panel.refresh_windows(
                serde_json::from_value(op["titles"].clone())?,
                op["select_current"].as_bool().unwrap_or(false),
            ),
            "repeat" => {
                result["valid"] = json!(PanelState::validate_repeat(&panel.repeat_count));
                match panel.repeat() {
                    Ok(value) => result["repeat"] = json!(value),
                    Err(error) => result["error"] = json!(error.message()),
                }
            }
            "ensure" => {
                if let Err(error) = panel.ensure_action_available() {
                    result["error"] = json!(error.message());
                }
            }
            "log" => panel.append_log(
                op["level"].as_str().unwrap_or("info"),
                op["message"].as_str().ok_or("message")?,
            ),
            "stop" => panel.stopped_action(),
            "close" => {
                if panel.begin_close() && panel.activity.recording {
                    panel.activity.recording_cleanup = true;
                }
            }
            "record_started" => {
                panel.recording_started(op["output"].as_str().ok_or("output")?.into())
            }
            "record_stopped" => panel.recording_stopped(),
            "poll" => {
                for (name, source) in [
                    ("recording", EventSource::Recording),
                    ("lifecycle", EventSource::Lifecycle),
                    ("gem", EventSource::Gem),
                    ("loot", EventSource::Loot),
                    ("summoner", EventSource::Summoner),
                ] {
                    if let Some(events) = op["events"][name].as_array() {
                        for event in events {
                            panel.event(
                                source,
                                &SummonerEvent {
                                    level: event["level"].as_str().ok_or("level")?.into(),
                                    message: event["message"].as_str().ok_or("message")?.into(),
                                    payload: event.get("payload").cloned(),
                                    priority: 1,
                                },
                            );
                        }
                    }
                }
                panel.refresh_controls();
            }
            "refresh" => panel.refresh_controls(),
            _ => return Err("operation".into()),
        }
        result["state"] = snapshot(&panel);
        rows.push(result);
    }
    Ok(json!(rows))
}
#[test]
fn panel_selection_repeat_status_and_log_projection_match_source() -> TestResult {
    let mut cases = vec![];
    for (titles, current) in [
        (vec![], "Diablo"),
        (vec!["Diablo2 Helper"], ""),
        (vec!["Other", "Diablo2 Helper"], ""),
        (vec!["Diablo2", "Diablo II: Resurrected"], ""),
        (vec!["Target suffix", "Target"], "Target"),
        (vec!["Straße", "Other"], "STRASSE"),
        (vec![" ", "Diablo2 Helper", "Other"], "unknown"),
    ] {
        cases.push(json!({"config":{"capture":{"window_title":current}},"ops":[{"op":"windows","titles":titles,"select_current":true},{"op":"windows","titles":["new"],"select_current":false},{"op":"windows","titles":["fallback"],"set":{"window_title":" "}}]}));
    }
    cases.push(json!({"config":{"characters":{"first":{"display_name":"Same"},"second":{"display_name":"Same"},"third":{}}},"ops":[{}, {"set":{"character":" Same "}},{"set":{"character":"missing"}}]}));
    for value in [
        "",
        " ",
        "5",
        "0",
        "-1",
        "+01",
        "1_000",
        "0005",
        "²",
        "１２",
        "٣",
        "\u{1c}1",
        "1__0",
        "_1",
        "1_",
        "abc",
        "9999999999999999999999999999999999999999999999999999999999999999999999",
    ] {
        cases.push(json!({"ops":[{"op":"repeat","set":{"repeat_count":value}}]}));
    }
    for recording in [false, true] {
        for status in [
            "running",
            "complete",
            "incomplete",
            "failed",
            "stopped",
            "unknown",
        ] {
            cases.push(json!({"ops":[{"activity":{"summoner":true,"recording":recording},"set":{"status":"Summoner Run"}},{"op":"stop"},{"op":"poll","activity":{"summoner":false},"events":{"summoner":[{"level":"info","message":"result","payload":{"status":status,"output":"evidence"}}]}},{}]}));
        }
    }
    cases.extend([
        json!({"ops":[{"activity":{"gem":true}},{"op":"ensure"},{"op":"stop"},{"activity":{"gem":false}},{"op":"ensure"},{"op":"close"},{"op":"ensure"},{"op":"close"}]}),
        json!({"ops":[{"activity":{"summoner":true,"recording":true}},{"op":"close"},{"op":"close"},{"op":"poll","activity":{"summoner":false}},{"op":"poll","activity":{"recording":false}},{"op":"poll","activity":{"recording_cleanup":false}}]}),
        json!({"ops":[{"op":"record_started","output":"movie.avi"},{"activity":{"recording":true}},{"op":"poll","events":{"recording":[{"level":"error","message":"encoder failed"}],"lifecycle":[{"level":"error","message":"town failed"}],"gem":[{"level":"error","message":"gem failed"}],"loot":[{"level":"error","message":"loot failed"}],"summoner":[{"level":"info","message":"final","payload":{"status":"incomplete"}}]}},{"activity":{"recording":false}},{"op":"record_stopped"}]}),
        json!({"ops":[{"op":"poll","events":{"gem":[{"level":"error","message":"source gem status"}]}},{}]}),
    ]);
    for value in [
        json!(50),
        json!(-3),
        json!(50.9),
        json!(true),
        json!("80"),
        json!("-10"),
        json!("1_000"),
    ] {
        let ops=(0..105).map(|i|json!({"op":"log","level":if i%2==0{"info"}else{"warning"},"message":if i%7==0{format!("한글 {i}\nextra")}else{format!("line {i}")}})).collect::<Vec<_>>();
        cases.push(json!({"config":{"gui":{"visible_log_lines":value}},"ops":ops}));
    }
    let expected = crate::test_support::legacy_expectation("panel-state-oracle", &json!(cases))?;
    for (index, case) in cases.iter().enumerate() {
        let actual = exercise(case)?;
        let rows = actual.as_array().ok_or("rows")?;
        assert_eq!(
            rows.len(),
            expected[index].as_array().ok_or("expected rows")?.len()
        );
        for (row, value) in rows.iter().enumerate() {
            assert_eq!(
                *value, expected[index][row],
                "panel state case {index} row {row}"
            );
        }
    }
    Ok(())
}
#[test]
fn numeric_edit_filter_matches_entire_baseline_unicode_set() -> TestResult {
    let expected =
        crate::test_support::legacy_expectation("panel-state-oracle", &json!([{"digits":true}]))?;
    let mut numeric = BTreeMap::new();
    for item in expected[0].as_array().ok_or("numeric chars")? {
        let text = item[0].as_str().ok_or("text")?;
        let valid = item[1].as_bool().ok_or("digit")?;
        assert_eq!(PanelState::validate_repeat(text), valid, "{text}");
        numeric.insert(text.chars().next().ok_or("char")?, valid);
    }
    for code in 0..=0x10ffff {
        if let Some(c) = char::from_u32(code) {
            assert_eq!(
                numeric_input::digit(c),
                numeric.get(&c).copied().unwrap_or(false),
                "U+{code:x}"
            );
        }
    }
    assert!(PanelState::validate_repeat(""));
    Ok(())
}

use super::*;
use crate::test_field_world::compare;
use serde_json::json;
use std::{error::Error, sync::mpsc, time::Instant};
type TestResult = Result<(), Box<dyn Error>>;
const OUTPUT: &str = "recordings/summoner/evidence/gui-20000101-000000-0123456789";
fn config() -> Result<BotConfig, ConfigError> {
    BotConfig::from_value(json!({"characters":{"selected":{},"flash":{}}}))
}
fn output() -> Result<PathBuf, RunError> {
    Ok(PathBuf::from(OUTPUT))
}
fn normalize(value: &mut Value) {
    match value {
        Value::Object(values) => {
            for (key, value) in values {
                if key == "output" {
                    if let Some(text) = value.as_str() {
                        *value = json!(text.replace('\\', "/"));
                    }
                } else {
                    normalize(value);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                normalize(value);
            }
        }
        _ => (),
    }
}
fn plain_error(error: RunError) -> String {
    if let RunError::Control(crate::common::error::ControlError::Configuration(error)) = &error {
        return error.message().into();
    }
    error.to_string()
}
fn request(case: &Value) -> Result<SummonerStart, Box<dyn Error>> {
    Ok(SummonerStart {
        repeat_count: case["count"].as_u64().map(|v| v as u8),
        difficulty: serde_json::from_value(
            case.get("difficulty").cloned().unwrap_or(json!("hell")),
        )?,
        character_key: if case.get("character").is_some() {
            case["character"].as_str().map(str::to_owned)
        } else {
            Some("selected".into())
        },
    })
}
fn exercise(case: &Value) -> Result<Value, Box<dyn Error>> {
    let mut session = SummonerGuiSession::new(config()?);
    let calls = Arc::new(Mutex::new(vec![]));
    let observed = calls.clone();
    let task = case.clone();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let request = request(case)?;
    let result=session.start_with(request.clone(),output,move|packet| {
        let o=&packet.options;
        observed.lock().unwrap_or_else(|e|e.into_inner()).push(json!({"north_cycles":packet.north_cycles,"live":o.live,"resume_arcane":o.resume_arcane,"casts":o.casts,"difficulty":o.difficulty,"output":o.output,"continuous":o.continuous,"field_seconds":o.field_seconds,"segments":o.segments,"automatic":o.automatic,"character_key":o.character_key,"engage_monsters":o.engage_monsters,"automap":o.automap.name()}));
        if task["blocked"]==true {entered_tx.send(()).map_err(|e|RunError::Runtime(e.to_string()))?;release_rx.recv_timeout(Duration::from_secs(2)).map_err(|e|RunError::Runtime(e.to_string()))?;}
        if task["log"]==true{(packet.emit)("observed hub");}
        if task["cancel"]==true{packet.stop.cancel();}
        if task["error"]==true{return Err(RunError::Runtime("arrival missing".into()));}
        Ok(task["code"].as_i64().unwrap_or(0) as i32)
    });
    let mut result = match result {
        Err(error) => json!({"error":plain_error(error),"running":session.is_running()}),
        Ok(output) => {
            let mut during = json!({});
            if case["blocked"] == true {
                entered_rx.recv_timeout(Duration::from_secs(2))?;
                session.close();
                during["running"] = json!(session.is_running());
                during["restart"] = json!(plain_error(
                    session
                        .start_with(
                            request,
                            crate::runs::summoner::gui_session::tests::output,
                            |_| Ok(0)
                        )
                        .err()
                        .ok_or("restart accepted")?
                ));
                during["update"] = json!(plain_error(
                    session
                        .update_config(config()?)
                        .err()
                        .ok_or("update accepted")?
                ));
                release_tx.send(())?;
            }
            let deadline = Instant::now() + Duration::from_secs(3);
            while session.is_running() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(!session.is_running());
            let result = session.result()?;
            // 완료 후 join은 결과의 취소 판정을 뒤집지 않는다.
            session.shutdown(Duration::from_secs(1))?;
            json!({"output":output,"result":result,"running":session.is_running(),"calls":*calls.lock().unwrap_or_else(|e|e.into_inner()),"during":during})
        }
    };
    result["events"] = serde_json::to_value(session.events().drain())?;
    normalize(&mut result);
    Ok(result)
}
#[test]
fn gui_session_options_events_results_and_cancellation_match_python() -> TestResult {
    let mut cases = vec![];
    for count in [None, Some(1), Some(3), Some(5)] {
        for difficulty in ["normal", "nightmare", "hell"] {
            cases.push(json!({"count":count,"difficulty":difficulty,"log":true}));
        }
    }
    cases.extend([
        json!({"code":2}),
        json!({"code":-1}),
        json!({"error":true}),
        json!({"cancel":true}),
        json!({"cancel":true,"error":true}),
        json!({"blocked":true}),
        json!({"count":0}),
        json!({"count":6}),
        json!({"character":null}),
        json!({"character":"missing"}),
    ]);
    let expected =
        crate::test_support::legacy_expectation("summoner-session-oracle", &json!(cases))?;
    for (index, case) in cases.iter().enumerate() {
        compare(
            &exercise(case)?,
            &expected[index],
            &format!("Summoner GUI session {index}"),
        );
    }
    assert!(!PathBuf::from(OUTPUT).exists());
    Ok(())
}
#[test]
fn session_timeout_retains_owner_restart_and_bounded_event_result() -> TestResult {
    let mut session = SummonerGuiSession::new(config()?);
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    session.start_with(
        SummonerStart {
            character_key: Some("selected".into()),
            ..Default::default()
        },
        output,
        move |packet| {
            entered_tx
                .send(())
                .map_err(|e| RunError::Runtime(e.to_string()))?;
            release_rx
                .recv()
                .map_err(|e| RunError::Runtime(e.to_string()))?;
            assert!(packet.stop.is_cancelled());
            for _ in 0..300 {
                (packet.emit)("bounded log");
            }
            Ok(0)
        },
    )?;
    entered_rx.recv_timeout(Duration::from_secs(2))?;
    assert!(session.shutdown(Duration::from_millis(1)).is_err());
    assert!(session.is_running());
    assert_eq!(session.result()?, None);
    release_tx.send(())?;
    session.shutdown(Duration::from_secs(2))?;
    assert_eq!(session.result()?, Some(SummonerResult::Stopped));
    let events = session.events().drain();
    assert_eq!(events.len(), 257);
    assert!(events[0].message.contains("omitted"));
    assert_eq!(
        events
            .last()
            .and_then(|e| e.payload.as_ref())
            .map(|v| &v["status"]),
        Some(&json!("stopped"))
    );
    session.start_with(
        SummonerStart {
            character_key: Some("selected".into()),
            ..Default::default()
        },
        output,
        |_| panic!("fixture panic"),
    )?;
    while session.is_running() {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(session.result()?, Some(SummonerResult::Failed));
    session.shutdown(Duration::from_secs(1))?;
    assert!(
        session
            .events()
            .drain()
            .iter()
            .any(|e| e.message.contains("fixture panic"))
    );
    assert!(!PathBuf::from(OUTPUT).exists());
    Ok(())
}
#[test]
fn public_definition_preserves_source_metadata_and_borrowed_profile() -> TestResult {
    use super::super::definition::*;
    let cases = vec![
        json!({"metadata":true}),
        json!({"metadata":true,"profile":{"goal":"goal","hunting":{"waypoint_act":2,"waypoint_name":"Arcane","target_monsters":["The Summoner","other"],"target_areas":["area"],"fight_style":"safe","search_timeout_seconds":90.0}}}),
        json!({"metadata":true,"profile":{"hunting":{"waypoint_act":0,"waypoint_name":"Arcane","target_monsters":[""]}}}),
        json!({"metadata":true,"missing":true}),
        json!({"metadata":true,"profile":{"hunting":{"target_monsters":["Summoner",3]}}}),
    ];
    let expected =
        crate::test_support::legacy_expectation("summoner-session-oracle", &json!(cases))?;
    for (index, case) in cases.iter().enumerate() {
        let config = BotConfig::from_value(if case["missing"] == true {
            json!({})
        } else {
            json!({"run_profiles":{"summoner":case.get("profile").cloned().unwrap_or(json!({}))}})
        })?;
        let result = match resolve_summoner_run(&config) {
            Ok(context) => match summarize_summoner_payload(&context) {
                Ok(summary) => {
                    json!({"definition":context.definition,"summary":summary,"identity":std::ptr::eq(context.profile(),&config.section("run_profiles").ok_or("profiles")?["summoner"])})
                }
                Err(error) => json!({"error":plain_error(error)}),
            },
            Err(e) => json!({"error":e.to_string()}),
        };
        compare(
            &result,
            &expected[index],
            &format!("Summoner definition {index}"),
        );
    }
    let adapter: SummonerRunOrchestrator = build_summoner_orchestrator(config()?);
    assert!(!adapter.is_running());
    Ok(())
}

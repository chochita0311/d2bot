use super::*;
use crate::test_field_world::compare;
use serde_json::{Value, json};
use std::{cell::RefCell, error::Error};
type TestResult = Result<(), Box<dyn Error>>;
struct Fake {
    case: Value,
    stop: CooperativeStop,
    index: u64,
    queue: VecDeque<ActionEvent>,
    events: RefCell<Vec<Value>>,
    finished: usize,
}
impl EntrySession for Fake {
    fn start(&mut self, d: Difficulty, resume: bool) -> Result<(), RunError> {
        let mut options = json!({"town_loop":true,"destination":"arcane","arrival_only":true});
        if d == Difficulty::Hell {
            options["reset_act2_start"] = json!(true);
        }
        if resume {
            options["resume_town"] = json!(true);
        }
        self.events
            .borrow_mut()
            .push(json!(["start", 1, d.name(), options]));
        if let Some(e) = self.case["start_error"].as_str() {
            return Err(RunError::Runtime(e.into()));
        }
        Ok(())
    }
    fn running(&mut self) -> Result<bool, RunError> {
        let index = self.index;
        self.index += 1;
        if self.case["stop_iteration"].as_u64() == Some(index) {
            self.stop.set();
        }
        Ok(index < self.case["running"].as_u64().unwrap_or(1))
    }
    fn has_events(&mut self) -> bool {
        !self.queue.is_empty()
    }
    fn next_event(&mut self, _: Duration) -> Result<Option<ActionEvent>, RunError> {
        if let Some(e) = self.case["event_error"].as_str() {
            return Err(RunError::Runtime(e.into()));
        }
        Ok(self.queue.pop_front())
    }
    fn request_stop(&self) {
        self.events.borrow_mut().push(json!(["stop"]));
    }
    fn last_result(&self) -> Result<Option<TownOutcome>, RunError> {
        let value = self
            .case
            .get("last_result")
            .cloned()
            .unwrap_or(json!("passed"));
        Ok(match value.as_str() {
            Some("passed") => Some(TownOutcome::Passed),
            Some("failed") => Some(TownOutcome::Failed),
            Some("stopped") => Some(TownOutcome::Stopped),
            _ => None,
        })
    }
    fn finish(&mut self, _: Duration) -> Result<(), RunError> {
        self.finished += 1;
        if self.case["cleanup_error"] == true {
            Err(RunError::Runtime("entry join delayed".into()))
        } else {
            Ok(())
        }
    }
}
fn exercise(case: &Value) -> Result<(Value, usize), Box<dyn Error>> {
    let stop = CooperativeStop::default();
    if case["stopped"] == true {
        stop.set();
    }
    let rows = case
        .get("events")
        .cloned()
        .unwrap_or(json!([{"level":"info","message":"arrived"}]));
    let queue = rows
        .as_array()
        .ok_or("events")?
        .iter()
        .map(|row| {
            Ok(ActionEvent {
                level: match row["level"].as_str() {
                    Some("warning") => EventLevel::Warning,
                    Some("error") => EventLevel::Error,
                    _ => EventLevel::Info,
                },
                message: row["message"].as_str().ok_or("message")?.into(),
            })
        })
        .collect::<Result<VecDeque<_>, Box<dyn Error>>>()?;
    let mut fake = Fake {
        case: case.clone(),
        stop: stop.clone(),
        index: 0,
        queue,
        events: RefCell::new(vec![]),
        finished: 0,
    };
    let difficulty = match case["difficulty"].as_str() {
        Some("normal") => Difficulty::Normal,
        Some("nightmare") => Difficulty::Nightmare,
        _ => Difficulty::Hell,
    };
    let mut emits = vec![];
    let result = enter_arcane(
        &mut fake,
        difficulty,
        case["resume_town"] == true,
        &stop,
        |s| emits.push(s.to_owned()),
    );
    let error = result
        .err()
        .map(|e| json!({"kind":"RuntimeError","message":e.to_string()}));
    Ok((
        json!({"error":error,"events":fake.events.borrow().clone(),"emits":emits}),
        fake.finished,
    ))
}
#[test]
fn entry_options_stop_events_and_results_match_source() -> TestResult {
    let mut cases: Value = serde_json::from_str(
        r#"[{}, {"difficulty":"normal"},{"difficulty":"nightmare","resume_town":true},{"stopped":true},{"running":0,"events":[]},{"running":3,"stop_iteration":1},{"last_result":"failed"},{"last_result":"stopped"},{"last_result":null},{"start_error":"start failed"},{"event_error":"event channel failed"},{"running":0,"events":[{"level":"warning","message":"current proof missing"},{"level":"error","message":"failed"}]}]"#,
    )?;
    for c in cases.as_array_mut().ok_or("cases")? {
        c["operation"] = json!("entry");
    }
    let expected =
        crate::test_support::legacy_expectation("main-execution-oracle", &json!({"cases":cases}))?;
    for (i, c) in cases.as_array().ok_or("cases")?.iter().enumerate() {
        let (result, finished) = exercise(c)?;
        compare(&result, &expected[i], &format!("entry[{i}]"));
        assert_eq!(
            finished,
            usize::from(c["stopped"] != true && c.get("start_error").is_none())
        );
    }
    Ok(())
}
#[test]
fn entry_failure_and_join_error_both_remain_visible() -> TestResult {
    let (result, finished) =
        exercise(&json!({"event_error":"event channel failed","cleanup_error":true}))?;
    assert_eq!(finished, 1);
    let message = result["error"]["message"].as_str().ok_or("error")?;
    assert!(message.contains("event channel failed"));
    assert!(message.contains("entry join delayed"));
    Ok(())
}

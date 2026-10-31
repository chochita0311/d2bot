use super::*;
use crate::{actions::town_loop::Difficulty, test_field_world::compare};
use std::{cell::RefCell, error::Error, path::PathBuf, rc::Rc};
type TestResult = Result<(), Box<dyn Error>>;
struct State {
    case: Value,
    index: usize,
    stopped: bool,
    events: Vec<Value>,
}
struct Runner(Rc<RefCell<State>>);
impl Runner {
    fn row(&self) -> Value {
        let s = self.0.borrow();
        let rows = s.case["rooms"].as_array();
        rows.and_then(|v| v.get(s.index.saturating_sub(1).min(v.len().saturating_sub(1))))
            .cloned()
            .unwrap_or(json!({}))
    }
}
impl RoomRunner for Runner {
    fn stopped(&self) -> bool {
        self.0.borrow().stopped
    }
    fn run_once(&mut self, o: &SummonerOptions) -> Result<i32, RunError> {
        let name = o
            .output
            .file_name()
            .ok_or_else(|| RunError::Runtime("room name".into()))?
            .to_string_lossy()
            .into_owned();
        {
            let mut s = self.0.borrow_mut();
            s.index += 1;
            s.events.push(json!(["room",name,{"resume_arcane":o.resume_arcane,"resume_town":o.resume_town,"resume_labels_off":o.resume_labels_off,"return_from":o.return_from,"continue_navigation":o.continue_navigation,"finish_room":o.finish_room}]));
        }
        let row = self.row();
        if row["stop"] == true {
            self.0.borrow_mut().stopped = true;
        }
        if let Some(e) = row["error"].as_str() {
            return Err(RunError::Runtime(e.into()));
        }
        Ok(row["code"].as_i64().unwrap_or(0) as i32)
    }
    fn read_report(&mut self, p: &Path) -> Result<Value, RunError> {
        let name = p
            .parent()
            .and_then(Path::file_name)
            .ok_or_else(|| RunError::Runtime("room path".into()))?
            .to_string_lossy();
        self.0
            .borrow_mut()
            .events
            .push(json!(["read", format!("{name}/report.json")]));
        let row = self.row();
        if let Some(e) = row["read_error"].as_str() {
            return Err(RunError::Runtime(e.into()));
        }
        Ok(row
            .get("report")
            .cloned()
            .unwrap_or(json!({"status":"north_room_closed"})))
    }
}
struct Output {
    state: Rc<RefCell<State>>,
    path: PathBuf,
    fail: bool,
}
impl RunOutput for Output {
    fn report_path(&self) -> &Path {
        &self.path
    }
    fn save_report(&mut self, r: &Value) -> Result<(), RunError> {
        self.state.borrow_mut().events.push(json!(["report", r]));
        if self.fail {
            Err(RunError::Runtime("summary write failed".into()))
        } else {
            Ok(())
        }
    }
    fn save_frame(&mut self, _: &EncounterEvidence) -> Result<(), RunError> {
        Err(RunError::Runtime("unexpected image".into()))
    }
}
fn exercise(c: &Value) -> TestResultValue {
    let state = Rc::new(RefCell::new(State {
        case: c.clone(),
        index: 0,
        stopped: c["stopped"] == true,
        events: vec![],
    }));
    let mut runner = Runner(state.clone());
    let mut out = Output {
        state: state.clone(),
        path: PathBuf::from("out/report.json"),
        fail: c["write_error"] == true,
    };
    let mut options = SummonerOptions::new(PathBuf::from("out"), Difficulty::Hell);
    options.live = c["options"]["live"].as_bool().unwrap_or(true);
    options.continuous = c["options"]["continuous"].as_bool().unwrap_or(true);
    options.automatic = c["options"]["automatic"].as_bool().unwrap_or(true);
    options.all_wings = c["options"]["all_wings"].as_bool().unwrap_or(false);
    options.resume_arcane = c["options"]["resume_arcane"] == true;
    options.resume_town = c["options"]["resume_town"] == true;
    options.resume_labels_off = c["options"]["resume_labels_off"] == true;
    if let Some(v) = c["options"]["wing"].as_str() {
        options.wing = serde_json::from_value(json!(v))?;
    }
    if let Some(v) = c["options"]["return_from"].as_str() {
        options.return_from = Some(serde_json::from_value(json!(v))?);
    }
    if let Some(v) = c["options"]["continue_navigation"].as_str() {
        options.continue_navigation = Some(PathBuf::from(v));
    }
    let mut emits = vec![];
    let result = execute_cycles(
        &mut runner,
        &mut out,
        &options,
        c["cycles"].as_u64().unwrap_or(2) as u8,
        |s| emits.push(s.to_owned()),
    );
    let (result, error) = match result {
        Ok(v) => (json!(v), Value::Null),
        Err(e) => {
            let kind = match &e {
                RunError::Control(ControlError::Configuration(e)) => e.kind(),
                _ => "RuntimeError",
            };
            (
                Value::Null,
                json!({"kind":kind,"message":super::super::engine::failure_message(&e)}),
            )
        }
    };
    let s = state.borrow();
    Ok(json!({"result":result,"error":error,"events":s.events,"emits":emits}))
}
type TestResultValue = Result<Value, Box<dyn Error>>;
#[test]
fn current_room_cycles_match_source_without_counting_recovery_as_acceptance() -> TestResult {
    let mut cases: Value = serde_json::from_str(
        r#"[
      {},{"cycles":1},{"cycles":5},{"options":{"resume_arcane":true,"resume_town":true,"resume_labels_off":true,"continue_navigation":"old.json"}},
      {"options":{"return_from":"east"}},{"stopped":true},{"rooms":[{"stop":true}]},{"rooms":[{}, {"code":2}]},
      {"rooms":[{"report":{"status":"field_paused"}}]},{"rooms":[{"report":{"status":null}}]},
      {"rooms":[{"error":"child failed"}]},{"rooms":[{"read_error":"report missing"}]},{"rooms":[{"report":{}}]},
      {"options":{"live":false,"automatic":false}},{"options":{"automatic":false}},{"cycles":0},{"cycles":6},
      {"options":{"continuous":false}},{"options":{"all_wings":true}},{"options":{"wing":"east"}}
    ]"#,
    )?;
    for c in cases.as_array_mut().ok_or("cases")? {
        c["operation"] = json!("cycles");
    }
    let expected =
        crate::test_support::legacy_expectation("main-execution-oracle", &json!({"cases":cases}))?;
    for (i, c) in cases.as_array().ok_or("cases")?.iter().enumerate() {
        compare(&exercise(c)?, &expected[i], &format!("cycles[{i}]"));
    }
    Ok(())
}
#[test]
fn child_and_summary_write_failures_remain_visible() -> TestResult {
    let got = exercise(&json!({"rooms":[{"error":"child failed"}],"write_error":true}))?;
    let error = got["error"]["message"].as_str().ok_or("error")?;
    assert!(error.contains("child failed"));
    assert!(error.contains("summary write failed"));
    assert_eq!(got["events"][1][1]["status"], "failed");
    Ok(())
}

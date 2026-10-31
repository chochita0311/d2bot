use super::*;
use std::cell::Cell;
struct Runner<'a> {
    case: &'a Value,
    now: Cell<f64>,
    focus: Cell<bool>,
    index: usize,
    events: Vec<Value>,
    observer: ArcanePathObserver,
}
impl SearchEnvironment for Runner<'_> {
    fn clock(&self) -> f64 {
        self.now.get()
    }
    fn focused(&self) -> Result<bool, RunError> {
        Ok(self.focus.get())
    }
    fn interrupted(&self) -> bool {
        false
    }
    fn observe(&mut self, generation: Option<u64>) -> Result<Option<SearchObservation>, RunError> {
        let rows = self.case["rows"]
            .as_array()
            .ok_or_else(|| RunError::Runtime("rows".into()))?;
        let Some(row) = rows.get(self.index) else {
            return Ok(None);
        };
        self.index += 1;
        self.now.set(
            row["now"]
                .as_f64()
                .unwrap_or(self.now.get() + row["dt"].as_f64().unwrap_or(0.0)),
        );
        if let Some(error) = row["error"].as_str() {
            return Err(RunError::Runtime(error.into()));
        }
        let size = Size::new(200, 200).map_err(crate::common::error::ObservationError::from)?;
        let mask = GrayFrame::from_vec(size, vec![1; 40000])
            .map_err(crate::common::error::ObservationError::from)?;
        let view = self.observer.from_mask(
            mask.view(),
            super::super::super::navigation::PathContext {
                sequence: self.index as u64,
                generation: generation.unwrap_or(0),
                captured_at: self.clock(),
                frame_size: size,
                viewport: PixelRect::full(size),
                anchor: [100.0, 100.0],
            },
        )?;
        self.now
            .set(self.now.get() + row["age"].as_f64().unwrap_or(0.0));
        Ok(Some(SearchObservation {
            view,
            motion: CameraMotion {
                displacement: Some([0.0; 2]),
                inliers: 20,
                reason: MotionReason::BackgroundTranslation,
                matches: 20,
                inlier_ratio: None,
                linear_error: None,
            },
            movement_allowed: row["allowed"].as_bool().unwrap_or(true),
            terminal: serde_json::from_value(row["terminal"].clone())?,
            hub_ready: false,
            hover: None,
        }))
    }
    fn move_pointer(&mut self, aim: [i32; 2]) -> Result<bool, RunError> {
        self.events.push(json!(["move", aim]));
        self.now
            .set(self.now.get() + self.case["move_delay"].as_f64().unwrap_or(0.0));
        if self.case["move_error"] == true {
            return Err(RunError::Runtime("move failed".into()));
        }
        self.focus.set(self.case["lose_focus"] != true);
        Ok(self.case["move_rejected"] != true)
    }
    fn press(&mut self, key: &str) -> Result<bool, RunError> {
        self.events.push(json!(["press", key]));
        if self.case["press_error"] == true {
            return Err(RunError::Runtime("press failed".into()));
        }
        Ok(self.case["press_rejected"] != true)
    }
    fn key_up(&mut self, key: &str) -> Result<(), RunError> {
        self.events.push(json!(["up", key]));
        if self.case["release_error"] == true {
            Err(RunError::Runtime("release failed".into()))
        } else {
            Ok(())
        }
    }
    fn wait(&mut self, seconds: f64) -> Result<bool, RunError> {
        self.events.push(json!(["wait", seconds]));
        self.now.set(self.now.get() + seconds);
        if self.case["wait_error"] == true {
            return Err(RunError::Runtime("wait failed".into()));
        }
        Ok(self.case["wait_stop"] == true
            || self.index >= self.case["rows"].as_array().map_or(0, Vec::len))
    }
}
fn runner(case: &Value) -> Result<Value, Box<dyn Error>> {
    let mut search = ArcaneSearch::new(observer()?, serde_json::from_value(limits())?, 0.32, 0.0)?;
    let mut env = Runner {
        case,
        now: Cell::new(0.0),
        focus: Cell::new(true),
        index: 0,
        events: vec![],
        observer: observer()?,
    };
    let mut actions = vec![];
    let result = run_search(&mut search, &mut env, "f2", |action| {
        actions.push(action.clone());
        if case["emit_error"] == true {
            Err(RunError::Runtime("emit failed".into()))
        } else {
            Ok(())
        }
    });
    let mut output = match result {
        Ok(action) => json!({"result":action}),
        Err(error) => json!({"error":error.to_string()}),
    };
    output["state"] = snapshot(&search);
    output["events"] = json!(env.events);
    output["actions"] = json!(actions);
    Ok(output)
}
#[test]
fn retained_search_runner_matches_source_input_refusal_handoff_and_release() -> TestResult {
    let rows = json!([{},{"dt":0.05},{"terminal":"summoner_present"}]);
    let mut cases = vec![
        json!({"rows":rows}),
        json!({"rows":[{"error":"capture failed"}]}),
        json!({"rows":[{"age":0.51}]}),
        json!({"rows":[{"allowed":false}]}),
    ];
    for flag in [
        "move_error",
        "move_rejected",
        "press_error",
        "press_rejected",
        "wait_error",
        "wait_stop",
        "emit_error",
        "lose_focus",
        "release_error",
    ] {
        let mut case = json!({"rows":rows});
        case[flag] = json!(true);
        cases.push(case);
    }
    cases.push(json!({"rows":rows,"move_delay":0.5}));
    let requests = cases
        .iter()
        .map(|case| json!({"runner":case}))
        .collect::<Vec<_>>();
    let expected = crate::test_support::legacy_expectation("search-oracle", &json!(requests))?;
    for (index, case) in cases.iter().enumerate() {
        compare(
            &runner(case)?,
            &expected[index],
            &format!("search runner {index}"),
        );
    }
    let combined = runner(&json!({"rows":rows,"move_error":true,"release_error":true}))?;
    let error = combined["error"].as_str().ok_or("error")?;
    assert!(error.contains("move failed") && error.contains("release failed"));
    assert_eq!(
        combined["state"]["review_reason"],
        "observer_or_input_error"
    );
    assert_eq!(
        combined["events"].as_array().and_then(|v| v.last()),
        Some(&json!(["up", "f2"]))
    );
    Ok(())
}

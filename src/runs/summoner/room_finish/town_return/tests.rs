use super::*;
use crate::{
    runs::summoner::encounter_execution::EncounterEnvironment,
    test_field_world::{World, compare, config, frame_index},
};
use std::error::Error;
type TestResult = Result<(), Box<dyn Error>>;
struct Runtime {
    world: World,
    data: Value,
}
impl Runtime {
    fn send(&mut self, action: Value) -> Result<(), RunError> {
        self.world.event(json!([
            "dispatch",
            self.world.index(),
            action,
            crate::common::time::MonotonicClock::now(&self.world)
        ]));
        self.world.delay("dispatch_delay");
        if let Some(e) = self.world.sample()["dispatch_error"].as_str() {
            return Err(RunError::Runtime(e.into()));
        }
        Ok(())
    }
}
impl TownFinishRuntime for Runtime {
    fn begin(&mut self) -> Result<(), RunError> {
        Ok(())
    }
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError> {
        EncounterEnvironment::capture(&mut self.world)
    }
    fn cursor_shift(&mut self, _: &Arc<BgrFrame>) -> Result<CursorShift, RunError> {
        self.world
            .event(json!(["cursor_shift", self.world.index()]));
        Ok(if self.world.sample()["shift"] == "pending" {
            CursorShift::Pending
        } else {
            CursorShift::Unrelated
        })
    }
    fn step(&mut self, _: &Arc<BgrFrame>) -> Result<Option<TownAction>, RunError> {
        self.world.event(json!(["plan", self.world.index()]));
        self.world.delay("plan_delay");
        let s = self.world.sample();
        if let Some(e) = s["plan_error"].as_str() {
            return Err(RunError::Runtime(e.into()));
        }
        self.data["phase"] = s.get("phase").cloned().unwrap_or(json!("done"));
        s.get("action")
            .map(|v| {
                serde_json::from_value(v.clone())
                    .map_err(|e| crate::common::error::ObservationError::from(e).into())
            })
            .transpose()
    }
    fn phase(&self) -> &str {
        self.data["phase"].as_str().unwrap_or("invalid")
    }
    fn snapshot(&self) -> Result<Value, RunError> {
        Ok(self.data.clone())
    }
    fn dispatch(&mut self, _: &Arc<BgrFrame>, a: &TownAction) -> Result<(), RunError> {
        self.send(json!(a))
    }
    fn hidden_map(&self) -> bool {
        self.data["arcane_map_inspection"]["status"] == "hidden"
    }
    fn clear_hud(&mut self, _: &Arc<BgrFrame>) -> Result<bool, RunError> {
        self.world.event(json!(["town_frame", self.world.index()]));
        self.world.delay("town_delay");
        let s = self.world.sample();
        self.world.event(json!(["find", "hud", self.world.index()]));
        if s["hud"] == false {
            return Ok(false);
        }
        for (name, key) in [
            ("waypoint_header", "panel"),
            ("save_exit", "save_exit"),
            ("npc_cancel", "npc"),
        ] {
            self.world.event(json!(["find", name, self.world.index()]));
            if s[key] == true {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn map_restore_requested(&mut self) {
        self.data["arcane_map_inspection"]["status"] = json!("restore_requested_after_error");
    }
    fn restore_map(&mut self, _: &Arc<BgrFrame>) -> Result<(), RunError> {
        self.send(json!({"kind":"key","key":"tab","reason":RESTORE_REASON}))
    }
    fn release_mouse(&mut self) -> Result<(), RunError> {
        self.world.event(json!(["release"]));
        if self.world.state().case["release_error"] == true {
            return Err(RunError::Runtime("mouse release failed".into()));
        }
        Ok(())
    }
}
fn exercise(c: &Value) -> Result<Value, Box<dyn Error>> {
    let w = World::new(c);
    if c["stop_at"].as_f64().is_some_and(|at| at <= 10.0) {
        w.1.set();
    }
    let mut runtime = Runtime {
        world: w.clone(),
        data: c
            .get("planner")
            .cloned()
            .unwrap_or(json!({"phase":"destination_arrival"})),
    };
    let mut evidence = vec![];
    let mut emits = vec![];
    let result = return_to_act1_and_exit(&w, &mut runtime, &mut evidence, |s| {
        emits.push(s.to_owned())
    });
    let (result, error) = match result {
        Ok(v) => (v, Value::Null),
        Err(e) => (
            Value::Null,
            json!({"kind":"RuntimeError","message":e.to_string()}),
        ),
    };
    let s = w.state();
    Ok(
        json!({"result":result,"error":error,"events":s.events,"emits":emits,"evidence":evidence.iter().map(|e|json!({"file":e.filename,"frame":frame_index(&e.frame)})).collect::<Vec<_>>(),"now":s.now,"planner":runtime.data}),
    )
}
#[test]
fn room_return_preserves_phase_actions_shift_wait_and_error_map_restoration() -> TestResult {
    let (raw, _) = config()?;
    let action = json!({"kind":"key","key":"esc","reason":"test town action","phase":"act1_return","number":1});
    let base = json!({"frames":[{"phase":"act1_return","action":action},{"phase":"done"}]});
    let mut cases = vec![base.clone()];
    for extra in [
        json!({"stop_at":10.0}),
        json!({"stop_at":10.02}),
        json!({"focus_lost_at":10.0}),
        json!({"release_error":true}),
        json!({"frames":[{"plan_error":"current waypoint absent"}]}),
        json!({"frames":[{"phase":"done","plan_delay":0.25}]}),
        json!({"frames":[{"phase":"done","plan_delay":0.25,"action":action}]}),
        json!({"frames":[{"shift":"pending","phase":"done"}]}),
        json!({"frames":[{"shift":"pending"},{"phase":"act1_return"},{"shift":"pending"},{"phase":"done"}]}),
    ] {
        let mut c = base.clone();
        for (k, v) in extra.as_object().ok_or("object")? {
            c[k] = v.clone();
        }
        cases.push(c);
    }
    for obstruction in [
        json!({}),
        json!({"panel":true}),
        json!({"hud":false}),
        json!({"save_exit":true}),
        json!({"npc":true}),
        json!({"town_delay":0.25}),
        json!({"dispatch_error":"restore request failed"}),
    ] {
        let mut sample = obstruction;
        sample["plan_error"] = json!("current waypoint absent");
        cases.push(json!({"planner":{"phase":"destination_arrival","arcane_map_inspection":{"status":"hidden"}},"frames":[sample]}));
    }
    for extra in [json!({"stop_at":10.0}), json!({"focus_lost_at":10.0})] {
        let mut c = json!({"planner":{"phase":"destination_arrival","arcane_map_inspection":{"status":"hidden"}},"frames":[{"plan_error":"current waypoint absent"}]});
        for (k, v) in extra.as_object().ok_or("object")? {
            c[k] = v.clone();
        }
        cases.push(c);
    }
    for c in &mut cases {
        c["operation"] = json!("town");
        for frame in c["frames"].as_array_mut().ok_or("frames")? {
            if frame.get("capture_delay").is_none() {
                frame["capture_delay"] = json!(0.01);
            }
        }
    }
    let expected = crate::test_support::legacy_expectation(
        "canyon-execution-oracle",
        &json!({"policy":raw,"cases":cases}),
    )?;
    for (i, c) in cases.iter().enumerate() {
        compare(&exercise(c)?, &expected[i], &format!("room return[{i}]"));
    }
    Ok(())
}
#[test]
fn failed_room_return_retains_primary_and_mouse_release_errors() -> TestResult {
    let c = json!({"frames":[{"plan_error":"current waypoint absent"}],"release_error":true});
    let got = exercise(&c)?;
    assert_eq!(
        got["error"]["message"],
        "current waypoint absent; cleanup also failed: mouse release failed"
    );
    assert_eq!(
        got["events"]
            .as_array()
            .ok_or("events")?
            .last()
            .ok_or("last")?,
        &json!(["release"])
    );
    Ok(())
}

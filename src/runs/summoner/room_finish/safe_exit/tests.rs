use super::*;
use crate::{
    common::{error::ObservationError, frame::Size},
    runs::summoner::encounter_execution::EncounterEnvironment,
    test_field_world::{World, compare, config, frame_index, point},
};
use std::error::Error;
type TestResult = Result<(), Box<dyn Error>>;
impl ExitEnvironment for World {
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError> {
        let tiny = EncounterEnvironment::capture(self)?;
        let s = self.state();
        let size = point(&s.case["frame_size"], [1922.0, 1140.0]);
        drop(s);
        let mut frame = BgrFrame::zeros(
            Size::new(size[0] as usize, size[1] as usize).map_err(ObservationError::from)?,
        )
        .map_err(ObservationError::from)?;
        frame.pixels_mut()[..3].copy_from_slice(tiny.pixels());
        Ok(Arc::new(frame))
    }
    fn frame_to_desktop(&self, p: [f64; 2]) -> Result<DesktopPoint, ControlError> {
        Ok([
            (p[0] - 1922.0).round_ties_even() as i64,
            p[1].round_ties_even() as i64,
        ])
    }
}
struct Vision(World);
impl MenuVision for Vision {
    fn find(&mut self, _: &Arc<BgrFrame>, name: &str) -> Result<Option<[f64; 2]>, RunError> {
        self.0.event(json!(["town_frame", self.0.index()]));
        self.0.delay("town_delay");
        self.0.event(json!(["find", name, self.0.index()]));
        let s = self.0.sample();
        self.0.advance(s["menu_delay"].as_f64().unwrap_or(0.08));
        Ok(if s.get(name) == Some(&Value::Null) {
            None
        } else {
            Some(point(
                &s[name],
                if name == "save_exit" {
                    [600.0, 300.0]
                } else {
                    [600.0, 650.0]
                },
            ))
        })
    }
}
fn exercise(c: &Value) -> Result<Value, Box<dyn Error>> {
    let mut w = World::new(c);
    if c["stop_at"].as_f64().is_some_and(|at| at <= 10.0) {
        w.1.set();
    }
    let backend = w.clone();
    let guards = w.clone();
    let mut vision = Vision(w.clone());
    let mut evidence = vec![];
    let mut diagnostics = json!({});
    let outcome = exit_to_character(
        &backend,
        &guards,
        &mut w,
        &mut vision,
        &mut evidence,
        &mut diagnostics,
    );
    let error = outcome
        .err()
        .map(|e| json!({"kind":"RuntimeError","message":e.to_string()}))
        .unwrap_or(Value::Null);
    let state = w.state();
    Ok(
        json!({"result":null,"error":error,"events":state.events,"emits":state.emits,"evidence":evidence.iter().map(|e|json!({"file":e.filename,"frame":frame_index(&e.frame)})).collect::<Vec<_>>(),"now":state.now,"diagnostics":diagnostics}),
    )
}
#[test]
fn emergency_exit_requires_current_menu_settled_pointer_and_selection() -> TestResult {
    let (raw, _) = config()?;
    let mut cases: Value = serde_json::from_str(
        r#"[
 {},{"frames":[{"menu_delay":0.3}]},{"pointer_drift":true},{"stop_at":10.11},{"focus_lost_at":10.11},{"focus_lost_at":10},{"stop_at":10},
 {"frames":[{"save_exit":null}]},{"press_delay":7.9},{"frames":[{"play":null}]},
 {"frame_size":[200,140],"frames":[{"save_exit":[633.5,376.5],"menu_delay":0.0}]},
 {"frames":[{"capture_delay":0.25}]},{"move_delay":0.3},{"click_delay":0.3},
 {"frames":[{"save_exit":[600,300]},{"save_exit":[601,300]},{"save_exit":[601,300]}]},
 {"frames":[{"save_exit":[600,300]},{"save_exit":[620,300]},{"save_exit":[620,300]}]}
 ]"#,
    )?;
    for c in cases.as_array_mut().ok_or("cases")? {
        c["operation"] = json!("exit");
        if !c["frames"].is_array() {
            c["frames"] = json!([{}]);
        }
        for f in c["frames"].as_array_mut().ok_or("frames")? {
            if f.get("capture_delay").is_none() {
                f["capture_delay"] = json!(0.02);
            }
        }
    }
    let expected = crate::test_support::legacy_expectation(
        "canyon-execution-oracle",
        &json!({"policy":raw,"cases":cases}),
    )?;
    for (i, c) in cases.as_array().ok_or("cases")?.iter().enumerate() {
        compare(&exercise(c)?, &expected[i], &format!("safe exit[{i}]"));
    }
    Ok(())
}

use super::*;
use crate::{
    common::time::MonotonicClock,
    runs::summoner::encounter_execution::EncounterEnvironment,
    test_field_world::{World, compare, config, frame_index, point},
};
use std::error::Error;
type TestResult = Result<(), Box<dyn Error>>;
impl CanyonBackend for World {
    fn click(&self) -> Result<(), ControlError> {
        self.event(json!(["click", self.now()]));
        let delay = self.state().case["click_delay"].as_f64().unwrap_or(0.0);
        self.advance(delay);
        Ok(())
    }
}
impl CanyonEnvironment for World {
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError> {
        EncounterEnvironment::capture(self)
    }
    fn hud(&mut self, f: &Arc<BgrFrame>) -> Result<HudReading, RunError> {
        EncounterEnvironment::hud(self, f)
    }
    fn frame_to_desktop(&self, p: [f64; 2]) -> Result<DesktopPoint, ControlError> {
        Ok([
            (p[0] - 1922.0).round_ties_even() as i64,
            p[1].round_ties_even() as i64,
        ])
    }
    fn emit(&mut self, s: &str) {
        EncounterEnvironment::emit(self, s)
    }
}
struct Vision(World);
impl Vision {
    fn read(&self, key: &str) -> Option<[f64; 2]> {
        self.0.event(json!([key, self.0.index()]));
        self.0.delay(&format!("{key}_delay"));
        let value = self.0.sample()[key].clone();
        value.as_array().map(|_| point(&value, [0.0; 2]))
    }
}
impl CanyonVision for Vision {
    fn town_frame(&mut self, _: &Arc<BgrFrame>) -> Result<(), RunError> {
        self.0.event(json!(["town_frame", self.0.index()]));
        self.0.delay("town_delay");
        Ok(())
    }
    fn find(&mut self, name: &str) -> Result<bool, RunError> {
        self.0.event(json!(["find", name, self.0.index()]));
        let s = self.0.sample();
        let area = s["area"].as_str().unwrap_or("arcane");
        Ok(match name {
            "area_canyon" => area == "canyon",
            "area_arcane" => area == "arcane",
            "hud" => s["hud"].as_bool().unwrap_or(true),
            "world_canyon_waypoint" => s["waypoint"] == true,
            "save_exit" => s["save_exit"] == true,
            "waypoint_header" => s["panel"] == true,
            _ => return Err(RunError::Runtime(format!("unknown town query {name}"))),
        })
    }
    fn steps(&mut self, _: &Arc<BgrFrame>) -> Result<Option<[f64; 2]>, RunError> {
        Ok(self.read("stairs"))
    }
    fn portal(&mut self, _: &Arc<BgrFrame>) -> Result<Option<[f64; 2]>, RunError> {
        Ok(self.read("portal"))
    }
    fn journal(&mut self, _: &Arc<BgrFrame>) -> Result<Option<[f64; 2]>, RunError> {
        Ok(self.read("journal"))
    }
    fn prepares_ground(&self) -> bool {
        self.0.state().case["camera"] == true
    }
    fn prepare(
        &mut self,
        _: &Arc<BgrFrame>,
        at: f64,
        _: &HudReading,
        focus: bool,
    ) -> Result<(), RunError> {
        self.0.event(json!(["prepare", self.0.index(), at, focus]));
        self.0.delay("prepare_delay");
        Ok(())
    }
    fn ground(&mut self, _: &Arc<BgrFrame>, at: f64) -> Result<Option<[f64; 2]>, RunError> {
        self.0.event(json!(["ground", self.0.index(), at]));
        self.0.delay("ground_delay");
        let s = self.0.sample();
        Ok(if s.get("player") == Some(&Value::Null) {
            None
        } else {
            Some(point(&s["player"], [961.0, 578.0]))
        })
    }
    fn landing(
        &mut self,
        _: &Arc<BgrFrame>,
        sequence: u64,
        at: f64,
        player: [f64; 2],
        journal: [f64; 2],
    ) -> Result<Option<[f64; 2]>, RunError> {
        self.0
            .event(json!(["path", self.0.index(), sequence, at, player]));
        self.0.delay("path_delay");
        self.0
            .event(json!(["landing", self.0.index(), journal, player]));
        self.0.delay("landing_delay");
        let s = self.0.sample();
        Ok(if s.get("landing") == Some(&Value::Null) {
            None
        } else {
            Some(point(&s["landing"], [1098.0, 450.0]))
        })
    }
}
fn exercise(c: &Value, policy: &CharacterSurvivalPolicy) -> Result<Value, Box<dyn Error>> {
    let mut w = World::new(c);
    let backend = w.clone();
    let guards = w.clone();
    let mut v = Vision(w.clone());
    let mut evidence = vec![];
    let result = enter_canyon(
        &backend,
        &guards,
        &mut w,
        &mut v,
        CanyonRequest {
            policy,
            kill_confirmed: c["kill"].as_bool().unwrap_or(true),
            loot_complete: c["loot"].as_bool().unwrap_or(true),
            buff_expires_at: c["buff"].as_f64().unwrap_or(100.0),
            reference_seconds: c["reference"].as_f64().unwrap_or(0.32),
            movement_key: "f2",
        },
        &mut evidence,
    );
    let (result, error) = match result {
        Ok(v) => (v, Value::Null),
        Err(RunError::Control(ControlError::Configuration(e))) => {
            (Value::Null, json!({"kind":e.kind(),"message":e.message()}))
        }
        Err(e) => (
            Value::Null,
            json!({"kind":"RuntimeError","message":e.to_string()}),
        ),
    };
    let s = w.state();
    Ok(
        json!({"result":result,"error":error,"events":s.events,"emits":s.emits,"now":s.now,"evidence":evidence.iter().map(|e|json!({"file":e.filename,"frame":frame_index(&e.frame)})).collect::<Vec<_>>()}),
    )
}
#[test]
fn canyon_finish_current_guards_and_sequence_match_source() -> TestResult {
    let (raw, policy) = config()?;
    let mut frames = vec![];
    for sample in [
        json!({"journal":[938,369.5]}),
        json!({}),
        json!({"portal":[800,514]}),
        json!({"area":"canyon","stairs":[1010.5,154.5]}),
    ] {
        frames.push(sample.clone());
        frames.push(sample);
    }
    frames.push(json!({"area":"canyon","waypoint":true}));
    let base = json!({"frames":frames});
    let mut cases = vec![base.clone()];
    for extra in [
        json!({"kill":false}),
        json!({"loot":false}),
        json!({"buff":10}),
        json!({"stop_at":10.01}),
        json!({"focus_lost_at":10}),
        json!({"pointer_drift":true}),
        json!({"camera":true}),
        json!({"move_delay":0.3}),
        json!({"focus_lost_at":10.055}),
        json!({"stop_at":10.055}),
    ] {
        let mut c = base.clone();
        for (k, v) in extra.as_object().ok_or("object")? {
            c[k] = v.clone();
        }
        cases.push(c);
    }
    for sample in [
        json!({"life":5}),
        json!({"life":null}),
        json!({"merc":"unverified"}),
        json!({"save_exit":true}),
        json!({"panel":true}),
        json!({"journal":[938,369.5],"player":null}),
        json!({"journal":[938,369.5],"player":[938,700],"landing":null}),
        json!({"journal":[938,369.5],"player":[938,700]}),
        json!({"area":"canyon"}),
        json!({"area":"canyon","stairs":[1010.5,154.5]}),
        json!({"portal":[10,20]}),
    ] {
        cases.push(json!({"frames":[sample],"buff":12.0}));
    }
    let mut open = base.clone();
    open["frames"] = json!([{"portal":[800,514]},{"portal":[800,514]},{"area":"canyon","life":null},{"area":"canyon","waypoint":true}]);
    cases.push(open);
    for sample in [
        json!({"capture_delay":0.25}),
        json!({"life":5}),
        json!({"hover":true,"pointer_drift":true}),
        json!({"town_delay":0.25}),
    ] {
        let mut c = base.clone();
        for (k, v) in sample.as_object().ok_or("sample")? {
            c["frames"][1][k] = v.clone();
        }
        cases.push(c);
    }
    for c in &mut cases {
        for f in c["frames"].as_array_mut().ok_or("frames")? {
            if f.get("capture_delay").is_none() {
                f["capture_delay"] = json!(0.01);
            }
        }
    }
    let expected = crate::test_support::legacy_expectation(
        "canyon-execution-oracle",
        &json!({"policy":raw,"cases":cases}),
    )?;
    for (i, c) in cases.iter().enumerate() {
        compare(
            &exercise(c, &policy)?,
            &expected[i],
            &format!("canyon loop[{i}]"),
        );
    }
    Ok(())
}

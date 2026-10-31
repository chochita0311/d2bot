use super::*;
fn dispatched(value: &Value) -> Result<BuffDispatchResult, Box<dyn Error>> {
    Ok(BuffDispatchResult {
        status: "dispatched_unconfirmed".into(),
        buff_dispatches: value["buff_dispatches"]
            .as_array()
            .ok_or("dispatches")?
            .iter()
            .map(|v| {
                Ok(BuffDispatch {
                    key: v["key"].as_str().ok_or("key")?.into(),
                    cast_request_at: v["cast_request_at"].as_f64().unwrap_or(0.0),
                    cast_request_monotonic: v["cast_request_monotonic"].as_f64().ok_or("mono")?,
                })
            })
            .collect::<Result<Vec<_>, Box<dyn Error>>>()?,
        equipment_dispatch_balanced: true,
        effect_verified: false,
        held_after: vec![],
        errors: vec![],
        trace_phase: "after_dispatch",
    })
}
#[test]
fn retained_buff_request_times_and_deadlines_match_source() -> TestResult {
    let (raw, policy) = config()?;
    let buff = json!({"status":"dispatched_unconfirmed","buff_dispatches":policy.buffs().iter().map(|b|json!({"key":b.key(),"cast_request_at":1000.0,"cast_request_monotonic":10.0})).collect::<Vec<_>>()});
    let base = json!({"kind":"reuse","wall":1001.0,"mono":11.0,"report":{"buff":buff,"supervised_effect":{"character_id":"flash","source":"supervised_visual","effect_verified":true,"equipment_restored":true,"sequence_effects_verified":true,"observed_at":1000.5}}});
    let mut cases = vec![base.clone()];
    for extra in [
        json!({"wall":999.9}),
        json!({"mono":9.9}),
        json!({"mono":13.0}),
        json!({"mono":13.00001}),
        json!({"wall":2000.0}),
    ] {
        let mut c = base.clone();
        for (k, v) in extra.as_object().ok_or("object")? {
            c[k] = v.clone();
        }
        cases.push(c);
    }
    for (key, value) in [
        ("character_id", json!("other")),
        ("source", json!("runtime_screen")),
        ("effect_verified", json!(false)),
        ("equipment_restored", json!(1)),
        ("sequence_effects_verified", Value::Null),
        ("observed_at", json!(999.9)),
        ("observed_at", json!(true)),
        ("observed_at", json!("1001")),
    ] {
        let mut c = base.clone();
        c["report"]["supervised_effect"][key] = value;
        cases.push(c);
    }
    for safe_exit in [Value::Null, json!({}), json!(false)] {
        let mut c = base.clone();
        c["report"]["safe_exit"] = safe_exit;
        cases.push(c);
    }
    for field in ["cast_request_at", "cast_request_monotonic"] {
        for value in [Value::Null, json!(true), json!("10")] {
            let mut c = base.clone();
            c["report"]["buff"]["buff_dispatches"][0][field] = value;
            cases.push(c);
        }
    }
    let mut missing = base.clone();
    missing["report"]["buff"]["buff_dispatches"] = json!([]);
    cases.push(missing);
    let mut last = base.clone();
    last["report"]["buff"]["buff_dispatches"]
        .as_array_mut()
        .ok_or("dispatch")?
        .insert(
            0,
            json!({"key":policy.buffs()[0].key(),"cast_request_at":1,"cast_request_monotonic":1}),
        );
    cases.push(last);
    for bound in [policy
        .buffs()
        .iter()
        .map(|b| b.duration_seconds() - b.renew_before_seconds() - 5.0)
        .fold(f64::INFINITY, f64::min)]
    {
        for delta in [-0.00001, 0.0, 0.00001] {
            let mut c = base.clone();
            c["wall"] = json!(1000.0 + bound + delta);
            c["mono"] = json!(10.0 + bound + delta);
            cases.push(c);
        }
    }
    cases.push(json!({"kind":"renew_deadlines","dispatched":buff}));
    cases.push(json!({"kind":"renew_deadlines","dispatched":{"buff_dispatches":[]}}));
    let mut empty_policy = raw.clone();
    empty_policy["buffs"] = json!([]);
    let mut empty = base.clone();
    empty["policy"] = empty_policy.clone();
    cases.push(empty);
    cases.push(json!({"kind":"renew_deadlines","dispatched":buff,"policy":empty_policy}));
    let expected = support::legacy_expectation(
        "field-preparation-oracle",
        &json!({"policy":raw,"cases":cases}),
    )?;
    for (i, c) in cases.iter().enumerate() {
        let selected = if let Some(raw) = c.get("policy") {
            CharacterSurvivalPolicy::from_value(raw)?
        } else {
            policy.clone()
        };
        let got = if c["kind"] == "reuse" {
            reusable_buffs(
                &c["report"],
                &selected,
                c["wall"].as_f64().ok_or("wall")?,
                c["mono"].as_f64().ok_or("mono")?,
            )
            .map(|v| json!([v.buff, v.deadlines]))
            .unwrap_or(Value::Null)
        } else {
            match buff_renewal_deadlines(&dispatched(&c["dispatched"])?, &selected) {
                Ok(v) => json!(v),
                Err(e) => run_error(e),
            }
        };
        compare(&got, &expected[i], &format!("reuse[{i}]"));
    }
    Ok(())
}
struct Signals {
    env: Env,
    index: usize,
}
impl ProbeSignals<RunError> for Signals {
    fn try_read(&mut self) -> Result<Option<Value>, RunError> {
        let now = self.env.clock();
        self.env.event(json!(["signal", self.index, now]));
        let row = self.env.0.borrow().case["signals"]
            .as_array()
            .and_then(|a| a.get(self.index))
            .cloned();
        let Some(mut row) = row else { return Ok(None) };
        if now < row["available_at"].as_f64().unwrap_or(0.0) {
            return Ok(None);
        }
        self.index += 1;
        if let Some(age) = row["observed_age"].as_f64() {
            row["observed_at"] = json!(self.env.wall_clock() - age);
            row.as_object_mut()
                .ok_or_else(|| RunError::Runtime("signal object".into()))?
                .remove("observed_age");
        }
        Ok(Some(row))
    }
}
fn exercise(case: &Value, policy: &CharacterSurvivalPolicy) -> Result<Value, Box<dyn Error>> {
    let mut env = Env::new(case);
    let mut signals = Signals {
        env: env.clone(),
        index: 0,
    };
    let mut record = json!({});
    let mut evidence = vec![];
    let mut emits = vec![];
    let result = if case["kind"] == "probe" {
        let options = ProbeWait {
            deadline: case["deadline"].as_f64().unwrap_or(11.0),
            phase: case["phase"].as_str().unwrap_or("field_safety"),
            retry_stale: case["retry_stale"] == true,
            retry_unverified: case["retry_unverified"] == true,
        };
        wait_probe_signal(
            &mut env,
            &mut signals,
            policy,
            options,
            |env, s| {
                validate_buff_site_permit(s, policy, env.wall_clock())?;
                Ok(())
            },
            &mut NoPreparationChecks,
            |s| emits.push(s.to_owned()),
        )
    } else {
        let order = serde_json::from_value::<Vec<String>>(case["order"].clone())?;
        let pauses = PauseSchedule::new(
            serde_json::from_value(case.get("pauses").cloned().unwrap_or(json!({})))?,
            0.4,
        )?;
        renew_supervised_field_buffs(
            &mut env,
            &mut signals,
            SupervisedBuffRequest {
                policy,
                order: &order,
                pauses: &pauses,
                prefix: "renew",
            },
            &mut record,
            &mut evidence,
            |s| emits.push(s.to_owned()),
        )
        .map(|r| json!(r))
    };
    let (result, error) = match result {
        Ok(v) => (v, Value::Null),
        Err(e) => (Value::Null, run_error(e)),
    };
    let e = evidence
        .iter()
        .map(|e| json!({"file":e.filename,"frame":e.frame.pixels()[0]}))
        .collect::<Vec<_>>();
    let s = env.0.borrow();
    Ok(
        json!({"result":result,"error":error,"record":record,"evidence":e,"events":s.events,"emits":emits,"now":s.now}),
    )
}
#[test]
fn supervised_signals_and_full_renewal_match_original() -> TestResult {
    let (raw, policy) = config()?;
    let safety = json!({"phase":"field_safety","character_id":"flash","source":"supervised_visual","field":true,"monsters_clear":true,"belt_closed":true,"potions_remaining":1,"observed_age":0,"area":"arcane_sanctuary"});
    let mut effect = safety.clone();
    effect["phase"] = json!("initial_effect");
    for key in [
        "effect_verified",
        "equipment_restored",
        "sequence_effects_verified",
    ] {
        effect[key] = json!(true);
    }
    let order = std::iter::once("w".to_owned())
        .chain(policy.buffs().iter().map(|b| b.key().to_owned()))
        .chain(std::iter::once("w".to_owned()))
        .collect::<Vec<_>>();
    let base = json!({"kind":"renew","order":order,"signals":[safety,effect],"frames":[{}]});
    let mut cases = vec![base.clone()];
    for extra in [
        json!({"frames":[{"hazards":[null]}]}),
        json!({"frames":[{"ground":null}],"stop_at":10.3}),
        json!({"frames":[{"life":5}]}),
        json!({"frames":[{"life":null}],"stop_at":10.4}),
        json!({"focus_lost_at":10.5}),
        json!({"stop_at":10.5}),
        json!({"stop_on_key":"w"}),
        json!({"fail_key":"w"}),
        json!({"press_delay":1.0}),
    ] {
        let mut c = base.clone();
        for (k, v) in extra.as_object().ok_or("object")? {
            c[k] = v.clone();
        }
        cases.push(c);
    }
    for (key, value) in [
        ("area", json!("canyon")),
        ("monsters_clear", json!(false)),
        ("buff_safety_scope", json!("nearby")),
        ("source", json!("runtime_screen")),
    ] {
        let mut c = base.clone();
        c["signals"][0][key] = value;
        cases.push(c);
    }
    let mut nearby = base.clone();
    nearby["signals"][0]["buff_safety_scope"] = json!("nearby");
    nearby["signals"][0]["nearby_monsters_clear"] = json!(true);
    nearby["signals"][0]["monsters_clear"] = json!(false);
    cases.push(nearby);
    let mut stale = base.clone();
    let mut old = safety.clone();
    old["observed_age"] = json!(16);
    stale["signals"]
        .as_array_mut()
        .ok_or("signals")?
        .insert(0, old);
    cases.push(stale);
    let mut wrong = base.clone();
    wrong["signals"][1]["phase"] = json!("field_safety");
    cases.push(wrong);
    let mut bad_effect = base.clone();
    bad_effect["signals"][1]["effect_verified"] = json!(false);
    cases.push(bad_effect);
    for extra in [
        json!({}),
        json!({"frames":[{"life":5}]}),
        json!({"frames":[{"capture_delay":0.25}]}),
        json!({"focus_lost_at":10}),
        json!({"retry_stale":true}),
        json!({"retry_unverified":true,"frames":[{"life":null},{}]}),
    ] {
        let mut c = json!({"kind":"probe","signals":[safety],"frames":[{}]});
        for (k, v) in extra.as_object().ok_or("object")? {
            c[k] = v.clone();
        }
        cases.push(c);
    }
    let expected = support::legacy_expectation(
        "field-preparation-oracle",
        &json!({"policy":raw,"cases":cases}),
    )?;
    for (i, c) in cases.iter().enumerate() {
        compare(
            &exercise(c, &policy)?,
            &expected[i],
            &format!("supervised[{i}]"),
        );
    }
    Ok(())
}

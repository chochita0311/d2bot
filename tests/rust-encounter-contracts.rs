use d2bot::common::{
    encounters::*,
    frame::{PixelRect, Size},
    survival::ObservationStamp,
};
use serde_json::{Value, json};
use std::{error::Error, sync::Arc};
mod support;
fn context(raw: &Value) -> Result<Arc<SceneContext>, Box<dyn Error>> {
    let stamp = &raw["stamp"];
    let stamp = ObservationStamp::new(
        stamp["character_id"].as_str().ok_or("character")?,
        stamp["room_id"].as_str().ok_or("room")?,
        stamp["sequence"].as_u64().ok_or("sequence")?,
        stamp["observed_at"].as_f64().ok_or("at")?,
    )?;
    let [w, h]: [usize; 2] = serde_json::from_value(raw["frame_size"].clone())?;
    let [l, t, r, b]: [usize; 4] = serde_json::from_value(raw["viewport"].clone())?;
    Ok(Arc::new(SceneContext::new(
        stamp,
        raw["region_id"].as_str().ok_or("region")?,
        raw["movement_generation"].as_u64().ok_or("generation")?,
        Size::new(w, h)?,
        PixelRect::new(l, t, r, b)?,
    )?))
}
fn scene(sequence: u64, at: f64) -> Value {
    json!({"stamp":{"character_id":"character","room_id":"room","sequence":sequence,"observed_at":at},"region_id":"region","movement_generation":0,"frame_size":[200,100],"viewport":[0,0,200,100]})
}
fn candidate(raw: &Value) -> Result<VisualCandidate, Box<dyn Error>> {
    let [l, t, r, b]: [usize; 4] = serde_json::from_value(raw["box"].clone())?;
    let identity = CandidateIdentity::new(
        raw["asset_id"].as_str().ok_or("asset")?,
        raw["kind"].as_str().ok_or("kind")?,
        serde_json::from_value(raw["category"].clone())?,
        serde_json::from_value(raw["appearance"].clone())?,
    )?;
    Ok(VisualCandidate::new(
        context(&raw["context"])?,
        identity,
        raw["score"].as_f64().ok_or("score")?,
        PixelRect::new(l, t, r, b)?,
        serde_json::from_value(raw["ground"].clone())?,
    )?)
}
fn witness(raw: &Value) -> Result<Option<LifeWitness>, Box<dyn Error>> {
    if raw.is_null() {
        return Ok(None);
    }
    Ok(Some(LifeWitness::new(
        context(&raw["context"])?,
        raw["target_id"].as_str().ok_or("target")?,
        raw["kind"].as_str().ok_or("kind")?,
        serde_json::from_value(raw["state"].clone())?,
        serde_json::from_value(raw["provenance"].clone())?,
        raw["linked_alive_sequence"].as_u64(),
    )?))
}
#[test]
fn current_distance_boundaries_hysteresis_and_missing_evidence_match_source()
-> Result<(), Box<dyn Error>> {
    let current = scene(10, 10.0);
    let base = json!({"kind":"proximity","current":current,"candidate":{"context":current,"asset_id":"asset","kind":"monster-kind","category":"monster","appearance":"alive","score":0.95,"box":[90,40,150,70],"ground":[110,50]},"player":[100,50],"radius":[0.1,0.2],"now":10,"max_age":3,"target_id":"a"});
    let mut cases = Vec::new();
    for x in [110.0, 120.0, 120.001, 139.999, 140.0] {
        let mut case = base.clone();
        case["candidate"]["ground"] = json!([x, 50]);
        cases.push(case);
    }
    for (path, value) in [
        ("/candidate/category", json!("monster-group")),
        ("/candidate/category", json!("landmark")),
        ("/candidate/appearance", json!("corpse")),
        ("/candidate/ground", Value::Null),
        ("/player", Value::Null),
        ("/now", json!(9)),
        ("/now", json!(13.01)),
        ("/candidate/context/stamp/sequence", json!(9)),
    ] {
        let mut case = base.clone();
        *case.pointer_mut(path).ok_or("path")? = value;
        cases.push(case);
    }
    let mut retained = base.clone();
    retained["candidate"]["ground"] = json!([130, 50]);
    retained["previous"] =
        json!({"state":"near","distance":0.05,"reason":"candidate-radius","target_id":"a"});
    retained["previous_context"] = scene(9, 9.0);
    cases.push(retained.clone());
    for (path, value) in [
        ("/previous/target_id", json!("b")),
        ("/target_id", Value::Null),
        ("/previous_context/movement_generation", json!(1)),
        ("/previous_context/stamp/sequence", json!(10)),
        ("/previous_context/stamp/observed_at", json!(10)),
        ("/previous_context/stamp/observed_at", json!(6.9)),
    ] {
        let mut case = retained.clone();
        *case.pointer_mut(path).ok_or("path")? = value;
        cases.push(case);
    }
    let expected = support::legacy_expectation("encounter-oracle", &json!(cases))?;
    for (i, case) in cases.iter().enumerate() {
        let current = context(&case["current"])?;
        let candidate = candidate(&case["candidate"])?;
        let previous = case.get("previous").map(|v| Proximity {
            state: match v["state"].as_str() {
                Some("near") => ProximityState::Near,
                _ => ProximityState::Unknown,
            },
            distance: v["distance"].as_f64(),
            reason: "candidate-radius",
            target_id: v["target_id"].as_str().map(str::to_owned),
        });
        let previous_context = case.get("previous_context").map(context).transpose()?;
        let [enter, leave]: [f64; 2] = serde_json::from_value(case["radius"].clone())?;
        let actual = proximity(
            &candidate,
            &current,
            serde_json::from_value(case["player"].clone())?,
            CombatRadius::new(enter, leave)?,
            ProximityOptions {
                now: case["now"].as_f64().ok_or("now")?,
                max_age: case["max_age"].as_f64().ok_or("age")?,
                target_id: case["target_id"].as_str(),
                previous: previous.as_ref(),
                previous_context: previous_context.as_deref(),
            },
        )?;
        let actual = serde_json::to_value(actual)?;
        for key in ["state", "reason", "target_id"] {
            assert_eq!(actual[key], expected[i][key], "case {i} {key}");
        }
        match (
            actual["distance"].as_f64(),
            expected[i]["distance"].as_f64(),
        ) {
            (Some(a), Some(b)) => assert!((a - b).abs() <= 1e-12, "case {i}"),
            (None, None) => (),
            _ => return Err("distance unknown differs".into()),
        }
    }
    assert_eq!(expected[1]["state"], "near");
    assert_eq!(expected[4]["state"], "far");
    assert_eq!(expected[13]["state"], "near");
    Ok(())
}
#[test]
fn life_needs_independent_ordered_linked_current_witnesses() -> Result<(), Box<dyn Error>> {
    let raw = |sequence: u64, at: f64, state: &str, link: Option<u64>| json!({"context":scene(sequence,at),"target_id":"target","kind":"summoner","state":state,"provenance":"linked-observation","linked_alive_sequence":link});
    let base = json!({"kind":"life","current":scene(12,10.2),"now":10.2,"max_age":3,"witness":raw(12,10.2,"dead",Some(10)),"previous_alive":raw(10,10.0,"alive",None),"first_dead":raw(11,10.1,"dead",Some(10))});
    let mut cases = vec![base.clone()];
    for (path, value) in [
        ("/witness", Value::Null),
        ("/witness/state", json!("alive")),
        ("/witness/state", json!("unknown")),
        ("/witness/provenance", json!("template-candidate")),
        ("/previous_alive", Value::Null),
        ("/first_dead", Value::Null),
        ("/first_dead/linked_alive_sequence", Value::Null),
        ("/witness/linked_alive_sequence", json!(11)),
        ("/previous_alive/context/movement_generation", json!(1)),
        ("/previous_alive/context/stamp/character_id", json!("other")),
        ("/first_dead/context/stamp/room_id", json!("other")),
        ("/previous_alive/context/stamp/observed_at", json!(0)),
        ("/first_dead/context/stamp/sequence", json!(10)),
        ("/first_dead/context/stamp/observed_at", json!(10.2)),
        ("/witness/context/stamp/sequence", json!(13)),
        ("/first_dead/provenance", json!("template-candidate")),
        ("/now", json!(10.1)),
    ] {
        let mut case = base.clone();
        *case.pointer_mut(path).ok_or("path")? = value;
        cases.push(case);
    }
    let expected = support::legacy_expectation("encounter-oracle", &json!(cases))?;
    for (i, case) in cases.iter().enumerate() {
        let current = context(&case["current"])?;
        let current_witness = witness(&case["witness"])?;
        let alive = witness(&case["previous_alive"])?;
        let dead = witness(&case["first_dead"])?;
        let actual = life_state(
            &current,
            current_witness.as_ref(),
            case["now"].as_f64().ok_or("now")?,
            3.0,
            alive.as_ref(),
            dead.as_ref(),
        )?;
        assert_eq!(serde_json::to_value(actual)?, expected[i], "life case {i}");
    }
    assert_eq!(expected[0], "dead");
    assert_eq!(expected[1], "unknown");
    assert_eq!(expected[2], "alive");
    Ok(())
}
#[test]
fn invalid_native_geometry_and_nonfinite_contract_values_are_errors() -> Result<(), Box<dyn Error>>
{
    let scene = context(&scene(10, 10.0))?;
    assert!(CombatRadius::new(0.2, 0.1).is_err());
    assert!(CombatRadius::new(f64::NAN, 0.2).is_err());
    let identity = CandidateIdentity::new(
        "asset",
        "kind",
        CandidateCategory::Monster,
        Appearance::Alive,
    )?;
    assert!(
        VisualCandidate::new(
            Arc::clone(&scene),
            identity.clone(),
            f64::INFINITY,
            PixelRect::new(90, 40, 150, 70)?,
            Some([100.0, 50.0])
        )
        .is_err()
    );
    assert!(
        VisualCandidate::new(
            Arc::clone(&scene),
            identity.clone(),
            0.9,
            PixelRect::new(90, 40, 150, 70)?,
            Some([150.0, 50.0])
        )
        .is_err()
    );
    assert!(
        VisualCandidate::new(
            Arc::clone(&scene),
            identity,
            0.9,
            PixelRect::new(190, 40, 200, 70)?,
            Some([199.5, 50.0])
        )
        .is_err()
    );
    assert!(scene.fresh(f64::NAN, 3.0).is_err());
    assert!(
        SceneContext::new(
            scene.stamp().clone(),
            "region",
            0,
            Size::new(200, 100)?,
            PixelRect::new(0, 0, 201, 100)?
        )
        .is_err()
    );
    Ok(())
}

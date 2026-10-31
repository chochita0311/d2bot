use d2bot::{
    common::{
        frame::{BgrFrame, Size},
        vision_ops::codec,
    },
    runs::summoner::terminal_vision::{
        ArcaneTerminalObserver, TerminalObservation, TerminalStabilityTracker,
    },
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, error::Error, path::Path, sync::Arc};
mod support;
#[test]
fn original_three_chests_anchor_current_support_for_approach_only() -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let owner = root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
    let initial = Arc::new(codec::load_color(
        owner.join("references/terminal/north-metal-three-chests.png"),
    )?);
    let shifted = frame(
        &json!({"path":"assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-metal-one-chest-after-camera-shift.png"}),
    )?;
    let mut observer = ArcaneTerminalObserver::load(owner.join("terminal-patterns.json"))?;
    let original = observer
        .observe(initial.view())?
        .ok_or("original three chests")?;
    assert!(observer.observe(shifted.view())?.is_none());
    assert!(
        observer
            .confirm_relocalized_parts(shifted.view(), &original)?
            .is_none()
    );
    let mut tracker = TerminalStabilityTracker::new(2.0)?;
    assert!(
        tracker
            .update(
                &observer,
                TerminalObservation {
                    frame: &shifted,
                    reading: None,
                    acquired_at: 45.0,
                    displacement: None
                }
            )?
            .is_none()
    );
    assert!(tracker.approach().is_none());
    tracker.update(
        &observer,
        TerminalObservation {
            frame: &initial,
            reading: Some(original),
            acquired_at: 45.024815,
            displacement: None,
        },
    )?;
    for (at, displacement) in [(45.7523203, None), (45.8523203, Some([0.0, 0.0]))] {
        assert!(
            tracker
                .update(
                    &observer,
                    TerminalObservation {
                        frame: &shifted,
                        reading: None,
                        acquired_at: at,
                        displacement
                    }
                )?
                .is_none()
        );
        let approach = tracker
            .approach()
            .ok_or("anchored current two-part approach")?;
        assert_eq!(
            approach.provenance,
            "current_anchored_support_approach_only"
        );
        assert_eq!(approach.reference_observed_at, Some(45.024815));
        assert!(approach.current_parts.iter().filter(|p| **p).count() >= 2);
        assert!(
            (approach.point[0] - 1786.0).abs() <= 3.0 && (approach.point[1] - 299.17).abs() <= 3.0
        );
    }
    tracker.update(
        &observer,
        TerminalObservation {
            frame: &shifted,
            reading: None,
            acquired_at: 47.024816,
            displacement: Some([0.0, 0.0]),
        },
    )?;
    assert!(tracker.approach().is_none());
    assert_eq!(tracker.reference_state(), (false, None));
    for boxes in [vec![[1692, 304, 1748, 346]], vec![[1379, 242, 1428, 269]]] {
        let hidden = frame(
            &json!({"path":"assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-metal-one-chest-after-camera-shift.png","clear":boxes}),
        )?;
        let full = observer.observe(initial.view())?.ok_or("full reset")?;
        tracker.update(
            &observer,
            TerminalObservation {
                frame: &initial,
                reading: Some(full),
                acquired_at: 50.0,
                displacement: None,
            },
        )?;
        tracker.update(
            &observer,
            TerminalObservation {
                frame: &hidden,
                reading: None,
                acquired_at: 50.1,
                displacement: None,
            },
        )?;
        assert!(
            tracker.approach().is_none(),
            "single remaining landmark cannot approach"
        );
        tracker.reset();
    }
    Ok(())
}
fn compare(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert!(
            (a.as_f64().unwrap_or(f64::NAN) - b.as_f64().unwrap_or(f64::NAN)).abs() <= 2e-5,
            "{path}: {a} != {b}"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (k, a) in a {
                compare(a, &b[k], &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
fn frame(raw: &Value) -> Result<Arc<BgrFrame>, Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut source = if let Some(placements) = raw["terminal_scene"].as_array() {
        let owner = root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
        let data: Value =
            serde_json::from_slice(&std::fs::read(owner.join("terminal-patterns.json"))?)?;
        let mut image = BgrFrame::zeros(Size::new(1922, 1140)?)?;
        for (part, point) in data["parts"]
            .as_array()
            .ok_or("parts")?
            .iter()
            .zip(placements)
        {
            let gray = codec::load_gray(owner.join(part["path"].as_str().ok_or("path")?))?;
            let [x, y]: [usize; 2] = serde_json::from_value(point.clone())?;
            for row in 0..gray.size().height() {
                for col in 0..gray.size().width() {
                    let v = gray.pixels()[row * gray.size().width() + col];
                    let at = ((y + row) * 1922 + x + col) * 3;
                    image.pixels_mut()[at..at + 3].fill(v);
                }
            }
        }
        image
    } else {
        codec::load_color(root.join(raw["path"].as_str().ok_or("path")?))?
    };
    if let Some(boxes) = raw["clear"].as_array() {
        for bounds in boxes {
            let [l, t, r, b]: [usize; 4] = serde_json::from_value(bounds.clone())?;
            for y in t..b {
                source.pixels_mut()[(y * 1922 + l) * 3..(y * 1922 + r) * 3].fill(0);
            }
        }
    }
    let mut frame = source.clone();
    if let Some(shift) = raw["shift"].as_array() {
        let dx = shift[0].as_i64().ok_or("dx")?;
        let dy = shift[1].as_i64().ok_or("dy")?;
        frame = BgrFrame::zeros(Size::new(1922, 1140)?)?;
        for y in 0..1140 {
            for x in 0..1922 {
                let sx = x as i64 - dx;
                let sy = y as i64 - dy;
                if (0..1922).contains(&sx) && (0..1140).contains(&sy) {
                    frame.pixels_mut()[(y * 1922 + x) * 3..(y * 1922 + x + 1) * 3].copy_from_slice(
                        &source.pixels()[(sy as usize * 1922 + sx as usize) * 3
                            ..(sy as usize * 1922 + sx as usize + 1) * 3],
                    );
                }
            }
        }
    }
    Ok(Arc::new(frame))
}
#[test]
fn all_declared_terminal_sources_and_reviewed_negatives_preserve_current_layout()
-> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let directory = root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
    let path = directory.join("terminal-patterns.json");
    let data: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
    let mut paths = BTreeSet::new();
    paths.insert(
        data["source"]["reference"]
            .as_str()
            .ok_or("source")?
            .to_owned(),
    );
    for layout in data["alternate_layouts"].as_array().ok_or("layouts")? {
        paths.insert(
            layout["source"]["reference"]
                .as_str()
                .ok_or("reference")?
                .to_owned(),
        );
    }
    for name in [
        "north-two-chest-junction.png",
        "north-dark-metal-faces-lower-unverified.png",
        "north-square-right-lighting-covered.png",
        "north-two-chests-lower-floor-spoke-obscured.png",
        "north-item-label-occlusion.png",
        "north-mixed-small-and-arched-chests.png",
        "north-three-arched-chest-faces.png",
    ] {
        paths.insert(format!(
            "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/{name}"
        ));
    }
    let mut cases: Vec<_> = paths.iter().map(|p| json!({"frame":{"path":p}})).collect();
    cases.push(json!({"frame":{"path":data["source"]["reference"],"clear":[[0,0,1922,1140]]}}));
    cases.extend([
        json!({"frame":{"terminal_scene":[[700,400],[1268,549],[1282,397]]}}),
        json!({"frame":{"terminal_scene":[[500,500],[1068,649],[1082,497]]}}),
        json!({"frame":{"terminal_scene":[[700,400],[1268,549],[1282,397]],"clear":[[1280,390,1380,470]]}}),
        json!({"frame":{"terminal_scene":[[500,400],[800,400],[1100,400]]}}),
    ]);
    let expected =
        support::legacy_expectation("observer-oracle", &json!({"kind":"terminal","cases":cases}))?;
    let mut observer = ArcaneTerminalObserver::load(path)?;
    for (index, case) in cases.iter().enumerate() {
        let frame = frame(&case["frame"])?;
        let actual = serde_json::to_value(observer.observe(frame.view())?)?;
        compare(&actual, &expected[index], &format!("case{index}"));
    }
    assert!(
        observer
            .observe(BgrFrame::zeros(Size::new(1267, 753)?)?.view())
            .is_err()
    );
    Ok(())
}
#[test]
fn partial_terminal_tracking_preserves_original_age_motion_and_relocalization()
-> Result<(), Box<dyn Error>> {
    let path = "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-floor-view.png";
    let hidden = json!({"path":path,"clear":[[976,287,1194,429]]});
    let mut cases = vec![
        json!({"frame":hidden,"at":0.0,"motion":[0,0]}),
        json!({"frame":{"path":path},"at":0.0}),
        json!({"frame":hidden,"at":0.8,"motion":[0,0]}),
        json!({"frame":hidden,"at":2.01,"motion":[0,0]}),
    ];
    for motion in [Value::Null, json!([2, 0]), json!([300, -150])] {
        cases.extend([
            json!({"frame":{"path":path},"at":3.0}),
            json!({"frame":hidden,"at":3.1,"motion":motion}),
            json!({"frame":hidden,"at":3.2,"motion":[0,0]}),
        ]);
    }
    cases.extend([json!({"frame":{"path":path},"at":4.0}),json!({"frame":{"path":path,"clear":[[976,287,1194,429],[1287,253,1376,329]]},"at":4.1,"motion":[0,0]})]);
    cases.extend([
        json!({"frame":{"path":path},"at":5.0}),
        json!({"relocalize":true}),
        json!({"frame":hidden,"at":5.1,"motion":[0,0]}),
        json!({"frame":hidden,"at":5.2,"motion":[0,0]}),
        json!({"reset":true}),
    ]);
    run_track_cases(cases)
}
fn run_track_cases(cases: Vec<Value>) -> Result<(), Box<dyn Error>> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
    let expected = support::legacy_expectation(
        "observer-oracle",
        &json!({"kind":"terminal","track":true,"cases":cases}),
    )?;
    let mut observer = ArcaneTerminalObserver::load(directory.join("terminal-patterns.json"))?;
    let mut tracker = TerminalStabilityTracker::new(2.0)?;
    for (index, case) in cases.iter().enumerate() {
        if case["reset"] == true {
            tracker.reset();
            assert!(expected[index].is_null());
            continue;
        }
        if case["relocalize"] == true {
            tracker.require_relocalization();
            assert!(expected[index].is_null());
            continue;
        }
        let frame = frame(&case["frame"])?;
        let mut reading = if case["observe"] == false {
            None
        } else {
            observer.observe(frame.view())?
        };
        if let Some(raw) = case["force_part"].as_object() {
            let current = reading.as_mut().ok_or("forced current reading")?;
            let index = current
                .part_ids
                .iter()
                .position(|id| Some(id.as_str()) == raw["old"].as_str())
                .ok_or("old part")?;
            current.part_ids[index] = raw["new"].as_str().ok_or("new part")?.into();
            current.evidence_boxes[index] = serde_json::from_value(raw["box"].clone())?;
            reading = observer.confirm_static_parts(frame.view(), current)?;
            reading
                .as_mut()
                .ok_or("forced static confirmation")?
                .provenance = "current_three_parts".into();
        }
        let tracked = tracker.update(
            &observer,
            TerminalObservation {
                frame: &frame,
                reading: reading.clone(),
                acquired_at: case["at"].as_f64().ok_or("at")?,
                displacement: serde_json::from_value(case["motion"].clone())?,
            },
        )?;
        let actual = json!({"reading":reading,"tracked":tracked,"status":tracker});
        compare(&actual, &expected[index], &format!("track{index}"));
    }
    Ok(())
}

#[test]
fn moving_terminal_parts_need_agreeing_current_support_and_independent_settling()
-> Result<(), Box<dyn Error>> {
    let base = json!({"terminal_scene":[[700,400],[1268,549],[1282,397]]});
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let directory = root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
    let data: Value =
        serde_json::from_slice(&std::fs::read(directory.join("terminal-patterns.json"))?)?;
    let last = codec::load_gray(directory.join(data["parts"][2]["path"].as_str().ok_or("path")?))?;
    let hidden = json!({"terminal_scene":[[700,400],[1268,549],[1282,397]],"shift":[300,-150],"clear":[[1282,397,1282+last.size().width(),397+last.size().height()]]});
    let one = json!({"terminal_scene":[[700,400],[1268,549],[1282,397]],"shift":[700,-150]});
    run_track_cases(vec![
        json!({"frame":base,"at":0.0}),
        json!({"frame":one,"at":0.2,"observe":false,"motion":[-700,150]}),
        json!({"frame":one,"at":0.9,"observe":false,"motion":[0,0]}),
        json!({"frame":one,"at":2.01,"observe":false,"motion":[0,0]}),
        json!({"frame":base,"at":3.0}),
        json!({"frame":hidden,"at":3.2,"observe":false,"motion":[-300,150]}),
        json!({"frame":hidden,"at":3.3,"observe":false,"motion":[2,0]}),
        json!({"frame":hidden,"at":3.4,"observe":false,"motion":[0,0]}),
        json!({"frame":hidden,"at":5.01,"observe":false,"motion":[0,0]}),
        json!({"frame":base,"at":6.0}),
        json!({"frame":hidden,"at":6.3,"observe":false}),
        json!({"frame":hidden,"at":6.9,"observe":false,"motion":[0,0]}),
        json!({"frame":hidden,"at":8.01,"observe":false,"motion":[0,0]}),
    ])
}
#[test]
fn changed_terminal_part_identity_rechecks_the_actual_previous_frame() -> Result<(), Box<dyn Error>>
{
    let path = "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-chest-body-face-static.png";
    let replacement = json!({"old":"templates/terminal/stone-chest-dark.png","new":"templates/terminal/dark-metal-chest-right-face.png","box":[1659,492,1696,521]});
    run_track_cases(vec![
        json!({"frame":{"path":path},"at":10.0}),
        json!({"frame":{"path":path},"at":10.2,"force_part":replacement}),
        json!({"frame":{"path":path},"at":10.4,"force_part":replacement}),
        json!({"reset":true}),
        json!({"frame":{"path":path,"clear":[[1657,490,1698,523]]},"at":20.0}),
        json!({"frame":{"path":path},"at":20.2,"force_part":replacement}),
        json!({"reset":true}),
        json!({"frame":{"path":path},"at":30.0}),
        json!({"frame":{"path":path},"at":30.25,"force_part":replacement}),
        json!({"reset":true}),
        json!({"frame":{"path":path},"at":40.0}),
        json!({"relocalize":true}),
        json!({"frame":{"path":path},"at":40.2,"force_part":replacement}),
    ])
}

#[test]
fn gold_floor_uses_current_color_for_static_motion_and_relocalized_support()
-> Result<(), Box<dyn Error>> {
    let prefix = "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/";
    let reference =
        json!({"path":format!("{prefix}north-square-arched-pair-gold-filigree-covered.png")});
    let followup =
        json!({"path":format!("{prefix}north-square-arched-pair-gold-filigree-followup.png")});
    let mut one_hidden = reference.clone();
    one_hidden["clear"] = json!([[1060, 720, 1150, 800]]);
    let mut two_hidden = reference.clone();
    two_hidden["clear"] = json!([[1060, 720, 1150, 800], [1050, 560, 1170, 665]]);
    let mut cases = Vec::new();
    for raw in [followup, one_hidden, two_hidden] {
        for method in ["static", "approach", "relocalized"] {
            cases.push(json!({"frame":raw,"method":method,"motion":[0,0]}));
        }
    }
    let expected = support::legacy_expectation(
        "observer-oracle",
        &json!({"kind":"terminal_confirm","reference":reference,"cases":cases}),
    )?;
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
    let mut observer = ArcaneTerminalObserver::load(directory.join("terminal-patterns.json"))?;
    let frame = frame(&reference)?;
    let reference = observer
        .observe(frame.view())?
        .ok_or("full gold reference")?;
    for (i, case) in cases.iter().enumerate() {
        let current = self::frame(&case["frame"])?;
        let reading = match case["method"].as_str().ok_or("method")? {
            "static" => observer.confirm_static_parts(current.view(), &reference)?,
            "approach" => {
                observer.confirm_approach_parts(current.view(), &reference, [0.0, 0.0])?
            }
            _ => observer.confirm_relocalized_parts(current.view(), &reference)?,
        };
        compare(
            &serde_json::to_value(reading)?,
            &expected[i],
            &format!("gold{i}"),
        );
    }
    Ok(())
}
#[test]
fn terminal_rejects_the_shared_nonterminal_native_corpus() -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let directory = root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
    let mut cases = Vec::new();
    for group in ["hub", "entry", "navigation", "summoner"] {
        let mut paths = std::fs::read_dir(directory.join("references").join(group))?
            .map(|e| e.map(|e| e.path()))
            .collect::<Result<Vec<_>, _>>()?;
        paths.sort();
        for path in paths {
            if path.extension().is_none_or(|e| e != "png") {
                continue;
            }
            if codec::load_color(&path)?.size() != Size::new(1922, 1140)? {
                continue;
            }
            cases.push(
                json!({"frame":{"path":path.strip_prefix(root)?.to_str().ok_or("utf8 path")?}}),
            );
        }
    }
    assert!(cases.len() >= 36);
    let expected =
        support::legacy_expectation("observer-oracle", &json!({"kind":"terminal","cases":cases}))?;
    let mut observer = ArcaneTerminalObserver::load(directory.join("terminal-patterns.json"))?;
    for (index, case) in cases.iter().enumerate() {
        assert!(expected[index].is_null(), "baseline negative{index}");
        assert!(
            observer.observe(frame(&case["frame"])?.view())?.is_none(),
            "negative{index}"
        );
    }
    Ok(())
}
#[test]
fn terminal_tracker_retains_only_the_required_previous_full_frame() -> Result<(), Box<dyn Error>> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
    let mut observer = ArcaneTerminalObserver::load(directory.join("terminal-patterns.json"))?;
    let mut tracker = TerminalStabilityTracker::new(2.0)?;
    let first = frame(&json!({"terminal_scene":[[700,400],[1268,549],[1282,397]]}))?;
    let weak = Arc::downgrade(&first);
    let reading = observer.observe(first.view())?;
    tracker.update(
        &observer,
        TerminalObservation {
            frame: &first,
            reading,
            acquired_at: 0.0,
            displacement: None,
        },
    )?;
    drop(first);
    assert_eq!(weak.strong_count(), 1);
    let blank = Arc::new(BgrFrame::zeros(Size::new(1922, 1140)?)?);
    tracker.update(
        &observer,
        TerminalObservation {
            frame: &blank,
            reading: None,
            acquired_at: 0.1,
            displacement: Some([0.0, 0.0]),
        },
    )?;
    assert!(weak.upgrade().is_none());
    assert!(
        tracker
            .update(
                &observer,
                TerminalObservation {
                    frame: &blank,
                    reading: None,
                    acquired_at: f64::NAN,
                    displacement: None
                }
            )
            .is_err()
    );
    Ok(())
}

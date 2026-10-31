mod support;

use d2bot::common::error::ControlError;
use d2bot::common::frame::{BgrFrame, Size};
use d2bot::common::frame_geometry::PhysicalTarget;
use d2bot::common::realtime::{
    DecisionSnapshot, FrameSource, RealtimeRuntimeState, RuntimeFrame, RuntimeSnapshot,
    VisionChannel, VisionSnapshot,
};
use d2bot::common::time::MonotonicClock;
use serde_json::{Value, json};
use std::error::Error;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

type TestResult = Result<(), Box<dyn Error>>;
#[derive(Default)]
struct Clock(AtomicU64);
impl Clock {
    fn set(&self, now: f64) {
        self.0.store(now.to_bits(), Ordering::Release);
    }
}
impl MonotonicClock for Clock {
    fn now(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Acquire))
    }
}
fn source(sequence: u64, at: f64, generation: u64) -> Result<FrameSource, ControlError> {
    Ok(FrameSource::new(sequence, at, generation)?)
}
fn outcome(result: Result<Value, ControlError>) -> Value {
    match result {
        Ok(value) => json!({"value":value}),
        Err(error) => {
            let kind = match &error {
                ControlError::Configuration(error) => error.kind(),
                _ => "RuntimeError",
            };
            let message = match &error {
                ControlError::Configuration(error) => error.message(),
                _ => return json!({"error_type":kind,"error_message":error.to_string()}),
            };
            json!({"error_type":kind,"error_message":message})
        }
    }
}
fn vision(snapshot: Option<&Arc<VisionSnapshot<Value>>>, channel: &str) -> Value {
    snapshot.map_or(Value::Null,|snapshot| { let source = snapshot.source(); json!({"channel":channel,"payload":snapshot.payload(),"source_sequence_id":source.sequence(),"source_captured_at":source.captured_at(),"produced_at":snapshot.produced_at(),"source_generation":source.generation()}) })
}
fn summary(snapshot: &RuntimeSnapshot<Value, Value>) -> Value {
    json!({"sampled_at":snapshot.sampled_at,"latest":snapshot.latest_frame.as_ref().map(|frame| { let source = frame.source(); json!({"sequence_id":source.sequence(),"captured_at":source.captured_at(),"generation":source.generation(),"target":frame.target().as_value(),"wall_captured_at":frame.wall_captured_at()}) }),"recent":snapshot.recent_frames.iter().map(|frame| frame.source().sequence()).collect::<Vec<_>>(),"fast":vision(snapshot.fast_vision.as_ref(),"fast"),"slow":vision(snapshot.slow_vision.as_ref(),"slow"),"decision":snapshot.latest_decision.as_ref().map(|decision| { let source = decision.source(); json!({"payload":decision.payload(),"produced_at":decision.produced_at(),"source_sequence_id":source.map(FrameSource::sequence),"source_captured_at":source.map(FrameSource::captured_at),"source_generation":source.map(FrameSource::generation).unwrap_or(0)}) }),"generation":snapshot.generation})
}
fn run(request: &Value) -> Result<Value, Box<dyn Error>> {
    let clock = Arc::new(Clock::default());
    let state = RealtimeRuntimeState::<Value, Value>::new(clock.clone());
    let image = Arc::new(BgrFrame::zeros(Size::new(1, 1)?)?);
    let mut saved = Vec::new();
    let mut results = Vec::new();
    for operation in request.as_array().ok_or("operations missing")? {
        clock.set(operation["now"].as_f64().ok_or("now missing")?);
        let result = (|| -> Result<Value, ControlError> {
            let generation = operation["generation"]
                .as_u64()
                .unwrap_or(state.generation()?);
            let seq = operation["sequence"].as_u64().unwrap_or(0);
            let captured = operation["captured_at"].as_f64().unwrap_or(0.0);
            let result = match operation["op"].as_str() {
                Some("frame") => {
                    let raw = &operation["target"];
                    let target = PhysicalTarget::new(
                        raw["left"]
                            .as_i64()
                            .and_then(|left| i32::try_from(left).ok())
                            .unwrap_or(0),
                        raw["top"]
                            .as_i64()
                            .and_then(|top| i32::try_from(top).ok())
                            .unwrap_or(0),
                        1,
                        1,
                    )
                    .map_err(|error| ControlError::Runtime(error.to_string()))?;
                    json!(state.publish_frame(RuntimeFrame::new(
                        source(seq, captured, generation)?,
                        image.clone(),
                        target,
                        operation["wall"].as_f64(),
                        None
                    ))?)
                }
                Some("vision") => {
                    let channel = if operation["channel"] == "slow" {
                        VisionChannel::Slow
                    } else {
                        VisionChannel::Fast
                    };
                    let raw_channel = operation
                        .get("snapshot_channel")
                        .unwrap_or(&operation["channel"]);
                    let snapshot_channel = if raw_channel == "slow" {
                        VisionChannel::Slow
                    } else {
                        VisionChannel::Fast
                    };
                    let payload = operation
                        .get("payload")
                        .filter(|payload| !payload.is_null())
                        .cloned()
                        .map(Arc::new);
                    json!(state.publish_vision(
                        channel,
                        VisionSnapshot::new(
                            snapshot_channel,
                            payload,
                            source(seq, captured, generation)?,
                            operation["produced_at"].as_f64().unwrap_or(0.0)
                        )?
                    )?)
                }
                Some("decision") => {
                    let source =
                        if operation["sequence"].is_null() || operation["captured_at"].is_null() {
                            None
                        } else {
                            Some(source(seq, captured, generation)?)
                        };
                    json!(state.publish_decision(DecisionSnapshot::new(
                        Arc::new(operation["payload"].clone()),
                        source,
                        operation["produced_at"].as_f64().unwrap_or(0.0)
                    )?)?)
                }
                Some("invalidate") => json!(state.invalidate()?),
                Some("save") => {
                    saved.push(state.snapshot()?);
                    Value::Null
                }
                Some("current") => json!(state.is_current_generation(
                    &saved[operation["index"].as_u64().unwrap_or(0) as usize]
                )?),
                _ => Value::Null,
            };
            Ok(result)
        })();
        let mut result = outcome(result);
        result["snapshot"] = outcome(state.snapshot().map(|snapshot| summary(&snapshot)));
        results.push(result);
    }
    Ok(json!(results))
}
fn assert_values(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Number(actual), Value::Number(expected)) => {
            assert_eq!(actual.as_f64(), expected.as_f64(), "{path}")
        }
        (Value::Array(actual), Value::Array(expected)) => {
            assert_eq!(actual.len(), expected.len(), "{path}");
            for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                assert_values(actual, expected, &format!("{path}/{index}"));
            }
        }
        (Value::Object(actual), Value::Object(expected)) => {
            assert_eq!(
                actual.len(),
                expected.len(),
                "{path}: keys {:?} vs {:?}",
                actual.keys().collect::<Vec<_>>(),
                expected.keys().collect::<Vec<_>>()
            );
            for (key, expected) in expected {
                assert_values(&actual[key], expected, &format!("{path}/{key}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}

#[test]
fn provenance_generation_null_results_and_decision_precedence_match_python() -> TestResult {
    let mut buffer = Vec::new();
    for index in 0..20 {
        buffer.push(json!({"op":"frame","now":index+10,"sequence":index,"captured_at":index+10,"wall":1800000000-index}));
    }
    let cases = vec![
        json!(buffer),
        json!([
            {"op":"frame","now":10,"sequence":0,"captured_at":10}, {"op":"save","now":10},
            {"op":"vision","now":10,"sequence":0,"captured_at":10,"produced_at":10,"payload":"enemy"},
            {"op":"decision","now":10,"sequence":0,"captured_at":10,"produced_at":10,"payload":"travel"},
            {"op":"invalidate","now":10},{"op":"current","now":10,"index":0},
            {"op":"frame","now":11,"sequence":1,"captured_at":11,"generation":0},
            {"op":"frame","now":11,"sequence":2,"captured_at":11},
            {"op":"vision","now":11,"sequence":0,"captured_at":10,"produced_at":11,"generation":0,"payload":"old"},
            {"op":"decision","now":11,"sequence":0,"captured_at":10,"produced_at":11,"generation":0,"payload":"old"},
            {"op":"frame","now":12,"sequence":3,"captured_at":12,"target":{"left":2,"top":0,"width":1,"height":1}},
            {"op":"vision","now":12,"channel":"slow","sequence":2,"captured_at":11,"produced_at":12,"generation":1,"payload":"old"}
        ]),
        json!([
            {"op":"frame","now":10,"sequence":0,"captured_at":10},{"op":"frame","now":11,"sequence":1,"captured_at":11},
            {"op":"vision","now":11,"sequence":1,"captured_at":11,"produced_at":11,"payload":"current"},
            {"op":"vision","now":11,"sequence":0,"captured_at":10,"produced_at":11,"payload":"old"},
            {"op":"vision","now":11,"sequence":1,"captured_at":11,"produced_at":11,"payload":null},
            {"op":"vision","now":11,"sequence":2,"captured_at":11,"produced_at":11,"payload":"future"},
            {"op":"vision","now":11,"sequence":1,"captured_at":12,"produced_at":12,"payload":"future"},
            {"op":"vision","now":11,"sequence":1,"captured_at":11,"produced_at":12,"payload":"future"},
            {"op":"frame","now":12,"sequence":2,"captured_at":12},
            {"op":"vision","now":12,"sequence":2,"captured_at":12,"produced_at":12,"payload":null},
            {"op":"vision","now":12,"sequence":2,"captured_at":12,"produced_at":12,"channel":"fast","snapshot_channel":"slow","payload":"bad"},
            {"op":"decision","now":12,"sequence":2,"captured_at":12,"produced_at":12,"payload":"latest"},
            {"op":"decision","now":12,"sequence":1,"captured_at":11,"produced_at":12,"payload":"old"},
            {"op":"decision","now":12,"sequence":2,"captured_at":10,"produced_at":11,"payload":"old"},
            {"op":"decision","now":12,"sequence":2,"captured_at":10,"produced_at":12,"payload":"same_sequence_allowed"},
            {"op":"decision","now":12,"produced_at":12,"payload":"no_source"},
            {"op":"frame","now":12,"sequence":3,"captured_at":13},{"op":"frame","now":12,"sequence":2,"captured_at":12},
            {"op":"snapshot","now":11},{"op":"snapshot","now":13}
        ]),
    ];
    let expected = support::legacy_expectation("realtime-oracle", &json!(cases))?;
    for (index, request) in cases.iter().enumerate() {
        assert_values(&run(request)?, &expected[index], &format!("case/{index}"));
    }
    Ok(())
}

#[test]
fn old_frames_and_payloads_are_released_while_current_buffer_stays_bounded() -> TestResult {
    let clock = Arc::new(Clock::default());
    let state = RealtimeRuntimeState::<String, String>::new(clock.clone());
    let target = PhysicalTarget::new(0, 0, 1, 1)?;
    let mut weak_frames = Vec::new();
    let mut weak_payloads = Vec::new();
    for index in 0_u32..1000 {
        let time = f64::from(index);
        clock.set(time);
        let frame = Arc::new(BgrFrame::zeros(Size::new(1, 1)?)?);
        weak_frames.push(Arc::downgrade(&frame));
        let source = source(u64::from(index), time, 0)?;
        state.publish_frame(RuntimeFrame::new(source, frame, target, None, None))?;
        let payload = Arc::new(format!("frame {index}"));
        weak_payloads.push(Arc::downgrade(&payload));
        state.publish_vision(
            VisionChannel::Fast,
            VisionSnapshot::new(VisionChannel::Fast, Some(payload), source, time)?,
        )?;
    }
    assert_eq!(
        weak_frames
            .iter()
            .filter(|frame| frame.strong_count() > 0)
            .count(),
        5
    );
    assert_eq!(
        weak_payloads
            .iter()
            .filter(|payload| payload.strong_count() > 0)
            .count(),
        1
    );
    let snapshot = state.snapshot()?;
    assert_eq!(snapshot.recent_frames.len(), 5);
    state.invalidate()?;
    assert!(!state.is_current_generation(&snapshot)?);
    // An explicitly retained snapshot owns its five frames until the caller drops it.
    drop(snapshot);
    assert!(weak_frames.iter().all(|frame| frame.strong_count() == 0));
    assert!(
        weak_payloads
            .iter()
            .all(|payload| payload.strong_count() == 0)
    );
    Ok(())
}

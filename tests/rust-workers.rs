use d2bot::common::error::ControlError;
use d2bot::common::frame::{BgrFrame, Size};
use d2bot::common::frame_geometry::PhysicalTarget;
use d2bot::common::realtime::{
    CapturedFrame, FrameSource, RealtimeVisionRuntime, RuntimeFrame, RuntimeIntervals,
    ScreenSource, VisionChannel, VisionSnapshot,
};
use d2bot::common::time::MonotonicClock;
use d2bot::common::workers::{Cancellation, WorkerGroup};
use std::error::Error;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, AtomicUsize, Ordering},
    mpsc,
};
use std::time::Duration;

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
fn packet() -> Result<CapturedFrame, ControlError> {
    Ok(CapturedFrame {
        frame: Arc::new(
            BgrFrame::zeros(
                Size::new(1, 1).map_err(|error| ControlError::Runtime(error.to_string()))?,
            )
            .map_err(|error| ControlError::Runtime(error.to_string()))?,
        ),
        target: PhysicalTarget::new(0, 0, 1, 1)
            .map_err(|error| ControlError::Runtime(error.to_string()))?,
        wall_captured_at: Some(1800000000.0),
        geometry: None,
    })
}
fn frame(sequence: u64, time: f64, generation: u64) -> Result<RuntimeFrame, Box<dyn Error>> {
    let packet = packet()?;
    Ok(RuntimeFrame::new(
        FrameSource::new(sequence, time, generation)?,
        packet.frame,
        packet.target,
        packet.wall_captured_at,
        None,
    ))
}

struct Capture {
    calls: usize,
    closed: Arc<AtomicUsize>,
    grab: Box<dyn FnMut(usize) -> Result<CapturedFrame, ControlError> + Send>,
    close_fails: bool,
}
impl ScreenSource for Capture {
    fn grab(&mut self) -> Result<CapturedFrame, ControlError> {
        let call = self.calls;
        self.calls += 1;
        (self.grab)(call)
    }
    fn close(&mut self) -> Result<(), ControlError> {
        self.closed.fetch_add(1, Ordering::AcqRel);
        if self.close_fails {
            Err(ControlError::Backend("capture close failed".into()))
        } else {
            Ok(())
        }
    }
}
fn dormant(token: Cancellation, closed: Arc<AtomicUsize>) -> Capture {
    Capture {
        calls: 0,
        closed,
        close_fails: false,
        grab: Box::new(move |_| {
            token.wait(Duration::from_secs(10))?;
            packet()
        }),
    }
}

#[test]
fn capture_timestamps_start_and_closes_after_inflight_invalidation() -> TestResult {
    let clock = Arc::new(Clock::default());
    clock.set(10.0);
    let mut runtime =
        RealtimeVisionRuntime::<String, String>::new(clock.clone(), RuntimeIntervals::default());
    let state = runtime.state().clone();
    let token = runtime.cancellation();
    let closed = Arc::new(AtomicUsize::new(0));
    let source = Capture {
        calls: 0,
        closed: closed.clone(),
        close_fails: false,
        grab: Box::new(move |call| {
            clock.set(11.0 + call as f64);
            if call == 0 {
                state.invalidate()?;
            } else {
                token.cancel();
            }
            packet()
        }),
    };
    runtime.start(move || Ok(source), |_| Ok(None), |_| Ok(None), |_| Ok(None))?;
    // Wait for the source's second grab without cancelling it prematurely.
    let timeout = std::time::Instant::now() + Duration::from_secs(2);
    while !runtime.cancellation().is_cancelled() && std::time::Instant::now() < timeout {
        std::thread::yield_now();
    }
    runtime.stop(Duration::from_secs(2))?;
    let latest = runtime.state().latest_frame()?.ok_or("latest missing")?;
    assert_eq!(latest.source().sequence(), 1);
    assert_eq!(latest.source().generation(), 1);
    assert_eq!(latest.source().captured_at(), 11.0);
    assert_eq!(latest.wall_captured_at(), Some(1800000000.0));
    assert_eq!(closed.load(Ordering::Acquire), 1);
    assert!(runtime.faults()?.is_empty());
    Ok(())
}

#[test]
fn capture_and_close_failures_are_reported_and_joined() -> TestResult {
    let clock = Arc::new(Clock::default());
    clock.set(10.0);
    let mut runtime =
        RealtimeVisionRuntime::<String, String>::new(clock, RuntimeIntervals::default());
    let closed = Arc::new(AtomicUsize::new(0));
    let (entered_tx, entered_rx) = mpsc::channel();
    let source = Capture {
        calls: 0,
        closed: closed.clone(),
        close_fails: true,
        grab: Box::new(move |_| {
            entered_tx
                .send(())
                .map_err(|error| ControlError::Runtime(error.to_string()))?;
            Err(ControlError::Backend("capture failed".into()))
        }),
    };
    runtime.start(move || Ok(source), |_| Ok(None), |_| Ok(None), |_| Ok(None))?;
    entered_rx.recv_timeout(Duration::from_secs(2))?;
    runtime.stop(Duration::from_secs(2))?;
    let faults = runtime.faults()?;
    assert_eq!(faults.len(), 1);
    assert_eq!(faults[0].stage, "d2-capture");
    assert!(faults[0].message.contains("capture failed"));
    assert!(faults[0].message.contains("capture close failed"));
    assert_eq!(closed.load(Ordering::Acquire), 1);
    Ok(())
}

#[test]
fn slow_worker_cannot_publish_previous_generation_after_new_capture() -> TestResult {
    let clock = Arc::new(Clock::default());
    clock.set(10.0);
    let mut runtime =
        RealtimeVisionRuntime::<String, String>::new(clock.clone(), RuntimeIntervals::default());
    runtime.state().publish_frame(frame(10, 10.0, 0)?)?;
    let source = dormant(runtime.cancellation(), Arc::new(AtomicUsize::new(0)));
    let token = runtime.cancellation();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    runtime.start(
        move || Ok(source),
        |_| Ok(None),
        move |_| {
            entered_tx
                .send(())
                .map_err(|error| ControlError::Runtime(error.to_string()))?;
            resume_rx
                .recv_timeout(Duration::from_secs(3))
                .map_err(|error| ControlError::Runtime(error.to_string()))?;
            token.cancel();
            Ok(Some("old enemy".into()))
        },
        |_| Ok(None),
    )?;
    let entered = entered_rx.recv_timeout(Duration::from_secs(2));
    if let Err(error) = entered {
        let _ = resume_tx.send(());
        let _ = runtime.stop(Duration::from_secs(2));
        return Err(error.into());
    }
    runtime.state().invalidate()?;
    clock.set(11.0);
    runtime.state().publish_frame(frame(11, 11.0, 1)?)?;
    resume_tx.send(())?;
    // The callback cancels only after computing its late result; let it finish before stop.
    let timeout = std::time::Instant::now() + Duration::from_secs(2);
    while !runtime.cancellation().is_cancelled() && std::time::Instant::now() < timeout {
        std::thread::yield_now();
    }
    runtime.stop(Duration::from_secs(2))?;
    assert!(runtime.state().snapshot()?.slow_vision.is_none());
    assert!(runtime.faults()?.is_empty());
    Ok(())
}

#[test]
fn late_decision_is_rejected_and_fast_worker_skips_backlog_and_clears_none() -> TestResult {
    for decision_test in [false, true] {
        let clock = Arc::new(Clock::default());
        clock.set(10.0);
        let mut runtime =
            RealtimeVisionRuntime::<String, String>::new(clock, RuntimeIntervals::default());
        for sequence in 0..=10 {
            runtime.state().publish_frame(frame(sequence, 10.0, 0)?)?;
        }
        runtime.state().publish_vision(
            VisionChannel::Fast,
            VisionSnapshot::new(
                VisionChannel::Fast,
                Some(Arc::new("old".into())),
                FrameSource::new(9, 10.0, 0)?,
                10.0,
            )?,
        )?;
        let source = dormant(runtime.cancellation(), Arc::new(AtomicUsize::new(0)));
        let fast_token = runtime.cancellation();
        let decision_token = runtime.cancellation();
        let state = runtime.state().clone();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let fast_seen = seen.clone();
        let (done_tx, done_rx) = mpsc::channel();
        let fast_done = done_tx.clone();
        runtime.start(
            move || Ok(source),
            move |frame| {
                fast_seen
                    .lock()
                    .map_err(|_| ControlError::Runtime("seen poisoned".into()))?
                    .push(frame.source().sequence());
                if !decision_test {
                    fast_token.cancel();
                    let _ = fast_done.send(());
                }
                Ok(None)
            },
            |_| Ok(None),
            move |_| {
                if decision_test {
                    state.invalidate()?;
                    decision_token.cancel();
                    let _ = done_tx.send(());
                    Ok(Some("old move".into()))
                } else {
                    Ok(None)
                }
            },
        )?;
        done_rx.recv_timeout(Duration::from_secs(2))?;
        runtime.stop(Duration::from_secs(2))?;
        let snapshot = runtime.state().snapshot()?;
        if decision_test {
            assert!(snapshot.latest_decision.is_none());
        } else {
            assert_eq!(*seen.lock().map_err(|_| "seen poisoned")?, vec![10]);
            assert!(snapshot.fast_vision.is_none());
        }
        assert!(runtime.faults()?.is_empty());
    }
    Ok(())
}

#[test]
fn finish_timeout_retains_handle_for_retry_and_cancellation_wakes_long_wait() -> TestResult {
    let mut group = WorkerGroup::new(Cancellation::default(), 1);
    let (entered_tx, entered_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    group.spawn("blocked", move |_| {
        entered_tx
            .send(())
            .map_err(|error| ControlError::Runtime(error.to_string()))?;
        resume_rx
            .recv()
            .map_err(|error| ControlError::Runtime(error.to_string()))?;
        Ok(())
    })?;
    entered_rx.recv_timeout(Duration::from_secs(2))?;
    assert!(group.finish(Duration::ZERO).is_err());
    resume_tx.send(())?;
    group.finish(Duration::from_secs(2))?;
    assert!(group.faults()?.is_empty());
    let mut group = WorkerGroup::new(Cancellation::default(), 1);
    let (entered_tx, entered_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    group.spawn("waiting", move |token| {
        entered_tx
            .send(())
            .map_err(|error| ControlError::Runtime(error.to_string()))?;
        assert!(token.wait(Duration::from_secs(30))?);
        done_tx
            .send(())
            .map_err(|error| ControlError::Runtime(error.to_string()))?;
        Ok(())
    })?;
    entered_rx.recv_timeout(Duration::from_secs(2))?;
    group.cancellation().cancel();
    done_rx.recv_timeout(Duration::from_secs(1))?;
    group.finish(Duration::from_secs(2))?;
    Ok(())
}

#[test]
fn panicking_capture_still_closes_and_reports_worker_fault() -> TestResult {
    let clock = Arc::new(Clock::default());
    clock.set(10.0);
    let mut runtime =
        RealtimeVisionRuntime::<String, String>::new(clock, RuntimeIntervals::default());
    let closed = Arc::new(AtomicUsize::new(0));
    let (entered_tx, entered_rx) = mpsc::channel();
    let source = Capture {
        calls: 0,
        closed: closed.clone(),
        close_fails: false,
        grab: Box::new(move |_| {
            let _ = entered_tx.send(());
            panic!("capture panic fixture")
        }),
    };
    runtime.start(move || Ok(source), |_| Ok(None), |_| Ok(None), |_| Ok(None))?;
    entered_rx.recv_timeout(Duration::from_secs(2))?;
    runtime.stop(Duration::from_secs(2))?;
    assert_eq!(closed.load(Ordering::Acquire), 1);
    assert!(runtime.faults()?.iter().any(
        |fault| fault.stage == "d2-capture" && fault.message.contains("capture panic fixture")
    ));
    Ok(())
}

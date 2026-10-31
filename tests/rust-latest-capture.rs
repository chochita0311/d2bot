use d2bot::common::{
    capture::LatestCapture,
    error::ControlError,
    frame::{BgrFrame, Size},
    frame_geometry::PhysicalTarget,
    realtime::{CapturedFrame, ScreenSource},
    time::MonotonicClock,
    workers::CooperativeStop,
};
use std::{
    error::Error,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};
#[derive(Default)]
struct Clock(AtomicU64);
impl MonotonicClock for Clock {
    fn now(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::SeqCst))
    }
}
struct Source {
    frames: mpsc::Receiver<u8>,
    entered: mpsc::Sender<()>,
    closed: Arc<AtomicBool>,
    weak: Arc<Mutex<Vec<Weak<BgrFrame>>>>,
    close_fails: bool,
}
impl ScreenSource for Source {
    fn grab(&mut self) -> Result<CapturedFrame, ControlError> {
        self.entered
            .send(())
            .map_err(|e| ControlError::Runtime(e.to_string()))?;
        let pixel = self
            .frames
            .recv_timeout(Duration::from_secs(2))
            .map_err(|e| ControlError::Runtime(e.to_string()))?;
        let frame = Arc::new(
            BgrFrame::from_vec(
                Size::new(4, 3).map_err(|e| ControlError::Runtime(e.to_string()))?,
                vec![pixel; 36],
            )
            .map_err(|e| ControlError::Runtime(e.to_string()))?,
        );
        self.weak
            .lock()
            .map_err(|_| ControlError::Runtime("test weak lock".into()))?
            .push(Arc::downgrade(&frame));
        Ok(CapturedFrame {
            frame,
            target: PhysicalTarget::new(0, 0, 4, 3)
                .map_err(|e| ControlError::Runtime(e.to_string()))?,
            geometry: None,
            wall_captured_at: None,
        })
    }
    fn close(&mut self) -> Result<(), ControlError> {
        self.closed.store(true, Ordering::SeqCst);
        if self.close_fails {
            Err(ControlError::Runtime("close fixture failure".into()))
        } else {
            Ok(())
        }
    }
    fn capture_method(&self) -> Option<&'static str> {
        Some("fixture")
    }
}
#[test]
fn requests_bound_capture_preserve_source_and_join_delayed_owner() -> Result<(), Box<dyn Error>> {
    let (frames_tx, frames) = mpsc::channel();
    let (entered, entered_rx) = mpsc::channel();
    let closed = Arc::new(AtomicBool::new(false));
    let weak = Arc::new(Mutex::new(Vec::new()));
    let clock = Arc::new(Clock::default());
    let stop = CooperativeStop::default();
    let capture = Arc::new(LatestCapture::new(
        Source {
            frames,
            entered,
            closed: closed.clone(),
            weak: weak.clone(),
            close_fails: false,
        },
        clock.clone(),
        stop.clone(),
    )?);
    entered_rx.recv_timeout(Duration::from_secs(2))?;
    frames_tx.send(1)?;
    let first = capture.grab(None)?;
    assert_eq!(first.sequence, 1);
    assert_eq!(first.started_at, 0.0);
    assert_eq!(first.method, Some("fixture"));
    assert!(entered_rx.recv_timeout(Duration::from_millis(30)).is_err());
    clock.0.store(1.0_f64.to_bits(), Ordering::SeqCst);
    let reader = {
        let capture = capture.clone();
        std::thread::spawn(move || capture.grab(Some(0.5)))
    };
    entered_rx.recv_timeout(Duration::from_secs(2))?;
    frames_tx.send(2)?;
    let current = reader.join().map_err(|_| "reader panicked")??;
    assert_eq!(current.sequence, 2);
    assert_eq!(current.started_at, 1.0);
    assert_eq!(current.completed_at, 1.0);
    assert_eq!(current.captured.frame.pixels()[0], 2);
    assert_eq!(first.captured.frame.pixels()[0], 1);
    let reader = {
        let capture = capture.clone();
        std::thread::spawn(move || capture.grab(None))
    };
    entered_rx.recv_timeout(Duration::from_secs(2))?;
    stop.set();
    assert!(
        reader
            .join()
            .map_err(|_| "stopped reader panicked")?
            .is_err()
    );
    let mut capture = Arc::try_unwrap(capture).map_err(|_| "capture still shared")?;
    assert!(capture.finish(Duration::ZERO).is_err());
    assert!(!capture.is_finished());
    assert!(!closed.load(Ordering::SeqCst));
    frames_tx.send(3)?;
    capture.finish(Duration::from_secs(2))?;
    assert!(closed.load(Ordering::SeqCst));
    assert!(capture.is_finished());
    assert_eq!(current.captured.frame.pixels()[0], 2);
    drop(first);
    drop(current);
    assert!(
        weak.lock()
            .map_err(|_| "weak poisoned")?
            .iter()
            .all(|f| f.upgrade().is_none())
    );
    Ok(())
}
#[test]
fn close_failure_and_stop_are_reported_without_waiting_for_compute() -> Result<(), Box<dyn Error>> {
    let (frames_tx, frames) = mpsc::channel();
    let (entered, entered_rx) = mpsc::channel();
    let closed = Arc::new(AtomicBool::new(false));
    let stop = CooperativeStop::default();
    let mut capture = LatestCapture::new(
        Source {
            frames,
            entered,
            closed: closed.clone(),
            weak: Arc::new(Mutex::new(vec![])),
            close_fails: true,
        },
        Arc::new(Clock::default()),
        stop.clone(),
    )?;
    entered_rx.recv_timeout(Duration::from_secs(2))?;
    stop.set();
    assert!(capture.grab(None).is_err());
    assert!(capture.grab(Some(f64::NAN)).is_err());
    frames_tx.send(1)?;
    assert!(
        capture
            .finish(Duration::from_secs(2))
            .is_err_and(|e| e.to_string().contains("close fixture failure"))
    );
    assert!(closed.load(Ordering::SeqCst));
    assert!(capture.is_finished());
    Ok(())
}

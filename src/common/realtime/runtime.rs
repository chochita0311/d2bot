use super::{
    DecisionSnapshot, FrameSource, RealtimeRuntimeState, RuntimeFrame, RuntimeSnapshot,
    VisionChannel, VisionSnapshot,
};
use crate::common::{
    config::ConfigError,
    error::ControlError,
    frame::BgrFrame,
    frame_geometry::{FrameGeometry, PhysicalTarget},
    time::{self, MonotonicClock},
    workers::{Cancellation, WorkerFault, WorkerGroup},
};
use std::sync::Arc;
use std::time::Duration;

pub struct CapturedFrame {
    pub frame: Arc<BgrFrame>,
    pub target: PhysicalTarget,
    pub wall_captured_at: Option<f64>,
    pub geometry: Option<Arc<FrameGeometry>>,
}
pub trait ScreenSource: Send + 'static {
    fn grab(&mut self) -> Result<CapturedFrame, ControlError>;
    fn close(&mut self) -> Result<(), ControlError>;
    fn capture_method(&self) -> Option<&'static str> {
        None
    }
}

struct OwnedSource<C: ScreenSource>(Option<C>);
impl<C: ScreenSource> OwnedSource<C> {
    fn close(&mut self) -> Result<(), ControlError> {
        match self.0.take() {
            Some(mut source) => source.close(),
            None => Ok(()),
        }
    }
}
impl<C: ScreenSource> Drop for OwnedSource<C> {
    fn drop(&mut self) {
        crate::common::error::best_effort_cleanup(|| self.close());
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RuntimeIntervals {
    capture_seconds: f64,
    fast: Duration,
    slow: Duration,
    decision: Duration,
    idle: Duration,
}
impl RuntimeIntervals {
    pub fn new(
        capture_fps: f64,
        fast: f64,
        slow: f64,
        decision: f64,
        idle: f64,
    ) -> Result<Self, ConfigError> {
        if !capture_fps.is_finite() {
            return Err(ConfigError::value("capture FPS must be finite"));
        }
        let duration = |value, name| {
            time::nonnegative(value, name)?;
            Duration::try_from_secs_f64(value)
                .map_err(|_| ConfigError::value(format!("{name} exceeds runtime duration range")))
        };
        Ok(Self {
            capture_seconds: 1.0 / capture_fps.max(1.0),
            fast: duration(fast, "fast_interval")?,
            slow: duration(slow, "slow_interval")?,
            decision: duration(decision, "decision_interval")?,
            idle: duration(idle, "idle_sleep")?,
        })
    }
}
impl Default for RuntimeIntervals {
    fn default() -> Self {
        Self {
            capture_seconds: 0.125,
            fast: Duration::from_millis(10),
            slow: Duration::from_millis(40),
            decision: Duration::from_millis(10),
            idle: Duration::from_millis(5),
        }
    }
}

pub struct RealtimeVisionRuntime<V: Send + Sync + 'static, D: Send + Sync + 'static> {
    state: Arc<RealtimeRuntimeState<V, D>>,
    clock: Arc<dyn MonotonicClock>,
    intervals: RuntimeIntervals,
    workers: WorkerGroup,
    started: bool,
}
impl<V: Send + Sync + 'static, D: Send + Sync + 'static> RealtimeVisionRuntime<V, D> {
    pub fn new(clock: Arc<dyn MonotonicClock>, intervals: RuntimeIntervals) -> Self {
        Self {
            state: Arc::new(RealtimeRuntimeState::new(clock.clone())),
            clock,
            intervals,
            workers: WorkerGroup::new(Cancellation::default(), 4),
            started: false,
        }
    }
    pub fn state(&self) -> &Arc<RealtimeRuntimeState<V, D>> {
        &self.state
    }
    pub fn cancellation(&self) -> Cancellation {
        self.workers.cancellation()
    }
    pub fn faults(&self) -> Result<Vec<WorkerFault>, ControlError> {
        self.workers.faults()
    }
    pub fn stop(&mut self, timeout: Duration) -> Result<(), ControlError> {
        self.workers.finish(timeout)
    }
    pub fn start<C: ScreenSource>(
        &mut self,
        factory: impl FnOnce() -> Result<C, ControlError> + Send + 'static,
        fast: impl FnMut(Arc<RuntimeFrame>) -> Result<Option<V>, ControlError> + Send + 'static,
        slow: impl FnMut(Arc<RuntimeFrame>) -> Result<Option<V>, ControlError> + Send + 'static,
        decision: impl FnMut(&RuntimeSnapshot<V, D>) -> Result<Option<D>, ControlError> + Send + 'static,
    ) -> Result<(), ControlError> {
        if self.started || self.cancellation().is_cancelled() {
            return Err(ControlError::Runtime(
                "runtime can only be started once".into(),
            ));
        }
        self.started = true;
        let result = (|| {
            let state = self.state.clone();
            let clock = self.clock.clone();
            let delay = self.intervals.capture_seconds;
            self.workers.spawn("d2-capture", move |token| {
                capture_loop(state, clock, factory, delay, token)
            })?;
            self.spawn_vision(VisionChannel::Fast, fast, self.intervals.fast)?;
            self.spawn_vision(VisionChannel::Slow, slow, self.intervals.slow)?;
            let state = self.state.clone();
            let clock = self.clock.clone();
            let interval = self.intervals.decision;
            self.workers.spawn("d2-decision", move |token| {
                decision_loop(state, clock, decision, interval, token)
            })
        })();
        if let Err(error) = result {
            return Err(match self.stop(Duration::from_secs(3)) {
                Ok(()) => error,
                Err(cleanup) => {
                    ControlError::Runtime(format!("{error}; worker cleanup failed: {cleanup}"))
                }
            });
        }
        Ok(())
    }
    fn spawn_vision(
        &mut self,
        channel: VisionChannel,
        processor: impl FnMut(Arc<RuntimeFrame>) -> Result<Option<V>, ControlError> + Send + 'static,
        interval: Duration,
    ) -> Result<(), ControlError> {
        let state = self.state.clone();
        let clock = self.clock.clone();
        let idle = self.intervals.idle;
        self.workers.spawn(
            if channel == VisionChannel::Fast {
                "d2-fast-vision"
            } else {
                "d2-slow-vision"
            },
            move |token| vision_loop(state, clock, channel, processor, interval, idle, token),
        )
    }
}

fn capture_loop<V, D, C: ScreenSource>(
    state: Arc<RealtimeRuntimeState<V, D>>,
    clock: Arc<dyn MonotonicClock>,
    factory: impl FnOnce() -> Result<C, ControlError>,
    delay: f64,
    token: Cancellation,
) -> Result<(), ControlError> {
    let mut source = OwnedSource(Some(factory()?));
    let result = (|| {
        let mut sequence = 0;
        while !token.is_cancelled() {
            let generation = state.generation()?;
            let started = clock.now();
            let capture = source
                .0
                .as_mut()
                .ok_or_else(|| ControlError::Runtime("capture source already closed".into()))?;
            let packet = capture.grab()?;
            state.publish_frame(RuntimeFrame::new(
                FrameSource::new(sequence, started, generation)?,
                packet.frame,
                packet.target,
                packet.wall_captured_at,
                packet.geometry,
            ))?;
            sequence = sequence
                .checked_add(1)
                .ok_or_else(|| ControlError::Runtime("capture sequence exhausted".into()))?;
            let remaining = delay - (clock.now() - started);
            if remaining > 0.0 {
                token.wait(Duration::try_from_secs_f64(remaining).map_err(|_| {
                    ControlError::Runtime("capture wait exceeds runtime range".into())
                })?)?;
            }
        }
        Ok(())
    })();
    let closed = source.close();
    match (result, closed) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(error), Err(cleanup)) => Err(ControlError::Runtime(format!(
            "{error}; capture close also failed: {cleanup}"
        ))),
    }
}

fn vision_loop<V, D>(
    state: Arc<RealtimeRuntimeState<V, D>>,
    clock: Arc<dyn MonotonicClock>,
    channel: VisionChannel,
    mut processor: impl FnMut(Arc<RuntimeFrame>) -> Result<Option<V>, ControlError>,
    interval: Duration,
    idle: Duration,
    token: Cancellation,
) -> Result<(), ControlError> {
    let mut last_sequence = None;
    while !token.is_cancelled() {
        let Some(frame) = state
            .latest_frame()?
            .filter(|frame| Some(frame.source.sequence) != last_sequence)
        else {
            token.wait(idle)?;
            continue;
        };
        let payload = processor(frame.clone())?;
        last_sequence = Some(frame.source.sequence);
        state.publish_vision(
            channel,
            VisionSnapshot::new(channel, payload.map(Arc::new), frame.source, clock.now())?,
        )?;
        if state
            .latest_frame()?
            .is_some_and(|frame| Some(frame.source.sequence) != last_sequence)
        {
            continue;
        }
        if !interval.is_zero() {
            token.wait(interval)?;
        }
    }
    Ok(())
}

fn decision_loop<V, D>(
    state: Arc<RealtimeRuntimeState<V, D>>,
    clock: Arc<dyn MonotonicClock>,
    mut decision: impl FnMut(&RuntimeSnapshot<V, D>) -> Result<Option<D>, ControlError>,
    interval: Duration,
    token: Cancellation,
) -> Result<(), ControlError> {
    while !token.is_cancelled() {
        let snapshot = state.snapshot()?;
        if let Some(payload) = decision(&snapshot)? {
            state.publish_decision(DecisionSnapshot::new(
                Arc::new(payload),
                snapshot.latest_frame.as_ref().map(|frame| frame.source),
                clock.now(),
            )?)?;
        }
        if !interval.is_zero() {
            token.wait(interval)?;
        }
    }
    Ok(())
}

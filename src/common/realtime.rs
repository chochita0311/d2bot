//! Bounded immutable screen evidence with a shared monotonic origin.

mod runtime;
pub use runtime::{CapturedFrame, RealtimeVisionRuntime, RuntimeIntervals, ScreenSource};

use super::config::ConfigError;
use super::error::ControlError;
use super::frame::BgrFrame;
use super::frame_geometry::{FrameGeometry, PhysicalTarget};
use super::time::{self, MonotonicClock};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameSource {
    sequence: u64,
    captured_at: f64,
    generation: u64,
}
impl FrameSource {
    pub fn new(sequence: u64, captured_at: f64, generation: u64) -> Result<Self, ConfigError> {
        time::nonnegative(captured_at, "source_captured_at")?;
        Ok(Self {
            sequence,
            captured_at,
            generation,
        })
    }
    pub fn sequence(self) -> u64 {
        self.sequence
    }
    pub fn captured_at(self) -> f64 {
        self.captured_at
    }
    pub fn generation(self) -> u64 {
        self.generation
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeFrame {
    source: FrameSource,
    frame: Arc<BgrFrame>,
    target: PhysicalTarget,
    wall_captured_at: Option<f64>,
    geometry: Option<Arc<FrameGeometry>>,
}
impl RuntimeFrame {
    pub fn new(
        source: FrameSource,
        frame: Arc<BgrFrame>,
        target: PhysicalTarget,
        wall_captured_at: Option<f64>,
        geometry: Option<Arc<FrameGeometry>>,
    ) -> Self {
        Self {
            source,
            frame,
            target,
            wall_captured_at,
            geometry,
        }
    }
    pub fn source(&self) -> FrameSource {
        self.source
    }
    pub fn frame(&self) -> &Arc<BgrFrame> {
        &self.frame
    }
    pub fn target(&self) -> PhysicalTarget {
        self.target
    }
    pub fn wall_captured_at(&self) -> Option<f64> {
        self.wall_captured_at
    }
    pub fn geometry(&self) -> Option<&FrameGeometry> {
        self.geometry.as_deref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisionChannel {
    Fast,
    Slow,
}
impl VisionChannel {
    fn index(self) -> usize {
        match self {
            Self::Fast => 0,
            Self::Slow => 1,
        }
    }
}

#[derive(Debug)]
pub struct VisionSnapshot<V> {
    channel: VisionChannel,
    payload: Option<Arc<V>>,
    source: FrameSource,
    produced_at: f64,
}
impl<V> VisionSnapshot<V> {
    pub fn new(
        channel: VisionChannel,
        payload: Option<Arc<V>>,
        source: FrameSource,
        produced_at: f64,
    ) -> Result<Self, ConfigError> {
        time::nonnegative(produced_at, "produced_at")?;
        Ok(Self {
            channel,
            payload,
            source,
            produced_at,
        })
    }
    pub fn payload(&self) -> Option<&V> {
        self.payload.as_deref()
    }
    pub fn source(&self) -> FrameSource {
        self.source
    }
    pub fn produced_at(&self) -> f64 {
        self.produced_at
    }
}

#[derive(Debug)]
pub struct DecisionSnapshot<D> {
    payload: Arc<D>,
    source: Option<FrameSource>,
    produced_at: f64,
}
impl<D> DecisionSnapshot<D> {
    pub fn new(
        payload: Arc<D>,
        source: Option<FrameSource>,
        produced_at: f64,
    ) -> Result<Self, ConfigError> {
        time::nonnegative(produced_at, "produced_at")?;
        Ok(Self {
            payload,
            source,
            produced_at,
        })
    }
    pub fn payload(&self) -> &D {
        &self.payload
    }
    pub fn source(&self) -> Option<FrameSource> {
        self.source
    }
    pub fn produced_at(&self) -> f64 {
        self.produced_at
    }
}

#[derive(Debug)]
pub struct RuntimeSnapshot<V, D> {
    pub sampled_at: f64,
    pub latest_frame: Option<Arc<RuntimeFrame>>,
    pub recent_frames: Vec<Arc<RuntimeFrame>>,
    pub fast_vision: Option<Arc<VisionSnapshot<V>>>,
    pub slow_vision: Option<Arc<VisionSnapshot<V>>>,
    pub latest_decision: Option<Arc<DecisionSnapshot<D>>>,
    pub generation: u64,
}

struct State<V, D> {
    last_clock: Option<f64>,
    generation: u64,
    last_frame_sequence: Option<u64>,
    last_frame_time: Option<f64>,
    channel_sources: [Option<FrameSource>; 2],
    latest_frame: Option<Arc<RuntimeFrame>>,
    recent_frames: VecDeque<Arc<RuntimeFrame>>,
    vision: [Option<Arc<VisionSnapshot<V>>>; 2],
    decision: Option<Arc<DecisionSnapshot<D>>>,
}
impl<V, D> State<V, D> {
    fn invalidate(&mut self) -> Result<u64, ControlError> {
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| ControlError::Runtime("runtime generation exhausted".into()))?;
        self.latest_frame = None;
        self.recent_frames.clear();
        self.vision = [None, None];
        self.decision = None;
        self.channel_sources = [None, None];
        Ok(self.generation)
    }
}

pub struct RealtimeRuntimeState<V, D> {
    state: Mutex<State<V, D>>,
    clock: Arc<dyn MonotonicClock>,
}
impl<V, D> RealtimeRuntimeState<V, D> {
    pub const FRAME_BUFFER_SIZE: usize = 5;
    pub fn new(clock: Arc<dyn MonotonicClock>) -> Self {
        Self {
            clock,
            state: Mutex::new(State {
                last_clock: None,
                generation: 0,
                last_frame_sequence: None,
                last_frame_time: None,
                channel_sources: [None, None],
                latest_frame: None,
                recent_frames: VecDeque::with_capacity(Self::FRAME_BUFFER_SIZE),
                vision: [None, None],
                decision: None,
            }),
        }
    }
    fn lock(&self) -> Result<MutexGuard<'_, State<V, D>>, ControlError> {
        self.state
            .lock()
            .map_err(|_| ControlError::Runtime("realtime state lock was poisoned".into()))
    }
    fn now(&self, state: &mut State<V, D>) -> Result<f64, ControlError> {
        let now = time::nonnegative(self.clock.now(), "runtime clock")?;
        if state.last_clock.is_some_and(|last| now < last) {
            return Err(ControlError::Runtime(
                "runtime monotonic clock moved backwards".into(),
            ));
        }
        state.last_clock = Some(now);
        Ok(now)
    }
    pub fn generation(&self) -> Result<u64, ControlError> {
        Ok(self.lock()?.generation)
    }
    pub fn invalidate(&self) -> Result<u64, ControlError> {
        self.lock()?.invalidate()
    }
    pub fn is_current_generation(
        &self,
        snapshot: &RuntimeSnapshot<V, D>,
    ) -> Result<bool, ControlError> {
        let state = self.lock()?;
        Ok(snapshot.generation == state.generation
            && snapshot.latest_frame.is_some()
            && state.latest_frame.is_some())
    }
    pub fn publish_frame(&self, mut frame: RuntimeFrame) -> Result<bool, ControlError> {
        let mut state = self.lock()?;
        let now = self.now(&mut state)?;
        let source = frame.source;
        if source.generation != state.generation
            || state
                .last_frame_sequence
                .is_some_and(|last| source.sequence <= last)
            || source.captured_at > now
            || state
                .last_frame_time
                .is_some_and(|last| source.captured_at < last)
        {
            return Ok(false);
        }
        if state
            .latest_frame
            .as_ref()
            .is_some_and(|latest| latest.target != frame.target)
        {
            frame.source.generation = state.invalidate()?;
        }
        let frame = Arc::new(frame);
        state.last_frame_sequence = Some(source.sequence);
        state.last_frame_time = Some(source.captured_at);
        state.latest_frame = Some(frame.clone());
        if state.recent_frames.len() == Self::FRAME_BUFFER_SIZE {
            state.recent_frames.pop_front();
        }
        state.recent_frames.push_back(frame);
        Ok(true)
    }
    pub fn publish_vision(
        &self,
        channel: VisionChannel,
        snapshot: VisionSnapshot<V>,
    ) -> Result<bool, ControlError> {
        if channel != snapshot.channel {
            return Err(ConfigError::value("matching fast/slow vision channel required").into());
        }
        let mut state = self.lock()?;
        let now = self.now(&mut state)?;
        let Some(latest) = &state.latest_frame else {
            return Ok(false);
        };
        let source = snapshot.source;
        let previous = state.channel_sources[channel.index()];
        if source.generation != state.generation
            || previous.is_some_and(|previous| {
                source.sequence <= previous.sequence || source.captured_at < previous.captured_at
            })
            || source.sequence > latest.source.sequence
            || source.captured_at > latest.source.captured_at
            || source.captured_at > snapshot.produced_at
            || snapshot.produced_at > now
        {
            return Ok(false);
        }
        state.channel_sources[channel.index()] = Some(source);
        state.vision[channel.index()] = if snapshot.payload.is_some() {
            Some(Arc::new(snapshot))
        } else {
            None
        };
        Ok(true)
    }
    pub fn publish_decision(&self, snapshot: DecisionSnapshot<D>) -> Result<bool, ControlError> {
        let mut state = self.lock()?;
        let now = self.now(&mut state)?;
        let Some(latest) = &state.latest_frame else {
            return Ok(false);
        };
        let Some(source) = snapshot.source else {
            return Ok(false);
        };
        if source.generation != state.generation
            || source.sequence > latest.source.sequence
            || source.captured_at > latest.source.captured_at
            || source.captured_at > snapshot.produced_at
            || snapshot.produced_at > now
        {
            return Ok(false);
        }
        if state.decision.as_ref().is_some_and(|previous| {
            previous
                .source
                .is_some_and(|previous| source.sequence < previous.sequence)
                || snapshot.produced_at < previous.produced_at
        }) {
            return Ok(false);
        }
        state.decision = Some(Arc::new(snapshot));
        Ok(true)
    }
    pub fn latest_frame(&self) -> Result<Option<Arc<RuntimeFrame>>, ControlError> {
        Ok(self.lock()?.latest_frame.clone())
    }
    pub fn snapshot(&self) -> Result<RuntimeSnapshot<V, D>, ControlError> {
        let mut state = self.lock()?;
        let now = self.now(&mut state)?;
        Ok(RuntimeSnapshot {
            sampled_at: now,
            latest_frame: state.latest_frame.clone(),
            recent_frames: state.recent_frames.iter().cloned().collect(),
            fast_vision: state.vision[0].clone(),
            slow_vision: state.vision[1].clone(),
            latest_decision: state.decision.clone(),
            generation: state.generation,
        })
    }
}

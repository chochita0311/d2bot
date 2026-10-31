//! 현재 장면·대상·생사 근거의 계약. 요청한 입력이나 미검출을 확정 상태로 쓰지 않는다.
use super::{
    config::ConfigError,
    frame::{PixelRect, Size},
    survival::ObservationStamp,
    time,
};
use serde::{Serialize, Serializer};
use std::sync::Arc;
fn serialize_size<S: Serializer>(size: &Size, serializer: S) -> Result<S::Ok, S::Error> {
    [size.width(), size.height()].serialize(serializer)
}
fn serialize_rect<S: Serializer>(r: &PixelRect, serializer: S) -> Result<S::Ok, S::Error> {
    [r.left(), r.top(), r.right(), r.bottom()].serialize(serializer)
}
fn serialize_context<S: Serializer>(
    context: &Arc<SceneContext>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    context.as_ref().serialize(serializer)
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SceneContext {
    stamp: ObservationStamp,
    region_id: String,
    movement_generation: u64,
    #[serde(serialize_with = "serialize_size")]
    frame_size: Size,
    #[serde(serialize_with = "serialize_rect")]
    viewport: PixelRect,
}
impl SceneContext {
    pub fn new(
        stamp: ObservationStamp,
        region_id: impl Into<String>,
        movement_generation: u64,
        frame_size: Size,
        viewport: PixelRect,
    ) -> Result<Self, ConfigError> {
        let region_id = region_id.into();
        time::text(&region_id, "region_id")?;
        viewport
            .checked_in(frame_size)
            .map_err(|_| ConfigError::value("viewport must be inside the native frame"))?;
        Ok(Self {
            stamp,
            region_id,
            movement_generation,
            frame_size,
            viewport,
        })
    }
    pub fn stamp(&self) -> &ObservationStamp {
        &self.stamp
    }
    pub fn region_id(&self) -> &str {
        &self.region_id
    }
    pub fn movement_generation(&self) -> u64 {
        self.movement_generation
    }
    pub fn frame_size(&self) -> Size {
        self.frame_size
    }
    pub fn viewport(&self) -> PixelRect {
        self.viewport
    }
    pub fn same_scene(&self, other: &Self) -> bool {
        self.stamp.character_id() == other.stamp.character_id()
            && self.stamp.room_id() == other.stamp.room_id()
            && self.region_id == other.region_id
            && self.movement_generation == other.movement_generation
            && self.frame_size == other.frame_size
            && self.viewport == other.viewport
    }
    pub fn fresh(&self, now: f64, max_age: f64) -> Result<bool, ConfigError> {
        time::nonnegative(now, "now")?;
        time::nonnegative(max_age, "max_age")?;
        Ok((0.0..=max_age).contains(&(now - self.stamp.observed_at())))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CandidateCategory {
    Monster,
    MonsterGroup,
    Landmark,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    Alive,
    Corpse,
    Landmark,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CandidateIdentity {
    asset_id: String,
    kind: String,
    category: CandidateCategory,
    appearance: Appearance,
}
impl CandidateIdentity {
    pub fn new(
        asset_id: impl Into<String>,
        kind: impl Into<String>,
        category: CandidateCategory,
        appearance: Appearance,
    ) -> Result<Self, ConfigError> {
        let asset_id = asset_id.into();
        let kind = kind.into();
        time::text(&asset_id, "asset_id")?;
        time::text(&kind, "kind")?;
        Ok(Self {
            asset_id,
            kind,
            category,
            appearance,
        })
    }
    pub fn asset_id(&self) -> &str {
        &self.asset_id
    }
    pub fn kind(&self) -> &str {
        &self.kind
    }
    pub fn category(&self) -> CandidateCategory {
        self.category
    }
    pub fn appearance(&self) -> Appearance {
        self.appearance
    }
}
fn checked_point(point: [f64; 2], size: Size, name: &str) -> Result<[f64; 2], ConfigError> {
    for (v, limit) in point.into_iter().zip([size.width(), size.height()]) {
        time::nonnegative(v, name)?;
        if v > (limit - 1) as f64 {
            return Err(ConfigError::value(format!("{name} is out of range")));
        }
    }
    Ok(point)
}
fn contains(rect: PixelRect, [x, y]: [f64; 2]) -> bool {
    rect.left() as f64 <= x
        && x < rect.right() as f64
        && rect.top() as f64 <= y
        && y < rect.bottom() as f64
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VisualCandidate {
    #[serde(serialize_with = "serialize_context")]
    context: Arc<SceneContext>,
    #[serde(flatten)]
    identity: CandidateIdentity,
    score: f64,
    #[serde(rename = "box", serialize_with = "serialize_rect")]
    bounds: PixelRect,
    ground: Option<[f64; 2]>,
}
impl VisualCandidate {
    pub fn new(
        context: Arc<SceneContext>,
        identity: CandidateIdentity,
        score: f64,
        bounds: PixelRect,
        ground: Option<[f64; 2]>,
    ) -> Result<Self, ConfigError> {
        time::nonnegative(score, "score")?;
        if score > 1.0 {
            return Err(ConfigError::value("score is out of range"));
        }
        let view = context.viewport;
        if bounds.left() < view.left()
            || bounds.top() < view.top()
            || bounds.right() > view.right()
            || bounds.bottom() > view.bottom()
        {
            return Err(ConfigError::value(
                "candidate must be inside the observed viewport",
            ));
        }
        if let Some(point) = ground {
            checked_point(point, context.frame_size, "ground")?;
            if !contains(bounds, point) {
                return Err(ConfigError::value("ground must be inside candidate box"));
            }
        }
        Ok(Self {
            context,
            identity,
            score,
            bounds,
            ground,
        })
    }
    pub fn context(&self) -> &Arc<SceneContext> {
        &self.context
    }
    pub fn identity(&self) -> &CandidateIdentity {
        &self.identity
    }
    pub fn score(&self) -> f64 {
        self.score
    }
    pub fn bounds(&self) -> PixelRect {
        self.bounds
    }
    pub fn ground(&self) -> Option<[f64; 2]> {
        self.ground
    }
}
#[derive(Debug, Clone, Copy)]
pub struct CombatRadius {
    enter: f64,
    leave: f64,
}
impl CombatRadius {
    pub fn new(enter: f64, leave: f64) -> Result<Self, ConfigError> {
        time::nonnegative(enter, "enter")?;
        time::nonnegative(leave, "leave")?;
        if enter <= 0.0 || enter >= leave || leave > 1.0 {
            return Err(ConfigError::value("radius requires 0 < enter < leave <= 1"));
        }
        Ok(Self { enter, leave })
    }
    pub fn enter(self) -> f64 {
        self.enter
    }
    pub fn leave(self) -> f64 {
        self.leave
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProximityState {
    Unknown,
    Near,
    Far,
    Boundary,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Proximity {
    pub state: ProximityState,
    pub distance: Option<f64>,
    pub reason: &'static str,
    pub target_id: Option<String>,
}
pub struct ProximityOptions<'a> {
    pub now: f64,
    pub max_age: f64,
    pub target_id: Option<&'a str>,
    pub previous: Option<&'a Proximity>,
    pub previous_context: Option<&'a SceneContext>,
}
pub fn proximity(
    candidate: &VisualCandidate,
    current: &SceneContext,
    player_ground: Option<[f64; 2]>,
    radius: CombatRadius,
    options: ProximityOptions<'_>,
) -> Result<Proximity, ConfigError> {
    let ProximityOptions {
        now,
        max_age,
        target_id,
        previous,
        previous_context,
    } = options;
    if let Some(id) = target_id {
        time::text(id, "target_id")?;
    }
    let unknown = |reason| Proximity {
        state: ProximityState::Unknown,
        distance: None,
        reason,
        target_id: target_id.map(str::to_owned),
    };
    if candidate.context.as_ref() != current || !current.fresh(now, max_age)? {
        return Ok(unknown("stale-or-different-frame"));
    }
    if candidate.identity.category != CandidateCategory::Monster
        || candidate.identity.appearance != Appearance::Alive
    {
        return Ok(unknown("not-an-alive-monster-candidate"));
    }
    let (Some(player), Some(ground)) = (player_ground, candidate.ground) else {
        return Ok(unknown("ground-not-observed"));
    };
    checked_point(player, current.frame_size, "player_ground")?;
    if !contains(current.viewport, player) {
        return Ok(unknown("player-outside-viewport"));
    }
    let distance = ((ground[0] - player[0]) / current.frame_size.width() as f64)
        .hypot((ground[1] - player[1]) / current.frame_size.height() as f64);
    let retain = if let (Some(previous), Some(id), Some(context)) =
        (previous, target_id, previous_context)
    {
        previous.state == ProximityState::Near
            && previous.target_id.as_deref() == Some(id)
            && current.same_scene(context)
            && context.stamp.sequence() < current.stamp.sequence()
            && context.stamp.observed_at() < current.stamp.observed_at()
            && context.fresh(now, max_age)?
    } else {
        false
    };
    let state = if distance <= radius.enter || (retain && distance < radius.leave) {
        ProximityState::Near
    } else if distance >= radius.leave {
        ProximityState::Far
    } else {
        ProximityState::Boundary
    };
    Ok(Proximity {
        state,
        distance: Some(distance),
        reason: "candidate-radius",
        target_id: target_id.map(str::to_owned),
    })
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LifeState {
    Alive,
    Dead,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WitnessProvenance {
    HumanReviewed,
    LinkedObservation,
    TemplateCandidate,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LifeWitness {
    #[serde(serialize_with = "serialize_context")]
    context: Arc<SceneContext>,
    target_id: String,
    kind: String,
    state: LifeState,
    provenance: WitnessProvenance,
    linked_alive_sequence: Option<u64>,
}
impl LifeWitness {
    pub fn new(
        context: Arc<SceneContext>,
        target_id: impl Into<String>,
        kind: impl Into<String>,
        state: LifeState,
        provenance: WitnessProvenance,
        linked_alive_sequence: Option<u64>,
    ) -> Result<Self, ConfigError> {
        let target_id = target_id.into();
        let kind = kind.into();
        time::text(&target_id, "target_id")?;
        time::text(&kind, "kind")?;
        Ok(Self {
            context,
            target_id,
            kind,
            state,
            provenance,
            linked_alive_sequence,
        })
    }
    pub fn context(&self) -> &Arc<SceneContext> {
        &self.context
    }
    pub fn target_id(&self) -> &str {
        &self.target_id
    }
    pub fn kind(&self) -> &str {
        &self.kind
    }
    pub fn state(&self) -> LifeState {
        self.state
    }
    pub fn provenance(&self) -> WitnessProvenance {
        self.provenance
    }
    pub fn linked_alive_sequence(&self) -> Option<u64> {
        self.linked_alive_sequence
    }
}
pub fn life_state(
    current: &SceneContext,
    witness: Option<&LifeWitness>,
    now: f64,
    max_age: f64,
    previous_alive: Option<&LifeWitness>,
    first_dead: Option<&LifeWitness>,
) -> Result<LifeState, ConfigError> {
    if !current.fresh(now, max_age)? {
        return Ok(LifeState::Unknown);
    }
    let Some(witness) = witness else {
        return Ok(LifeState::Unknown);
    };
    if witness.context.as_ref() != current
        || witness.provenance == WitnessProvenance::TemplateCandidate
    {
        return Ok(LifeState::Unknown);
    }
    if witness.state == LifeState::Alive {
        return Ok(LifeState::Alive);
    }
    let (Some(alive), Some(dead)) = (previous_alive, first_dead) else {
        return Ok(LifeState::Unknown);
    };
    if witness.state != LifeState::Dead
        || alive.state != LifeState::Alive
        || dead.state != LifeState::Dead
    {
        return Ok(LifeState::Unknown);
    }
    for item in [alive, dead] {
        if item.provenance == WitnessProvenance::TemplateCandidate
            || item.target_id != witness.target_id
            || item.kind != witness.kind
            || !current.same_scene(&item.context)
            || !item.context.fresh(now, max_age)?
        {
            return Ok(LifeState::Unknown);
        }
    }
    let [a, b, c] = [
        alive.context.stamp(),
        dead.context.stamp(),
        witness.context.stamp(),
    ];
    if !(a.sequence() < b.sequence()
        && b.sequence() < c.sequence()
        && a.observed_at() < b.observed_at()
        && b.observed_at() < c.observed_at())
        || dead.linked_alive_sequence != Some(a.sequence())
        || witness.linked_alive_sequence != Some(a.sequence())
    {
        return Ok(LifeState::Unknown);
    }
    Ok(LifeState::Dead)
}

//! 실제 배경 이동과 현재 출구를 기억하는 유한한 국소 경로 그래프.
mod matching;
mod selection;
#[cfg(test)]
mod tests;
use super::{
    navigation::{ArcanePathObserver, PathChoice, PathView, Wing},
    terrain_patterns::TerrainLanding,
};
use crate::common::{
    config::ConfigError, error::ObservationError, scene_motion::SceneFeatures, time,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

type ExitKey = (usize, Option<usize>, Wing);
struct PlaceReference {
    features: Arc<SceneFeatures>,
    anchor: [f64; 2],
    position: [f64; 2],
    chart: usize,
}
#[derive(Clone, Copy, Default)]
pub struct GraphChooseOptions<'a> {
    pub excluded: &'a [[f64; 4]],
    pub landings: &'a [TerrainLanding],
    pub hub: Option<[f64; 2]>,
    pub prefer_goal_exit: bool,
}
pub struct ArcaneRouteGraph {
    maximum_references: usize,
    maximum_places: usize,
    place_radius: f64,
    reference_spacing: f64,
    references: Vec<PlaceReference>,
    places: Vec<[f64; 2]>,
    place_charts: Vec<usize>,
    edges: BTreeSet<(usize, usize)>,
    blocked: Vec<(usize, [f64; 2])>,
    position: [f64; 2],
    localized: bool,
    current_place: Option<usize>,
    started: bool,
    last_seen: Option<f64>,
    reference_cursor: usize,
    entry: Option<Wing>,
    kind: &'static str,
    reason: String,
    ports: Vec<(Wing, [i32; 2])>,
    port_selection: Option<Value>,
    relocalizations: usize,
    traversed: BTreeMap<ExitKey, usize>,
    pending_exit: Option<Wing>,
    exit_intent: Option<Wing>,
    pending_retreat: Option<ExitKey>,
    retreated_branches: BTreeSet<ExitKey>,
    escape_retreats: BTreeSet<ExitKey>,
    failed_exits: BTreeMap<ExitKey, usize>,
    chart: usize,
    next_chart: usize,
    missing_since: Option<f64>,
}
impl Default for ArcaneRouteGraph {
    fn default() -> Self {
        Self::bounded(12, 96, 120.0, 300.0)
    }
}
impl ArcaneRouteGraph {
    pub fn new(
        maximum_references: usize,
        maximum_places: usize,
        place_radius: f64,
        reference_spacing: f64,
    ) -> Result<Self, ConfigError> {
        if !(2..=32).contains(&maximum_references) || !(4..=256).contains(&maximum_places) {
            return Err(ConfigError::value("bounded route memory geometry required"));
        }
        time::positive(place_radius, "place_radius")?;
        time::positive(reference_spacing, "reference_spacing")?;
        Ok(Self::bounded(
            maximum_references,
            maximum_places,
            place_radius,
            reference_spacing,
        ))
    }
    fn bounded(
        maximum_references: usize,
        maximum_places: usize,
        place_radius: f64,
        reference_spacing: f64,
    ) -> Self {
        Self {
            maximum_references,
            maximum_places,
            place_radius,
            reference_spacing,
            references: Vec::new(),
            places: Vec::new(),
            place_charts: Vec::new(),
            edges: BTreeSet::new(),
            blocked: Vec::new(),
            position: [0.0; 2],
            localized: false,
            current_place: None,
            started: false,
            last_seen: None,
            reference_cursor: 0,
            entry: None,
            kind: "unobserved",
            reason: "unobserved".into(),
            ports: Vec::new(),
            port_selection: None,
            relocalizations: 0,
            traversed: BTreeMap::new(),
            pending_exit: None,
            exit_intent: None,
            pending_retreat: None,
            retreated_branches: BTreeSet::new(),
            escape_retreats: BTreeSet::new(),
            failed_exits: BTreeMap::new(),
            chart: 0,
            next_chart: 1,
            missing_since: None,
        }
    }
    pub fn position(&self) -> [f64; 2] {
        self.position
    }
    pub fn localized(&self) -> bool {
        self.localized
    }
    pub fn places(&self) -> &[[f64; 2]] {
        &self.places
    }
    pub fn observe(
        &mut self,
        features: Option<&Arc<SceneFeatures>>,
        anchor: [f64; 2],
        displacement: Option<[f64; 2]>,
        captured_at: f64,
    ) -> Result<bool, ObservationError> {
        time::nonnegative(captured_at, "route captured_at")?;
        finite_point(anchor)?;
        if let Some(point) = displacement {
            finite_point(point)?;
        }
        if self.last_seen.is_some_and(|last| captured_at <= last) {
            return Ok(false);
        }
        self.last_seen = Some(captured_at);
        let (previous, old_place, old_chart) = (self.position, self.current_place, self.chart);
        let (mut relocated, mut new_chart) = (false, false);
        if !self.started {
            self.started = true;
            self.localized = true;
        } else if let Some(delta) = displacement {
            self.position = std::array::from_fn(|i| self.position[i] + delta[i]);
            self.localized = true;
        } else {
            let found = self.relocate(features, anchor)?;
            self.localized = found.is_some();
            if let Some((chart, position)) = found {
                self.chart = chart;
                self.position = position;
                relocated = true;
                self.relocalizations += 1;
            }
        }
        if !self.localized {
            let missing = *self.missing_since.get_or_insert(captured_at);
            if captured_at - missing < 0.64 {
                self.reason = "place_unlocalized_memory_retained".into();
                return Ok(false);
            }
            // place 상한/과거 재대응과 무관하게 서로 다른 국소 좌표계를 구분한다.
            let next = self
                .next_chart
                .checked_add(1)
                .ok_or_else(|| ObservationError::invalid("route chart counter exhausted"))?;
            self.chart = self.next_chart;
            self.next_chart = next;
            self.position = [0.0; 2];
            self.localized = true;
            new_chart = true;
            self.entry = None;
        }
        self.missing_since = None;
        let mut nearest = None;
        for (index, (point, chart)) in self.places.iter().zip(&self.place_charts).enumerate() {
            if *chart == self.chart {
                let d = distance(self.position, *point);
                if nearest.is_none_or(|(_, before)| d < before) {
                    nearest = Some((index, d));
                }
            }
        }
        self.current_place =
            if let Some((index, _)) = nearest.filter(|(_, d)| *d <= self.place_radius) {
                Some(index)
            } else if self.places.len() < self.maximum_places {
                let index = self.places.len();
                self.places.push(self.position);
                self.place_charts.push(self.chart);
                Some(index)
            } else {
                None
            };
        let moved = displacement.is_some()
            && old_chart == self.chart
            && distance(previous, self.position) >= 64.0;
        if moved {
            if let Some(direction) = axis(subtract(self.position, previous)) {
                if self.pending_retreat == Some((old_chart, old_place, direction))
                    && self.current_place.is_some()
                    && self.current_place != old_place
                {
                    self.retreated_branches.insert((
                        self.chart,
                        self.current_place,
                        opposite(direction),
                    ));
                }
                self.pending_retreat = None;
                self.entry = Some(opposite(direction));
                self.pending_exit = None;
                if let (Some(old), Some(current)) = (old_place, self.current_place)
                    && old != current
                {
                    self.edges.insert((old, current));
                    *self
                        .traversed
                        .entry((old_chart, Some(old), direction))
                        .or_default() += 1;
                }
            }
        } else if relocated && (old_chart != self.chart || old_place != self.current_place) {
            self.entry = None;
            self.pending_exit = None;
            self.pending_retreat = None;
        }
        if let Some(features) = features.filter(|f| f.descriptors().is_some()) {
            let near = self
                .references
                .iter()
                .filter(|r| r.chart == self.chart)
                .map(|r| distance(self.position, r.position))
                .reduce(f64::min);
            if near.is_none_or(|d| d >= self.reference_spacing) {
                self.references.push(PlaceReference {
                    features: Arc::clone(features),
                    anchor,
                    position: self.position,
                    chart: self.chart,
                });
                if self.references.len() > self.maximum_references {
                    self.references.remove(0);
                }
            }
        }
        self.reason = if new_chart {
            "new_local_chart"
        } else if relocated {
            "place_relocalized"
        } else {
            "place_observed"
        }
        .into();
        Ok(relocated || new_chart)
    }
    pub fn fail_landing(
        &mut self,
        aim: Option<[i32; 2]>,
        anchor: [f64; 2],
    ) -> Result<(), ObservationError> {
        finite_point(anchor)?;
        self.pending_exit = None;
        self.exit_intent = None;
        self.pending_retreat = None;
        if let Some(aim) = aim.filter(|_| self.localized) {
            let delta = subtract(aim.map(f64::from), anchor);
            if let Some(direction) = axis(delta)
                && self.current_place.is_some()
            {
                *self
                    .failed_exits
                    .entry((self.chart, self.current_place, direction))
                    .or_default() += 1;
            }
            self.blocked.push((
                self.chart,
                std::array::from_fn(|i| self.position[i] + f64::from(aim[i]) - anchor[i]),
            ));
            if self.blocked.len() > 48 {
                self.blocked.remove(0);
            }
        }
        Ok(())
    }
    pub fn summary(&self) -> Value {
        let ports: serde_json::Map<String, Value> = self
            .ports
            .iter()
            .map(|(name, point)| (name.name().into(), json!(point)))
            .collect();
        json!({"position":self.position,"chart":self.chart,"coordinate_scope":"local_chart","localized":self.localized,"place":self.current_place,"places":self.places.len(),"edges":self.edges.len(),"entry":self.entry,"kind":self.kind,"ports":ports,"port_selection":self.port_selection,"reason":self.reason,"relocalizations":self.relocalizations,"references":self.references.len(),"traversed_exits":self.traversed.len(),"failed_exits":self.failed_exits.len(),"pending_exit":self.pending_exit,"exit_intent":self.exit_intent,"pending_retreat":self.pending_retreat,"retreated_branches":self.retreated_branches.len(),"escape_retreats":self.escape_retreats.len(),"feature_bytes":self.references.iter().map(|r|r.features.feature_bytes()).sum::<usize>()})
    }
}
fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
fn subtract(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn finite_point(point: [f64; 2]) -> Result<(), ObservationError> {
    if point.iter().any(|n| !n.is_finite()) {
        Err(ObservationError::invalid("finite route point required"))
    } else {
        Ok(())
    }
}
fn axis(vector: [f64; 2]) -> Option<Wing> {
    if vector[0].hypot(vector[1] * 2.0) == 0.0 {
        return None;
    }
    let mut best = None;
    for name in Wing::ALL {
        let [hx, hy] = name.heading();
        let score = vector[0] * hx + vector[1] * 4.0 * hy;
        if best.is_none_or(|(_, before)| score > before) {
            best = Some((name, score));
        }
    }
    best.map(|(name, _)| name)
}
fn opposite(wing: Wing) -> Wing {
    match wing {
        Wing::North => Wing::South,
        Wing::East => Wing::West,
        Wing::South => Wing::North,
        Wing::West => Wing::East,
    }
}

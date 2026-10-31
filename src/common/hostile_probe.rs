//! 근접 색상은 호버 후보만 만든다. 현재 패널·커서·새 화면의 양성 근거만 반환한다.
use super::{
    error::{ControlError, ObservationError},
    field_control::{DesktopPoint, FieldInputBackend},
    frame::{BgrView, PixelRect},
    time::MonotonicClock,
    vision_ops::{
        color,
        components::{Connectivity, connected_components},
        morphology::{Kernel, Morphology, morphology},
    },
};
use serde::Serialize;
use std::sync::Arc;
pub fn nearby_actor_proposals(
    frame: BgrView<'_>,
    player: Option<[f64; 2]>,
) -> Result<Vec<[i32; 2]>, ObservationError> {
    nearby_actor_proposals_with_radius(frame, player, [300.0, 180.0])
}
pub fn nearby_actor_proposals_with_radius(
    frame: BgrView<'_>,
    player: Option<[f64; 2]>,
    radius: [f64; 2],
) -> Result<Vec<[i32; 2]>, ObservationError> {
    if radius.iter().any(|r| !r.is_finite() || *r <= 0.0) {
        return Err(ObservationError::invalid(
            "finite positive reaction radius required",
        ));
    }
    let Some([px, py]) = player else {
        return Ok(vec![]);
    };
    if !px.is_finite() || !py.is_finite() {
        return Err(ObservationError::invalid("finite player ground required"));
    }
    let size = frame.size();
    let left = ((px - radius[0]) as i64)
        .max(24)
        .clamp(0, size.width() as i64) as usize;
    let top = ((py - radius[1]) as i64)
        .max(55)
        .clamp(0, size.height() as i64) as usize;
    let right = 1900
        .min((px + radius[0]) as i64)
        .clamp(0, size.width() as i64) as usize;
    let bottom = 900
        .min((py + radius[1]) as i64)
        .clamp(0, size.height() as i64) as usize;
    if left >= right || top >= bottom {
        return Ok(vec![]);
    }
    let hsv = color::hsv(frame.crop(PixelRect::new(left, top, right, bottom)?)?)?;
    let mask = color::in_range(hsv.view(), [35, 120, 80], [90, 255, 255])?;
    let mask = morphology(
        mask.view(),
        Kernel::Rectangle {
            width: 5,
            height: 5,
        },
        Morphology::Close,
    )?;
    let components = connected_components(mask.view(), Connectivity::Eight)?;
    let mut points = vec![];
    for item in components.stats.iter().skip(1) {
        let Some(rect) = item.rect else {
            continue;
        };
        let Some([x, y]) = item.centroid else {
            continue;
        };
        let (x, y) = (x + left as f64, y + top as f64);
        if (20..=180).contains(&rect.width())
            && (5..=100).contains(&rect.height())
            && item.area >= 40
            && near([x, y], [px, py], radius)
            && (x - px).hypot(y - py) >= 64.0
        {
            points.push([
                x.round_ties_even() as i32,
                (y - 80.0).round_ties_even() as i32,
            ]);
        }
    }
    points.sort_by(|a, b| {
        distance(a.map(f64::from), [px, py], 80.0).total_cmp(&distance(
            b.map(f64::from),
            [px, py],
            80.0,
        ))
    });
    points.truncate(8);
    Ok(points)
}
type PointerMap = Box<dyn Fn([i32; 2]) -> Result<DesktopPoint, ControlError> + Send + Sync>;
#[derive(Debug, Clone, Serialize)]
pub struct ProbeSnapshot {
    pub pending: Option<([i32; 2], f64)>,
    pub pending_ground: Option<[f64; 2]>,
    pub index: usize,
    pub next_proposal_at: Option<f64>,
}
pub struct NearbyHostileProbe<O> {
    observer: O,
    backend: Arc<dyn FieldInputBackend>,
    clock: Arc<dyn MonotonicClock>,
    to_desktop: PointerMap,
    state: ProbeSnapshot,
    radius: [f64; 2],
}
const OFFSETS: [[i32; 2]; 10] = [
    [-80, -130],
    [80, -130],
    [0, -80],
    [-160, -80],
    [160, -80],
    [-240, -40],
    [240, -40],
    [-120, 0],
    [120, 0],
    [0, -160],
];
impl<O> NearbyHostileProbe<O> {
    pub fn new<T, E>(
        observer: O,
        backend: Arc<dyn FieldInputBackend>,
        origin: DesktopPoint,
        clock: Arc<dyn MonotonicClock>,
    ) -> Self
    where
        O: for<'a> FnMut(BgrView<'a>) -> Result<Option<T>, E>,
        E: From<ControlError>,
    {
        Self {
            radius: [300.0, 180.0],
            observer,
            backend,
            clock,
            to_desktop: Box::new(move |point| {
                origin[0]
                    .checked_add(i64::from(point[0]))
                    .zip(origin[1].checked_add(i64::from(point[1])))
                    .map(|(x, y)| [x, y])
                    .ok_or_else(|| {
                        ControlError::Runtime(
                            "probe pointer outside desktop coordinate range".into(),
                        )
                    })
            }),
            state: ProbeSnapshot {
                pending: None,
                pending_ground: None,
                index: 0,
                next_proposal_at: None,
            },
        }
    }
    pub fn with_mapper(
        mut self,
        mapper: impl Fn([i32; 2]) -> Result<DesktopPoint, ControlError> + Send + Sync + 'static,
    ) -> Self {
        self.to_desktop = Box::new(mapper);
        self
    }
    pub fn with_radius(mut self, radius: [f64; 2]) -> Result<Self, ControlError> {
        if radius.iter().any(|r| !r.is_finite() || *r <= 0.0) {
            return Err(ControlError::Runtime(
                "finite positive reaction radius required".into(),
            ));
        }
        self.radius = radius;
        Ok(self)
    }
    pub fn radius(&self) -> [f64; 2] {
        self.radius
    }
    pub fn reset(&mut self) {
        self.state.pending = None;
        self.state.pending_ground = None;
    }
    pub fn snapshot(&self) -> ProbeSnapshot {
        self.state.clone()
    }
    pub fn has_pending(&self) -> bool {
        self.state.pending.is_some()
    }
    pub fn proposal_due(&self, now: f64) -> bool {
        !now.is_nan() && self.state.next_proposal_at.is_none_or(|at| now >= at)
    }
    pub fn observe<T, E>(
        &mut self,
        frame: BgrView<'_>,
        player: Option<[f64; 2]>,
        acquired: f64,
        points: Option<&[[i32; 2]]>,
    ) -> Result<Option<T>, E>
    where
        O: for<'a> FnMut(BgrView<'a>) -> Result<Option<T>, E>,
        E: From<ControlError>,
    {
        let now = self.clock.now();
        let Some(player) = player.filter(|_| (0.0..0.25).contains(&(now - acquired))) else {
            self.reset();
            return Ok(None);
        };
        if let Some((point, requested)) = self.state.pending {
            let absolute = (self.to_desktop)(point)?;
            let ground = self.state.pending_ground;
            let confirmed = (0.05..=0.5).contains(&(acquired - requested))
                && ground.is_some_and(|ground| near(ground, player, self.radius))
                && points.is_none_or(|points| {
                    points.iter().any(|p| {
                        near(
                            [f64::from(p[0]), f64::from(p[1]) + 80.0],
                            player,
                            self.radius,
                        ) && ground
                            .is_some_and(|ground| distance(p.map(f64::from), ground, 80.0) <= 64.0)
                    })
                })
                && self.backend.pointer_matches(absolute)?;
            if confirmed {
                let target = (self.observer)(frame)?;
                if target.is_some() && (0.0..0.25).contains(&(self.clock.now() - acquired)) {
                    self.state.pending = Some((point, acquired));
                    return Ok(target);
                }
            } else if (0.0..0.05).contains(&(acquired - requested)) {
                return Ok(None);
            }
            self.reset();
        }
        let (point, ground) = if let Some(points) = points {
            if points.is_empty() || self.state.next_proposal_at.is_some_and(|next| now < next) {
                return Ok(None);
            }
            let point = points[self.state.index % points.len()];
            self.state.next_proposal_at = Some(now + 0.5);
            (point, [f64::from(point[0]), f64::from(point[1]) + 80.0])
        } else {
            let [dx, dy] = OFFSETS[self.state.index % OFFSETS.len()];
            let point = [
                (player[0] + f64::from(dx)) as i32,
                (player[1] + f64::from(dy)) as i32,
            ];
            (point, point.map(f64::from))
        };
        self.state.index += 1;
        if !near(ground, player, self.radius) {
            return Ok(None);
        }
        if !(24..frame.size().width() as i64 - 24).contains(&i64::from(point[0]))
            || !(55..900).contains(&point[1])
        {
            return Ok(None);
        }
        if !(0.0..0.25).contains(&(self.clock.now() - acquired)) {
            return Ok(None);
        }
        self.backend.move_pointer((self.to_desktop)(point)?)?;
        self.state.pending = Some((point, self.clock.now()));
        self.state.pending_ground = Some(ground);
        Ok(None)
    }
}
fn near(point: [f64; 2], player: [f64; 2], radius: [f64; 2]) -> bool {
    ((point[0] - player[0]) / radius[0]).hypot((point[1] - player[1]) / radius[1]) <= 1.0
}
fn distance(point: [f64; 2], ground: [f64; 2], offset: f64) -> f64 {
    (point[0] - ground[0]).hypot(point[1] + offset - ground[1])
}

//! 현재 관찰한 굴곡의 조향. 경로 선택·전체 경로의 관측 시한과 입력 소유권은 별도다.
use super::navigation::{ArcanePathObserver, ChooseOptions, PathChoice, PathView, Wing};
use crate::common::{error::ObservationError, time};
use serde::Serialize;
pub fn angle(left: [f64; 2], right: [f64; 2]) -> f64 {
    let length = left[0].hypot(left[1]) * right[0].hypot(right[1]);
    if length == 0.0 {
        0.0
    } else {
        ((left[0] * right[0] + left[1] * right[1]) / length)
            .clamp(-1.0, 1.0)
            .acos()
            .to_degrees()
    }
}
pub fn floor_supports_aim(
    view: &PathView,
    aim: [i32; 2],
    clearance_pixels: f64,
) -> Result<bool, ObservationError> {
    let x = ((f64::from(aim[0]) - view.context.viewport.left() as f64) / view.cell_pixels as f64)
        .floor();
    let y = ((f64::from(aim[1]) - view.context.viewport.top() as f64) / view.cell_pixels as f64)
        .floor();
    if x < 0.0
        || y < 0.0
        || x >= view.floor().size().width() as f64
        || y >= view.floor().size().height() as f64
    {
        return Ok(false);
    }
    let clearance = view.clearance()?;
    // 배열 전체의 f32 곱과 달리 NumPy 단일 원소×정수는 f64로 승격된다.
    Ok(
        f64::from(clearance.pixels()[y as usize * clearance.size().width() + x as usize])
            * view.cell_pixels as f64
            >= clearance_pixels,
    )
}
#[derive(Clone, Copy)]
pub struct RouteFlowOptions<'a> {
    pub position: [f64; 2],
    pub visited: &'a [[f64; 2]],
    pub excluded: &'a [[f64; 4]],
}
impl Default for RouteFlowOptions<'_> {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0],
            visited: &[],
            excluded: &[],
        }
    }
}
#[derive(Debug, Serialize)]
pub struct ArcaneRouteFlow {
    detour: Option<[f64; 2]>,
    forward_frames: u64,
    observed_at: Option<f64>,
    reason: String,
}
impl ArcaneRouteFlow {
    pub fn new(detour: Option<[f64; 2]>) -> Self {
        Self {
            detour: detour.map(Self::axis),
            forward_frames: 0,
            observed_at: None,
            reason: "global_flow".into(),
        }
    }
    pub fn detour(&self) -> Option<[f64; 2]> {
        self.detour
    }
    pub fn reason(&self) -> &str {
        &self.reason
    }
    pub fn follows(direction: [f64; 2], heading: [f64; 2], degrees: f64) -> bool {
        angle(
            [direction[0], direction[1] * 2.0],
            [heading[0], heading[1] * 2.0],
        ) < degrees
    }
    pub fn axis(direction: [f64; 2]) -> [f64; 2] {
        let mut best = Wing::North.heading();
        let mut minimum = angle([direction[0], direction[1] * 2.0], [best[0], best[1] * 2.0]);
        for wing in &Wing::ALL[1..] {
            let heading = wing.heading();
            let value = angle(
                [direction[0], direction[1] * 2.0],
                [heading[0], heading[1] * 2.0],
            );
            if value < minimum {
                minimum = value;
                best = heading;
            }
        }
        best
    }
    pub fn choose(
        &mut self,
        path: &ArcanePathObserver,
        view: &PathView,
        wing: Wing,
        global: &PathChoice,
        now: f64,
        options: RouteFlowOptions<'_>,
    ) -> Result<PathChoice, ObservationError> {
        if !(0.0..0.25).contains(&(now - view.context.captured_at)) {
            return Ok(PathChoice::unknown("route_frame_stale"));
        }
        let fresh = self
            .observed_at
            .is_none_or(|t| view.context.captured_at > t);
        if fresh {
            self.observed_at = Some(view.context.captured_at);
        }
        let heading = wing.heading();
        if self.detour == Some(heading) {
            self.detour = None;
        }
        let direction = global.aim.map(|p| {
            [
                f64::from(p[0]) - view.context.anchor[0],
                f64::from(p[1]) - view.context.anchor[1],
            ]
        });
        let forward = direction.is_some_and(|d| Self::follows(d, heading, 20.0));
        let geometry = path
            .teleport()
            .ok_or_else(|| ObservationError::invalid("route flow requires teleport geometry"))?;
        let minimum = geometry
            .maximum_distance
            .min(200.0_f64.max(geometry.preferred_distance.unwrap_or(240.0) * 0.5));
        let wide = direction.is_some_and(|d| d[0].hypot(d[1]) >= minimum);
        let choose = |local_heading| {
            path.choose(
                view,
                wing,
                ChooseOptions {
                    position: options.position,
                    visited: options.visited,
                    excluded: options.excluded,
                    heading: local_heading,
                    ..ChooseOptions::default()
                },
            )
        };
        if self.detour.is_none() {
            if direction.is_none_or(|d| Self::axis(d) != heading) {
                let direct = choose(None)?;
                if direct.aim.is_some_and(|p| {
                    let d = [
                        f64::from(p[0]) - view.context.anchor[0],
                        f64::from(p[1]) - view.context.anchor[1],
                    ];
                    d[0].hypot(d[1]) >= minimum && Self::axis(d) == heading
                }) {
                    self.reason = "global_flow".into();
                    return Ok(direct);
                }
            }
            if direction.is_some_and(|d| !Self::follows(d, heading, 150.0)) {
                let mut exit: Option<(f64, PathChoice, [f64; 2])> = None;
                let mut probe: Option<(f64, PathChoice, [f64; 2])> = None;
                let [dx, dy] = heading;
                for side in [[-dy * 2.0, dx / 2.0], [dy * 2.0, -dx / 2.0]] {
                    let candidate = choose(Some(side))?;
                    let Some(point) = candidate.aim else {
                        continue;
                    };
                    let vector = [
                        f64::from(point[0]) - view.context.anchor[0],
                        f64::from(point[1]) - view.context.anchor[1],
                    ];
                    if !Self::follows(vector, side, 40.0) {
                        continue;
                    }
                    let goal = projection(vector, heading);
                    let distance = vector[0].hypot(vector[1]);
                    if distance >= minimum {
                        if exit.as_ref().is_none_or(|e| goal > e.0) {
                            exit = Some((goal, candidate, side));
                        }
                    } else if projection(vector, side) >= geometry.clearance_pixels * 2.0
                        && probe.as_ref().is_none_or(|e| goal > e.0)
                    {
                        // 처음 꺾이는 계단도 이미 굴곡 안일 때와 같은 현재 바닥 탐침을 쓴다.
                        probe = Some((goal, candidate, side));
                    }
                }
                let probing = exit.is_none() && probe.is_some();
                if let Some((_, mut candidate, side)) = exit.or(probe) {
                    self.detour = Some(side);
                    self.reason = if probing {
                        "observed_corner_probe"
                    } else {
                        "observed_corner_turn"
                    }
                    .into();
                    candidate.reason = self.reason.clone();
                    return Ok(candidate);
                }
                self.reason = "opposite_exit_without_observed_corner".into();
                return Ok(global.clone());
            }
            if wide && direction.is_some_and(|d| Self::axis(d) != heading) {
                self.detour = direction.map(Self::axis);
                self.reason = "bend_entered".into();
            } else {
                self.reason = "global_flow".into();
            }
            return Ok(global.clone());
        }
        if fresh {
            self.forward_frames = if forward && wide {
                self.forward_frames
                    .checked_add(1)
                    .ok_or_else(|| ObservationError::invalid("route forward counter exhausted"))?
            } else {
                0
            };
        }
        let detour = self
            .detour
            .ok_or_else(|| ObservationError::invalid("route detour missing"))?;
        let local = choose(Some(detour))?;
        let mut advance = 0.0;
        if let Some(point) = local.aim {
            let d = [
                f64::from(point[0]) - view.context.anchor[0],
                f64::from(point[1]) - view.context.anchor[1],
            ];
            if Self::follows(d, detour, 40.0) {
                advance = projection(d, detour);
            }
        }
        if self.forward_frames >= 2 && advance < minimum {
            self.detour = None;
            self.forward_frames = 0;
            self.reason = "forward_reopened".into();
            return Ok(global.clone());
        }
        if local.aim.is_some() && advance >= geometry.clearance_pixels * 2.0 {
            self.reason = "follow_observed_bend".into();
            return Ok(local);
        }
        let mut exits: Option<(f64, PathChoice, [f64; 2])> = None;
        let mut probes: Option<(f64, PathChoice, [f64; 2])> = None;
        let [dx, dy] = detour;
        for next in [[-dy * 2.0, dx / 2.0], [dy * 2.0, -dx / 2.0]] {
            if forward && self.forward_frames < 2 && Self::follows(next, heading, 20.0) {
                continue;
            }
            let candidate = choose(Some(next))?;
            let Some(point) = candidate.aim else {
                continue;
            };
            let vector = [
                f64::from(point[0]) - view.context.anchor[0],
                f64::from(point[1]) - view.context.anchor[1],
            ];
            let along = projection(vector, next);
            if !Self::follows(vector, next, 40.0) {
                continue;
            }
            let goal = projection(vector, heading);
            if along >= minimum {
                if exits.as_ref().is_none_or(|e| goal > e.0) {
                    exits = Some((goal, candidate, next));
                }
            } else if along >= geometry.clearance_pixels * 2.0
                && probes.as_ref().is_none_or(|e| goal > e.0)
            {
                probes = Some((goal, candidate, next));
            }
        }
        let probing = exits.is_none() && probes.is_some();
        if let Some((_, mut candidate, next)) = exits.or(probes) {
            self.detour = Some(next);
            self.forward_frames = 0;
            self.reason = if probing {
                "observed_corner_probe"
            } else {
                "observed_corner_turn"
            }
            .into();
            candidate.reason = self.reason.clone();
            return Ok(candidate);
        }
        self.reason = "bend_continuation_unobserved".into();
        Ok(PathChoice::unknown(&self.reason))
    }
}
fn projection(d: [f64; 2], h: [f64; 2]) -> f64 {
    (d[0] * h[0] + d[1] * h[1]) / h[0].hypot(h[1])
}
#[derive(Debug, Serialize)]
pub struct ContinuousSteering {
    #[serde(skip)]
    reference_seconds: f64,
    #[serde(skip)]
    clearance_pixels: f64,
    aim: Option<[i32; 2]>,
    steered_at: Option<f64>,
    observed_at: Option<f64>,
    pending: Option<[f64; 2]>,
}
impl ContinuousSteering {
    pub fn new(
        reference_seconds: f64,
        clearance_pixels: f64,
        aim: Option<[i32; 2]>,
    ) -> Result<Self, ObservationError> {
        time::positive(reference_seconds, "reference_seconds")?;
        time::positive(clearance_pixels, "clearance_pixels")?;
        Ok(Self {
            reference_seconds,
            clearance_pixels,
            aim,
            steered_at: None,
            observed_at: None,
            pending: None,
        })
    }
    pub fn aim(&self) -> Option<[i32; 2]> {
        self.aim
    }
    /// 현재 조준만 폐기한다. 기존 관찰/시전 시각과 방향 대기는 유지한다.
    pub fn clear_aim(&mut self) {
        self.aim = None;
    }
    pub fn update(
        &mut self,
        view: &PathView,
        choice: &PathChoice,
        now: f64,
    ) -> Result<(bool, &'static str), ObservationError> {
        let Some(aim) = choice.aim.filter(|_| {
            self.observed_at
                .is_none_or(|t| view.context.captured_at > t)
        }) else {
            return Ok((false, "no_new_path"));
        };
        time::nonnegative(now, "now")?;
        self.observed_at = Some(view.context.captured_at);
        let direction = [
            f64::from(aim[0]) - view.context.anchor[0],
            f64::from(aim[1]) - view.context.anchor[1],
        ];
        let mut reason = "cast_reference";
        let mut changed = self.aim.is_none();
        if let Some(previous) = self.aim {
            let supported = floor_supports_aim(view, previous, self.clearance_pixels)?;
            let vector = [
                f64::from(previous[0]) - view.context.anchor[0],
                f64::from(previous[1]) - view.context.anchor[1],
            ];
            let turning = angle(vector, direction) >= 35.0;
            if !supported
                && angle(vector, direction) >= 150.0
                && self.pending.is_none_or(|p| angle(p, direction) >= 20.0)
            {
                self.pending = Some(direction);
                return Ok((false, "opposite_floor_recheck"));
            }
            if !supported {
                changed = true;
                reason = "previous_floor_closed";
            } else if turning && self.pending.is_some_and(|p| angle(p, direction) < 20.0) {
                changed = true;
                reason = "bend_confirmed_twice";
            } else if self
                .steered_at
                .is_none_or(|t| now - t >= self.reference_seconds)
            {
                changed = true;
            }
            self.pending = turning.then_some(direction);
        }
        if changed {
            self.aim = Some(aim);
            self.steered_at = Some(now);
            self.pending = None;
        }
        Ok((changed, if changed { reason } else { "hold_current_aim" }))
    }
}

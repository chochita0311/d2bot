use super::*;
struct Candidate {
    point: [f64; 2],
    distance: f64,
    score: f64,
    accepted: bool,
    detour: bool,
}
pub(super) fn choose(
    observer: &ArcanePathObserver,
    view: &PathView,
    floor: &GrayFrame,
    wing: Wing,
    geometry: TeleportGeometry,
    options: ChooseOptions<'_>,
) -> Result<PathChoice, ObservationError> {
    let mut clearance = floor_clearance(floor)?;
    for v in clearance.pixels_mut() {
        *v *= view.cell_pixels as f32;
    }
    let mut heading = options.heading.unwrap_or_else(|| wing.heading());
    let norm = heading[0].hypot(heading[1]);
    if !norm.is_finite() || norm == 0.0 {
        return Err(ObservationError::invalid(
            "finite nonzero steering direction required",
        ));
    }
    heading = heading.map(|v| v / norm);
    let mut frontier = None;
    let mut labels = None;
    let width = floor.size().width();
    if options.prefer_frontier && options.destination.is_none() {
        let components = components::connected_components(floor.view(), Connectivity::Four)?;
        let mut nearest: Option<(f64, u32)> = None;
        for (index, label) in components.labels.pixels().iter().enumerate() {
            if *label == 0 {
                continue;
            }
            let distance = distance(
                view.point([index % width, index / width]),
                view.context.anchor,
            );
            if nearest.is_none_or(|(d, l)| distance < d || (distance == d && *label < l)) {
                nearest = Some((distance, *label));
            }
        }
        if let Some((_, label)) = nearest.filter(|(d, _)| *d <= observer.settings.seed_radius) {
            let edge = (geometry.clearance_pixels / view.cell_pixels as f64).ceil() + 1.0;
            let mut exit: Option<[f64; 4]> = None;
            for (index, current) in components.labels.pixels().iter().enumerate() {
                if *current != label {
                    continue;
                }
                let [x, y] = [index % width, index / width];
                if x.min(y)
                    .min(width - 1 - x)
                    .min(floor.size().height() - 1 - y) as f64
                    > edge
                    || f64::from(clearance.pixels()[index]) < geometry.clearance_pixels
                {
                    continue;
                }
                let point = view.point([x, y]);
                let [dx, dy] = [
                    point[0] - view.context.anchor[0],
                    point[1] - view.context.anchor[1],
                ];
                let distance = dx.hypot(dy);
                let alignment = if distance == 0.0 {
                    -1.0
                } else {
                    (dx * heading[0] + dy * heading[1]) / distance
                };
                if distance >= 200.0 && alignment > 150.0_f64.to_radians().cos() {
                    let candidate = [alignment, distance, dx, dy];
                    if exit.is_none_or(|e| lex_greater(candidate, e)) {
                        exit = Some(candidate);
                    }
                }
            }
            if let Some([_, distance, dx, dy]) = exit {
                heading = [dx / distance, dy / distance];
                frontier = Some(label);
            }
        }
        labels = Some(components.labels);
    }
    let [hx, hy] = heading;
    let mut candidates = Vec::new();
    for (index, clearance) in clearance.pixels().iter().enumerate() {
        if *clearance < geometry.clearance_pixels as f32
            || frontier.is_some_and(|l| {
                labels
                    .as_ref()
                    .is_none_or(|labels| labels.pixels()[index] != l)
            })
        {
            continue;
        }
        let point = view.point([index % width, index / width]);
        let vector = [
            point[0] - view.context.anchor[0],
            point[1] - view.context.anchor[1],
        ];
        let [dx, dy] = vector;
        let distance = (dx * dx + dy * dy).sqrt();
        let world = [dx + options.position[0], dy + options.position[1]];
        let mut eligible =
            distance > view.cell_pixels as f64 && distance <= geometry.maximum_distance;
        let (score, accepted, detour) = if let Some(destination) = options.destination {
            (
                -((world[0] - destination[0]).powi(2) + (world[1] - destination[1]).powi(2)).sqrt()
                    - 0.05 * distance,
                eligible,
                false,
            )
        } else {
            for old in options.visited {
                eligible &= (world[0] - old[0]).powi(2) + (world[1] - old[1]).powi(2)
                    >= observer.settings.revisit_radius.powi(2);
            }
            let advance = dx * hx + dy * hy;
            let sideways = (dx * hy - dy * hx).abs();
            (
                advance
                    - if options.heading.is_some() {
                        0.75
                    } else {
                        0.12
                    } * sideways,
                eligible && advance > view.cell_pixels as f64,
                options.allow_detour
                    && eligible
                    && advance <= view.cell_pixels as f64
                    && advance > distance * 150.0_f64.to_radians().cos(),
            )
        };
        candidates.push(Candidate {
            point,
            distance,
            score,
            accepted,
            detour,
        });
    }
    let detouring = !candidates.iter().any(|c| c.accepted) && candidates.iter().any(|c| c.detour);
    if detouring {
        let minimum = geometry
            .maximum_distance
            .min(200.0_f64.max(geometry.preferred_distance.unwrap_or(240.0) * 0.5));
        let wide = candidates.iter().any(|c| c.detour && c.distance >= minimum);
        for c in &mut candidates {
            c.accepted = c.detour && (!wide || c.distance >= minimum);
            let [dx, dy] = [
                c.point[0] - view.context.anchor[0],
                c.point[1] - view.context.anchor[1],
            ];
            c.score = dx * hx + dy * hy - 0.12 * (dx * hy - dy * hx).abs();
        }
    }
    let preferred = geometry
        .preferred_distance
        .unwrap_or(geometry.maximum_distance);
    let nearby = candidates
        .iter()
        .any(|c| c.accepted && c.distance <= preferred);
    let mut selected: Option<&Candidate> = None;
    for c in &candidates {
        if !c.accepted || (nearby && c.distance > preferred) {
            continue;
        }
        if selected.is_none_or(|b| {
            if nearby {
                c.score > b.score
            } else {
                c.distance < b.distance || (c.distance == b.distance && c.score > b.score)
            }
        }) {
            selected = Some(c);
        }
    }
    let Some(selected) = selected else {
        return Ok(PathChoice::unknown("teleport_floor_unobserved"));
    };
    Ok(PathChoice {
        aim: Some(selected.point.map(|v| v.round_ties_even() as i32)),
        reason: if frontier.is_some() {
            "observed_floor_frontier"
        } else if detouring {
            "observed_teleport_detour"
        } else {
            "observed_teleport_floor"
        }
        .into(),
        path: vec![selected.point],
    })
}
fn lex_greater(a: [f64; 4], b: [f64; 4]) -> bool {
    for (a, b) in a.into_iter().zip(b) {
        if a != b {
            return a > b;
        }
    }
    false
}

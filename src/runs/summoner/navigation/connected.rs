use super::*;
use std::{cmp::Ordering, collections::BinaryHeap};
struct Entry {
    cost: f64,
    x: usize,
    y: usize,
}
impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Entry {}
impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .total_cmp(&self.cost)
            .then(other.x.cmp(&self.x))
            .then(other.y.cmp(&self.y))
    }
}
pub(super) fn choose(
    observer: &ArcanePathObserver,
    view: &PathView,
    floor: &GrayFrame,
    wing: Wing,
    options: ChooseOptions<'_>,
) -> Result<PathChoice, ObservationError> {
    let size = floor.size();
    let width = size.width();
    let length = floor.pixels().len();
    let mut costs = vec![f64::INFINITY; length];
    let mut parents = vec![None; length];
    let mut order = Vec::new();
    let mut queue = BinaryHeap::new();
    let mut candidates = 0;
    for y in 0..size.height() {
        for x in 0..width {
            if floor.pixels()[y * width + x] == 0 {
                continue;
            }
            candidates += 1;
            let distance = distance(view.point([x, y]), view.context.anchor);
            if distance <= observer.settings.seed_radius {
                let cost = distance / view.cell_pixels as f64;
                costs[y * width + x] = cost;
                order.push(y * width + x);
                queue.push(Entry { cost, x, y });
            }
        }
    }
    if candidates == 0 {
        return Ok(PathChoice::unknown("floor_unobserved"));
    }
    if queue.is_empty() {
        return Ok(PathChoice::unknown("player_floor_unobserved"));
    }
    while let Some(Entry { cost, x, y }) = queue.pop() {
        if cost != costs[y * width + x] {
            continue;
        }
        for (dx, dy) in [
            (1isize, 0isize),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ] {
            let (Some(nx), Some(ny)) = (x.checked_add_signed(dx), y.checked_add_signed(dy)) else {
                continue;
            };
            if nx >= width || ny >= size.height() || floor.pixels()[ny * width + nx] == 0 {
                continue;
            }
            if dx != 0
                && dy != 0
                && (floor.pixels()[y * width + nx] == 0 || floor.pixels()[ny * width + x] == 0)
            {
                continue;
            }
            let next = cost + (dx as f64).hypot(dy as f64);
            let index = ny * width + nx;
            if next < costs[index] {
                if costs[index].is_infinite() {
                    order.push(index);
                }
                costs[index] = next;
                parents[index] = Some(y * width + x);
                queue.push(Entry {
                    cost: next,
                    x: nx,
                    y: ny,
                });
            }
        }
    }
    let heading = wing.heading();
    let norm = heading[0].hypot(heading[1]);
    let [hx, hy] = heading.map(|v| v / norm);
    let score = |index: usize| {
        let p = view.point([index % width, index / width]);
        let [dx, dy] = [p[0] - view.context.anchor[0], p[1] - view.context.anchor[1]];
        let world = [options.position[0] + dx, options.position[1] + dy];
        if let Some(destination) = options.destination {
            -distance(world, destination) - 0.05 * costs[index] * view.cell_pixels as f64
        } else {
            let repeated = options
                .visited
                .iter()
                .any(|v| distance(world, *v) < observer.settings.revisit_radius);
            dx * hx + dy * hy
                - 0.12 * (dx * hy - dy * hx).abs()
                - if repeated {
                    observer.settings.maximum_step
                } else {
                    0.0
                }
        }
    };
    let mut target = *order
        .first()
        .ok_or_else(|| ObservationError::invalid("path seed insertion missing"))?;
    let mut best = score(target);
    for index in &order[1..] {
        let value = score(*index);
        if value > best {
            target = *index;
            best = value;
        }
    }
    let point = view.point([target % width, target / width]);
    if options.destination.is_none()
        && (point[0] - view.context.anchor[0]) * hx + (point[1] - view.context.anchor[1]) * hy
            <= view.cell_pixels as f64
    {
        return Ok(PathChoice::unknown("no_observed_progress"));
    }
    let mut cells = vec![target];
    let mut current = target;
    while let Some(parent) = parents[current] {
        cells.push(parent);
        current = parent;
    }
    cells.reverse();
    let mut simplified = vec![cells[0]];
    let mut cursor = 0;
    while cursor + 1 < cells.len() {
        let following = (cursor + 1..cells.len())
            .rev()
            .find(|i| {
                line_visible(
                    floor,
                    [cells[cursor] % width, cells[cursor] / width],
                    [cells[*i] % width, cells[*i] / width],
                )
            })
            .ok_or_else(|| ObservationError::invalid("connected path lost its adjacent segment"))?;
        simplified.push(cells[following]);
        cursor = following;
    }
    let path: Vec<_> = simplified
        .iter()
        .map(|i| view.point([i % width, i / width]))
        .collect();
    let mut travelled = distance(view.context.anchor, path[0]);
    let mut aim = path[0];
    let mut reason = "connected_floor";
    if travelled >= observer.settings.maximum_step {
        return Ok(PathChoice {
            aim: None,
            reason: "step_too_short".into(),
            path,
        });
    }
    for i in 1..path.len() {
        let step = distance(path[i], path[i - 1]);
        let remaining = observer.settings.maximum_step - travelled;
        if step > remaining {
            aim = std::array::from_fn(|a| {
                path[i - 1][a] + (path[i][a] - path[i - 1][a]) * remaining / step
            });
            break;
        }
        travelled += step;
        aim = path[i];
        if i + 1 < path.len() {
            let before: [f64; 2] = std::array::from_fn(|a| path[i][a] - path[i - 1][a]);
            let after: [f64; 2] = std::array::from_fn(|a| path[i + 1][a] - path[i][a]);
            let cosine = (before[0] * after[0] + before[1] * after[1])
                / ((before[0] * before[0] + before[1] * before[1]).sqrt()
                    * (after[0] * after[0] + after[1] * after[1]).sqrt());
            if cosine.clamp(-1.0, 1.0).acos().to_degrees() >= observer.settings.turn_degrees {
                reason = "observed_bend";
                break;
            }
        }
    }
    Ok(PathChoice {
        aim: Some(aim.map(|v| v.round_ties_even() as i32)),
        reason: reason.into(),
        path,
    })
}
fn line_visible(floor: &GrayFrame, a: [usize; 2], b: [usize; 2]) -> bool {
    let count = 2 * a[0].abs_diff(b[0]).max(a[1].abs_diff(b[1])) + 1;
    let step = std::array::from_fn::<_, 2, _>(|i| (b[i] as f64 - a[i] as f64) / (count - 1) as f64);
    (0..count).all(|index| {
        let [x, y] = std::array::from_fn(|i| {
            if index + 1 == count {
                b[i]
            } else {
                (a[i] as f64 + index as f64 * step[i]).round_ties_even() as usize
            }
        });
        floor.pixels()[y * floor.size().width() + x] != 0
    })
}

use super::VisionError;
use crate::common::frame::{FloatMap, FrameError, GrayView, Image, PixelRect};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connectivity {
    Four,
    Eight,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Component {
    pub label: u32,
    pub rect: Option<PixelRect>,
    pub area: usize,
    pub centroid: Option<[f64; 2]>,
}

#[derive(Debug)]
pub struct Components {
    pub labels: Image<u32, 1>,
    /// Includes background label0, even when its area is zero.
    pub stats: Vec<Component>,
}

struct Accumulator {
    label: u32,
    area: usize,
    min: [usize; 2],
    max: [usize; 2],
    sum: [f64; 2],
}
impl Accumulator {
    fn new(label: u32) -> Self {
        Self {
            label,
            area: 0,
            min: [usize::MAX; 2],
            max: [0; 2],
            sum: [0.0; 2],
        }
    }
    fn add(&mut self, x: usize, y: usize) {
        self.area += 1;
        for (axis, value) in [x, y].into_iter().enumerate() {
            self.min[axis] = self.min[axis].min(value);
            self.max[axis] = self.max[axis].max(value);
            self.sum[axis] += value as f64;
        }
    }
    fn finish(self) -> Result<Component, VisionError> {
        let (rect, centroid) = if self.area == 0 {
            (None, None)
        } else {
            (
                Some(PixelRect::new(
                    self.min[0],
                    self.min[1],
                    self.max[0] + 1,
                    self.max[1] + 1,
                )?),
                Some([
                    self.sum[0] / self.area as f64,
                    self.sum[1] / self.area as f64,
                ]),
            )
        };
        Ok(Component {
            label: self.label,
            area: self.area,
            rect,
            centroid,
        })
    }
}

pub fn connected_components(
    source: GrayView<'_>,
    connectivity: Connectivity,
) -> Result<Components, VisionError> {
    let size = source.size();
    let count = size.width() * size.height();
    if count >= u32::MAX as usize {
        return Err(VisionError::Invalid("component label range exceeded"));
    }
    let mut labels = Image::<u32, 1>::zeros(size)?;
    let mut queue = Vec::new();
    queue
        .try_reserve_exact(count)
        .map_err(FrameError::Allocation)?;
    let mut stats = Vec::new();
    let mut background = Accumulator::new(0);
    for y in 0..size.height() {
        for (x, value) in source
            .row(y)
            .ok_or(VisionError::Invalid("missing component row"))?
            .iter()
            .enumerate()
        {
            if *value == 0 {
                background.add(x, y);
            }
        }
    }
    stats.push(background.finish()?);
    // OpenCV's default 8-neighbor labeling scans 2x2 blocks; 4-neighbor scans rows.
    let block = if connectivity == Connectivity::Eight {
        2
    } else {
        1
    };
    for by in (0..size.height()).step_by(block) {
        for bx in (0..size.width()).step_by(block) {
            for y in by..(by + block).min(size.height()) {
                for x in bx..(bx + block).min(size.width()) {
                    let start = y * size.width() + x;
                    if labels.pixels()[start] != 0
                        || source
                            .row(y)
                            .ok_or(VisionError::Invalid("missing component row"))?[x]
                            == 0
                    {
                        continue;
                    }
                    let label = u32::try_from(stats.len())
                        .map_err(|_| VisionError::Invalid("component label range exceeded"))?;
                    let mut accumulator = Accumulator::new(label);
                    queue.clear();
                    queue.push(start);
                    labels.pixels_mut()[start] = label;
                    let mut index = 0;
                    while index < queue.len() {
                        let pixel = queue[index];
                        index += 1;
                        let (px, py) = (pixel % size.width(), pixel / size.width());
                        accumulator.add(px, py);
                        for dy in -1isize..=1 {
                            for dx in -1isize..=1 {
                                if (dx == 0 && dy == 0)
                                    || (connectivity == Connectivity::Four && dx != 0 && dy != 0)
                                {
                                    continue;
                                }
                                let (nx, ny) = (px as isize + dx, py as isize + dy);
                                if nx < 0
                                    || ny < 0
                                    || nx >= size.width() as isize
                                    || ny >= size.height() as isize
                                {
                                    continue;
                                }
                                let (nx, ny) = (nx as usize, ny as usize);
                                let next = ny * size.width() + nx;
                                if labels.pixels()[next] == 0
                                    && source
                                        .row(ny)
                                        .ok_or(VisionError::Invalid("missing component row"))?[nx]
                                        != 0
                                {
                                    labels.pixels_mut()[next] = label;
                                    queue.push(next);
                                }
                            }
                        }
                    }
                    stats.push(accumulator.finish()?);
                }
            }
        }
    }
    Ok(Components { labels, stats })
}

/// Two-pass L2 chamfer distance with the same mask5 costs as the existing navigation.
pub fn distance_l2_mask5(source: GrayView<'_>) -> Result<FloatMap, VisionError> {
    let size = source.size();
    let mut output = FloatMap::zeros(size)?;
    for y in 0..size.height() {
        for (x, value) in source
            .row(y)
            .ok_or(VisionError::Invalid("missing distance row"))?
            .iter()
            .enumerate()
        {
            output.pixels_mut()[y * size.width() + x] = if *value == 0 { 0.0 } else { f32::MAX };
        }
    }
    let offsets = [
        (-1, 0, 1.0f32),
        (0, -1, 1.0),
        (-1, -1, 1.4),
        (1, -1, 1.4),
        (-2, -1, 2.1969),
        (2, -1, 2.1969),
        (-1, -2, 2.1969),
        (1, -2, 2.1969),
    ];
    for reverse in [false, true] {
        for row in 0..size.height() {
            let y = if reverse {
                size.height() - 1 - row
            } else {
                row
            };
            for column in 0..size.width() {
                let x = if reverse {
                    size.width() - 1 - column
                } else {
                    column
                };
                let index = y * size.width() + x;
                let mut best = output.pixels()[index];
                for (dx, dy, cost) in offsets {
                    let sign = if reverse { -1 } else { 1 };
                    let (nx, ny) = (x as isize + dx * sign, y as isize + dy * sign);
                    if nx >= 0
                        && ny >= 0
                        && nx < size.width() as isize
                        && ny < size.height() as isize
                    {
                        best = best
                            .min(output.pixels()[ny as usize * size.width() + nx as usize] + cost);
                    }
                }
                output.pixels_mut()[index] = best;
            }
        }
    }
    Ok(output)
}

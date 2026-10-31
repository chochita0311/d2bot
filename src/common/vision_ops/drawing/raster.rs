use super::tables;
use crate::common::frame::BgrFrame;
pub(super) const ONE: i64 = 1 << 16;
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Point {
    pub x: i64,
    pub y: i64,
}
impl Point {
    pub fn pixel(p: [i32; 2]) -> Self {
        Self {
            x: i64::from(p[0]) * ONE,
            y: i64::from(p[1]) * ONE,
        }
    }
}
pub(super) struct Canvas<'a> {
    frame: &'a mut BgrFrame,
    color: [u8; 3],
    width: i64,
    height: i64,
}
impl<'a> Canvas<'a> {
    pub fn new(frame: &'a mut BgrFrame, color: [u8; 3]) -> Self {
        let size = frame.size();
        Self {
            frame,
            color,
            width: size.width() as i64,
            height: size.height() as i64,
        }
    }
    fn point(&mut self, x: i64, y: i64, alpha: Option<i64>) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return;
        }
        let index = (y as usize * self.width as usize + x as usize) * 3;
        let pixel = &mut self.frame.pixels_mut()[index..index + 3];
        for (channel, color) in pixel.iter_mut().zip(self.color) {
            if let Some(a) = alpha {
                let mut value = i64::from(*channel);
                for _ in 0..2 {
                    value += ((i64::from(color) - value) * a + 127) >> 8;
                }
                *channel = value as u8;
            } else {
                *channel = color;
            }
        }
    }
    fn span(&mut self, y: i64, first: i64, last: i64) {
        if y < 0 || y >= self.height {
            return;
        }
        for x in first.max(0)..=last.min(self.width - 1) {
            self.point(x, y, None);
        }
    }
    pub fn disk(&mut self, center: Point, radius: i64) {
        let (cx, cy) = ((center.x + ONE / 2) >> 16, (center.y + ONE / 2) >> 16);
        let (mut error, mut dx, mut dy, mut plus, mut minus) = (0, radius, 0, 1, 2 * radius - 1);
        while dx >= dy {
            self.span(cy - dy, cx - dx, cx + dx);
            self.span(cy + dy, cx - dx, cx + dx);
            self.span(cy - dx, cx - dy, cx + dy);
            self.span(cy + dx, cx - dy, cx + dy);
            dy += 1;
            error += plus;
            plus += 2;
            if error > 0 {
                error -= minus;
                dx -= 1;
                minus -= 2;
            }
        }
    }
    pub fn polyline(&mut self, points: &[Point], aa: bool) {
        for (index, pair) in points.windows(2).enumerate() {
            self.thick(pair[0], pair[1], aa, if index == 0 { 3 } else { 2 }, false);
        }
    }
    pub fn thick(
        &mut self,
        mut first: Point,
        mut last: Point,
        aa: bool,
        flags: u8,
        integer_input: bool,
    ) {
        if integer_input
            && [first, last]
                .iter()
                .any(|p| p.x < 0 || p.y < 0 || p.x >= self.width * ONE || p.y >= self.height * ONE)
        {
            let mut a = Point {
                x: (first.x >> 16) + 2,
                y: (first.y >> 16) + 2,
            };
            let mut b = Point {
                x: (last.x >> 16) + 2,
                y: (last.y >> 16) + 2,
            };
            clip(self.width + 4, self.height + 4, &mut a, &mut b);
            first = Point {
                x: (a.x - 2) * ONE,
                y: (a.y - 2) * ONE,
            };
            last = Point {
                x: (b.x - 2) * ONE,
                y: (b.y - 2) * ONE,
            };
        }
        let dx = (first.x - last.x) as f64 / ONE as f64;
        let dy = (last.y - first.y) as f64 / ONE as f64;
        let length = dx * dx + dy * dy;
        if length > f64::EPSILON {
            let ratio = ONE as f64 / length.sqrt();
            let ox = (dy * ratio).round_ties_even() as i64;
            let oy = (dx * ratio).round_ties_even() as i64;
            self.convex(
                &[
                    Point {
                        x: first.x + ox,
                        y: first.y + oy,
                    },
                    Point {
                        x: first.x - ox,
                        y: first.y - oy,
                    },
                    Point {
                        x: last.x - ox,
                        y: last.y - oy,
                    },
                    Point {
                        x: last.x + ox,
                        y: last.y + oy,
                    },
                ],
                aa,
            );
        }
        for (index, point) in [first, last].into_iter().enumerate() {
            if flags & (index as u8 + 1) != 0 {
                if aa {
                    self.convex(&ellipse(point, ONE), true);
                } else {
                    self.disk(point, 1);
                }
            }
        }
    }
    fn line(&mut self, mut first: Point, mut last: Point, aa: bool) {
        if !clip(self.width * ONE, self.height * ONE, &mut first, &mut last) {
            return;
        }
        let dx = last.x - first.x;
        let dy = last.y - first.y;
        let horizontal = dx.abs() > dy.abs();
        let (major, minor) = if horizontal { (dx, dy) } else { (dy, dx) };
        if major < 0 {
            std::mem::swap(&mut first, &mut last);
        }
        let minor = if major < 0 { -minor } else { minor };
        let step = (minor * ONE) / (major.abs() | 1);
        if !aa {
            let count = if horizontal {
                (last.x - first.x) >> 16
            } else {
                (last.y - first.y) >> 16
            };
            self.point((last.x + ONE / 2) >> 16, (last.y + ONE / 2) >> 16, None);
            first.x += ONE / 2;
            first.y += ONE / 2;
            for _ in 0..=count {
                self.point(first.x >> 16, first.y >> 16, None);
                if horizontal {
                    first.x += ONE;
                    first.y += step;
                } else {
                    first.x += step;
                    first.y += ONE;
                }
            }
            return;
        }
        let (start, end) = if horizontal {
            (first.x, last.x + ONE)
        } else {
            (first.y, last.y + ONE)
        };
        let mut count = (end >> 16) - (start >> 16);
        let offset = -(start & (ONE - 1));
        if horizontal {
            first.y += ((step * offset) >> 16) + ONE / 2;
        } else {
            first.x += ((step * offset) >> 16) + ONE / 2;
        }
        let mut slope = (step >> 11) & 63;
        if step < 0 {
            slope ^= 63;
        }
        let slope = if slope & 32 != 0 {
            256
        } else {
            tables::SLOPE[slope as usize]
        };
        let i = (start >> 9) & 120;
        let j = (end >> 9) & 120;
        let mut endpoints = [0i64; 9];
        let t0 = slope << 7;
        let t1 = ((120 - i) | 4) * slope;
        let t2 = (j | 4) * slope;
        endpoints[8] = slope;
        endpoints[1] = (((((j - i) & 120) | 4) * slope) >> 8) & 511;
        endpoints[3] = endpoints[1];
        endpoints[2] = (t1 >> 8) & 511;
        endpoints[4] = (((((j - i) + 128) | 4) * slope) >> 8) & 511;
        endpoints[5] = ((t1 + t0) >> 8) & 511;
        endpoints[6] = (t2 >> 8) & 511;
        endpoints[7] = ((t2 + t0) >> 8) & 511;
        let mut position = start >> 16;
        let mut seen = 0;
        while count >= 0 {
            let secondary = if horizontal { first.y } else { first.x };
            let base = (secondary >> 16) - 1;
            let distance = (secondary >> 11) & 31;
            let a = ((i64::from(seen >= 2) + 1) & (seen | 2)) * 3;
            let b = (i64::from(count >= 2) + 1) & (count | 2);
            let correction = endpoints[(a + b) as usize];
            for (offset, index) in [distance + 32, distance, 63 - distance]
                .into_iter()
                .enumerate()
            {
                let alpha = ((correction * tables::FILTER[index as usize]) >> 8) & 255;
                if horizontal {
                    self.point(position, base + offset as i64, Some(alpha));
                } else {
                    self.point(base + offset as i64, position, Some(alpha));
                }
            }
            position += 1;
            seen += 1;
            count -= 1;
            if horizontal {
                first.y += step;
            } else {
                first.x += step;
            }
        }
    }
    fn convex(&mut self, points: &[Point], aa: bool) {
        if points.is_empty() {
            return;
        }
        let mut previous = points[points.len() - 1];
        for point in points {
            self.line(previous, *point, aa);
            previous = *point;
        }
        if points.len() < 3 {
            return;
        }
        let mut min_index = 0;
        for index in 1..points.len() {
            if points[index].y < points[min_index].y {
                min_index = index;
            }
        }
        let minimum = (points[min_index].y + ONE / 2) >> 16;
        let maximum = (points.iter().map(|p| p.y).max().unwrap_or(0) + ONE / 2) >> 16;
        let xmin = (points.iter().map(|p| p.x).min().unwrap_or(0) + ONE / 2) >> 16;
        let xmax = (points.iter().map(|p| p.x).max().unwrap_or(0) + ONE / 2) >> 16;
        if maximum < 0 || minimum >= self.height || xmax < 0 || xmin >= self.width {
            return;
        }
        #[derive(Clone, Copy)]
        struct Edge {
            index: usize,
            direction: usize,
            x: i64,
            dx: i64,
            end: i64,
        }
        let mut edge = [Edge {
            index: min_index,
            direction: 1,
            x: -ONE,
            dx: 0,
            end: minimum,
        }; 2];
        edge[1].direction = points.len() - 1;
        let mut remaining = points.len() as i64;
        let mut y = minimum;
        let maximum = maximum.min(self.height - 1);
        while y <= maximum {
            if !aa || y < maximum || y == minimum {
                for edge in &mut edge {
                    if y >= edge.end {
                        let mut from = edge.index;
                        let mut to = (from + edge.direction) % points.len();
                        while remaining > 0 {
                            remaining -= 1;
                            let end = (points[to].y + ONE / 2) >> 16;
                            if end > y {
                                edge.end = end;
                                edge.dx = ((points[to].x - points[from].x) * 2 + (end - y))
                                    / (2 * (end - y));
                                edge.x = points[from].x;
                                edge.index = to;
                                break;
                            }
                            from = to;
                            to = (to + edge.direction) % points.len();
                        }
                        // C++의 실패한 마지막 post-decrement도 종료 상태에 포함된다.
                        if edge.end <= y {
                            remaining -= 1;
                        }
                    }
                }
            }
            if remaining < 0 {
                break;
            }
            let (left, right) = if edge[0].x > edge[1].x {
                (1, 0)
            } else {
                (0, 1)
            };
            let first = (edge[left].x + if aa { ONE - 1 } else { ONE / 2 }) >> 16;
            let last = (edge[right].x + if aa { 0 } else { ONE / 2 }) >> 16;
            self.span(y, first, last);
            for e in &mut edge {
                e.x += e.dx;
            }
            y += 1;
        }
    }
}
pub(super) fn ellipse(center: Point, radius: i64) -> Vec<Point> {
    let rounded = (radius + ONE / 2) >> 16;
    let delta = if rounded < 3 {
        90
    } else if rounded < 10 {
        30
    } else if rounded < 15 {
        18
    } else {
        5
    };
    let mut points = Vec::with_capacity(360 / delta + 2);
    for angle in (0..360 + delta).step_by(delta) {
        let angle = angle.min(360);
        let x = center.x as f64 + radius as f64 * f64::from(tables::SIN[450 - angle]);
        let y = center.y as f64 + radius as f64 * f64::from(tables::SIN[angle]);
        let fixed = |v: f64| {
            let integer = (v / ONE as f64).round_ties_even() as i64 * ONE;
            integer + (v - integer as f64).round_ties_even() as i64
        };
        let point = Point {
            x: fixed(x),
            y: fixed(y),
        };
        if points.last() != Some(&point) {
            points.push(point);
        }
    }
    if points.len() <= 1 {
        points = vec![center; 2];
    }
    points
}
fn clip(width: i64, height: i64, first: &mut Point, last: &mut Point) -> bool {
    let (right, bottom) = (width - 1, height - 1);
    let code = |p: Point| {
        i32::from(p.x < 0)
            + i32::from(p.x > right) * 2
            + i32::from(p.y < 0) * 4
            + i32::from(p.y > bottom) * 8
    };
    let (mut a, mut b) = (code(*first), code(*last));
    if a & b == 0 && a | b != 0 {
        if a & 12 != 0 {
            let y = if a < 8 { 0 } else { bottom };
            first.x += ((y - first.y) as f64 * (last.x - first.x) as f64
                / (last.y - first.y) as f64) as i64;
            first.y = y;
            a = i32::from(first.x < 0) + i32::from(first.x > right) * 2;
        }
        if b & 12 != 0 {
            let y = if b < 8 { 0 } else { bottom };
            last.x += ((y - last.y) as f64 * (last.x - first.x) as f64 / (last.y - first.y) as f64)
                as i64;
            last.y = y;
            b = i32::from(last.x < 0) + i32::from(last.x > right) * 2;
        }
        if a & b == 0 && a | b != 0 {
            if a != 0 {
                let x = if a == 1 { 0 } else { right };
                first.y += ((x - first.x) as f64 * (last.y - first.y) as f64
                    / (last.x - first.x) as f64) as i64;
                first.x = x;
                a = 0;
            }
            if b != 0 {
                let x = if b == 1 { 0 } else { right };
                last.y += ((x - last.x) as f64 * (last.y - first.y) as f64
                    / (last.x - first.x) as f64) as i64;
                last.x = x;
                b = 0;
            }
        }
    }
    a | b == 0
}

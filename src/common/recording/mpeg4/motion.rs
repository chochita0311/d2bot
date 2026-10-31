//! 정수 화소 16x16 이동과 한 개 벡터의 표준 예측. 검색 범위·메모리는 화면 크기로 제한된다.
use super::picture::{Picture, Plane};
use crate::common::frame::Size;
use std::io;
pub(super) type Vector = [i16; 2];
pub(super) struct Vectors {
    width: usize,
    values: Vec<Vector>,
}
impl Vectors {
    pub fn new(width: usize, height: usize) -> io::Result<Self> {
        let mut values = Vec::new();
        let length = width * height;
        values.try_reserve_exact(length).map_err(io::Error::other)?;
        values.resize(length, [0, 0]);
        Ok(Self { width, values })
    }
    pub fn predict(&self, x: usize, y: usize) -> Vector {
        let neighbors = [
            (x > 0).then(|| self.values[y * self.width + x - 1]),
            (y > 0).then(|| self.values[(y - 1) * self.width + x]),
            (y > 0 && x + 1 < self.width).then(|| self.values[(y - 1) * self.width + x + 1]),
        ];
        if neighbors.iter().flatten().count() == 1 {
            return neighbors.into_iter().flatten().next().unwrap_or([0, 0]);
        }
        std::array::from_fn(|axis| {
            let mut values = neighbors.map(|n| n.unwrap_or([0, 0])[axis]);
            values.sort_unstable();
            values[1]
        })
    }
    pub fn store(&mut self, x: usize, y: usize, vector: Vector) {
        self.values[y * self.width + x] = vector;
    }
}
fn shifted(plane: &Plane, x: usize, y: usize, [dx, dy]: Vector) -> u8 {
    plane.sample(
        (x as isize + dx as isize).clamp(0, plane.width as isize - 1) as usize,
        (y as isize + dy as isize).clamp(0, plane.height as isize - 1) as usize,
    )
}
pub(super) fn global(current: &Picture, reference: &Picture, visible: Size) -> Vector {
    if visible.width() <= 32 || visible.height() <= 32 {
        return [0, 0];
    }
    let a = &current.planes[0];
    let b = &reference.planes[0];
    let score = |vector: Vector, ceiling: u64| {
        let mut error = 0u64;
        for y in (16..visible.height() - 16).step_by(32) {
            for x in (16..visible.width() - 16).step_by(32) {
                error += u64::from(a.sample(x, y).abs_diff(shifted(b, x, y, vector)));
            }
            if error > ceiling {
                break;
            }
        }
        error
    };
    let mut best = [0, 0];
    let mut error = score(best, u64::MAX);
    if error == 0 {
        return best;
    }
    for dy in -16..=15 {
        for dx in -16..=15 {
            let vector = [dx, dy];
            let next = score(vector, error);
            if next < error {
                best = vector;
                error = next;
            }
        }
    }
    best
}
pub(super) fn choose(
    current: &Plane,
    reference: &Plane,
    visible: Size,
    x: usize,
    y: usize,
    global: Vector,
    predicted: Vector,
) -> Vector {
    let score = |vector: Vector| {
        let [dx, dy] = vector;
        if vector != [0, 0]
            && (!(-16..=15).contains(&dx)
                || !(-16..=15).contains(&dy)
                || x as isize + (dx as isize) < 0
                || y as isize + (dy as isize) < 0
                || x as isize + dx as isize + 16 > visible.width() as isize
                || y as isize + dy as isize + 16 > visible.height() as isize)
        {
            return u32::MAX;
        }
        let mut error = u32::from(dx.abs_diff(predicted[0]) + dy.abs_diff(predicted[1])) * 2;
        // 0 벡터는 padded block 안에 있고 나머지는 위에서 visible 경계를 확인했다.
        // 행 슬라이스의 경계를 한 번 검증한 뒤 같은64개 화소만 비교한다.
        let rx = (x as isize + dx as isize) as usize;
        let ry = (y as isize + dy as isize) as usize;
        for row in (0..16).step_by(2) {
            let a = (y + row) * current.width + x;
            let b = (ry + row) * reference.width + rx;
            error += current.data[a..a + 16]
                .iter()
                .step_by(2)
                .zip(reference.data[b..b + 16].iter().step_by(2))
                .map(|(a, b)| u32::from(a.abs_diff(*b)))
                .sum::<u32>()
                * 4;
        }
        error
    };
    let mut best = [0, 0];
    let mut error = score(best);
    for (index, vector) in [global, predicted].into_iter().enumerate() {
        if vector == [0, 0] || (index == 1 && vector == global) {
            continue;
        }
        let next = score(vector);
        if next < error {
            best = vector;
            error = next;
        }
    }
    if error <= 64 {
        return best;
    }
    let center = best;
    for dy in -1..=1 {
        for dx in -1..=1 {
            let vector = [center[0] + dx, center[1] + dy];
            if [[0, 0], global, predicted].contains(&vector) {
                continue;
            }
            let next = score(vector);
            if next < error {
                best = vector;
                error = next;
            }
        }
    }
    best
}
pub(super) fn block(plane: &Plane, x: usize, y: usize, vector: Vector, chroma: bool) -> [u8; 64] {
    if vector == [0, 0] {
        return plane.block(x, y);
    }
    if !chroma {
        let sx = x as isize + vector[0] as isize;
        let sy = y as isize + vector[1] as isize;
        if sx >= 0 && sy >= 0 && sx + 8 <= plane.width as isize && sy + 8 <= plane.height as isize {
            return plane.block(sx as usize, sy as usize);
        }
        return std::array::from_fn(|n| shifted(plane, x + n % 8, y + n / 8, vector));
    }
    let [dx, dy] = vector;
    let half_x = (dx & 1) != 0;
    let half_y = (dy & 1) != 0;
    let base = [dx >> 1, dy >> 1];
    let sx = x as isize + base[0] as isize;
    let sy = y as isize + base[1] as isize;
    if sx >= 0
        && sy >= 0
        && sx + 8 + isize::from(half_x) <= plane.width as isize
        && sy + 8 + isize::from(half_y) <= plane.height as isize
    {
        let a = plane.block(sx as usize, sy as usize);
        let b = if half_x {
            plane.block(sx as usize + 1, sy as usize)
        } else {
            a
        };
        let c = if half_y {
            plane.block(sx as usize, sy as usize + 1)
        } else {
            a
        };
        let d = if half_x && half_y {
            plane.block(sx as usize + 1, sy as usize + 1)
        } else {
            b
        };
        return std::array::from_fn(|n| match (half_x, half_y) {
            (false, false) => a[n],
            (true, false) => ((u16::from(a[n]) + u16::from(b[n]) + 1) >> 1) as u8,
            (false, true) => ((u16::from(a[n]) + u16::from(c[n]) + 1) >> 1) as u8,
            (true, true) => {
                ((u16::from(a[n]) + u16::from(b[n]) + u16::from(c[n]) + u16::from(d[n]) + 2) >> 2)
                    as u8
            }
        });
    }
    std::array::from_fn(|n| {
        let x = x + n % 8;
        let y = y + n / 8;
        let a = u16::from(shifted(plane, x, y, base));
        let value = match (half_x, half_y) {
            (false, false) => a,
            (true, false) => (a + u16::from(shifted(plane, x + 1, y, base)) + 1) >> 1,
            (false, true) => (a + u16::from(shifted(plane, x, y + 1, base)) + 1) >> 1,
            (true, true) => {
                (a + u16::from(shifted(plane, x + 1, y, base))
                    + u16::from(shifted(plane, x, y + 1, base))
                    + u16::from(shifted(plane, x + 1, y + 1, base))
                    + 2)
                    >> 2
            }
        };
        value as u8
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn row_predictions_preserve_clamped_borders_and_every_motion_vector() -> io::Result<()> {
        let size = Size::new(33, 17).map_err(io::Error::other)?;
        let mut picture = Picture::blank(size)?;
        let mut state = 739u32;
        for plane in &mut picture.planes {
            for value in &mut plane.data {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                *value = (state >> 24) as u8;
            }
            for y in [0, plane.height - 8] {
                for x in [0, plane.width - 8] {
                    for dy in -16..=15 {
                        for dx in -16..=15 {
                            for chroma in [false, true] {
                                let expected = std::array::from_fn(|n| {
                                    let (x, y) = (x + n % 8, y + n / 8);
                                    if !chroma {
                                        return shifted(plane, x, y, [dx, dy]);
                                    }
                                    let base = [dx >> 1, dy >> 1];
                                    let a = u16::from(shifted(plane, x, y, base));
                                    (match (dx & 1 != 0, dy & 1 != 0) {
                                        (false, false) => a,
                                        (true, false) => {
                                            (a + u16::from(shifted(plane, x + 1, y, base)) + 1) >> 1
                                        }
                                        (false, true) => {
                                            (a + u16::from(shifted(plane, x, y + 1, base)) + 1) >> 1
                                        }
                                        (true, true) => {
                                            (a + u16::from(shifted(plane, x + 1, y, base))
                                                + u16::from(shifted(plane, x, y + 1, base))
                                                + u16::from(shifted(plane, x + 1, y + 1, base))
                                                + 2)
                                                >> 2
                                        }
                                    }) as u8
                                });
                                assert_eq!(
                                    block(plane, x, y, [dx, dy], chroma),
                                    expected,
                                    "{x},{y} vector{dx},{dy} chroma{chroma}"
                                );
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn scalar_choose(
        current: &Plane,
        reference: &Plane,
        visible: Size,
        x: usize,
        y: usize,
        global: Vector,
        predicted: Vector,
    ) -> Vector {
        let score = |v: Vector| {
            let [dx, dy] = v;
            if v != [0, 0]
                && (!(-16..=15).contains(&dx)
                    || !(-16..=15).contains(&dy)
                    || x as isize + (dx as isize) < 0
                    || y as isize + (dy as isize) < 0
                    || x as isize + dx as isize + 16 > visible.width() as isize
                    || y as isize + dy as isize + 16 > visible.height() as isize)
            {
                return u32::MAX;
            }
            let mut score = u32::from(dx.abs_diff(predicted[0]) + dy.abs_diff(predicted[1])) * 2;
            for row in (0..16).step_by(2) {
                for col in (0..16).step_by(2) {
                    score += u32::from(current.sample(x + col, y + row).abs_diff(shifted(
                        reference,
                        x + col,
                        y + row,
                        v,
                    ))) * 4;
                }
            }
            score
        };
        let mut best = [0, 0];
        let mut error = score(best);
        for v in [global, predicted] {
            let e = score(v);
            if e < error {
                best = v;
                error = e;
            }
        }
        if error <= 64 {
            return best;
        }
        let center = best;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let v = [center[0] + dx, center[1] + dy];
                let e = score(v);
                if e < error {
                    best = v;
                    error = e;
                }
            }
        }
        best
    }
    #[test]
    fn bounded_row_search_preserves_scalar_scores_ties_and_padded_edges() -> io::Result<()> {
        let mut state = 739u32;
        for (width, height) in [(64, 48), (65, 49), (33, 17)] {
            let visible = Size::new(width, height).map_err(io::Error::other)?;
            let mut a = Picture::blank(visible)?;
            let mut b = Picture::blank(visible)?;
            for plane in [&mut a.planes[0], &mut b.planes[0]] {
                for value in &mut plane.data {
                    state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                    *value = (state >> 24) as u8;
                }
            }
            for reference in [&a.planes[0], &b.planes[0]] {
                for y in (0..a.planes[0].height).step_by(16) {
                    for x in (0..a.planes[0].width).step_by(16) {
                        for global in [[0, 0], [-16, -16], [-1, 1], [15, 15]] {
                            for predicted in [[0, 0], [-16, -16], [1, -1], [15, 15], global] {
                                assert_eq!(
                                    choose(
                                        &a.planes[0],
                                        reference,
                                        visible,
                                        x,
                                        y,
                                        global,
                                        predicted
                                    ),
                                    scalar_choose(
                                        &a.planes[0],
                                        reference,
                                        visible,
                                        x,
                                        y,
                                        global,
                                        predicted
                                    )
                                );
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

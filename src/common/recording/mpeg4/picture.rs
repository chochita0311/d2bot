//! 패딩한 YUV420 평면은 인코더 소유다. 원본 BGR 버퍼나 캡처 메모리를 참조하지 않는다.
use crate::common::frame::{BgrFrame, Size};
use std::io;
pub(super) struct Plane {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
}
impl Plane {
    fn new(width: usize, height: usize) -> io::Result<Self> {
        let length = width
            .checked_mul(height)
            .ok_or_else(|| io::Error::other("MPEG-4 plane dimensions overflow"))?;
        let mut data = Vec::new();
        data.try_reserve_exact(length).map_err(io::Error::other)?;
        data.resize(length, 0);
        Ok(Self {
            width,
            height,
            data,
        })
    }
    pub fn sample(&self, x: usize, y: usize) -> u8 {
        self.data[y * self.width + x]
    }
    pub fn block(&self, x: usize, y: usize) -> [u8; 64] {
        let mut block = [0; 64];
        for (row, target) in block.as_chunks_mut::<8>().0.iter_mut().enumerate() {
            let start = (y + row) * self.width + x;
            target.copy_from_slice(&self.data[start..start + 8]);
        }
        block
    }
}
pub(super) struct PlaneBand<'a> {
    width: usize,
    first_row: usize,
    data: &'a mut [u8],
}
impl PlaneBand<'_> {
    pub fn store_bytes(&mut self, x: usize, y: usize, values: &[u8; 64]) {
        for (row, source) in values.as_chunks::<8>().0.iter().enumerate() {
            let start = (y - self.first_row + row) * self.width + x;
            self.data[start..start + 8].copy_from_slice(source);
        }
    }
    pub fn store(&mut self, x: usize, y: usize, values: &[f64; 64]) {
        for (n, value) in values.iter().enumerate() {
            self.data[(y - self.first_row + n / 8) * self.width + x + n % 8] =
                rounded_pixel(*value);
        }
    }
}

fn rounded_pixel(value: f64) -> u8 {
    let clipped = value.clamp(0.0, 255.0);
    let whole = clipped as u8;
    // 양수 반올림은 같은 정수/소수 비교로 처리한다. +0.5의 경계 재반올림도 피한다.
    whole + u8::from(clipped - f64::from(whole) >= 0.5)
}

pub(super) struct Picture {
    pub planes: [Plane; 3],
}
impl Picture {
    pub fn blank(size: Size) -> io::Result<Self> {
        let width = size.width().div_ceil(16) * 16;
        let height = size.height().div_ceil(16) * 16;
        Ok(Self {
            planes: [
                Plane::new(width, height)?,
                Plane::new(width / 2, height / 2)?,
                Plane::new(width / 2, height / 2)?,
            ],
        })
    }
    pub fn from_bgr(frame: &BgrFrame) -> io::Result<Self> {
        let size = frame.size();
        let mut image = Self::blank(size)?;
        image.update(frame)?;
        Ok(image)
    }
    pub fn update(&mut self, frame: &BgrFrame) -> io::Result<()> {
        let size = frame.size();
        if self.planes[0].width != size.width().div_ceil(16) * 16
            || self.planes[0].height != size.height().div_ceil(16) * 16
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "MPEG-4 source buffer dimensions differ",
            ));
        }
        let [luma, u, v] = &mut self.planes;
        // 유효 행은 연속 슬라이스로 변환하고, 경계 복제는 행/영상 끝에서만 한다.
        for (source, target) in frame
            .bytes()
            .chunks_exact(size.width() * 3)
            .zip(luma.data.chunks_exact_mut(luma.width))
        {
            for (pixel, value) in source
                .as_chunks::<3>()
                .0
                .iter()
                .zip(&mut target[..size.width()])
            {
                let [b, g, r] = pixel.map(i32::from);
                *value = (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16) as u8;
            }
            let edge = target[size.width() - 1];
            target[size.width()..].fill(edge);
        }
        let last = (size.height() - 1) * luma.width;
        for row in size.height()..luma.height {
            luma.data
                .copy_within(last..last + luma.width, row * luma.width);
        }
        let chroma = |sum: [i32; 3]| {
            let [b, g, r] = sum.map(|n| (n + 2) / 4);
            [
                (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128) as u8,
                (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128) as u8,
            ]
        };
        for (row, (u_row, v_row)) in u
            .data
            .chunks_exact_mut(u.width)
            .zip(v.data.chunks_exact_mut(v.width))
            .enumerate()
        {
            let source_row = |y: usize| {
                let start = y.min(size.height() - 1) * size.width() * 3;
                &frame.bytes()[start..start + size.width() * 3]
            };
            let top = source_row(2 * row);
            let bottom = source_row(2 * row + 1);
            for (((a, b), out_u), out_v) in top
                .as_chunks::<6>()
                .0
                .iter()
                .zip(bottom.as_chunks::<6>().0)
                .zip(&mut u_row[..size.width() / 2])
                .zip(&mut v_row[..size.width() / 2])
            {
                let sum = std::array::from_fn(|k| {
                    i32::from(a[k]) + i32::from(a[k + 3]) + i32::from(b[k]) + i32::from(b[k + 3])
                });
                let [cu, cv] = chroma(sum);
                *out_u = cu;
                *out_v = cv;
            }
            let edge = 3 * (size.width() - 1);
            let [cu, cv] = chroma(std::array::from_fn(|k| {
                2 * (i32::from(top[edge + k]) + i32::from(bottom[edge + k]))
            }));
            u_row[size.width() / 2..].fill(cu);
            v_row[size.width() / 2..].fill(cv);
        }
        Ok(())
    }
    pub fn same_pixels(&self, other: &Self) -> bool {
        self.planes
            .iter()
            .zip(&other.planes)
            .all(|(a, b)| a.width == b.width && a.height == b.height && a.data == b.data)
    }
    pub fn same_macroblock(&self, other: &Self, x: usize, y: usize) -> bool {
        self.planes
            .iter()
            .zip(&other.planes)
            .enumerate()
            .all(|(p, (a, b))| {
                let span = if p == 0 { 16 } else { 8 };
                (0..span).all(|row| {
                    let start = (y * span + row) * a.width + x * span;
                    a.data[start..start + span] == b.data[start..start + span]
                })
            })
    }
    pub fn bands_mut(&mut self, mb_rows: usize) -> impl Iterator<Item = [PlaneBand<'_>; 3]> {
        let [luma, u, v] = &mut self.planes;
        let widths = [luma.width, u.width, v.width];
        luma.data
            .chunks_mut(mb_rows * 16 * widths[0])
            .zip(u.data.chunks_mut(mb_rows * 8 * widths[1]))
            .zip(v.data.chunks_mut(mb_rows * 8 * widths[2]))
            .enumerate()
            .map(move |(index, ((luma, u), v))| {
                [
                    PlaneBand {
                        width: widths[0],
                        first_row: index * mb_rows * 16,
                        data: luma,
                    },
                    PlaneBand {
                        width: widths[1],
                        first_row: index * mb_rows * 8,
                        data: u,
                    },
                    PlaneBand {
                        width: widths[2],
                        first_row: index * mb_rows * 8,
                        data: v,
                    },
                ]
            })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn contiguous_color_conversion_preserves_odd_sizes_and_padding()
    -> Result<(), Box<dyn std::error::Error>> {
        use super::*;
        for (width, height) in [(1, 1), (2, 2), (17, 15), (64, 48), (65, 49), (1922, 1140)] {
            let size = Size::new(width, height)?;
            let mut frame = BgrFrame::zeros(size)?;
            let mut state = 739u32;
            for value in frame.pixels_mut() {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                *value = (state >> 24) as u8;
            }
            let converted = Picture::from_bgr(&frame)?;
            let pixel = |x: usize, y: usize| {
                let at = (y.min(height - 1) * width + x.min(width - 1)) * 3;
                std::array::from_fn::<_, 3, _>(|k| i32::from(frame.bytes()[at + k]))
            };
            for (plane_index, plane) in converted.planes.iter().enumerate() {
                for y in 0..plane.height {
                    for x in 0..plane.width {
                        let expected = if plane_index == 0 {
                            let [b, g, r] = pixel(x, y);
                            (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16).clamp(0, 255) as u8
                        } else {
                            let mut sum = [0; 3];
                            for dy in 0..2 {
                                for dx in 0..2 {
                                    let p = pixel(x * 2 + dx, y * 2 + dy);
                                    for k in 0..3 {
                                        sum[k] += p[k];
                                    }
                                }
                            }
                            let [b, g, r] = sum.map(|n| (n + 2) / 4);
                            if plane_index == 1 {
                                (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128).clamp(0, 255)
                                    as u8
                            } else {
                                (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128).clamp(0, 255) as u8
                            }
                        };
                        assert_eq!(
                            plane.sample(x, y),
                            expected,
                            "{width}x{height} plane{plane_index} at{x},{y}"
                        );
                    }
                }
            }
        }
        Ok(())
    }
    #[test]
    fn reconstructed_pixel_rounding_matches_scalar_at_half_boundaries() {
        for n in -2..258 {
            let value = n as f64 + 0.5;
            for sample in [value.next_down(), value, value.next_up()] {
                assert_eq!(
                    super::rounded_pixel(sample),
                    sample.round().clamp(0.0, 255.0) as u8
                );
            }
        }
        for value in [f64::NEG_INFINITY, f64::INFINITY, f64::NAN] {
            assert_eq!(
                super::rounded_pixel(value),
                value.round().clamp(0.0, 255.0) as u8
            );
        }
    }
}

//! 정규직교 8x8 DCT와 H.263 방식 양자화. 외부 코덱 호출은 없다.
use std::{f64::consts::PI, sync::OnceLock};
pub(super) const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];
fn matrix() -> &'static [[f64; 8]; 8] {
    static MATRIX: OnceLock<[[f64; 8]; 8]> = OnceLock::new();
    MATRIX.get_or_init(|| {
        std::array::from_fn(|k| {
            std::array::from_fn(|x| {
                if k == 0 {
                    1.0 / 8.0f64.sqrt()
                } else {
                    0.5 * ((2 * x + 1) as f64 * k as f64 * PI / 16.0).cos()
                }
            })
        })
    })
}
pub(super) fn forward(samples: &[f64; 64]) -> [f64; 64] {
    let m = matrix();
    let mut rows = [0.0; 64];
    let mut out = [0.0; 64];
    for (source, target) in samples
        .as_chunks::<8>()
        .0
        .iter()
        .zip(rows.as_chunks_mut::<8>().0)
    {
        *target = forward_axis(*source, m);
    }
    for x in 0..8 {
        let values = forward_axis(std::array::from_fn(|y| rows[y * 8 + x]), m);
        for (y, value) in values.into_iter().enumerate() {
            out[y * 8 + x] = value;
        }
    }
    out
}
fn forward_axis(input: [f64; 8], m: &[[f64; 8]; 8]) -> [f64; 8] {
    let sum: [f64; 4] = std::array::from_fn(|i| input[i] + input[7 - i]);
    let difference: [f64; 4] = std::array::from_fn(|i| input[i] - input[7 - i]);
    let a = sum[0] + sum[3];
    let b = sum[1] + sum[2];
    let c = sum[0] - sum[3];
    let d = sum[1] - sum[2];
    let mut output = [0.0; 8];
    output[0] = (a + b) * m[0][0];
    output[4] = (a - b) * m[4][0];
    output[2] = c * m[2][0] + d * m[2][1];
    output[6] = c * m[6][0] + d * m[6][1];
    for k in [1, 3, 5, 7] {
        output[k] = (0..4).map(|x| difference[x] * m[k][x]).sum();
    }
    output
}
fn inverse_axis(input: [f64; 8], m: &[[f64; 8]; 8]) -> [f64; 8] {
    // 짝/홀 주파수로 나눈 전치 변환. 수치 회귀와 독립 decoder의 품질 조건을 지킨다.
    let a = input[0] * m[0][0];
    let b = input[4] * m[4][0];
    let c = input[2] * m[2][0] + input[6] * m[6][0];
    let d = input[2] * m[2][1] + input[6] * m[6][1];
    let even = [a + b + c, a - b + d, a - b - d, a + b - c];
    let mut output = [0.0; 8];
    for x in 0..4 {
        let odd = [1, 3, 5, 7]
            .into_iter()
            .map(|k| input[k] * m[k][x])
            .sum::<f64>();
        output[x] = even[x] + odd;
        output[7 - x] = even[x] - odd;
    }
    output
}
pub(super) fn inverse(coefficients: &[i16; 64]) -> [f64; 64] {
    let m = matrix();
    let mut rows = [0.0; 64];
    let mut out = [0.0; 64];
    for (source, target) in coefficients
        .as_chunks::<8>()
        .0
        .iter()
        .zip(rows.as_chunks_mut::<8>().0)
    {
        *target = inverse_axis(source.map(f64::from), m);
    }
    for x in 0..8 {
        let values = inverse_axis(std::array::from_fn(|y| rows[y * 8 + x]), m);
        for (y, value) in values.into_iter().enumerate() {
            out[y * 8 + x] = value;
        }
    }
    out
}
pub(super) fn dc_scale(q: u8, luma: bool) -> i16 {
    let q = i16::from(q);
    if q < 5 {
        8
    } else if luma {
        if q < 9 {
            2 * q
        } else if q < 25 {
            q + 8
        } else {
            2 * q - 16
        }
    } else if q < 25 {
        (q + 13) / 2
    } else {
        q - 6
    }
}
pub(super) fn quantize(
    samples: &[f64; 64],
    q: u8,
    intra: bool,
    luma: bool,
) -> ([i16; 64], [i16; 64]) {
    let dct = forward(samples);
    let mut levels = [0; 64];
    let mut dequantized = [0; 64];
    for index in 0..64 {
        if intra && index == 0 {
            let scale = dc_scale(q, luma);
            levels[index] = (dct[index] / f64::from(scale))
                .round()
                .clamp(-2047.0, 2047.0) as i16;
            dequantized[index] =
                (i32::from(levels[index]) * i32::from(scale)).clamp(-2048, 2047) as i16;
        } else {
            let value = dct[index];
            let magnitude = if intra {
                value.abs() / (2.0 * f64::from(q))
            } else {
                (value.abs() - f64::from(q) / 2.0).max(0.0) / (2.0 * f64::from(q))
            };
            let level = (magnitude.floor().min(2047.0) as i16) * if value < 0.0 { -1 } else { 1 };
            levels[index] = level;
            if level != 0 {
                let absolute = (2 * i32::from(level.abs()) + 1) * i32::from(q)
                    - i32::from(q.is_multiple_of(2));
                dequantized[index] =
                    (absolute * i32::from(level.signum())).clamp(-2048, 2047) as i16;
            }
        }
    }
    (levels, dequantized)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn factored_transforms_match_direct_matrix_and_preserve_round_trip() {
        let m = matrix();
        let mut state = 739u32;
        for case in 0..600 {
            let input = std::array::from_fn(|i| {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                if case < 64 {
                    if i == case { 255.0 } else { 0.0 }
                } else if case < 68 {
                    [-255.0, 0.0, 255.0, 2047.0][case - 64]
                } else {
                    f64::from(((state >> 16) % 4096) as i32 - 2048)
                }
            });
            let direct_axis = |axis: [f64; 8], inverse: bool| {
                std::array::from_fn::<_, 8, _>(|i| {
                    (0..8)
                        .map(|k| axis[k] * if inverse { m[k][i] } else { m[i][k] })
                        .sum::<f64>()
                })
            };
            let mut rows = [0.0; 64];
            let mut expected = [0.0; 64];
            for (source, target) in input
                .as_chunks::<8>()
                .0
                .iter()
                .zip(rows.as_chunks_mut::<8>().0)
            {
                *target = direct_axis(*source, false);
            }
            for x in 0..8 {
                let column = direct_axis(std::array::from_fn(|y| rows[y * 8 + x]), false);
                for y in 0..8 {
                    expected[y * 8 + x] = column[y];
                }
            }
            let actual = forward(&input);
            for (a, b) in actual.into_iter().zip(expected) {
                assert!((a - b).abs() < 1e-9, "forward {a} vs {b}");
            }
            let coefficients = input.map(|x| x as i16);
            for (source, target) in coefficients
                .as_chunks::<8>()
                .0
                .iter()
                .zip(rows.as_chunks_mut::<8>().0)
            {
                *target = direct_axis(source.map(f64::from), true);
            }
            for x in 0..8 {
                let column = direct_axis(std::array::from_fn(|y| rows[y * 8 + x]), true);
                for y in 0..8 {
                    expected[y * 8 + x] = column[y];
                }
            }
            for (a, b) in inverse(&coefficients).into_iter().zip(expected) {
                assert!((a - b).abs() < 1e-9, "inverse {a} vs {b}");
            }
            for source in input.as_chunks::<8>().0 {
                for (a, b) in inverse_axis(forward_axis(*source, m), m)
                    .into_iter()
                    .zip(source)
                {
                    assert!((a - b).abs() < 1e-9, "roundtrip {a} vs {b}");
                }
            }
        }
    }
}

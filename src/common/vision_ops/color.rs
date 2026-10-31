use super::VisionError;
use crate::common::frame::{BgrView, GrayFrame, Image};

pub fn gray(source: BgrView<'_>) -> Result<GrayFrame, VisionError> {
    let mut output = GrayFrame::zeros(source.size())?;
    for y in 0..source.size().height() {
        let row = source
            .row(y)
            .ok_or(VisionError::Invalid("missing color row"))?;
        for (x, pixel) in row.as_chunks::<3>().0.iter().enumerate() {
            let [b, g, r] = [
                u32::from(pixel[0]),
                u32::from(pixel[1]),
                u32::from(pixel[2]),
            ];
            output.pixels_mut()[y * source.size().width() + x] =
                ((b * 3735 + g * 19235 + r * 9798 + 16384) >> 15) as u8;
        }
    }
    Ok(output)
}

/// OpenCV's uint8 hue convention: H in [0,180), S and V in [0,255].
pub fn hsv(source: BgrView<'_>) -> Result<Image<u8, 3>, VisionError> {
    let mut output = Image::zeros(source.size())?;
    // 분모는 uint8 범위이므로 같은 나눗셈·반올림 결과를 한 번만 만든다.
    static DIVISORS: std::sync::OnceLock<([i32; 256], [i32; 256])> = std::sync::OnceLock::new();
    let (saturation_divisors, hue_divisors) = DIVISORS.get_or_init(|| {
        (
            std::array::from_fn(|i| {
                if i == 0 {
                    0
                } else {
                    ((255.0 * 4096.0) / i as f64).round_ties_even() as i32
                }
            }),
            std::array::from_fn(|i| {
                if i == 0 {
                    0
                } else {
                    ((180.0 * 4096.0) / (6.0 * i as f64)).round_ties_even() as i32
                }
            }),
        )
    });
    for y in 0..source.size().height() {
        let row = source
            .row(y)
            .ok_or(VisionError::Invalid("missing color row"))?;
        for (x, pixel) in row.as_chunks::<3>().0.iter().enumerate() {
            let [b, g, r] = [
                i32::from(pixel[0]),
                i32::from(pixel[1]),
                i32::from(pixel[2]),
            ];
            let value = b.max(g).max(r);
            let diff = value - b.min(g).min(r);
            let (hue, saturation) = if diff == 0 {
                (0, 0)
            } else {
                let saturation_divisor = saturation_divisors[value as usize];
                let hue_divisor = hue_divisors[diff as usize];
                let raw_hue = if value == r {
                    g - b
                } else if value == g {
                    b - r + 2 * diff
                } else {
                    r - g + 4 * diff
                };
                let hue = (raw_hue * hue_divisor + 2048) >> 12;
                (
                    if hue < 0 { hue + 180 } else { hue },
                    (diff * saturation_divisor + 2048) >> 12,
                )
            };
            let start = (y * source.size().width() + x) * 3;
            output.pixels_mut()[start..start + 3].copy_from_slice(&[
                hue as u8,
                saturation as u8,
                value as u8,
            ]);
        }
    }
    Ok(output)
}

pub fn lab(source: BgrView<'_>) -> Result<Image<u8, 3>, VisionError> {
    let mut output = Image::zeros(source.size())?;
    let (gamma, nonlinear) = lab_tables();
    for y in 0..source.size().height() {
        let row = source
            .row(y)
            .ok_or(VisionError::Invalid("missing color row"))?;
        for (x, pixel) in row.as_chunks::<3>().0.iter().enumerate() {
            let [b, g, r] = pixel.map(|v| gamma[v as usize]);
            let fx = nonlinear[((b * 778 + g * 1541 + r * 1777 + 2048) >> 12) as usize];
            let fy = nonlinear[((b * 296 + g * 2929 + r * 871 + 2048) >> 12) as usize];
            let fz = nonlinear[((b * 3575 + g * 448 + r * 73 + 2048) >> 12) as usize];
            let values = [
                (296 * fy - (16 * 255 * 32768 + 50) / 100 + 16384) >> 15,
                (500 * (fx - fy) + 128 * 32768 + 16384) >> 15,
                (200 * (fy - fz) + 128 * 32768 + 16384) >> 15,
            ];
            let start = (y * source.size().width() + x) * 3;
            for (output, value) in output.pixels_mut()[start..start + 3].iter_mut().zip(values) {
                *output = value.clamp(0, 255) as u8;
            }
        }
    }
    Ok(output)
}

// 고정 OpenCV RGB2Lab_b의 Q12 행렬·Q15 비선형 표를 재현한다.
// 일반 실수 Lab를 마지막에 반올림하면 팔레트 경계 픽셀의 분류가 달라진다.
// 원본·저작권 고지: THIRD-PARTY-NOTICES.md.
fn lab_tables() -> &'static ([i32; 256], [i32; 3072]) {
    static TABLES: std::sync::OnceLock<([i32; 256], [i32; 3072])> = std::sync::OnceLock::new();
    TABLES.get_or_init(|| {
        let gamma = std::array::from_fn(|i| {
            let x = f64::from(i as f32 / 255.0_f32);
            let linear = if x <= 809.0 / 20000.0 {
                x / (323.0 / 25.0)
            } else {
                ((x + 11.0 / 200.0) / (1.0 + 11.0 / 200.0)).powf(12.0 / 5.0)
            } as f32;
            (2040.0_f32 * linear).round_ties_even() as i32
        });
        let nonlinear = std::array::from_fn(|i| {
            let x = (1.0_f32 / 2040.0_f32) * i as f32;
            let value = if x < 216.0_f32 / 24389.0_f32 {
                x.mul_add(841.0_f32 / 108.0_f32, 16.0_f32 / 116.0_f32)
            } else {
                lab_table_cbrt(x)
            };
            (32768.0_f32 * value).round_ties_even() as i32
        });
        (gamma, nonlinear)
    })
}

// 비선형 표의 양수·정규화된 입력에만 사용한다. 원본은 유리 근사 후
// f64 가수를 잘라 f32를 조립하므로 표준 cbrt의 최종 반올림과 다르다.
fn lab_table_cbrt(x: f32) -> f32 {
    let bits = x.to_bits();
    let exponent = ((bits >> 23) & 255) as i32 - 127;
    let mut shift = exponent % 3;
    if shift >= 0 {
        shift -= 3;
    }
    let output_exponent = (exponent - shift) / 3 - 1;
    let fraction =
        f64::from_bits((((shift + 1023) as u64) << 52) | (u64::from(bits & 0x007f_ffff) << 29));
    let [a, b, c, d, e, f, g, h, i] = [
        0x4046a09e6653ba70,
        0x406808f46c6116e0,
        0x405dca97439cae14,
        0x402add70d2827500,
        0x3fc4f15f83f55d2d,
        0x402d9e20660edb21,
        0x4062ff15c0285815,
        0x406510d06a8112ce,
        0x4040fecbc9e2c375,
    ]
    .map(f64::from_bits);
    let result = ((((a * fraction + b) * fraction + c) * fraction + d) * fraction + e)
        / ((((f * fraction + g) * fraction + h) * fraction + i) * fraction + 1.0);
    f32::from_bits(
        (((output_exponent + 127) as u32) << 23)
            + ((result.to_bits() & 0x000f_ffff_ffff_ffff) >> 29) as u32,
    )
}

pub fn in_range<const C: usize>(
    source: crate::common::frame::ImageView<'_, u8, C>,
    lower: [u8; C],
    upper: [u8; C],
) -> Result<GrayFrame, VisionError> {
    let mut output = GrayFrame::zeros(source.size())?;
    for y in 0..source.size().height() {
        let row = source
            .row(y)
            .ok_or(VisionError::Invalid("missing mask row"))?;
        for (x, pixel) in row.as_chunks::<C>().0.iter().enumerate() {
            output.pixels_mut()[y * source.size().width() + x] = if pixel
                .iter()
                .zip(lower.iter().zip(upper.iter()))
                .all(|(value, (lo, hi))| lo <= value && value <= hi)
            {
                255
            } else {
                0
            };
        }
    }
    Ok(output)
}

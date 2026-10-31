//! 이동 예측/최종 bitstream 순서는 유지하고 독립 변환/복원만 행 밴드로 나눈다.
use super::{
    bits::Bits,
    invalid, motion,
    picture::{Picture, PlaneBand},
    syntax, transform,
};
use crate::common::{compute, frame::Size};
use std::io;

pub(super) struct Macroblock {
    vector: motion::Vector,
    predicted: motion::Vector,
    unchanged: bool,
    pattern: u8,
    levels: [[i16; 64]; 6],
}
impl Default for Macroblock {
    fn default() -> Self {
        Self {
            vector: [0, 0],
            predicted: [0, 0],
            unchanged: false,
            pattern: 0,
            levels: [[0; 64]; 6],
        }
    }
}
pub(super) struct PictureCoding<'a> {
    pub current: &'a Picture,
    pub reference: Option<&'a Picture>,
    pub previous_input: Option<&'a Picture>,
    pub key: bool,
    pub q: u8,
    pub visible: Size,
}
struct DcGrid {
    width: usize,
    values: Vec<i16>,
}
impl DcGrid {
    fn new(width: usize, height: usize) -> io::Result<Self> {
        let mut values = Vec::new();
        values
            .try_reserve_exact(width * height)
            .map_err(io::Error::other)?;
        values.resize(width * height, 1024);
        Ok(Self { width, values })
    }
    fn predict(&mut self, x: usize, y: usize, quantized: i16, scale: i16) -> i32 {
        let a = if x > 0 {
            i32::from(self.values[y * self.width + x - 1])
        } else {
            1024
        };
        let b = if x > 0 && y > 0 {
            i32::from(self.values[(y - 1) * self.width + x - 1])
        } else {
            1024
        };
        let c = if y > 0 {
            i32::from(self.values[(y - 1) * self.width + x])
        } else {
            1024
        };
        let predicted = if (a - b).abs() < (b - c).abs() { c } else { a };
        self.values[y * self.width + x] =
            (i32::from(quantized) * i32::from(scale)).clamp(-2048, 2047) as i16;
        i32::from(quantized) - (predicted + i32::from(scale) / 2) / i32::from(scale)
    }
}
pub(super) fn position(x: usize, y: usize, block: usize) -> (usize, usize, usize) {
    if block < 4 {
        (0, x * 16 + (block % 2) * 8, y * 16 + (block / 2) * 8)
    } else {
        (block - 3, x * 8, y * 8)
    }
}
fn transform_band(
    context: &PictureCoding<'_>,
    blocks: &mut [Macroblock],
    mut output: [PlaneBand<'_>; 3],
    first_row: usize,
    width: usize,
) {
    for (index, block) in blocks.iter_mut().enumerate() {
        let (x, y) = (index % width, first_row + index / width);
        block.pattern = 0;
        for b in 0..6 {
            let (plane, bx, by) = position(x, y, b);
            let predicted = match context.reference.filter(|_| !context.key) {
                Some(reference) => {
                    motion::block(&reference.planes[plane], bx, by, block.vector, plane != 0)
                }
                None => [0; 64],
            };
            if block.unchanged {
                output[plane].store_bytes(bx, by, &predicted);
                continue;
            }
            let source = context.current.planes[plane].block(bx, by);
            let (values, dequantized) = if !context.key
                && source
                    .iter()
                    .zip(&predicted)
                    .all(|(a, b)| a.abs_diff(*b) <= context.q / 2)
            {
                ([0; 64], [0; 64])
            } else {
                let residual =
                    std::array::from_fn(|n| f64::from(source[n]) - f64::from(predicted[n]));
                transform::quantize(&residual, context.q, context.key, plane == 0)
            };
            block.levels[b] = values;
            if values
                .iter()
                .skip(usize::from(context.key))
                .any(|v| *v != 0)
            {
                block.pattern |= 1 << (5 - b);
            }
            if dequantized.iter().all(|v| *v == 0) {
                output[plane].store_bytes(bx, by, &predicted);
            } else {
                let inverse = transform::inverse(&dequantized);
                output[plane].store(
                    bx,
                    by,
                    &std::array::from_fn(|n| inverse[n] + f64::from(predicted[n])),
                );
            }
        }
    }
}
pub(super) fn encode_picture(
    bits: &mut Bits,
    context: PictureCoding<'_>,
    output: &mut Picture,
    blocks: &mut [Macroblock],
) -> io::Result<()> {
    let workers = if context.current.planes[0].width * context.current.planes[0].height >= 256_000 {
        compute::parallelism()
    } else {
        1
    };
    encode_with_workers(bits, context, output, blocks, workers)
}
fn encode_with_workers(
    bits: &mut Bits,
    context: PictureCoding<'_>,
    output: &mut Picture,
    blocks: &mut [Macroblock],
    workers: usize,
) -> io::Result<()> {
    let width = context.current.planes[0].width;
    let height = context.current.planes[0].height;
    let (mb_width, mb_height) = (width / 16, height / 16);
    if blocks.len() != mb_width * mb_height {
        return Err(invalid("MPEG-4 macroblock scratch dimensions differ"));
    }
    let reference = if context.key {
        None
    } else {
        Some(
            context
                .reference
                .ok_or_else(|| invalid("Missing reference"))?,
        )
    };
    let global = reference.map_or([0, 0], |r| {
        motion::global(context.current, r, context.visible)
    });
    let mut vectors = if context.key {
        None
    } else {
        Some(motion::Vectors::new(mb_width, mb_height)?)
    };
    for (index, block) in blocks.iter_mut().enumerate() {
        let (x, y) = (index % mb_width, index / mb_width);
        block.unchanged = !context.key
            && context
                .previous_input
                .is_some_and(|p| context.current.same_macroblock(p, x, y));
        block.predicted = vectors.as_ref().map_or([0, 0], |v| v.predict(x, y));
        block.vector = match reference.filter(|_| !block.unchanged) {
            Some(r) => motion::choose(
                &context.current.planes[0],
                &r.planes[0],
                context.visible,
                x * 16,
                y * 16,
                global,
                block.predicted,
            ),
            None => [0, 0],
        };
        if let Some(vectors) = &mut vectors {
            vectors.store(x, y, block.vector);
        }
    }
    let workers = workers.min(mb_height).max(1);
    let rows = mb_height.div_ceil(workers);
    if workers == 1 {
        for (index, (band, blocks)) in output
            .bands_mut(rows)
            .zip(blocks.chunks_mut(rows * mb_width))
            .enumerate()
        {
            transform_band(&context, blocks, band, index * rows, mb_width);
        }
    } else {
        compute::scope(|scope| {
            for (index, (band, blocks)) in output
                .bands_mut(rows)
                .zip(blocks.chunks_mut(rows * mb_width))
                .enumerate()
            {
                let context = &context;
                scope.spawn(move |_| transform_band(context, blocks, band, index * rows, mb_width));
            }
        })
        .map_err(|e| match e {
            compute::ComputeError::Start(e) => e,
            compute::ComputeError::Panicked => io::Error::other("MPEG-4 transform worker panicked"),
        })?;
    }
    let mut dc = if context.key {
        Some([
            DcGrid::new(width / 8, height / 8)?,
            DcGrid::new(width / 16, height / 16)?,
            DcGrid::new(width / 16, height / 16)?,
        ])
    } else {
        None
    };
    for (index, block) in blocks.iter().enumerate() {
        if !context.key && block.pattern == 0 && block.vector == [0, 0] {
            bits.bit(true);
            continue;
        }
        syntax::macroblock(
            bits,
            context.key,
            block.pattern,
            std::array::from_fn(|axis| (block.vector[axis] - block.predicted[axis]) * 2),
        );
        for (b, values) in block.levels.iter().enumerate() {
            if let Some(dc) = &mut dc {
                let (plane, x, y) = position(index % mb_width, index / mb_width, b);
                let difference = dc[plane].predict(
                    x / 8,
                    y / 8,
                    values[0],
                    transform::dc_scale(context.q, plane == 0),
                );
                syntax::dc(bits, difference, plane == 0)?;
            }
            if block.pattern & (1 << (5 - b)) != 0 {
                syntax::coefficients(bits, values, context.key);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::frame::BgrFrame;
    #[test]
    fn parallel_transform_bands_preserve_serial_bits_and_reconstructed_pixels()
    -> Result<(), Box<dyn std::error::Error>> {
        let size = Size::new(640, 480)?;
        let mut frame = BgrFrame::zeros(size)?;
        let mut state = 739u32;
        for pixel in frame.pixels_mut() {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            *pixel = (state >> 24) as u8;
        }
        let previous = Picture::from_bgr(&frame)?;
        let mut reference = Picture::blank(size)?;
        let count = previous.planes[0].width / 16 * (previous.planes[0].height / 16);
        let mut serial_blocks: Vec<_> = (0..count).map(|_| Macroblock::default()).collect();
        let mut parallel_blocks: Vec<_> = (0..count).map(|_| Macroblock::default()).collect();
        let _compute = compute::ComputeLease::acquire()?;
        for key in [true, false] {
            let current = Picture::from_bgr(&frame)?;
            let mut serial = Picture::blank(size)?;
            let mut parallel = Picture::blank(size)?;
            let mut a = Bits::new(8 * 1024 * 1024)?;
            let mut b = Bits::new(8 * 1024 * 1024)?;
            let context = || PictureCoding {
                current: &current,
                reference: Some(&reference),
                previous_input: Some(&previous),
                key,
                q: 4,
                visible: size,
            };
            encode_with_workers(&mut a, context(), &mut serial, &mut serial_blocks, 1)?;
            encode_with_workers(&mut b, context(), &mut parallel, &mut parallel_blocks, 8)?;
            assert_eq!(a.bytes()?, b.bytes()?);
            assert!(serial.same_pixels(&parallel));
            reference = serial;
            for y in 24..80 {
                for x in 16..96 {
                    frame.pixels_mut()[(y * size.width() + x) * 3..(y * size.width() + x) * 3 + 3]
                        .fill(173);
                }
            }
        }
        Ok(())
    }
}

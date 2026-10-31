//! 직사각형 8비트 I/P VOP 구문과 표준 VLC만 사용한다.
use super::{bits::Bits, tables, transform::ZIGZAG};
use crate::common::frame::Size;
use std::{io, sync::OnceLock};
type Code = (u16, u8);
struct AcTables {
    codes: [[[[Code; 28]; 64]; 2]; 2],
    max_level: [[[u8; 64]; 2]; 2],
    max_run: [[[u8; 28]; 2]; 2],
}
fn ac_tables() -> &'static AcTables {
    static TABLES: OnceLock<AcTables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let mut result = AcTables {
            codes: [[[[(0, 0); 28]; 64]; 2]; 2],
            max_level: [[[0; 64]; 2]; 2],
            max_run: [[[0; 28]; 2]; 2],
        };
        for (intra, entries) in [(0, &tables::AC_INTER), (1, &tables::AC_INTRA)] {
            for &(last, run, level, code, bits) in entries {
                let last = usize::from(last);
                let run = usize::from(run);
                let level = usize::from(level);
                result.codes[intra][last][run][level] = (code, bits);
                result.max_level[intra][last][run] =
                    result.max_level[intra][last][run].max(level as u8);
                result.max_run[intra][last][level] =
                    result.max_run[intra][last][level].max(run as u8);
            }
        }
        result
    })
}
pub(super) fn vol(bits: &mut Bits, size: Size) {
    bits.start(0x20);
    bits.bit(false);
    bits.put(8, 1);
    bits.bit(false);
    bits.put(4, 1);
    bits.bit(true);
    bits.put(2, 1);
    bits.bit(true);
    bits.bit(false);
    bits.put(2, 0);
    bits.bit(true);
    bits.put(16, 8);
    bits.bit(true);
    bits.bit(false);
    bits.bit(true);
    bits.put(13, size.width() as u32);
    bits.bit(true);
    bits.put(13, size.height() as u32);
    bits.bit(true);
    bits.bit(false);
    bits.bit(true);
    bits.bit(false);
    bits.bit(false);
    bits.bit(false);
    bits.bit(true);
    bits.bit(true);
    bits.bit(false);
    bits.bit(false);
    bits.align();
}
pub(super) fn vop(bits: &mut Bits, index: u64, key: bool, q: u8) {
    bits.start(0xb6);
    bits.put(2, u32::from(!key));
    if index > 0 && index.is_multiple_of(8) {
        bits.bit(true);
    }
    bits.bit(false);
    bits.bit(true);
    bits.put(3, (index % 8) as u32);
    bits.bit(true);
    bits.bit(true);
    if !key {
        bits.bit(false);
    }
    bits.put(3, 0);
    bits.put(5, u32::from(q));
    if !key {
        bits.put(3, 1);
    }
}
pub(super) fn macroblock(bits: &mut Bits, key: bool, pattern: u8, motion: [i16; 2]) {
    if !key {
        bits.bit(false);
    }
    bits.code(if key {
        tables::MCBPC_I[usize::from(pattern & 3)]
    } else {
        tables::MCBPC_P[usize::from(pattern & 3)]
    });
    if key {
        bits.bit(false);
    }
    let cbpy = usize::from(pattern >> 2);
    bits.code(tables::CBPY[if key { cbpy } else { 15 - cbpy }]);
    if !key {
        for difference in motion {
            let value = (difference + 32).rem_euclid(64) - 32;
            bits.code(tables::MV[usize::from(value.unsigned_abs())]);
            if value != 0 {
                bits.bit(value < 0);
            }
        }
    }
}
pub(super) fn dc(bits: &mut Bits, value: i32, luma: bool) -> io::Result<()> {
    let size = (u32::BITS - value.unsigned_abs().leading_zeros()) as u8;
    let table = if luma {
        &tables::DC_LUMA
    } else {
        &tables::DC_CHROMA
    };
    bits.code(
        *table
            .get(usize::from(size))
            .ok_or_else(|| io::Error::other("MPEG-4 DC differential exceeds its VLC range"))?,
    );
    if size > 0 {
        let amplitude = if value < 0 {
            ((1u32 << size) - 1).wrapping_add_signed(value)
        } else {
            value as u32
        };
        bits.put(size, amplitude);
        if size > 8 {
            bits.bit(true);
        }
    }
    Ok(())
}
fn lookup(intra: usize, last: usize, run: usize, level: usize) -> Option<Code> {
    ac_tables().codes[intra][last]
        .get(run)?
        .get(level)
        .copied()
        .filter(|code| code.1 > 0)
}
fn coefficient(bits: &mut Bits, intra: bool, last: bool, run: u8, level: i16) {
    let mode = usize::from(intra);
    let ending = usize::from(last);
    let magnitude = usize::from(level.unsigned_abs());
    let run = usize::from(run);
    if let Some(code) = lookup(mode, ending, run, magnitude) {
        bits.code(code);
        bits.bit(level < 0);
        return;
    }
    let max = usize::from(ac_tables().max_level[mode][ending][run]);
    if let Some(code) = magnitude
        .checked_sub(max)
        .and_then(|value| lookup(mode, ending, run, value))
    {
        bits.put(7, 3);
        bits.bit(false);
        bits.code(code);
        bits.bit(level < 0);
        return;
    }
    if let Some(max) = ac_tables().max_run[mode][ending].get(magnitude)
        && let Some(code) = run
            .checked_sub(usize::from(*max) + 1)
            .and_then(|value| lookup(mode, ending, value, magnitude))
    {
        bits.put(7, 3);
        bits.put(2, 2);
        bits.code(code);
        bits.bit(level < 0);
        return;
    }
    bits.put(7, 3);
    bits.put(2, 3);
    bits.bit(last);
    bits.put(6, run as u32);
    bits.bit(true);
    bits.put(12, (i32::from(level) as u32) & 0xfff);
    bits.bit(true);
}
pub(super) fn coefficients(bits: &mut Bits, levels: &[i16; 64], intra: bool) {
    let mut values = [(0u8, 0i16); 64];
    let mut count = 0;
    let mut zeros = 0;
    for &index in ZIGZAG.iter().skip(usize::from(intra)) {
        let level = levels[index];
        if level == 0 {
            zeros += 1;
        } else {
            values[count] = (zeros, level);
            count += 1;
            zeros = 0;
        }
    }
    for (index, &(run, level)) in values[..count].iter().enumerate() {
        coefficient(bits, intra, index + 1 == count, run, level);
    }
}

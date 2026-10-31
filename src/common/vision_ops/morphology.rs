use super::VisionError;
use crate::common::frame::{GrayFrame, GrayView};

#[derive(Debug, Clone, Copy)]
pub enum Kernel {
    Rectangle { width: usize, height: usize },
    Cross3,
}

#[derive(Debug, Clone, Copy)]
pub enum Morphology {
    Erode,
    Dilate,
    Open,
    Close,
}

fn pass(
    source: GrayView<'_>,
    width: usize,
    vertical: bool,
    dilate: bool,
) -> Result<GrayFrame, VisionError> {
    if width == 0 || width > i32::MAX as usize {
        return Err(VisionError::Invalid(
            "positive native morphology size required",
        ));
    }
    let size = source.size();
    let mut output = GrayFrame::zeros(size)?;
    output.pixels_mut().fill(if dilate { 0 } else { 255 });
    // 커널 위치별로 연속 행을 합치면 경계의 중립값을 유지하면서 벡터 연산이 가능하다.
    // 정수 min/max는 순서와 무관하므로 기존 픽셀별 순회와 정확히 같다.
    let combine = |target: &mut [u8], row: &[u8]| {
        for (result, value) in target.iter_mut().zip(row) {
            *result = if dilate {
                (*result).max(*value)
            } else {
                (*result).min(*value)
            };
        }
    };
    for y in 0..size.height() {
        let target = &mut output.pixels_mut()[y * size.width()..(y + 1) * size.width()];
        for k in 0..width {
            let offset = k as isize - (width / 2) as isize;
            if vertical {
                let sy = y as isize + offset;
                if sy < 0 || sy >= size.height() as isize {
                    continue;
                }
                let row = source
                    .row(sy as usize)
                    .ok_or(VisionError::Invalid("missing morphology row"))?;
                combine(target, row);
            } else if offset.unsigned_abs() < size.width() {
                let row = source
                    .row(y)
                    .ok_or(VisionError::Invalid("missing morphology row"))?;
                if offset < 0 {
                    combine(
                        &mut target[offset.unsigned_abs()..],
                        &row[..size.width() - offset.unsigned_abs()],
                    );
                } else {
                    combine(
                        &mut target[..size.width() - offset as usize],
                        &row[offset as usize..],
                    );
                }
            }
        }
    }
    Ok(output)
}

fn basic(source: GrayView<'_>, kernel: Kernel, dilate: bool) -> Result<GrayFrame, VisionError> {
    match kernel {
        Kernel::Rectangle { width, height } => {
            let horizontal = pass(source, width, false, dilate)?;
            pass(horizontal.view(), height, true, dilate)
        }
        Kernel::Cross3 => {
            let mut horizontal = pass(source, 3, false, dilate)?;
            let vertical = pass(source, 3, true, dilate)?;
            for (value, other) in horizontal.pixels_mut().iter_mut().zip(vertical.bytes()) {
                *value = if dilate {
                    (*value).max(*other)
                } else {
                    (*value).min(*other)
                };
            }
            Ok(horizontal)
        }
    }
}

pub fn morphology(
    source: GrayView<'_>,
    kernel: Kernel,
    operation: Morphology,
) -> Result<GrayFrame, VisionError> {
    match operation {
        Morphology::Erode => basic(source, kernel, false),
        Morphology::Dilate => basic(source, kernel, true),
        Morphology::Open => basic(basic(source, kernel, false)?.view(), kernel, true),
        Morphology::Close => basic(basic(source, kernel, true)?.view(), kernel, false),
    }
}

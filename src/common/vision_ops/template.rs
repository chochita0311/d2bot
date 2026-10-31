use super::VisionError;
use crate::common::frame::{
    BgrView, FloatMap, FrameError, GrayFrame, GrayView, Image, ImageView, Size,
};
use rustfft::{Fft, FftNum, FftPlanner, num_complex::Complex};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, OnceLock},
};

pub trait MatchSample: Copy + Sync {
    /// true는 value()가 정확히 uint8 정수일 때만 허용한다.
    const BYTE: bool = false;
    fn value(self) -> f64;
    /// 정확한 정수 내적을 제공할 수 없는 타입은 기존 스칼라 경로를 사용한다.
    fn integer_dot(_source: &[Self], _pattern: &[Self]) -> Option<u64> {
        None
    }
}
impl MatchSample for u8 {
    const BYTE: bool = true;
    fn value(self) -> f64 {
        f64::from(self)
    }
    fn integer_dot(source: &[Self], pattern: &[Self]) -> Option<u64> {
        if source.len() != pattern.len() {
            return None;
        }
        u64::try_from(source.len()).ok()?.checked_mul(65_025)?;
        let mut total = 0_u64;
        // 한 블록의 전체 합도 i32에 들어간다: 16384 * 255 * 255 < 2^31.
        // 행/블록/채널을 재배치해도 uint8의 정확한 정수 내적은 변하지 않는다.
        for (a, b) in source.chunks(16_384).zip(pattern.chunks(16_384)) {
            let (a, tail_a) = a.as_chunks::<16>();
            let (b, tail_b) = b.as_chunks::<16>();
            let mut sum = wide::i32x4::ZERO;
            for (a, b) in a.iter().zip(b) {
                let a = wide::u8x16::new(*a);
                let b = wide::u8x16::new(*b);
                sum += wide::i16x8::from_u8x16_low(a).dot(wide::i16x8::from_u8x16_low(b));
                sum += wide::i16x8::from_u8x16_high(a).dot(wide::i16x8::from_u8x16_high(b));
            }
            total += u64::try_from(sum.reduce_add()).ok()?;
            total += tail_a
                .iter()
                .zip(tail_b)
                .map(|(&a, &b)| u64::from(a) * u64::from(b))
                .sum::<u64>();
        }
        Some(total)
    }
}
impl MatchSample for f32 {
    fn value(self) -> f64 {
        f64::from(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchMethod {
    Correlation,
    NormalizedCorrelation,
    NormalizedCoefficient,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Peak {
    pub score: f32,
    pub location: [usize; 2],
}

pub fn maximum(scores: &FloatMap) -> Option<Peak> {
    // 독립 누적기로 비교 의존성을 줄인 뒤 원본에서 첫 위치를 찾는다. max는 NaN을
    // 건너뛰고 두 번째 탐색이 동점 순서와 실제 픽셀의 signed zero까지 보존한다.
    let mut lanes = [f32::NEG_INFINITY; 8];
    let chunks = scores.pixels().as_chunks::<8>();
    for chunk in chunks.0 {
        for (maximum, value) in lanes.iter_mut().zip(chunk) {
            *maximum = maximum.max(*value);
        }
    }
    let best = lanes
        .into_iter()
        .chain(chunks.1.iter().copied())
        .fold(f32::NEG_INFINITY, f32::max);
    let index = scores.pixels().iter().position(|score| *score == best)?;
    Some(Peak {
        score: scores.pixels()[index],
        location: [index % scores.size().width(), index / scores.size().width()],
    })
}

fn zeros<T: Default + Clone>(len: usize) -> Result<Vec<T>, VisionError> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(len)
        .map_err(FrameError::Allocation)?;
    values.resize(len, T::default());
    Ok(values)
}

struct Statistics<const C: usize> {
    integral: Image<f64, C>,
    energy: Image<f64, 1>,
}
impl<const C: usize> Statistics<C> {
    fn new<T: MatchSample>(source: ImageView<'_, T, C>) -> Result<Self, VisionError> {
        let ss = source.size();
        let size = Size::new(ss.width() + 1, ss.height() + 1)?;
        let mut integral = Image::<f64, C>::zeros(size)?;
        let mut energy = Image::<f64, 1>::zeros(size)?;
        let stride = size.width();
        for y in 0..ss.height() {
            let row = source
                .row(y)
                .ok_or(VisionError::Invalid("missing correlation row"))?;
            let mut row_sum = [0.0; C];
            let mut row_energy = 0.0;
            for x in 0..ss.width() {
                for channel in 0..C {
                    let value = row[x * C + channel].value();
                    row_sum[channel] += value;
                    row_energy += value * value;
                    integral.pixels_mut()[((y + 1) * stride + x + 1) * C + channel] =
                        row_sum[channel] + integral.pixels()[(y * stride + x + 1) * C + channel];
                }
                energy.pixels_mut()[(y + 1) * stride + x + 1] =
                    row_energy + energy.pixels()[y * stride + x + 1];
            }
        }
        Ok(Self { integral, energy })
    }
}

/// 한 관찰 안에서 여러 템플릿이 공유하는 불변 회색 영상과 그 영상의 정확한 누적합.
/// 픽셀을 소유하고 수정 API를 제공하지 않으므로 다른 프레임의 통계를 섞을 수 없다.
pub struct GrayMatcher {
    image: GrayFrame,
    statistics: Statistics<1>,
}
impl GrayMatcher {
    pub fn new(image: GrayFrame) -> Result<Self, VisionError> {
        let statistics = Statistics::new(image.view())?;
        Ok(Self { image, statistics })
    }
    pub fn size(&self) -> Size {
        self.image.size()
    }
    pub fn match_pattern(
        &self,
        pattern: GrayView<'_>,
        method: MatchMethod,
    ) -> Result<FloatMap, VisionError> {
        match_with_statistics(self.image.view(), pattern, method, Some(&self.statistics))
    }
}

/// 한 scan의 불변 BGR 영역을 빌려 누적합을 공유한다. scan 밖에 픽셀을 보유하지 않는다.
pub struct BgrMatcher<'a> {
    image: BgrView<'a>,
    statistics: Statistics<3>,
}
impl<'a> BgrMatcher<'a> {
    pub fn new(image: BgrView<'a>) -> Result<Self, VisionError> {
        Ok(Self {
            image,
            statistics: Statistics::new(image)?,
        })
    }
    pub fn size(&self) -> Size {
        self.image.size()
    }
    pub fn match_pattern(
        &self,
        pattern: BgrView<'_>,
        method: MatchMethod,
    ) -> Result<FloatMap, VisionError> {
        match_with_statistics(self.image, pattern, method, Some(&self.statistics))
    }
}

type FftPlans<N> = [Arc<dyn Fft<N>>; 4];
struct CachedPlans<N: FftNum> {
    size: [usize; 2],
    transforms: FftPlans<N>,
}
fn fft_plans<N: FftNum>(
    width: usize,
    height: usize,
    cache: &Mutex<VecDeque<CachedPlans<N>>>,
) -> FftPlans<N> {
    // 최대 여덟 크기의 불변 알고리즘만 공유한다. 프레임·스펙트럼·scratch는 저장하지 않는다.
    if let Some(plans) = cache
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .iter()
        .find(|p| p.size == [width, height])
        .map(|p| p.transforms.clone())
    {
        return plans;
    }
    let mut planner = FftPlanner::new();
    let transforms = [
        planner.plan_fft_forward(width),
        planner.plan_fft_forward(height),
        planner.plan_fft_inverse(width),
        planner.plan_fft_inverse(height),
    ];
    let mut cache = cache.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(plans) = cache.iter().find(|p| p.size == [width, height]) {
        return plans.transforms.clone();
    }
    if cache.len() == 8 {
        cache.pop_front();
    }
    cache.push_back(CachedPlans {
        size: [width, height],
        transforms: transforms.clone(),
    });
    transforms
}

trait Spectral: FftNum + rustfft::num_traits::NumAssign + Default + Into<f64> {
    fn from_sample(value: f64) -> Self;
    fn plans(width: usize, height: usize) -> FftPlans<Self>;
}
impl Spectral for f32 {
    fn from_sample(value: f64) -> Self {
        value as f32
    }
    fn plans(width: usize, height: usize) -> FftPlans<Self> {
        static CACHE: OnceLock<Mutex<VecDeque<CachedPlans<f32>>>> = OnceLock::new();
        fft_plans(
            width,
            height,
            CACHE.get_or_init(|| Mutex::new(VecDeque::new())),
        )
    }
}
impl Spectral for f64 {
    fn from_sample(value: f64) -> Self {
        value
    }
    fn plans(width: usize, height: usize) -> FftPlans<Self> {
        static CACHE: OnceLock<Mutex<VecDeque<CachedPlans<f64>>>> = OnceLock::new();
        fft_plans(
            width,
            height,
            CACHE.get_or_init(|| Mutex::new(VecDeque::new())),
        )
    }
}

struct Fft2<N: Spectral> {
    x: Arc<dyn Fft<N>>,
    y: Arc<dyn Fft<N>>,
    inverse_x: Arc<dyn Fft<N>>,
    inverse_y: Arc<dyn Fft<N>>,
    transposed: Vec<Complex<N>>,
    scratch: Vec<Complex<N>>,
}
impl<N: Spectral> Fft2<N> {
    fn new(width: usize, height: usize) -> Result<Self, VisionError> {
        let [x, y, inverse_x, inverse_y] = N::plans(width, height);
        let scratch_len = [&x, &y, &inverse_x, &inverse_y]
            .into_iter()
            .map(|fft| fft.get_inplace_scratch_len())
            .max()
            .unwrap_or(0);
        Ok(Self {
            x,
            y,
            inverse_x,
            inverse_y,
            transposed: zeros(width * height)?,
            scratch: zeros(scratch_len)?,
        })
    }
    fn process(&mut self, values: &mut [Complex<N>], width: usize, inverse: bool) {
        let x = if inverse { &self.inverse_x } else { &self.x };
        let y = if inverse { &self.inverse_y } else { &self.y };
        // 행별 FFT는 묶어서 호출하고, 열은 연속 메모리로 전치해 같은 연산 순서를 유지한다.
        x.process_with_scratch(values, &mut self.scratch);
        let height = values.len() / width;
        transpose(values, &mut self.transposed, width, height);
        y.process_with_scratch(&mut self.transposed, &mut self.scratch);
        transpose(&self.transposed, values, height, width);
    }

    fn fork(&self) -> Result<Self, VisionError> {
        Ok(Self {
            x: self.x.clone(),
            y: self.y.clone(),
            inverse_x: self.inverse_x.clone(),
            inverse_y: self.inverse_y.clone(),
            transposed: zeros(self.transposed.len())?,
            scratch: zeros(self.scratch.len())?,
        })
    }
}

fn transpose<N: Copy>(source: &[Complex<N>], output: &mut [Complex<N>], w: usize, h: usize) {
    for top in (0..h).step_by(16) {
        for left in (0..w).step_by(16) {
            for y in top..(top + 16).min(h) {
                for x in left..(left + 16).min(w) {
                    output[x * h + y] = source[y * w + x];
                }
            }
        }
    }
}

struct CorrelationTiles<'a, T, N: Spectral, const C: usize> {
    source: ImageView<'a, T, C>,
    pattern: &'a [Complex<N>],
    size: Size,
    block: [usize; 2],
    step: [usize; 2],
    channel: usize,
}
impl<T: MatchSample, N: Spectral, const C: usize> CorrelationTiles<'_, T, N, C> {
    fn band(
        &self,
        output: &mut [f64],
        first_y: usize,
        fft: &mut Fft2<N>,
        tile: &mut [Complex<N>],
    ) -> Result<(), VisionError> {
        let [bw, bh] = self.block;
        let [step_x, step_y] = self.step;
        let rows = output.len() / self.size.width();
        for local_y in (0..rows).step_by(step_y) {
            let oy = first_y + local_y;
            for ox in (0..self.size.width()).step_by(step_x) {
                tile.fill(Complex::default());
                for y in 0..bh.min(self.source.size().height() - oy) {
                    let row = self
                        .source
                        .row(oy + y)
                        .ok_or(VisionError::Invalid("missing correlation row"))?;
                    for x in 0..bw.min(self.source.size().width() - ox) {
                        tile[y * bw + x].re =
                            N::from_sample(row[(ox + x) * C + self.channel].value());
                    }
                }
                fft.process(tile, bw, false);
                for (value, reference) in tile.iter_mut().zip(self.pattern) {
                    *value *= reference.conj();
                }
                fft.process(tile, bw, true);
                for y in 0..step_y.min(rows - local_y) {
                    for x in 0..step_x.min(self.size.width() - ox) {
                        output[(local_y + y) * self.size.width() + ox + x] +=
                            tile[y * bw + x].re.into() / tile.len() as f64;
                    }
                }
            }
        }
        Ok(())
    }
}

fn cross_correlation<T: MatchSample, const C: usize>(
    source: ImageView<'_, T, C>,
    template: ImageView<'_, T, C>,
    size: Size,
) -> Result<Vec<f64>, VisionError> {
    let mut output = zeros::<f64>(size.width() * size.height())?;
    let ts = template.size();
    let operations = size
        .width()
        .checked_mul(size.height())
        .and_then(|n| n.checked_mul(ts.width()))
        .and_then(|n| n.checked_mul(ts.height()))
        .and_then(|n| n.checked_mul(C));
    if operations.is_some_and(|operations| operations <= 4_000_000) {
        for y in 0..size.height() {
            for x in 0..size.width() {
                let mut sum = 0.0;
                let mut integer_sum = 0_u64;
                for ty in 0..ts.height() {
                    let source_row = source
                        .row(y + ty)
                        .ok_or(VisionError::Invalid("missing correlation row"))?;
                    let template_row = template
                        .row(ty)
                        .ok_or(VisionError::Invalid("missing template row"))?;
                    if T::BYTE
                        && let Some(dot) =
                            T::integer_dot(&source_row[x * C..(x + ts.width()) * C], template_row)
                    {
                        integer_sum += dot;
                        continue;
                    }
                    for (a, b) in source_row[x * C..(x + ts.width()) * C]
                        .iter()
                        .zip(template_row)
                    {
                        if T::BYTE {
                            // 직접 경로의 400만 연산 한도에서는 합이 2^53보다 작다.
                            // 정수 덧셈을 재배치해도 원래 f64 누적과 정확히 같다.
                            integer_sum += (a.value() as u64) * (b.value() as u64);
                        } else {
                            sum += a.value() * b.value();
                        }
                    }
                }
                output[y * size.width() + x] = if T::BYTE { integer_sum as f64 } else { sum };
            }
        }
        return Ok(output);
    }
    // 고정 OpenCV와 같이 uint8은 f32 DFT, 실수 입력은 f64 DFT를 쓴다.
    // 누적합·평균·정규화와 직접 경로의 정확한 정수 합은 계속 f64/u64다.
    if T::BYTE {
        fft_cross_correlation::<T, f32, C>(source, template, size, output)
    } else {
        fft_cross_correlation::<T, f64, C>(source, template, size, output)
    }
}

fn fft_cross_correlation<T: MatchSample, N: Spectral, const C: usize>(
    source: ImageView<'_, T, C>,
    template: ImageView<'_, T, C>,
    size: Size,
    mut output: Vec<f64>,
) -> Result<Vec<f64>, VisionError> {
    let ts = template.size();
    let block = |width: usize| {
        width
            .checked_mul(2)
            .and_then(usize::checked_next_power_of_two)
            .map(|n| n.max(128))
            .ok_or(VisionError::Invalid("FFT block dimensions overflow"))
    };
    let (bw, bh) = (block(ts.width())?, block(ts.height())?);
    let count = bw
        .checked_mul(bh)
        .ok_or(VisionError::Invalid("FFT block area overflow"))?;
    let (step_x, step_y) = (bw - ts.width() + 1, bh - ts.height() + 1);
    let mut fft = Fft2::<N>::new(bw, bh)?;
    let mut pattern = zeros::<Complex<N>>(count)?;
    let mut tile = zeros::<Complex<N>>(count)?;
    for channel in 0..C {
        pattern.fill(Complex::default());
        for y in 0..ts.height() {
            let row = template
                .row(y)
                .ok_or(VisionError::Invalid("missing template row"))?;
            for x in 0..ts.width() {
                pattern[y * bw + x].re = N::from_sample(row[x * C + channel].value());
            }
        }
        fft.process(&mut pattern, bw, false);
        let tiles = CorrelationTiles {
            source,
            pattern: &pattern,
            size,
            block: [bw, bh],
            step: [step_x, step_y],
            channel,
        };
        // 큰 현재 프레임만 공유 worker 수의 행 밴드로 분리한다. 출력은 겹치지 않고 모든 작업을
        // 반환 전에 join하므로 프레임 수명·채널 누적 순서·최댓값 동점 규칙은 그대로다.
        let workers = if output.len() >= 256_000 {
            crate::common::compute::parallelism().min(size.height().div_ceil(step_y))
        } else {
            1
        };
        if workers == 1 {
            tiles.band(&mut output, 0, &mut fft, &mut tile)?;
        } else {
            let band_rows = size.height().div_ceil(step_y).div_ceil(workers) * step_y;
            let mut results: Vec<_> = (0..workers).map(|_| Ok(())).collect();
            crate::common::compute::scope(|scope| -> Result<(), VisionError> {
                for ((index, band), result) in output
                    .chunks_mut(band_rows * size.width())
                    .enumerate()
                    .zip(&mut results)
                {
                    let context = &tiles;
                    let original_fft = &fft;
                    scope.spawn(move |_| {
                        *result = (|| {
                            let mut worker_fft = original_fft.fork()?;
                            let mut worker_tile = zeros(count)?;
                            context.band(band, index * band_rows, &mut worker_fft, &mut worker_tile)
                        })();
                    });
                }
                Ok(())
            })
            .map_err(VisionError::from)??;
            for result in results {
                result?;
            }
        }
    }
    Ok(output)
}

struct Normalization<'a, const C: usize> {
    size: Size,
    template_size: Size,
    statistics: &'a Statistics<C>,
    sums: [f64; C],
    template_norm: f64,
    n: f64,
    centered: bool,
}
impl<const C: usize> Normalization<'_, C> {
    fn band(&self, cross: &[f64], output: &mut [f32], first_row: usize) {
        let integral = &self.statistics.integral;
        let energy = &self.statistics.energy;
        let stride = integral.size().width();
        let width = self.size.width();
        for row in 0..output.len() / width {
            let y = first_row + row;
            for x in 0..width {
                let index = row * width + x;
                let mut numerator = cross[index];
                let corners = [
                    y * stride + x,
                    (y + self.template_size.height()) * stride + x + self.template_size.width(),
                    y * stride + x + self.template_size.width(),
                    (y + self.template_size.height()) * stride + x,
                ];
                let window_energy = energy.pixels()[corners[0]] + energy.pixels()[corners[1]]
                    - energy.pixels()[corners[2]]
                    - energy.pixels()[corners[3]];
                let mut mean_energy = 0.0;
                if self.centered {
                    for (channel, template_sum) in self.sums.iter().enumerate() {
                        let sum = integral.pixels()[corners[0] * C + channel]
                            + integral.pixels()[corners[1] * C + channel]
                            - integral.pixels()[corners[2] * C + channel]
                            - integral.pixels()[corners[3] * C + channel];
                        numerator -= sum * template_sum / self.n;
                        mean_energy += sum * sum / self.n;
                    }
                }
                let variance = (window_energy - mean_energy).max(0.0);
                let denominator =
                    if variance <= 0.5f64.min(10.0 * f64::from(f32::EPSILON) * window_energy) {
                        0.0
                    } else {
                        variance.sqrt() * self.template_norm
                    };
                let score = if numerator.abs() < denominator {
                    numerator / denominator
                } else if numerator.abs() < denominator * 1.125 {
                    numerator.signum()
                } else {
                    0.0
                };
                output[index] = score as f32;
            }
        }
    }
    fn apply(&self, cross: &[f64]) -> Result<FloatMap, VisionError> {
        let mut output = FloatMap::zeros(self.size)?;
        let workers = if output.pixels().len() >= 256_000 {
            crate::common::compute::parallelism().min(self.size.height())
        } else {
            1
        };
        if workers == 1 {
            self.band(cross, output.pixels_mut(), 0);
        } else {
            // FFT 작업이 모두 끝난 뒤 독립 픽셀의 정규화만 분산한다. 채널 합산 순서와
            // float32 저장 시점은 같으며 실패한 생성 경로에서도 시작한 작업을 모두 join한다.
            let rows = self.size.height().div_ceil(workers);
            let count = rows * self.size.width();
            crate::common::compute::scope(|scope| {
                for (index, band) in output.pixels_mut().chunks_mut(count).enumerate() {
                    let values = &cross[index * count..index * count + band.len()];
                    scope.spawn(move |_| self.band(values, band, index * rows));
                }
            })
            .map_err(VisionError::from)?;
        }
        Ok(output)
    }
}

pub fn match_template<T: MatchSample, const C: usize>(
    source: ImageView<'_, T, C>,
    template: ImageView<'_, T, C>,
    method: MatchMethod,
) -> Result<FloatMap, VisionError> {
    match_with_statistics(source, template, method, None)
}

fn match_with_statistics<T: MatchSample, const C: usize>(
    source: ImageView<'_, T, C>,
    template: ImageView<'_, T, C>,
    method: MatchMethod,
    statistics: Option<&Statistics<C>>,
) -> Result<FloatMap, VisionError> {
    let (ss, ts) = (source.size(), template.size());
    if ss.width() < ts.width() || ss.height() < ts.height() {
        return Err(VisionError::Invalid("template exceeds source dimensions"));
    }
    let size = Size::new(ss.width() - ts.width() + 1, ss.height() - ts.height() + 1)?;
    // BYTE 계약은 value()가 정확한 uint8임을 보장한다. 부동소수 영상은 계속 전부 검사한다.
    for y in 0..if T::BYTE { 0 } else { ss.height() } {
        if source
            .row(y)
            .ok_or(VisionError::Invalid("missing correlation row"))?
            .iter()
            .any(|value| !value.value().is_finite())
        {
            return Err(VisionError::Invalid("finite source samples required"));
        }
    }
    let n = (ts.width() * ts.height()) as f64;
    let mut sums = [0.0; C];
    let mut template_energy = 0.0;
    for y in 0..ts.height() {
        for (index, value) in template
            .row(y)
            .ok_or(VisionError::Invalid("missing template row"))?
            .iter()
            .enumerate()
        {
            let value = value.value();
            if !value.is_finite() {
                return Err(VisionError::Invalid("finite template samples required"));
            }
            sums[index % C] += value;
            template_energy += value * value;
        }
    }
    let centered = method == MatchMethod::NormalizedCoefficient;
    let template_variance = if centered {
        (template_energy - sums.iter().map(|sum| sum * sum / n).sum::<f64>()).max(0.0)
    } else {
        template_energy
    };
    if centered && template_variance < f64::EPSILON {
        let mut output = FloatMap::zeros(size)?;
        output.pixels_mut().fill(1.0);
        return Ok(output);
    }
    if method == MatchMethod::Correlation {
        let cross = cross_correlation(source, template, size)?;
        let mut output = FloatMap::zeros(size)?;
        for (value, score) in output.pixels_mut().iter_mut().zip(cross) {
            *value = score as f32;
            if !value.is_finite() {
                return Err(VisionError::Invalid("correlation exceeds f32 range"));
            }
        }
        return Ok(output);
    }
    let owned;
    let statistics = match statistics {
        Some(statistics) => statistics,
        None => {
            owned = Statistics::new(source)?;
            &owned
        }
    };
    let cross = cross_correlation(source, template, size)?;
    Normalization {
        size,
        template_size: ts,
        statistics,
        sums,
        template_norm: template_variance.sqrt(),
        n,
        centered,
    }
    .apply(&cross)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borrowed_bgr_statistics_keep_all_match_score_bits_and_float_validation()
    -> Result<(), VisionError> {
        use crate::common::frame::PixelRect;
        let size = Size::new(89, 53)?;
        let pixels = (0..size.width() * size.height() * 3)
            .map(|i| ((i * 31 + i / 17) % 256) as u8)
            .collect();
        let image = Image::<u8, 3>::from_vec(size, pixels)?;
        let source = image.view().crop(PixelRect::new(3, 5, 82, 47)?)?;
        let matcher = BgrMatcher::new(source)?;
        for bounds in [[5, 7, 18, 16], [27, 11, 43, 29], [7, 9, 8, 10]] {
            let pattern = image
                .view()
                .crop(PixelRect::new(bounds[0], bounds[1], bounds[2], bounds[3])?)?;
            for method in [
                MatchMethod::Correlation,
                MatchMethod::NormalizedCorrelation,
                MatchMethod::NormalizedCoefficient,
            ] {
                let expected = match_template(source, pattern, method)?;
                let actual = matcher.match_pattern(pattern, method)?;
                assert_eq!(actual.size(), expected.size());
                assert!(
                    actual
                        .pixels()
                        .iter()
                        .zip(expected.pixels())
                        .all(|(a, b)| a.to_bits() == b.to_bits())
                );
            }
        }
        let invalid = Image::<f32, 1>::from_vec(Size::new(2, 1)?, vec![0.0, f32::NAN])?;
        let pattern = Image::<f32, 1>::from_vec(Size::new(1, 1)?, vec![1.0])?;
        assert!(match_template(invalid.view(), pattern.view(), MatchMethod::Correlation).is_err());
        Ok(())
    }

    #[test]
    fn byte_dot_preserves_exact_sums_for_long_rows_and_tails() {
        for length in [0, 1, 7, 8, 15, 16, 127, 16_384, 16_385, 70_000] {
            let a = vec![255_u8; length];
            let b = (0..length).map(|i| (i % 256) as u8).collect::<Vec<_>>();
            let expected = a
                .iter()
                .zip(&b)
                .map(|(&a, &b)| u64::from(a) * u64::from(b))
                .sum::<u64>();
            assert_eq!(u8::integer_dot(&a, &b), Some(expected));
        }
        assert_eq!(u8::integer_dot(&[1, 2], &[1]), None);
    }

    #[test]
    fn maximum_lanes_preserve_first_ties_nan_infinity_and_signed_zero() -> Result<(), VisionError> {
        let mut zero_ties = vec![f32::NAN; 33];
        zero_ties[7] = -0.0;
        zero_ties[8] = 0.0;
        let mut negative_infinities = vec![f32::NEG_INFINITY; 17];
        negative_infinities[0] = f32::NAN;
        let cases = [
            vec![f32::NAN],
            vec![f32::NEG_INFINITY],
            vec![f32::NAN, -0.0, 0.0, f32::NEG_INFINITY],
            vec![f32::NAN, 0.0, -0.0, f32::NEG_INFINITY],
            vec![f32::INFINITY, 7.0, f32::INFINITY, f32::NAN],
            vec![f32::NAN; 33],
            zero_ties,
            negative_infinities,
            (0..101)
                .map(|i| if i % 13 == 0 { 9.0 } else { -3.0 })
                .collect(),
        ];
        for values in cases {
            let image = FloatMap::from_vec(Size::new(values.len(), 1)?, values)?;
            let mut expected: Option<(usize, f32)> = None;
            for (index, score) in image.pixels().iter().copied().enumerate() {
                if !score.is_nan() && expected.is_none_or(|(_, old)| score > old) {
                    expected = Some((index, score));
                }
            }
            assert_eq!(
                maximum(&image).map(|p| (p.location, p.score.to_bits())),
                expected.map(|(index, score)| ([index, 0], score.to_bits()))
            );
        }
        Ok(())
    }

    fn check_bands<const C: usize>() -> Result<(), VisionError> {
        let source_size = Size::new(619, 509)?;
        let template_size = Size::new(32, 17)?;
        let mut source = Image::<u8, C>::zeros(source_size)?;
        for y in 0..source_size.height() {
            for x in 0..source_size.width() {
                for channel in 0..C {
                    source.pixels_mut()[(y * source_size.width() + x) * C + channel] = if x < 42 {
                        0
                    } else {
                        ((x * 13 + y * 7 + channel * 23) % 256) as u8
                    };
                }
            }
        }
        let statistics = Statistics::new(source.view())?;
        let size = Size::new(588, 493)?;
        let mut cross = zeros::<f64>(size.width() * size.height())?;
        for (index, value) in cross.iter_mut().enumerate() {
            *value = (index % 1000) as f64 * 10_000.0;
        }
        for centered in [false, true] {
            let normalization = Normalization {
                size,
                template_size,
                statistics: &statistics,
                sums: [58_000.0; C],
                template_norm: 1000.0,
                n: (template_size.width() * template_size.height()) as f64,
                centered,
            };
            let mut expected = FloatMap::zeros(size)?;
            normalization.band(&cross, expected.pixels_mut(), 0);
            let actual = normalization.apply(&cross)?;
            for (actual, expected) in actual.pixels().iter().zip(expected.pixels()) {
                assert_eq!(actual.to_bits(), expected.to_bits());
            }
        }
        Ok(())
    }

    #[test]
    fn normalization_row_bands_keep_every_gray_and_color_score_bit() -> Result<(), VisionError> {
        check_bands::<1>()?;
        check_bands::<3>()
    }
}

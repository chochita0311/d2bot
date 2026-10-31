//! 검토한 UI 관찰기가 공유하는 자산 소유 경계와 픽셀 거리.
use super::{
    error::ObservationError,
    frame::{BgrView, Image},
};
use std::path::{Path, PathBuf};
pub(crate) fn normalize_absolute(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => (),
            std::path::Component::ParentDir => {
                result.pop();
            }
            _ => result.push(component.as_os_str()),
        }
    }
    result
}
pub(crate) struct AssetOwner {
    directory: PathBuf,
}
impl AssetOwner {
    pub fn new(directory: &Path) -> Result<Self, ObservationError> {
        Ok(Self {
            directory: directory.canonicalize()?,
        })
    }
    pub fn path(&self, name: &str) -> Result<PathBuf, ObservationError> {
        let path = normalize_absolute(&self.directory.join(name));
        if path == self.directory || !path.starts_with(&self.directory) {
            return Err(ObservationError::invalid(
                "Vision asset must stay within its owner",
            ));
        }
        let path = path.canonicalize()?;
        if path == self.directory || !path.starts_with(&self.directory) {
            return Err(ObservationError::invalid(
                "Vision asset must stay within its owner",
            ));
        }
        Ok(path)
    }
}
pub(crate) fn color_distance(
    image: BgrView<'_>,
    template: BgrView<'_>,
) -> Result<f64, ObservationError> {
    if image.size() != template.size() {
        return Ok(1.0);
    }
    let mut differences = Image::<f32, 3>::zeros(image.size())?;
    for y in 0..image.size().height() {
        let a = image
            .row(y)
            .ok_or_else(|| ObservationError::invalid("missing distance row"))?;
        let b = template
            .row(y)
            .ok_or_else(|| ObservationError::invalid("missing distance row"))?;
        for (x, (a, b)) in a.iter().zip(b).enumerate() {
            differences.pixels_mut()[y * image.size().width() * 3 + x] = f32::from(a.abs_diff(*b));
        }
    }
    let sum = pairwise_sum(differences.pixels());
    let mean = (f64::from(sum) / differences.pixels().len() as f64) as f32;
    Ok(f64::from(mean) / 255.0)
}

// NumPy 1.21.6 FLOAT_pairwise_sum의 8개 누산기와 블록 경계를 재현한다.
// 순차 f32 합산은 큰 색상 비교 영역에서 판정을 바꿀 수 있다.
// BSD 고지: licenses/numpy-bsd.txt, 고정 소스: THIRD-PARTY-NOTICES.md.
fn pairwise_sum(values: &[f32]) -> f32 {
    if values.len() < 8 {
        return values.iter().fold(0.0, |sum, v| sum + v);
    }
    if values.len() <= 128 {
        let mut lanes = [0.0_f32; 8];
        lanes.copy_from_slice(&values[..8]);
        let mut at = 8;
        while at + 8 <= values.len() {
            for lane in 0..8 {
                lanes[lane] += values[at + lane];
            }
            at += 8;
        }
        let mut sum = ((lanes[0] + lanes[1]) + (lanes[2] + lanes[3]))
            + ((lanes[4] + lanes[5]) + (lanes[6] + lanes[7]));
        for v in &values[at..] {
            sum += v;
        }
        return sum;
    }
    let split = (values.len() / 2) & !7;
    pairwise_sum(&values[..split]) + pairwise_sum(&values[split..])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn large_float32_distance_reduction_matches_pinned_numpy()
    -> Result<(), Box<dyn std::error::Error>> {
        for length in [1, 7, 8, 9, 127, 128, 129, 8191, 8192, 8193, 200_003] {
            let pixels: Vec<u8> = (0..length).map(|i| ((i * 73 + 19) % 256) as u8).collect();
            let values: Vec<f32> = pixels.iter().map(|v| f32::from(*v)).collect();
            let expected = crate::test_support::legacy_expectation(
                "observer-oracle",
                &serde_json::json!({"kind":"reduce","pixels":pixels.iter().map(|v|format!("{v:02x}")).collect::<String>()}),
            )?;
            assert_eq!(
                f64::from(pairwise_sum(&values)),
                expected["sum"].as_f64().ok_or("sum")?,
                "{length}"
            );
            let mean = (f64::from(pairwise_sum(&values)) / length as f64) as f32;
            assert_eq!(
                f64::from(mean) / 255.0,
                expected["distance"].as_f64().ok_or("distance")?,
                "{length}"
            );
        }
        Ok(())
    }
}

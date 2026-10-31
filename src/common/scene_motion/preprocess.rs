//! Ordinary INTER_LINEAR with explicit fx/fy, distinct from ORB's exact pyramid.
use super::ConfigError;
#[cfg(test)]
use crate::common::frame::Size;
use crate::common::frame::{GrayFrame, GrayView};

pub(super) fn image_error(error: impl std::fmt::Display) -> ConfigError {
    ConfigError::new("VisionError", error.to_string())
}
pub(super) fn scaled_gray(source: GrayView<'_>, scale: f64) -> Result<GrayFrame, ConfigError> {
    crate::common::vision_ops::resize::scale_linear(source, scale).map_err(image_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scaled_pixels_match_ordinary_linear_including_odd_edges()
    -> Result<(), Box<dyn std::error::Error>> {
        for (width, height) in [(33, 29), (64, 48), (383, 287)] {
            let mut state = 13u32;
            let pixels = (0..width * height)
                .map(|_| {
                    state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    (state >> 24) as u8
                })
                .collect();
            let gray = GrayFrame::from_vec(Size::new(width, height)?, pixels)?;
            let hex = |bytes: &[u8]| bytes.iter().map(|n| format!("{n:02x}")).collect::<String>();
            for scale in [0.5, 0.7, 0.33, 1.0] {
                let expected = crate::test_support::legacy_expectation(
                    "motion-oracle",
                    &serde_json::json!({"kind":"scale","width":width,"height":height,"gray":hex(gray.pixels()),"scale":scale}),
                )?;
                let actual = scaled_gray(gray.view(), scale)?;
                assert_eq!(
                    serde_json::json!([actual.size().width(), actual.size().height()]),
                    expected["size"]
                );
                assert_eq!(
                    hex(actual.pixels()),
                    expected["gray"].as_str().ok_or("gray hex")?,
                    "{width}x{height} scale{scale}"
                );
            }
        }
        Ok(())
    }
}

//! 검토한 BGR 팔레트와 현재 이미지의 Lab 거리. 통행 가능성 자체는 증명하지 않는다.
use super::{
    frame::{BgrFrame, BgrView, FloatMap, Size},
    vision_ops::{VisionError, color},
};
#[derive(Clone)]
pub struct Palette {
    lab: Vec<[f32; 3]>,
}
impl Palette {
    pub fn new(colors: Vec<[u8; 3]>) -> Result<Self, VisionError> {
        if colors.is_empty() {
            return Err(VisionError::Invalid("non-empty BGR palette required"));
        }
        let image = BgrFrame::from_vec(
            Size::new(colors.len(), 1)?,
            colors.into_iter().flatten().collect(),
        )?;
        let lab = color::lab(image.view())?
            .pixels()
            .as_chunks::<3>()
            .0
            .iter()
            .map(|p| p.map(f32::from))
            .collect();
        Ok(Self { lab })
    }
    pub fn distance_map(&self, image: BgrView<'_>) -> Result<FloatMap, VisionError> {
        let lab = color::lab(image)?;
        let mut result = FloatMap::zeros(image.size())?;
        for (index, pixel) in lab.pixels().as_chunks::<3>().0.iter().enumerate() {
            let mut best = f32::INFINITY;
            for color in &self.lab {
                let [a, b, c] = std::array::from_fn(|i| f32::from(pixel[i]) - color[i]);
                best = best.min(((a * a + b * b) + c * c).sqrt());
            }
            result.pixels_mut()[index] = best;
        }
        Ok(result)
    }
    pub fn match_ratio(
        &self,
        image: BgrView<'_>,
        maximum_distance: f64,
    ) -> Result<f64, VisionError> {
        let distances = self.distance_map(image)?;
        Ok(distances
            .pixels()
            .iter()
            .filter(|v| **v <= maximum_distance as f32)
            .count() as f64
            / distances.pixels().len() as f64)
    }
}

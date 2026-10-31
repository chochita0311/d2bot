//! Owned packed BGR8 images and immutable views tied to the backing buffer.

use std::collections::TryReserveError;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum FrameError {
    Invalid(&'static str),
    Allocation(TryReserveError),
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => f.write_str(message),
            Self::Allocation(error) => write!(f, "cannot allocate frame: {error}"),
        }
    }
}

impl Error for FrameError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    width: usize,
    height: usize,
}

impl Size {
    pub fn new(width: usize, height: usize) -> Result<Self, FrameError> {
        if width == 0 || height == 0 {
            return Err(FrameError::Invalid("positive frame size required"));
        }
        let size = Self { width, height };
        size.byte_len()?;
        Ok(size)
    }

    pub fn width(self) -> usize {
        self.width
    }
    pub fn height(self) -> usize {
        self.height
    }
    pub fn row_bytes(self) -> usize {
        self.width * 3
    }

    pub fn byte_len(self) -> Result<usize, FrameError> {
        self.width
            .checked_mul(3)
            .and_then(|row| row.checked_mul(self.height))
            .ok_or(FrameError::Invalid("frame byte length overflow"))
    }
}

/// An end-exclusive rectangle in image pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    left: usize,
    top: usize,
    right: usize,
    bottom: usize,
}

impl PixelRect {
    pub fn new(left: usize, top: usize, right: usize, bottom: usize) -> Result<Self, FrameError> {
        if left >= right || top >= bottom {
            return Err(FrameError::Invalid("nonempty pixel rectangle required"));
        }
        Ok(Self {
            left,
            top,
            right,
            bottom,
        })
    }

    pub fn full(size: Size) -> Self {
        Self {
            left: 0,
            top: 0,
            right: size.width,
            bottom: size.height,
        }
    }

    pub fn left(self) -> usize {
        self.left
    }
    pub fn top(self) -> usize {
        self.top
    }
    pub fn right(self) -> usize {
        self.right
    }
    pub fn bottom(self) -> usize {
        self.bottom
    }
    pub fn width(self) -> usize {
        self.right - self.left
    }
    pub fn height(self) -> usize {
        self.bottom - self.top
    }

    pub fn checked_in(self, size: Size) -> Result<Self, FrameError> {
        if self.right > size.width || self.bottom > size.height {
            return Err(FrameError::Invalid("pixel rectangle outside frame"));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image<T, const C: usize> {
    size: Size,
    pixels: Vec<T>,
}

fn image_len<const C: usize>(size: Size) -> Result<usize, FrameError> {
    if C == 0 {
        return Err(FrameError::Invalid("positive image channel count required"));
    }
    size.width
        .checked_mul(size.height)
        .and_then(|count| count.checked_mul(C))
        .ok_or(FrameError::Invalid("image element count overflow"))
}

impl<T, const C: usize> Image<T, C> {
    pub fn from_vec(size: Size, pixels: Vec<T>) -> Result<Self, FrameError> {
        if pixels.len() != image_len::<C>(size)? {
            return Err(FrameError::Invalid(
                "packed image element count differs from frame size",
            ));
        }
        Ok(Self { size, pixels })
    }

    pub fn size(&self) -> Size {
        self.size
    }
    pub fn pixels(&self) -> &[T] {
        &self.pixels
    }
    pub fn into_pixels(self) -> Vec<T> {
        self.pixels
    }
    pub fn pixels_mut(&mut self) -> &mut [T] {
        &mut self.pixels
    }
    pub fn view(&self) -> ImageView<'_, T, C> {
        ImageView {
            size: self.size,
            stride: self.size.width * C,
            pixels: &self.pixels,
        }
    }
}

impl<T: Default + Clone, const C: usize> Image<T, C> {
    pub fn zeros(size: Size) -> Result<Self, FrameError> {
        let len = image_len::<C>(size)?;
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(len)
            .map_err(FrameError::Allocation)?;
        pixels.resize(len, T::default());
        Ok(Self { size, pixels })
    }
}

impl<const C: usize> Image<u8, C> {
    pub fn bytes(&self) -> &[u8] {
        &self.pixels
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ImageView<'a, T, const C: usize> {
    size: Size,
    /// Row stride in elements of T, not bytes (identical for u8).
    stride: usize,
    pixels: &'a [T],
}

impl<'a, T: Copy, const C: usize> ImageView<'a, T, C> {
    pub fn from_strided(size: Size, stride: usize, pixels: &'a [T]) -> Result<Self, FrameError> {
        image_len::<C>(size)?;
        let row_len = size.width * C;
        let extent = stride
            .checked_mul(size.height - 1)
            .and_then(|offset| offset.checked_add(row_len))
            .ok_or(FrameError::Invalid("strided frame byte length overflow"))?;
        if stride < row_len || pixels.len() < extent {
            return Err(FrameError::Invalid("strided image buffer is too short"));
        }
        Ok(Self {
            size,
            stride,
            pixels: &pixels[..extent],
        })
    }

    pub fn size(self) -> Size {
        self.size
    }
    pub fn stride(self) -> usize {
        self.stride
    }

    pub fn row(self, y: usize) -> Option<&'a [T]> {
        if y >= self.size.height {
            return None;
        }
        let start = y * self.stride;
        Some(&self.pixels[start..start + self.size.width * C])
    }

    pub fn crop(self, rect: PixelRect) -> Result<Self, FrameError> {
        rect.checked_in(self.size)?;
        let size = Size::new(rect.width(), rect.height())?;
        let start = rect.top * self.stride + rect.left * C;
        Self::from_strided(size, self.stride, &self.pixels[start..])
    }
}

impl<T: Copy + Default, const C: usize> ImageView<'_, T, C> {
    pub fn to_owned(self) -> Result<Image<T, C>, FrameError> {
        let mut output = Image::zeros(self.size)?;
        let row_len = self.size.width * C;
        for y in 0..self.size.height {
            let start = y * row_len;
            let row = self
                .row(y)
                .ok_or(FrameError::Invalid("missing frame row"))?;
            output.pixels[start..start + row_len].copy_from_slice(row);
        }
        Ok(output)
    }
}

pub type BgrFrame = Image<u8, 3>;
pub type GrayFrame = Image<u8, 1>;
pub type FloatMap = Image<f32, 1>;
pub type BgrView<'a> = ImageView<'a, u8, 3>;
pub type GrayView<'a> = ImageView<'a, u8, 1>;

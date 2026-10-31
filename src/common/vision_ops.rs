//! The image operations used by the application; no OpenCV or Python runtime.

pub mod codec;
pub mod color;
pub mod components;
pub mod drawing;
pub mod filters;
pub mod morphology;
pub mod resize;
pub mod template;

use super::frame::FrameError;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum VisionError {
    Invalid(&'static str),
    Frame(FrameError),
    Io(std::io::Error),
    Codec(image::ImageError),
    Jpeg(jpeg_decoder::Error),
    Png(png::DecodingError),
}
impl fmt::Display for VisionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => f.write_str(message),
            Self::Frame(error) => error.fmt(f),
            Self::Io(error) => error.fmt(f),
            Self::Codec(error) => error.fmt(f),
            Self::Jpeg(error) => error.fmt(f),
            Self::Png(error) => error.fmt(f),
        }
    }
}
impl Error for VisionError {}
impl From<super::compute::ComputeError> for VisionError {
    fn from(error: super::compute::ComputeError) -> Self {
        match error {
            super::compute::ComputeError::Start(e) => Self::Io(e),
            super::compute::ComputeError::Panicked => {
                Self::Invalid("vision compute worker panicked")
            }
        }
    }
}
impl From<FrameError> for VisionError {
    fn from(error: FrameError) -> Self {
        Self::Frame(error)
    }
}
impl From<std::io::Error> for VisionError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<image::ImageError> for VisionError {
    fn from(error: image::ImageError) -> Self {
        Self::Codec(error)
    }
}
impl From<jpeg_decoder::Error> for VisionError {
    fn from(error: jpeg_decoder::Error) -> Self {
        Self::Jpeg(error)
    }
}
impl From<png::DecodingError> for VisionError {
    fn from(error: png::DecodingError) -> Self {
        Self::Png(error)
    }
}

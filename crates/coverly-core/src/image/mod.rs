//! Downloaded art → the image a profile slot expects (spec §11: always
//! decoded and re-encoded, which also drops metadata). Hostile input is
//! bounded: byte size, sniffed format (PNG, JPEG, WebP, GIF, BMP only),
//! dimensions checked from the header before decoding, allocation limit.

use std::io::Cursor;

use ::image::codecs::jpeg::JpegEncoder;
use ::image::imageops::FilterType;
use ::image::{DynamicImage, ImageFormat as Codec, ImageReader, Limits, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

use crate::profiles::{ImageFormat, MediaSlot};

/// Bounds applied to every downloaded image before and while decoding.
#[derive(Clone, Copy, Debug)]
pub struct ImageLimits {
    /// Largest accepted input, in bytes.
    pub max_input_bytes: usize,
    /// Largest accepted width or height, in pixels.
    pub max_dimension: u32,
    /// Largest allocation the decoder may make, in bytes.
    pub max_alloc: u64,
}

impl Default for ImageLimits {
    fn default() -> Self {
        ImageLimits {
            max_input_bytes: 20 * 1024 * 1024,
            max_dimension: 8192,
            max_alloc: 256 * 1024 * 1024,
        }
    }
}

/// How an image is fitted into the slot's box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "kebab-case")]
pub enum Fit {
    /// Shrink to fit inside the box, keeping the aspect ratio.
    #[default]
    Contain,
    /// Contain, then centre on a canvas of exactly the box size.
    Pad { rgba: [u8; 4] },
}

/// What the re-encoded image must look like.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutputSpec {
    pub format: ImageFormat,
    /// Box the image must fit in, as `(width, height)`; `None` keeps the size.
    pub max_size: Option<(u32, u32)>,
    pub fit: Fit,
    /// JPEG quality, 1–100.
    pub jpeg_quality: u8,
}

impl OutputSpec {
    /// The spec a profile media slot asks for.
    pub fn for_slot(slot: &MediaSlot, fit: Fit, jpeg_quality: u8) -> OutputSpec {
        OutputSpec {
            format: slot.format,
            max_size: slot.max_size,
            fit,
            jpeg_quality: jpeg_quality.clamp(1, 100),
        }
    }
}

/// Why a downloaded image was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImageError {
    #[error("download is {bytes} bytes, over the limit")]
    TooLarge { bytes: usize },
    #[error("not an image")]
    NotAnImage,
    #[error("unsupported image format {0}")]
    Unsupported(String),
    #[error("image is {width}x{height}, over the limit")]
    Dimensions { width: u32, height: u32 },
    #[error("cannot decode image: {0}")]
    Decode(String),
    #[error("cannot encode image: {0}")]
    Encode(String),
}

/// A re-encoded image ready to be written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessedImage {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

fn reader<'a>(
    bytes: &'a [u8],
    codec: Codec,
    limits: &ImageLimits,
) -> ImageReader<Cursor<&'a [u8]>> {
    let mut l = Limits::default();
    l.max_image_width = Some(limits.max_dimension);
    l.max_image_height = Some(limits.max_dimension);
    l.max_alloc = Some(limits.max_alloc);
    let mut r = ImageReader::with_format(Cursor::new(bytes), codec);
    r.limits(l);
    r
}

/// Width and height read from the image header only (no full decode).
pub fn probe_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let codec = ::image::guess_format(bytes).ok()?;
    ImageReader::with_format(Cursor::new(bytes), codec)
        .into_dimensions()
        .ok()
}

/// Validates, decodes, resizes (never upscales) and re-encodes `bytes`.
pub fn process_image(
    bytes: &[u8],
    spec: &OutputSpec,
    limits: &ImageLimits,
) -> Result<ProcessedImage, ImageError> {
    if bytes.len() > limits.max_input_bytes {
        return Err(ImageError::TooLarge { bytes: bytes.len() });
    }
    let codec = ::image::guess_format(bytes).map_err(|_| ImageError::NotAnImage)?;
    if !matches!(
        codec,
        Codec::Png | Codec::Jpeg | Codec::WebP | Codec::Gif | Codec::Bmp
    ) {
        return Err(ImageError::Unsupported(format!("{codec:?}")));
    }
    let (w, h) = ImageReader::with_format(Cursor::new(bytes), codec)
        .into_dimensions()
        .map_err(|e| ImageError::Decode(e.to_string()))?;
    if w == 0 || h == 0 || w > limits.max_dimension || h > limits.max_dimension {
        return Err(ImageError::Dimensions {
            width: w,
            height: h,
        });
    }
    let mut img = reader(bytes, codec, limits).decode().map_err(|e| match e {
        ::image::ImageError::Limits(_) => ImageError::Dimensions {
            width: w,
            height: h,
        },
        other => ImageError::Decode(other.to_string()),
    })?;
    if let Some((mw, mh)) = spec.max_size {
        if mw == 0 || mh == 0 {
            return Err(ImageError::Encode(format!("empty target box {mw}x{mh}")));
        }
        if img.width() > mw || img.height() > mh {
            img = img.resize(mw, mh, FilterType::Lanczos3);
        }
        if let Fit::Pad { rgba } = spec.fit {
            let mut canvas = RgbaImage::from_pixel(mw, mh, Rgba(rgba));
            let (x, y) = ((mw - img.width()) / 2, (mh - img.height()) / 2);
            ::image::imageops::overlay(&mut canvas, &img.to_rgba8(), i64::from(x), i64::from(y));
            img = DynamicImage::ImageRgba8(canvas);
        }
    }
    let mut out = Cursor::new(Vec::new());
    match spec.format {
        ImageFormat::Png => img
            .write_to(&mut out, Codec::Png)
            .map_err(|e| ImageError::Encode(e.to_string()))?,
        ImageFormat::Jpg => {
            JpegEncoder::new_with_quality(&mut out, spec.jpeg_quality.clamp(1, 100))
                .encode_image(&DynamicImage::ImageRgb8(img.to_rgb8()))
                .map_err(|e| ImageError::Encode(e.to_string()))?
        }
    }
    Ok(ProcessedImage {
        width: img.width(),
        height: img.height(),
        bytes: out.into_inner(),
    })
}

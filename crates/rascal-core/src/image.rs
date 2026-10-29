//! Images: pixels in a frame, with a role and an identity. A canonical image is the lossless
//! decode of a job image, stored once and never modified.
//!
//! Identity and the pixel digest follow canonical serialization (ADR 0016).

use std::path::Path;

use image::{DynamicImage, GenericImageView};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::canonical;
use crate::error::{Error, Result};
use crate::frame::{Frame, Projection};
use crate::identity::Identity;

/// What an image is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Canonical,
    Working,
    Fill,
    Composite,
    Rendering,
    Plane,
    Inspection,
}

/// Which filler field a rendering fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RenderingRole {
    MaskInput,
    ImageInput,
}

/// An image record: the pixels are in `file`, a PNG beside the record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImageRecord {
    /// The record's version.
    pub image_version: u32,
    /// The local id.
    pub id: String,
    /// The frame id the pixels are in.
    pub frame: String,
    /// The frame's identity.
    pub frame_identity: String,
    pub role: Role,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rendering_role: Option<RenderingRole>,
    /// `sha256:` over width, height, channel count and the samples in row-major order.
    pub pixel_digest: String,
    /// `sha256:` over the pixel digest, the frame identity and the roles.
    pub identity: String,
    /// The PNG, relative to the record.
    pub file: String,
}

/// Pixels in a frame.
#[derive(Debug, Clone)]
pub struct Image {
    pub record: ImageRecord,
    pub pixels: DynamicImage,
}

impl Image {
    /// Wraps decoded pixels (RGB8 or RGBA8; anything else is converted to RGB8) in a frame.
    pub fn new(
        id: &str,
        pixels: DynamicImage,
        frame: &Frame,
        role: Role,
        rendering_role: Option<RenderingRole>,
    ) -> Image {
        let pixels = match pixels {
            DynamicImage::ImageRgb8(_)
            | DynamicImage::ImageRgba8(_)
            | DynamicImage::ImageLuma8(_) => pixels,
            other => DynamicImage::ImageRgb8(other.to_rgb8()),
        };
        let pixel_digest = pixel_digest(&pixels);
        let identity = canonical::identity_of(
            "image",
            &serde_json::json!({
                "pixels": pixel_digest.as_str(),
                "frame": frame.identity,
                "role": role,
                "rendering_role": rendering_role,
            }),
        );
        Image {
            record: ImageRecord {
                image_version: 1,
                id: id.to_string(),
                frame: frame.id.clone(),
                frame_identity: frame.identity.clone(),
                role,
                rendering_role,
                pixel_digest: pixel_digest.as_str().to_string(),
                identity: identity.as_str().to_string(),
                file: format!("{}.png", identity.hex()),
            },
            pixels,
        }
    }

    /// Decodes a job image (JPEG or PNG) as a canonical image in its raw frame. Without a
    /// projection an image exactly 2:1 is assumed equirectangular; the second value says so.
    pub fn import(path: &Path, projection: Option<Projection>) -> Result<(Image, Frame, bool)> {
        Image::from_decoded(decode_limited(path)?, projection)
    }

    /// `import` for bytes already in memory (an MCP client's base64 image).
    pub fn import_bytes(
        bytes: &[u8],
        projection: Option<Projection>,
    ) -> Result<(Image, Frame, bool)> {
        Image::from_decoded(decode_limited_bytes(bytes)?, projection)
    }

    fn from_decoded(
        decoded: DynamicImage,
        projection: Option<Projection>,
    ) -> Result<(Image, Frame, bool)> {
        let (width, height) = decoded.dimensions();
        let (projection, assumed) = match projection {
            Some(p) => (p, false),
            None if width == 2 * height => (Projection::Equirectangular, true),
            None => (Projection::Flat, false),
        };
        let frame = Frame::raw(width, height, projection);
        let image = Image::new("inputs.image", decoded, &frame, Role::Canonical, None);
        Ok((image, frame, assumed))
    }

    /// The identity as a typed value.
    pub fn identity(&self) -> Identity {
        Identity::parse_or_hash(&self.record.identity)
    }

    /// Width and height.
    pub fn dimensions(&self) -> (u32, u32) {
        self.pixels.dimensions()
    }

    /// Writes the PNG and the record (`<file>.json`) into `dir`; returns the PNG path.
    pub fn write(&self, dir: &Path) -> Result<std::path::PathBuf> {
        std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        let png = dir.join(&self.record.file);
        self.pixels
            .save_with_format(&png, image::ImageFormat::Png)
            .map_err(|source| Error::Image {
                path: png.clone(),
                source,
            })?;
        let record_path = dir.join(format!("{}.json", self.record.file));
        let text = serde_json::to_string_pretty(&self.record).map_err(|source| Error::Json {
            path: record_path.clone(),
            source,
        })?;
        std::fs::write(&record_path, text + "\n").map_err(|e| Error::io(&record_path, e))?;
        Ok(png)
    }
}

/// `sha256:` over width, height, channel count and the samples in row-major order.
pub fn pixel_digest(pixels: &DynamicImage) -> Identity {
    let (width, height) = pixels.dimensions();
    canonical::pixel_digest(
        width,
        height,
        u32::from(pixels.color().channel_count()),
        pixels.as_bytes(),
    )
}

/// The largest image rascal decodes: 32768 pixels a side and 2 GiB of pixel memory, bounds
/// that hold every supported input (a 16k by 8k panorama is 400 MB as RGBA) and refuse a
/// decompression bomb before it is allocated.
pub const MAX_SIDE: u32 = 32_768;
const MAX_ALLOC: u64 = 2 << 30;

/// Decodes PNG (or JPEG) bytes within the same limits as [`decode_limited`]: a filler's or a
/// pack's answer.
pub fn decode_limited_bytes(bytes: &[u8]) -> Result<DynamicImage> {
    decode_bytes_within(bytes, MAX_SIDE, MAX_SIDE)
}

/// Decodes bytes that must be at most `width` by `height`: an answer whose dimensions are
/// known in advance is refused from its header, before any pixel memory is allocated.
pub fn decode_bytes_within(bytes: &[u8], width: u32, height: u32) -> Result<DynamicImage> {
    let fail = |source| Error::Image {
        path: "<bytes>".into(),
        source,
    };
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(width.min(MAX_SIDE));
    limits.max_image_height = Some(height.min(MAX_SIDE));
    limits.max_alloc = Some(MAX_ALLOC);
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| Error::io(Path::new("<bytes>"), e))?;
    reader.limits(limits);
    reader.decode().map_err(fail)
}

/// Decodes an image file within [`MAX_SIDE`] and the memory bound; every decode of a file
/// rascal did not write itself goes through here.
pub fn decode_limited(path: &Path) -> Result<DynamicImage> {
    let fail = |source| Error::Image {
        path: path.to_path_buf(),
        source,
    };
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    let mut reader = image::ImageReader::open(path)
        .map_err(|e| Error::io(path, e))?
        .with_guessed_format()
        .map_err(|e| Error::io(path, e))?;
    reader.limits(limits);
    reader.decode().map_err(fail)
}

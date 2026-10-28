//! Images: pixels in a frame, with a role and an identity. A canonical image is the lossless
//! decode of a job image, stored once and never modified.
//!
//! REPLACED BY 1.2 (the pixel digest and image identity under canonical serialization; the
//! PNG encoder settings of ADR 0016).

use std::path::Path;

use image::{DynamicImage, GenericImageView};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
        let role_name = serde_json::to_string(&role).unwrap_or_default();
        let rendering_name = serde_json::to_string(&rendering_role).unwrap_or_default();
        let identity = Identity::of_parts(&[
            pixel_digest.as_str().as_bytes(),
            frame.identity.as_bytes(),
            role_name.as_bytes(),
            rendering_name.as_bytes(),
        ]);
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
        let decoded = image::open(path).map_err(|source| Error::Image {
            path: path.to_path_buf(),
            source,
        })?;
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
        Identity::parse(&self.record.identity)
            .unwrap_or_else(|| Identity::of_parts(&[self.record.identity.as_bytes()]))
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
    let channels = pixels.color().channel_count() as u32;
    Identity::of_parts(&[
        &width.to_le_bytes(),
        &height.to_le_bytes(),
        &channels.to_le_bytes(),
        pixels.as_bytes(),
    ])
}

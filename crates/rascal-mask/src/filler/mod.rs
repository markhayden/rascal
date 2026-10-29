//! The filler port (SPEC.md "Fillers"): a configured command or HTTP endpoint that receives an
//! image, and a mask for a mask-input profile, of exactly the dimensions rascal chose, and
//! returns a PNG of those dimensions. Anything else (another size, a non-zero exit, a timeout,
//! a non-PNG body) is an operational error; there is no resize and no fallback. Every call is
//! journaled without its key.

pub mod command;
pub mod http;

use std::path::Path;
use std::time::Duration;

use image::DynamicImage;
use rascal_core::Error;
use rascal_core::config::FillerConfig;
use rascal_core::error::Result;
use rascal_core::image::{Image, RenderingRole};
use serde::{Deserialize, Serialize};

/// Where each field of a request comes from, by profile role and `send_mask` (G9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fields {
    /// The `image` field: the working image for a mask-input profile, the rendering for an
    /// image-input one.
    pub image: ImageSource,
    /// The `mask` field: the rendering for a mask-input profile; for an image-input profile
    /// the `grey` rendering when `send_mask`, else nothing.
    pub mask: MaskSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageSource {
    Working,
    Rendering,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskSource {
    Rendering,
    Grey,
    None,
}

/// The request-field matrix.
pub fn request_fields(role: RenderingRole, send_mask: bool) -> Fields {
    match role {
        RenderingRole::MaskInput => Fields {
            image: ImageSource::Working,
            mask: MaskSource::Rendering,
        },
        RenderingRole::ImageInput => Fields {
            image: ImageSource::Rendering,
            mask: if send_mask {
                MaskSource::Grey
            } else {
                MaskSource::None
            },
        },
    }
}

/// A request to a filler: PNG bytes of exactly the dimensions the answer must have.
#[derive(Debug, Clone)]
pub struct FillRequest {
    pub image_png: Vec<u8>,
    pub mask_png: Option<Vec<u8>>,
    pub width: u32,
    pub height: u32,
    pub prompt: Option<String>,
}

impl FillRequest {
    /// Encodes the fields chosen by the matrix: `working` and `rendering` are the two images
    /// in the mask's frame; `grey` is the grey rendering for `send_mask`.
    pub fn build(
        role: RenderingRole,
        send_mask: bool,
        working: &Image,
        rendering: &Image,
        grey: Option<&Image>,
        prompt: Option<&str>,
    ) -> Result<FillRequest> {
        let fields = request_fields(role, send_mask);
        let image = match fields.image {
            ImageSource::Working => working,
            ImageSource::Rendering => rendering,
        };
        let mask = match fields.mask {
            MaskSource::Rendering => Some(rendering),
            MaskSource::Grey => Some(grey.ok_or_else(|| {
                Error::usage("send_mask needs the grey rendering alongside the image rendering")
            })?),
            MaskSource::None => None,
        };
        let (width, height) = (image.pixels.width(), image.pixels.height());
        if let Some(m) = mask
            && (m.pixels.width(), m.pixels.height()) != (width, height)
        {
            return Err(Error::usage(
                "the image and the mask of a request differ in size",
            ));
        }
        Ok(FillRequest {
            image_png: png_bytes(&image.pixels)?,
            mask_png: mask.map(|m| png_bytes(&m.pixels)).transpose()?,
            width,
            height,
            prompt: prompt.map(str::to_string),
        })
    }

    /// The request's size in bytes, for the journal.
    pub fn bytes(&self) -> u64 {
        (self.image_png.len() + self.mask_png.as_ref().map_or(0, Vec::len)) as u64
    }
}

/// PNG bytes of an image, with rascal's encoder settings.
pub fn png_bytes(pixels: &DynamicImage) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut bytes);
    pixels
        .write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|source| Error::Image {
            path: "<request>".into(),
            source,
        })?;
    Ok(bytes)
}

/// What the journal records about a call: never a key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FillerCall {
    /// The filler entry's name.
    pub filler: String,
    /// The command as configured (placeholders shown) or the URL.
    pub target: String,
    pub request_width: u32,
    pub request_height: u32,
    pub request_bytes: u64,
    pub response_bytes: u64,
    pub duration_ms: u64,
    /// The process exit code or the HTTP status.
    pub status: i64,
    /// `ok`, or the error the call ended in.
    pub outcome: String,
}

impl FillerCall {
    /// The journal note text: a fixed prefix and the record as JSON.
    pub fn note(&self) -> String {
        format!(
            "filler call: {}",
            serde_json::to_string(self).unwrap_or_default()
        )
    }
}

/// A configured filler.
pub trait Filler: Send + Sync {
    /// The entry's name.
    fn name(&self) -> &str;
    /// Calls the filler with `scratch` for any files the call needs. The answer's dimensions
    /// are asserted here, so a `Ok` fill is always the request's size.
    fn call(&self, request: &FillRequest, scratch: &Path) -> Result<(DynamicImage, FillerCall)>;
}

/// The filler for a configuration entry.
pub fn from_config(name: &str, config: &FillerConfig) -> Result<Box<dyn Filler>> {
    config.check(name)?;
    let timeout = Duration::from_secs(config.timeout_s);
    Ok(match (&config.command, &config.url) {
        (Some(argv), None) => Box::new(command::CommandFiller::new(name, argv.clone(), timeout)),
        (None, Some(url)) => Box::new(http::HttpFiller::new(
            name,
            url,
            config.key_env.clone(),
            timeout,
        )?),
        _ => unreachable!("checked"),
    })
}

/// Decodes a filler's PNG answer within rascal's limits and asserts its dimensions.
pub(crate) fn decode_answer(
    bytes: &[u8],
    request: &FillRequest,
    who: &str,
) -> Result<DynamicImage> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(Error::op(
            "filler",
            format!("{who}: the answer is not a PNG ({} bytes)", bytes.len()),
        ));
    }
    let decoded = rascal_core::image::decode_limited_bytes(bytes)
        .map_err(|e| Error::op("filler", format!("{who}: the answer did not decode: {e}")))?;
    if (decoded.width(), decoded.height()) != (request.width, request.height) {
        return Err(Error::op(
            "filler",
            format!(
                "{who}: the answer is {}x{} but the request was {}x{}; there is no resize",
                decoded.width(),
                decoded.height(),
                request.width,
                request.height
            ),
        ));
    }
    Ok(decoded)
}

/// A single-threaded runtime for one call.
pub(crate) fn runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| Error::op("filler", format!("runtime: {e}")))
}

/// Up to `limit` bytes of captured text, lossily, for a message.
pub(crate) fn excerpt(bytes: &[u8], limit: usize) -> String {
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(limit)]).into_owned();
    if bytes.len() > limit {
        format!("{text}…")
    } else {
        text
    }
}

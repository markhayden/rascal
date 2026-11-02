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

    /// The request downscaled so its longest side is `max_side`, with the size it had, or
    /// `None` when it already fits. The image is resampled with Lanczos, the mask with a
    /// triangle filter and, when it was binary, thresholded back to binary; the fill the
    /// filler returns at the scaled size is scaled to the support by `heal`.
    pub fn scaled(&self, max_side: u32) -> Result<Option<(FillRequest, (u32, u32))>> {
        let longest = self.width.max(self.height);
        if max_side == 0 || longest <= max_side {
            return Ok(None);
        }
        let scale = f64::from(max_side) / f64::from(longest);
        let side = |v: u32| ((f64::from(v) * scale).round() as u32).max(1);
        let (width, height) = (side(self.width), side(self.height));
        let decode = |bytes: &[u8]| {
            image::load_from_memory(bytes).map_err(|source| Error::Image {
                path: "<request>".into(),
                source,
            })
        };
        let image = decode(&self.image_png)?.resize_exact(
            width,
            height,
            image::imageops::FilterType::Lanczos3,
        );
        let mask_png = match &self.mask_png {
            None => None,
            Some(bytes) => {
                let mask = decode(bytes)?.to_luma8();
                let binary = mask.pixels().all(|p| p.0[0] == 0 || p.0[0] == 255);
                let mut scaled = image::imageops::resize(
                    &mask,
                    width,
                    height,
                    image::imageops::FilterType::Triangle,
                );
                if binary {
                    for p in scaled.pixels_mut() {
                        p.0[0] = if p.0[0] >= 128 { 255 } else { 0 };
                    }
                }
                Some(png_bytes(&DynamicImage::ImageLuma8(scaled))?)
            }
        };
        Ok(Some((
            FillRequest {
                image_png: png_bytes(&image)?,
                mask_png,
                width,
                height,
                prompt: self.prompt.clone(),
            },
            (self.width, self.height),
        )))
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
    /// Whether the call may have been charged.
    pub charge: Charge,
}

/// Whether a call may have been charged, by how far its request got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Charge {
    /// Refused before the request left rascal: no key, the connection refused, the command
    /// not started. Free.
    NotSent,
    /// The filler answered: a 2xx, or a command that exited 0, charged at the price; or the
    /// authentication rejected (401, 403), an answer that refused before any work, which
    /// counts zero (table-manners 4.5; [`FillerCall::refused_before_work`]).
    Known,
    /// Sent with no answer that settles it: timed out, killed, a failure status, a non-zero
    /// exit. Counted at the price, as unresolved; never as zero.
    Unresolved,
}

/// The note `heal` journals before a call, so a run stopped mid-call still shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallStarted {
    /// The filler entry's name.
    pub filler: String,
    /// The entry's price per call (`spend_usd_per_call`, 0 without one).
    pub spend_usd: f64,
}

impl CallStarted {
    /// The journal note text: a fixed prefix and the record as JSON.
    pub fn note(&self) -> String {
        format!(
            "{STARTED_PREFIX}{}",
            serde_json::to_string(self).unwrap_or_default()
        )
    }
}

/// The prefix of a [`CallStarted`] note.
pub const STARTED_PREFIX: &str = "filler call started: ";
/// The prefix of a [`FillerCall`] note.
pub const CALL_PREFIX: &str = "filler call: ";

/// What a run's filler calls cost, from its journal notes.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Spent {
    /// Every known and unresolved call at its price, an answer that refused before any work
    /// at zero.
    pub usd: f64,
    /// The part of `usd` no answer settled.
    pub unresolved_usd: f64,
    /// The total is a known zero, not read from a price (`spend.basis: measured`, table-manners
    /// 4.5): at least one answer refused before any work, and no call counted at its price.
    pub measured: bool,
}

/// The spend in a run's notes: each started call is settled by the call record that follows it
/// (in order), at its price unless not sent or refused before any work; a started call with no
/// record, or a record that does not parse, is unresolved.
pub fn spent(notes: &[String]) -> Spent {
    let mut open: std::collections::VecDeque<f64> = std::collections::VecDeque::new();
    let mut spent = Spent::default();
    let mut answered_free = false;
    let mut priced = false;
    for note in notes {
        if let Some(json) = note.strip_prefix(STARTED_PREFIX) {
            let price = serde_json::from_str::<CallStarted>(json)
                .map(|s| s.spend_usd)
                .unwrap_or(0.0);
            open.push_back(price);
        } else if let Some(json) = note.strip_prefix(CALL_PREFIX) {
            let price = open.pop_front().unwrap_or(0.0);
            match serde_json::from_str::<FillerCall>(json) {
                Ok(call) if call.charge == Charge::NotSent => {}
                Ok(call) if call.refused_before_work() => answered_free = true,
                Ok(call) if call.charge == Charge::Known => {
                    spent.usd += price;
                    priced = true;
                }
                Ok(_) | Err(_) => {
                    spent.usd += price;
                    spent.unresolved_usd += price;
                    priced = true;
                }
            }
        }
    }
    for price in open {
        spent.usd += price;
        spent.unresolved_usd += price;
        priced = true;
    }
    spent.measured = answered_free && !priced;
    spent
}

impl FillerCall {
    /// An answer that refused before any work: the authentication rejected (HTTP 401, 403).
    /// The call is known, and counts zero (table-manners 4.5).
    pub fn refused_before_work(&self) -> bool {
        self.charge == Charge::Known && matches!(self.status, 401 | 403)
    }

    /// The journal note text: a fixed prefix and the record as JSON.
    pub fn note(&self) -> String {
        format!(
            "{CALL_PREFIX}{}",
            serde_json::to_string(self).unwrap_or_default()
        )
    }
}

/// What a call produced: the fill when it succeeded, and the record either way, so a failed
/// call keeps its status, duration and bytes.
pub struct CallOutcome {
    pub fill: Result<DynamicImage>,
    pub call: FillerCall,
}

impl CallOutcome {
    /// The fill, or the error.
    pub fn into_result(self) -> Result<(DynamicImage, FillerCall)> {
        self.fill.map(|fill| (fill, self.call))
    }
}

/// A configured filler.
pub trait Filler: Send + Sync {
    /// The entry's name.
    fn name(&self) -> &str;
    /// Calls the filler with `scratch` for any files the call needs. The answer's dimensions
    /// are asserted here, so a `Ok` fill is always the request's size; the record is
    /// returned whether or not the call succeeded.
    fn call(&self, request: &FillRequest, scratch: &Path) -> CallOutcome;
}

/// The largest answer accepted for a request: eight bytes per pixel (RGBA16) plus a megabyte
/// of headers and chunks; a PNG of the request's size is far below it.
pub fn answer_cap(request: &FillRequest) -> u64 {
    8 * u64::from(request.width) * u64::from(request.height) + (1 << 20)
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
    // The dimensions from the header first, so a wrong size is named before any pixel memory
    // is allocated; then the decode, bounded to the request's size.
    let (width, height) = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| Error::op("filler", format!("{who}: the answer did not decode: {e}")))?
        .into_dimensions()
        .map_err(|e| Error::op("filler", format!("{who}: the answer did not decode: {e}")))?;
    if (width, height) != (request.width, request.height) {
        return Err(Error::op(
            "filler",
            format!(
                "{who}: the answer is {width}x{height} but the request was {}x{}; there is no resize",
                request.width, request.height
            ),
        ));
    }
    rascal_core::image::decode_bytes_within(bytes, request.width, request.height)
        .map_err(|e| Error::op("filler", format!("{who}: the answer did not decode: {e}")))
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

/// `text` with every occurrence of `secret` replaced, for a message that quotes a body.
pub(crate) fn scrub(text: String, secret: Option<&str>) -> String {
    match secret {
        Some(key) if !key.is_empty() => text.replace(key, "[key]"),
        _ => text,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn started(price: f64) -> String {
        CallStarted {
            filler: "f".into(),
            spend_usd: price,
        }
        .note()
    }

    fn call(charge: Charge) -> String {
        answered(charge, -1)
    }

    fn answered(charge: Charge, status: i64) -> String {
        FillerCall {
            filler: "f".into(),
            target: "t".into(),
            request_width: 1,
            request_height: 1,
            request_bytes: 1,
            response_bytes: 0,
            duration_ms: 0,
            status,
            outcome: "x".into(),
            charge,
        }
        .note()
    }

    /// Table-manners 4.5: a 401 or 403 is an answer, so the call is known, not unsent; it
    /// refused before any work, so it counts zero, and a known zero is `measured`.
    #[test]
    fn a_rejected_key_is_known_at_zero_and_measured() {
        for status in [401, 403] {
            let s = spent(&[started(0.04), answered(Charge::Known, status)]);
            assert_eq!(
                s,
                Spent {
                    usd: 0.0,
                    unresolved_usd: 0.0,
                    measured: true
                },
                "{status}"
            );
        }
        // Unsent calls beside it are zero too, and certain.
        let s = spent(&[
            started(0.04),
            call(Charge::NotSent),
            started(0.04),
            answered(Charge::Known, 401),
        ]);
        assert!(s.measured && s.usd == 0.0, "{s:?}");
        // An amount read from a price is an estimate, whatever else the run did.
        let s = spent(&[
            started(0.04),
            answered(Charge::Known, 401),
            started(0.04),
            answered(Charge::Known, 200),
        ]);
        assert!(!s.measured && (s.usd - 0.04).abs() < 1e-9, "{s:?}");
        let s = spent(&[started(0.04), answered(Charge::Known, 401), started(0.04)]);
        assert!(!s.measured, "{s:?}");
        // A zero no answer settled (nothing called, or nothing sent) stays an estimate.
        assert!(!spent(&[]).measured);
        assert!(!spent(&[started(0.04), call(Charge::NotSent)]).measured);
    }

    #[test]
    fn known_and_unresolved_calls_count_and_unsent_ones_do_not() {
        let notes = vec![
            started(0.04),
            call(Charge::Known),
            started(0.04),
            call(Charge::NotSent),
            started(0.25),
            call(Charge::Unresolved),
            "an unrelated note".to_string(),
        ];
        let s = spent(&notes);
        assert!((s.usd - 0.29).abs() < 1e-9, "{s:?}");
        assert!((s.unresolved_usd - 0.25).abs() < 1e-9, "{s:?}");
    }

    #[test]
    fn a_started_call_with_no_record_is_unresolved() {
        let s = spent(&[started(0.04)]);
        assert_eq!(
            s,
            Spent {
                usd: 0.04,
                unresolved_usd: 0.04,
                measured: false
            }
        );
        let s = spent(&[started(0.04), format!("{CALL_PREFIX}not json")]);
        assert_eq!(
            s,
            Spent {
                usd: 0.04,
                unresolved_usd: 0.04,
                measured: false
            }
        );
        assert_eq!(spent(&[]), Spent::default());
    }
}

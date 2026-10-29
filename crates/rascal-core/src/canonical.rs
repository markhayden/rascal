//! Canonical serialization (ADR 0016): the one way a record is turned into bytes before it is
//! hashed, and the one way stored pixels are encoded.
//!
//! - A hashed record is a JSON object with keys in sorted order and no whitespace, numbers in
//!   `serde_json`'s shortest round-trip form, and every reference to another artifact or
//!   frame replaced by that thing's identity (`sha256:<lower hex>`), never a local id, path or
//!   file name. The payload is prefixed by the record kind so a frame and a location with the
//!   same fields cannot collide.
//! - A pixel digest hashes width, height, channel count and the raw samples in row-major
//!   order; it depends on nothing else.
//! - Stored PNGs are 8-bit, not interlaced, with the encoder's default compression and
//!   adaptive filtering. "Byte-identical" images are images with identical decoded samples,
//!   so the encoder settings are a convention for reproducible files, not part of identity.

use std::path::Path;

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{DynamicImage, ImageEncoder};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::identity::{Identity, hex};

/// The canonical JSON text of a value: sorted keys, no whitespace.
pub fn json(value: &serde_json::Value) -> String {
    // `serde_json::Map` is a `BTreeMap` without the `preserve_order` feature, so keys are
    // already sorted; serializing compactly is the rest.
    serde_json::to_string(&sorted(value)).unwrap_or_default()
}

fn sorted(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::new();
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for key in keys {
                out.insert(key.clone(), sorted(&map[key]));
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(sorted).collect())
        }
        other => other.clone(),
    }
}

/// The identity of a record: `sha256` over the kind name, a zero byte and the canonical JSON
/// of the payload, in which references are already identities.
pub fn identity(kind: &str, payload: &serde_json::Value) -> Identity {
    let mut hasher = Sha256::new();
    hasher.update(kind.as_bytes());
    hasher.update([0u8]);
    hasher.update(json(payload).as_bytes());
    Identity::from_digest(&hasher.finalize())
}

/// The identity of a serializable payload.
pub fn identity_of<T: serde::Serialize>(kind: &str, payload: &T) -> Identity {
    identity(kind, &serde_json::to_value(payload).unwrap_or_default())
}

/// The pixel digest: `sha256` over width, height, channel count and the samples.
pub fn pixel_digest(width: u32, height: u32, channels: u32, samples: &[u8]) -> Identity {
    let mut hasher = Sha256::new();
    hasher.update(b"pixels");
    hasher.update([0u8]);
    hasher.update(width.to_le_bytes());
    hasher.update(height.to_le_bytes());
    hasher.update(channels.to_le_bytes());
    hasher.update(samples);
    Identity::from_digest(&hasher.finalize())
}

/// Writes an image as a PNG with the canonical encoder settings.
pub fn write_png(path: &Path, image: &DynamicImage) -> Result<()> {
    let file = std::fs::File::create(path).map_err(|e| Error::io(path, e))?;
    let writer = std::io::BufWriter::new(file);
    let encoder =
        PngEncoder::new_with_quality(writer, CompressionType::Default, FilterType::Adaptive);
    encoder
        .write_image(
            image.as_bytes(),
            image.width(),
            image.height(),
            image.color().into(),
        )
        .map_err(|source| Error::Image {
            path: path.to_path_buf(),
            source,
        })
}

/// Writes an 8-bit grey plane as a PNG with the canonical encoder settings.
pub fn write_gray(path: &Path, plane: &image::GrayImage) -> Result<()> {
    write_png(path, &DynamicImage::ImageLuma8(plane.clone()))
}

/// Lower-case hex, re-exported for callers that hash bytes themselves.
pub fn to_hex(bytes: &[u8]) -> String {
    hex(bytes)
}

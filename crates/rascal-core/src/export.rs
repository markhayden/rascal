//! The compatibility export (SPEC.md "Compatibility export", ADR 0009): a native envelope
//! projected onto the copied envelope schema. The projection keeps `status`, `summary`,
//! `measurements`, `regions`, `images`, `transforms` and `provenance`, drops `artifacts` and
//! every other native field, and fails, naming the item, on content the copied schema cannot
//! express: a `line` or `stroke` region, or an image whose mapping to its parent is not a plain
//! bbox. Nothing is approximated. A standalone location exports as the location object alone
//! and validates against the copied `Location` definition.

use crate::error::{Error, Result};
use crate::frame::{FrameKind, FrameTable};
use crate::location::Location;
use crate::op::Envelope;
use crate::schemas::{COPIED_ENVELOPE, SchemaDir};

/// Projects a native envelope onto the copied envelope's fields. `frames` resolves each
/// image's mapping to its parent.
pub fn project_envelope(envelope: &Envelope, frames: &FrameTable) -> Result<serde_json::Value> {
    for region in &envelope.regions {
        match &region.location {
            Location::Line { .. } | Location::Stroke { .. } => {
                return Err(Error::usage(format!(
                    "export: region {} is a {} location, which the copied schema cannot express",
                    region.id,
                    region.location.kind()
                )));
            }
            _ => {}
        }
    }
    let mut images = Vec::new();
    for image in &envelope.images {
        let frame = frames.require(&image.frame)?;
        let mapping = match frame.kind {
            FrameKind::Raw => serde_json::json!({ "bbox": [0.0, 0.0, 1.0, 1.0] }),
            FrameKind::Crop => {
                let parent = frames.require(frame.parent().unwrap_or_default())?;
                let [l, t, r, b] = frame.bbox.unwrap_or([0, 0, frame.width, frame.height]);
                let (w, h) = (f64::from(parent.width), f64::from(parent.height));
                serde_json::json!({ "bbox": [f64::from(l) / w, f64::from(t) / h, f64::from(r) / w, f64::from(b) / h] })
            }
            FrameKind::Cut => {
                return Err(Error::usage(format!(
                    "export: image {} is in {}, a cut frame whose mapping to its parent is two spans, not a plain bbox, which the copied schema cannot express",
                    image.identity, image.frame
                )));
            }
        };
        images.push(serde_json::json!({
            "frame": image.frame,
            "parent": image.parent,
            "mapping": mapping,
            "media_type": image.media_type,
            "width": image.width,
            "height": image.height,
            "base64": null,
        }));
    }
    let regions: Vec<serde_json::Value> = envelope
        .regions
        .iter()
        .map(|r| serde_json::json!({ "id": r.id, "location": r.location, "label": r.label, "score": r.score }))
        .collect();
    let measurements: Vec<serde_json::Value> = envelope
        .measurements
        .iter()
        .map(|m| serde_json::json!({ "name": m.name, "value": m.value, "unit": m.unit, "frame": m.frame }))
        .collect();
    let transforms: Vec<serde_json::Value> = envelope
        .transforms
        .iter()
        .map(|t| serde_json::json!({ "id": t.id, "from": t.from, "to": t.to, "kind": t.kind, "params": t.params }))
        .collect();
    Ok(serde_json::json!({
        "envelope_version": 1,
        "status": envelope.status,
        "summary": envelope.summary,
        "measurements": measurements,
        "regions": regions,
        "images": images,
        "transforms": transforms,
        "provenance": {
            "tool": envelope.provenance.tool,
            "version": envelope.provenance.version,
            "duration_ms": envelope.provenance.duration_ms,
            "lock_digest": null,
        },
    }))
}

/// A standalone location as the copied `Location` object. `line` and `stroke` cannot be
/// expressed and fail naming the kind.
pub fn project_location(location: &Location) -> Result<serde_json::Value> {
    match location {
        Location::Line { .. } | Location::Stroke { .. } => Err(Error::usage(format!(
            "export: a {} location has no copied form; the copied schema knows bbox, polygon, view, whole_image and absent",
            location.kind()
        ))),
        other => Ok(serde_json::to_value(other).unwrap_or_default()),
    }
}

/// Validates a projected envelope against the copied envelope schema.
pub fn validate_envelope(schemas: &SchemaDir, exported: &serde_json::Value) -> Result<()> {
    let validator = schemas.validator(COPIED_ENVELOPE)?;
    validator.validate(exported).map_err(|e| {
        Error::usage(format!(
            "export does not validate against the copied envelope schema: {e}"
        ))
    })
}

/// Validates a projected location against the copied `Location` definition, by `$ref`.
pub fn validate_location(schemas: &SchemaDir, exported: &serde_json::Value) -> Result<()> {
    let schema =
        serde_json::json!({ "$ref": "urn:rascal:schemas:copied:envelope-v1#/$defs/Location" });
    let validator = schemas.validator_for(&schema).map_err(|message| {
        Error::usage(format!(
            "the copied Location definition cannot be compiled: {message}"
        ))
    })?;
    validator.validate(exported).map_err(|e| {
        Error::usage(format!(
            "export does not validate against the copied Location definition: {e}"
        ))
    })
}

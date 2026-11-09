//! What crosses the wire between rascal and a tool pack (SPEC.md "The MCP server and client"):
//! the **annotation block** a pack puts on each tool, the **context** rascal sends with each
//! call as `_harness`, and the copied-shape manifest the two are joined from. The client that
//! starts packs and calls them lives in `rascal-mcp`; these are the shapes, so the schemas are
//! generated and checked with every other boundary type.
//!
//! A pack's tool carries, in its MCP `_meta`, `manifest` (the copied manifest shape: name,
//! version, the four texts, cost, bounds, the three schemas) and `rascal` (this crate's
//! annotation: the typed ports, the parameter table, the constant sets). rascal builds the
//! native [`Manifest`] from the two and checks it as it checks every op's.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::frame::Frame;
use crate::image::ImageRecord;
use crate::op::{Bounds, Cost, Manifest, Param, Port};

/// The `_meta` key of the copied-shape manifest on a pack's tool.
pub const META_MANIFEST: &str = "manifest";
/// The `_meta` key of rascal's annotation block on a pack's tool.
pub const META_RASCAL: &str = "rascal";
/// The argument rascal adds to every pack call.
pub const HARNESS: &str = "_harness";

/// The most images a pack tool may declare it returns.
pub const MAX_IMAGES: u32 = 64;
/// The largest image a pack tool may declare, in bytes of PNG.
pub const MAX_IMAGE_BYTES: u64 = 64 << 20;
/// The largest envelope a pack tool may declare.
pub const MAX_ENVELOPE_BYTES: u64 = 16 << 20;
/// The largest answer a pack may send on one line of the wire, and the largest request
/// `rascal serve` reads: what the transports are built to carry. A tool's declared images and
/// envelope, base64 included, must fit.
pub const MAX_ANSWER_BYTES: u64 = 256 << 20;

/// The wire size of a tool's largest possible answer: its images as base64 plus its envelope.
pub fn answer_bytes(bounds: &Bounds) -> u64 {
    u64::from(bounds.max_images)
        .saturating_mul(bounds.max_image_bytes)
        .saturating_mul(4)
        .div_ceil(3)
        .saturating_add(bounds.max_envelope_bytes)
}

/// The ceilings a pack tool's declared bounds must stay under: rascal decodes what a pack
/// returns, so a manifest may not promise more than rascal will accept.
pub fn check_bounds(bounds: &Bounds) -> std::result::Result<(), String> {
    if bounds.max_images > MAX_IMAGES {
        return Err(format!(
            "max_images {} is over rascal's ceiling {MAX_IMAGES}",
            bounds.max_images
        ));
    }
    if bounds.max_long_edge > crate::image::MAX_SIDE {
        return Err(format!(
            "max_long_edge {} is over rascal's ceiling {}",
            bounds.max_long_edge,
            crate::image::MAX_SIDE
        ));
    }
    if bounds.max_image_bytes > MAX_IMAGE_BYTES {
        return Err(format!(
            "max_image_bytes {} is over rascal's ceiling {MAX_IMAGE_BYTES}",
            bounds.max_image_bytes
        ));
    }
    if bounds.max_envelope_bytes > MAX_ENVELOPE_BYTES {
        return Err(format!(
            "max_envelope_bytes {} is over rascal's ceiling {MAX_ENVELOPE_BYTES}",
            bounds.max_envelope_bytes
        ));
    }
    if bounds.max_images > 0 && (bounds.max_long_edge == 0 || bounds.max_image_bytes == 0) {
        return Err("a tool that returns images needs max_long_edge and max_image_bytes".into());
    }
    let answer = answer_bytes(bounds);
    if answer > MAX_ANSWER_BYTES {
        return Err(format!(
            "max_images × max_image_bytes as base64 plus max_envelope_bytes is {answer} bytes, over the {MAX_ANSWER_BYTES} one answer may carry"
        ));
    }
    Ok(())
}

/// rascal's annotation block on a pack tool: what the copied manifest shape does not carry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackAnnotation {
    /// The block's version.
    pub annotation_version: u32,
    /// The input ports, by name: each is an argument taking an artifact id.
    pub inputs: Vec<Port>,
    /// The output ports; a `many` port takes the envelope's regions (a `Location` port) or
    /// its images (an `Image` port) in order, named `<port>_<n>`.
    pub outputs: Vec<Port>,
    /// The parameters, by name: each is an argument.
    #[serde(default)]
    pub params: Vec<Param>,
    /// The constant sets a step may select; the first is the default.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constants: Vec<String>,
}

/// The copied manifest shape (`schemas/copied/manifest-v1.json`), typed: the fields rascal
/// reads are named; the rest are kept as given so a manifest written for the other harness in
/// the family passes unchanged. Closed, as the copied schema is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompatManifest {
    #[serde(default = "one")]
    pub manifest_version: u32,
    pub name: String,
    pub version: String,
    pub purpose: String,
    pub when_to_use: String,
    pub when_not_to_use: String,
    pub output_guidance: String,
    pub cost: Cost,
    pub returns: Bounds,
    pub model_schema: serde_json::Value,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
    /// Whether the tool may run calls at once; rascal ANDs it with the config.
    #[serde(default)]
    pub concurrent: bool,
    #[serde(default)]
    pub accepts_hints: Vec<String>,
    #[serde(default)]
    pub evidence_kind: Option<String>,
    #[serde(default)]
    pub frame_args: Vec<String>,
    #[serde(default)]
    pub frames_out: Vec<String>,
    #[serde(default)]
    pub image_args: Vec<String>,
    #[serde(default)]
    pub pairs_with: Vec<String>,
    #[serde(default)]
    pub preconditions: String,
    #[serde(default)]
    pub requires: Vec<String>,
}

fn one() -> u32 {
    1
}

impl CompatManifest {
    /// The native manifest: the copied fields plus the annotation's ports, parameters and
    /// constant sets, with the schemas derived as for every op. The result is checked as the
    /// registry checks every manifest.
    pub fn with_annotation(&self, annotation: &PackAnnotation) -> Result<Manifest> {
        if annotation.annotation_version != 1 {
            return Err(Error::op(
                &self.name,
                format!(
                    "annotation_version {} is not supported (expected 1)",
                    annotation.annotation_version
                ),
            ));
        }
        if self.manifest_version != 1 {
            return Err(Error::op(
                &self.name,
                format!(
                    "manifest_version {} is not supported (expected 1)",
                    self.manifest_version
                ),
            ));
        }
        let mut manifest = Manifest::new(
            &self.name,
            &self.version,
            &self.purpose,
            &self.when_to_use,
            &self.when_not_to_use,
            &self.output_guidance,
            self.cost.clone(),
            annotation.inputs.clone(),
            annotation.outputs.clone(),
            annotation.params.clone(),
        );
        manifest.constants = annotation.constants.clone();
        manifest.returns = self.returns.clone();
        manifest
            .check()
            .map_err(|m| Error::op(&self.name, format!("manifest rejected: {m}")))?;
        check_bounds(&manifest.returns)
            .map_err(|m| Error::op(&self.name, format!("manifest rejected: {m}")))?;
        Ok(manifest)
    }
}

/// One image a call references, as the pack receives it: the port it was bound to, the
/// artifact, its frame and size, and the pixels by path or as base64 PNG bytes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContextImage {
    /// The input port the image was bound to.
    pub port: String,
    /// The workspace id the argument carries.
    pub id: String,
    pub identity: String,
    pub frame: String,
    pub width: u32,
    pub height: u32,
    /// The PNG's absolute path, when the pack accepts paths.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The PNG as standard base64, when the pack accepts base64.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base64: Option<String>,
    /// The image record.
    pub record: ImageRecord,
}

/// rascal's client-side context on a pack call, the `_harness` argument: the run and call
/// ids, the referenced images, and every frame the pack may name in its answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackContext {
    /// The context's version.
    pub context_version: u32,
    /// The run (the workspace root, as the pack would see it in a path).
    pub run_id: String,
    /// This call, unique within the run.
    pub call_id: String,
    pub images: Vec<ContextImage>,
    /// The frames the images are in, with their parents up to the root: a region or an image
    /// the pack returns must be in one of them.
    pub frames: Vec<Frame>,
}

impl PackContext {
    /// The frame ids a returned region or image may name.
    pub fn frame_ids(&self) -> BTreeMap<String, &Frame> {
        self.frames.iter().map(|f| (f.id.clone(), f)).collect()
    }
}

/// The Python pack's bundle manifest, `bundle.toml` (SPEC.md "The Python pack and its
/// bundle"): what `rascal tools setup` verifies before it installs. Every file in the bundle
/// is listed with its digest; nothing else is installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BundleManifest {
    pub bundle_version: u32,
    /// The pack's distribution name (`rascal-tools`).
    pub pack: String,
    pub pack_version: String,
    /// The interpreter line the bundle was built for (`3.12`).
    pub python: String,
    /// `macos-arm64` or `linux-x86_64`.
    pub platform: String,
    /// The pack wheel's path inside the bundle.
    pub wheel: String,
    /// Every file in the bundle by relative path, with its `sha256:<hex>` digest.
    pub files: BTreeMap<String, String>,
}

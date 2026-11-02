//! Previews (CONTRACT 4.7; PLAN.md 10.6): an artifact of a type that declares `preview: true`
//! drawn as a PNG for people. A Mask is `inspect`'s overlay on its frame's image when the store
//! holds that image, else its grey rendering (`render`); a location is its rasterized outline
//! drawn the same way on its image (on a plain canvas of its frame when the store holds no
//! image there), or that image alone when the location has no interior (`view`, `absent`).
//! The ops run in memory, never through the store: drawing commits nothing. Deterministic, so
//! the same artifact in the same store always gives the same bytes. A preview never stands for
//! the artifact; a host passes the reference on.

use std::collections::BTreeSet;
use std::sync::Arc;

use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::frame::{Frame, FrameTable};
use rascal_core::image::{Image, Role};
use rascal_core::op::{Context, Inputs, Params};
use rascal_core::registry::OpRegistry;
use rascal_core::workspace::Workspace;

/// The grey a canvas is drawn in when the store holds no image of a location's frame.
const CANVAS_GREY: u8 = 96;

/// The preview of the artifact `id` (a local id or identity) as PNG bytes, with the frame it
/// is drawn in; `None` for a type without one.
pub fn draw(store: &Workspace, ops: &OpRegistry, id: &str) -> Result<Option<(Vec<u8>, Frame)>> {
    let artifact = store.load(id)?;
    if !artifact.artifact_type().preview() {
        return Ok(None);
    }
    let frames = store.frames()?;
    let frame = frames.require(artifact.frame())?.clone();
    let image = frame_image(store, artifact.identity().as_str(), &frame.id)?
        .map(|image_id| store.load(&image_id))
        .transpose()?;
    let picture = match &artifact {
        Artifact::Mask(_) => match &image {
            Some(image) => overlay(ops, &frames, &artifact, image)?,
            None => {
                let mut params = Params::new();
                params.insert("for".to_string(), toml::Value::String("grey".to_string()));
                let mut inputs = Inputs::new();
                inputs.insert("mask".to_string(), artifact.clone());
                run(ops, &frames, "render", &inputs, &params, "image")?
            }
        },
        Artifact::Location { .. } => {
            let image = match image {
                Some(image) => image,
                None => canvas(&frame),
            };
            let mut inputs = Inputs::new();
            inputs.insert("location".to_string(), artifact.clone());
            inputs.insert("image".to_string(), image.clone());
            match run_optional(ops, &frames, "rasterize", &inputs, &Params::new(), "mask")? {
                Some(mask) => overlay(ops, &frames, &mask, &image)?,
                None => image,
            }
        }
        _ => return Ok(None),
    };
    let Artifact::Image(picture) = picture else {
        return Err(Error::op("preview", "the drawing is not an image"));
    };
    Ok(Some((png(&picture)?, frame)))
}

/// The image an artifact was drawn on in `frame`: following the records back from the artifact
/// (the outputs of the record that made it, then its inputs, nearest first), the first image in
/// that frame; else the one imported image the store holds in it. `None` when there is none, or
/// several and nothing tells them apart.
pub fn frame_image(store: &Workspace, identity: &str, frame: &str) -> Result<Option<String>> {
    let generation = store.snapshot()?;
    let records = store.records()?;
    let is_image_here = |identity: &str| {
        generation
            .artifact(identity)
            .filter(|e| e.artifact_type == ArtifactType::Image && e.frame == frame)
            .map(|e| e.id.clone())
    };
    let producer = |identity: &str| {
        records.iter().rev().find(|r| {
            r.outputs.values().any(|o| o == identity)
                || r.envelope
                    .artifacts
                    .values()
                    .any(|a| a.identity == identity)
        })
    };
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut frontier = vec![identity.to_string()];
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for current in frontier {
            if !seen.insert(current.clone()) {
                continue;
            }
            let Some(record) = producer(&current) else {
                continue;
            };
            for candidate in record.outputs.values().chain(record.inputs.values()) {
                if candidate.as_str() != identity
                    && let Some(id) = is_image_here(candidate)
                {
                    return Ok(Some(id));
                }
            }
            next.extend(record.inputs.values().cloned());
        }
        frontier = next;
    }
    let mut imported: Vec<&str> = generation
        .artifacts
        .iter()
        .filter(|a| a.artifact_type == ArtifactType::Image && a.frame == frame)
        .filter(|a| rascal_core::contract::import_file_sha256(&records, &a.identity).is_some())
        .map(|a| a.id.as_str())
        .collect();
    imported.sort_unstable();
    imported.dedup();
    Ok(match imported.as_slice() {
        [one] => Some((*one).to_string()),
        _ => None,
    })
}

/// `inspect`'s overlay of `mask` on `image`.
fn overlay(
    ops: &OpRegistry,
    frames: &FrameTable,
    mask: &Artifact,
    image: &Artifact,
) -> Result<Artifact> {
    let mut inputs = Inputs::new();
    inputs.insert("mask".to_string(), mask.clone());
    inputs.insert("image".to_string(), image.clone());
    run(ops, frames, "inspect", &inputs, &Params::new(), "overlay")
}

/// One op in memory, its parameters resolved as a step's, and the output `port`.
fn run(
    ops: &OpRegistry,
    frames: &FrameTable,
    name: &str,
    inputs: &Inputs,
    params: &Params,
    port: &str,
) -> Result<Artifact> {
    run_optional(ops, frames, name, inputs, params, port)?
        .ok_or_else(|| Error::op("preview", format!("{name} drew nothing")))
}

/// [`run`], `None` when the op is not applicable or makes nothing on `port`.
fn run_optional(
    ops: &OpRegistry,
    frames: &FrameTable,
    name: &str,
    inputs: &Inputs,
    params: &Params,
    port: &str,
) -> Result<Option<Artifact>> {
    let op = ops.require(name)?;
    let resolved = rascal_core::op::resolve_step(op, params)
        .map_err(|e| Error::op("preview", format!("{name}: {e:?}")))?;
    let output = op.run(&Context { frames }, inputs, &resolved.params)?;
    Ok(output.outputs.get(port).cloned())
}

/// A plain canvas of `frame`, for a location whose image the store does not hold.
fn canvas(frame: &Frame) -> Artifact {
    let pixels = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        frame.width,
        frame.height,
        image::Rgb([CANVAS_GREY; 3]),
    ));
    Artifact::Image(Arc::new(Image::new(
        "preview.canvas",
        pixels,
        frame,
        Role::Working,
        None,
    )))
}

/// An image's pixels as PNG bytes.
pub fn png(image: &Image) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    image
        .pixels
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .map_err(|source| Error::Image {
            path: "<preview>".into(),
            source,
        })?;
    Ok(bytes)
}

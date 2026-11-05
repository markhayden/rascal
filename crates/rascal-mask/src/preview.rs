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
use rascal_core::contract::{
    self,
    table_manners::images::{ImageOut, ImageReference, Locator},
};
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
        Artifact::BandProfile(band) => {
            let image = match image {
                Some(image) => image,
                None => canvas(&frame),
            };
            let mut inputs = Inputs::new();
            inputs.insert("band".to_string(), artifact.clone());
            let mask = run(ops, &frames, "profile", &inputs, &Params::new(), "mask")?;
            let drawn = overlay(ops, &frames, &mask, &image)?;
            let Artifact::Image(drawn) = drawn else {
                return Err(Error::op("preview", "the drawing is not an image"));
            };
            return Ok(Some((
                png_pixels(&with_width_chart(&drawn.pixels, &band.half, band.thin_px))?,
                frame,
            )));
        }
        _ => return Ok(None),
    };
    let Artifact::Image(picture) = picture else {
        return Err(Error::op("preview", "the drawing is not an image"));
    };
    Ok(Some((png(&picture)?, frame)))
}

/// A preview's PNG `bytes`, drawn in `frame`, as the image out a result or the artifact
/// resource lists (role `preview`, CONTRACT 4.7), located by `locator`.
pub fn image_out(bytes: &[u8], frame: &Frame, locator: Locator) -> Result<ImageOut> {
    let (width, height) = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| Error::op("preview", e.to_string()))?
        .into_dimensions()
        .map_err(|source| Error::Image {
            path: "<preview>".into(),
            source,
        })?;
    Ok(ImageOut {
        role: "preview".to_string(),
        image: ImageReference {
            file_sha256: contract::file_sha256(bytes),
            width,
            height,
            projection: contract::projection(frame),
            topology: contract::topology(frame),
            pixel_sha256: None,
        },
        locator,
        label: None,
    })
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

/// The band drawn on its image with a chart beside it: one bar per row, its length the row's
/// half-width against the widest row's, and a line where the thin minimum sits. Read top to
/// bottom alongside the image, the bars say where fan widened the band and where it held thin.
fn with_width_chart(drawn: &image::DynamicImage, half: &[f64], thin: f64) -> image::DynamicImage {
    let picture = drawn.to_rgb8();
    let (width, height) = picture.dimensions();
    let panel = (width / 3).clamp(160, 480);
    let margin = 12u32;
    let mut out = image::RgbImage::from_pixel(width + panel, height, image::Rgb([24, 26, 26]));
    image::imageops::replace(&mut out, &picture, 0, 0);
    let widest = half.iter().copied().fold(thin.max(1.0), f64::max);
    let scale = f64::from(panel - 2 * margin) / widest;
    let x0 = width + margin;
    for y in 0..height {
        let row = half.get(y as usize).copied().unwrap_or(0.0);
        let length = (row * scale).round().max(0.0) as u32;
        for x in x0..(x0 + length).min(width + panel - margin) {
            out.put_pixel(x, y, image::Rgb([236, 64, 226]));
        }
    }
    let thin_x = x0 + (thin * scale).round() as u32;
    if thin_x < width + panel {
        for y in 0..height {
            out.put_pixel(thin_x, y, image::Rgb([164, 170, 165]));
        }
    }
    for y in 0..height {
        out.put_pixel(width, y, image::Rgb([52, 55, 53]));
    }
    image::DynamicImage::ImageRgb8(out)
}

/// Pixels as PNG bytes.
fn png_pixels(pixels: &image::DynamicImage) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    pixels
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

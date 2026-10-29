//! `rascal render <mask> --for <profile> [--image <path>] [--threshold t] [--colour #rrggbb]
//! [--strength s] --out <file>`: a stored mask rendered to a file destination, which is
//! overwritten. The file's format follows its extension: PNG by default, JPEG for `.jpg`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rascal_core::Error;
use rascal_core::artifact::Artifact;
use rascal_core::error::Result;
use rascal_core::frame::{Frame, Projection};
use rascal_core::image::Image;
use rascal_core::mask::Mask;
use rascal_core::op::Params;
use rascal_core::outdir::{self, OutKind};
use rascal_core::recipe::Run;
use rascal_core::registry::OpRegistry;

/// What `render` was asked to do.
pub struct RenderArgs {
    pub mask: PathBuf,
    pub profile: String,
    pub image: Option<PathBuf>,
    pub threshold: Option<i64>,
    pub colour: Option<String>,
    pub strength: Option<f64>,
    pub out: PathBuf,
}

/// Renders a stored mask to a file destination.
pub fn render(registry: &OpRegistry, args: &RenderArgs) -> Result<PathBuf> {
    let mask = Mask::read(&args.mask)?;
    let out = outdir::prepare(&args.out, OutKind::File)?;
    // A stored mask may live anywhere; the op runs in a throwaway workspace that knows the
    // mask's frame, rebuilt from its record (a raw frame is its size and projection).
    let scratch = std::env::temp_dir().join(format!(
        "rascal-render-{}-{}",
        std::process::id(),
        mask.identity().hex()
    ));
    std::fs::create_dir_all(&scratch).map_err(|e| Error::io(&scratch, e))?;
    let result = (|| {
        let run = Run::open(&scratch)?;
        let frame = frame_of(&mask);
        let mut inputs = BTreeMap::new();
        inputs.insert("mask".to_string(), Artifact::Mask(Arc::new(mask.clone())));
        match &args.image {
            Some(path) => {
                let (_, image_frame, image) = run.workspace.import(path, Some(frame.projection))?;
                if image_frame.id != frame.id {
                    return Err(Error::usage(format!(
                        "{}: the image is in frame {} but the mask is in {}",
                        path.display(),
                        image_frame.id,
                        frame.id
                    )));
                }
                inputs.insert("image".to_string(), image);
            }
            None => {
                // The frame must be known to the scratch workspace: a blank canonical image of
                // the mask's size stands in, which registers the frame without a photograph.
                let blank = image::DynamicImage::ImageLuma8(image::GrayImage::new(
                    frame.width,
                    frame.height,
                ));
                run.workspace.add_canonical(
                    Image::new(
                        "blank",
                        blank,
                        &frame,
                        rascal_core::image::Role::Canonical,
                        None,
                    ),
                    &frame,
                    "blank",
                )?;
            }
        }
        let mut params = Params::new();
        params.insert("for".to_string(), toml::Value::String(args.profile.clone()));
        if let Some(t) = args.threshold {
            params.insert("threshold".to_string(), toml::Value::Integer(t));
        }
        if let Some(c) = &args.colour {
            params.insert("colour".to_string(), toml::Value::String(c.clone()));
        }
        if let Some(s) = args.strength {
            params.insert("strength".to_string(), toml::Value::Float(s));
        }
        let output = run.invoke("render", "render", registry, &inputs, &params)?;
        match output.outputs.get("image") {
            Some(Artifact::Image(image)) => write_by_extension(&out, &image.pixels),
            _ => Err(Error::op("render", output.envelope.summary)),
        }
    })();
    let _ = std::fs::remove_dir_all(&scratch);
    result.map(|()| out)
}

/// PNG unless the destination says `.jpg` or `.jpeg`.
pub(crate) fn write_by_extension(path: &Path, pixels: &image::DynamicImage) -> Result<()> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "jpg" || ext == "jpeg" {
        let rgb = image::DynamicImage::ImageRgb8(pixels.to_rgb8());
        rgb.save_with_format(path, image::ImageFormat::Jpeg)
            .map_err(|source| Error::Image {
                path: path.to_path_buf(),
                source,
            })
    } else {
        rascal_core::canonical::write_png(path, pixels)
    }
}

/// The frame a stored mask is in, rebuilt from its record: a mask in a raw frame carries
/// its size and the frame id names the projection. A mask in a derived frame needs its
/// workspace, which `rascal <op> --workspace` provides.
pub(crate) fn frame_of(mask: &Mask) -> Frame {
    let (width, height) = mask.dimensions();
    let projection = if mask.record.frame.ends_with(":equirectangular") {
        Projection::Equirectangular
    } else {
        Projection::Flat
    };
    Frame::raw(width, height, projection)
}

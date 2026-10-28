//! `rascal run <recipe> <image> --out <dir>`: import the image, bind the inputs, run the
//! recipe, write the run directory. And `rascal render <mask> --for <profile> --out <file>`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rascal_core::Error;
use rascal_core::artifact::Artifact;
use rascal_core::error::Result;
use rascal_core::frame::Projection;
use rascal_core::image::Image;
use rascal_core::location::Location;
use rascal_core::mask::Mask;
use rascal_core::op::{Params, Status};
use rascal_core::outdir::{self, OutKind};
use rascal_core::recipe::{Recipe, Run};
use rascal_core::registry::OpRegistry;

/// The recipes embedded in the binary, by name.
const SHIPPED: &[(&str, &str)] = &[("rect", include_str!("../recipes/rect.toml"))];

/// A shipped recipe by name, or a recipe file by path.
pub fn load_recipe(name_or_path: &str) -> Result<Recipe> {
    if let Some((_, text)) = SHIPPED.iter().find(|(name, _)| *name == name_or_path) {
        return Recipe::parse(text, &format!("shipped recipe {name_or_path}"));
    }
    let path = Path::new(name_or_path);
    if path.is_file() {
        return Recipe::read(path);
    }
    let names: Vec<&str> = SHIPPED.iter().map(|(n, _)| *n).collect();
    Err(Error::usage(format!(
        "{name_or_path}: not a shipped recipe ({}) and not a file",
        names.join(", ")
    )))
}

/// What `run` was asked to do.
pub struct RunArgs {
    pub recipe: String,
    pub image: PathBuf,
    pub out: PathBuf,
    pub bbox: Option<String>,
    pub projection: Option<Projection>,
    pub strict: bool,
}

/// What `run` did.
pub struct RunReport {
    pub status: Status,
    pub stopped_at: Option<String>,
    pub mask_dir: Option<PathBuf>,
    pub renders: Vec<(String, PathBuf)>,
}

/// Runs a recipe. The run directory must be new or empty.
pub fn run(registry: &OpRegistry, args: &RunArgs) -> Result<RunReport> {
    let recipe = load_recipe(&args.recipe)?;
    recipe.validate(registry)?;
    let bbox = args.bbox.as_deref().map(Location::parse_bbox).transpose()?;
    if recipe.inputs.contains_key("location") && bbox.is_none() {
        return Err(Error::usage(format!(
            "recipe {} needs inputs.location: give --bbox x0,y0,x1,y1",
            recipe.name
        )));
    }
    if !args.image.is_file() {
        return Err(Error::io(
            &args.image,
            std::io::Error::new(std::io::ErrorKind::NotFound, "no such image"),
        ));
    }
    let out = outdir::prepare(&args.out, OutKind::RunDir)?;
    let mut run = Run::open(&out)?;

    let (image, frame, assumed) =
        Image::import(&args.image, args.projection.or(recipe.projection))?;
    if assumed {
        run.note(format!(
            "projection assumed equirectangular: {} is exactly 2:1 and --projection was not given",
            args.image.display()
        ))?;
    }
    let frame = run.frames.insert(frame).clone();
    let mut inputs: BTreeMap<String, Artifact> = BTreeMap::new();
    inputs.insert("image".to_string(), Artifact::Image(Arc::new(image)));
    if let Some(bbox) = bbox {
        let location = Location::bbox(&frame.id, bbox)?;
        location.validate(&frame)?;
        let identity = location.identity(&frame.identity);
        inputs.insert(
            "location".to_string(),
            Artifact::Location { location, identity },
        );
    }

    let outcome = recipe.run(registry, inputs, &mut run)?;
    let mask_dir = match outcome.outputs.get("mask") {
        Some(Artifact::Mask(mask)) => Some(mask.path_in(&out.join("artifacts"))),
        _ => None,
    };
    Ok(RunReport {
        status: outcome.status,
        stopped_at: outcome.stopped_at,
        mask_dir,
        renders: outcome.renders,
    })
}

/// Renders a stored mask to a file destination, which is overwritten.
pub fn render(
    registry: &OpRegistry,
    mask_dir: &Path,
    profile: &str,
    out: &Path,
) -> Result<PathBuf> {
    let mask = Mask::read(mask_dir)?;
    let out = outdir::prepare(out, OutKind::File)?;
    // A throwaway run beside nothing: the render op needs a frame table and a journal, and a
    // file destination has no run directory. REPLACED BY 1.7a (a private workspace).
    let scratch = std::env::temp_dir().join(format!(
        "rascal-render-{}-{}",
        std::process::id(),
        mask.identity().hex()
    ));
    std::fs::create_dir_all(&scratch).map_err(|e| Error::io(&scratch, e))?;
    let result = (|| {
        let mut run = Run::open(&scratch)?;
        run.frames.insert(frame_of(&mask));
        let mut inputs = BTreeMap::new();
        inputs.insert("mask".to_string(), Artifact::Mask(Arc::new(mask.clone())));
        let mut params = Params::new();
        params.insert("for".to_string(), toml::Value::String(profile.to_string()));
        let output = run.invoke("render", "render", registry, &inputs, &params)?;
        match output.outputs.get("image") {
            Some(Artifact::Image(image)) => image
                .pixels
                .save_with_format(&out, image::ImageFormat::Png)
                .map_err(|source| Error::Image {
                    path: out.clone(),
                    source,
                }),
            _ => Err(Error::op("render", output.envelope.summary)),
        }
    })();
    let _ = std::fs::remove_dir_all(&scratch);
    result.map(|()| out)
}

/// The frame a stored mask is in, rebuilt from its record. REPLACED BY 1.7a (frames come from
/// the workspace registry, not from the mask).
fn frame_of(mask: &Mask) -> rascal_core::frame::Frame {
    let (width, height) = mask.dimensions();
    let projection = if mask.record.frame.ends_with(":equirectangular") {
        Projection::Equirectangular
    } else {
        Projection::Flat
    };
    rascal_core::frame::Frame::raw(width, height, projection)
}

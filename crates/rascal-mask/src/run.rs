//! `rascal run <recipe> <image> --out <dir>`: import the image, bind the inputs, run the
//! recipe, write the run directory. And `rascal render <mask> --for <profile> --out <file>`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rascal_core::Error;
use rascal_core::artifact::Artifact;
use rascal_core::error::Result;
use rascal_core::frame::Projection;
use rascal_core::location::Location;
use rascal_core::mask::Mask;
use rascal_core::op::Status;
use rascal_core::outdir::{self, OutKind};
use rascal_core::recipe::{InputType, Recipe, Run};
use rascal_core::registry::OpRegistry;
use rascal_core::workspace::Workspace;

pub use rascal_mask::recipes::load_recipe;

/// What `run` was asked to do.
pub struct RunArgs {
    pub recipe: String,
    pub image: PathBuf,
    pub out: PathBuf,
    pub bbox: Option<String>,
    pub location: Option<PathBuf>,
    pub stroke: Option<PathBuf>,
    pub protect: Option<PathBuf>,
    pub mask: Option<PathBuf>,
    pub prompt: Option<String>,
    pub config: Option<PathBuf>,
    pub workspace: Option<PathBuf>,
    pub projection: Option<Projection>,
    pub strict: bool,
    /// A recorded run whose pack calls answer instead of the packs (G4).
    pub replay: Option<PathBuf>,
}

/// A bound and executed recipe, with the registry it ran in (the shipped ops plus the
/// configured packs' tools) and the packs themselves, which stop when dropped.
pub struct Prepared {
    pub run: Run,
    pub outcome: rascal_core::recipe::Outcome,
    pub image: Artifact,
    pub registry: OpRegistry,
    pub packs: Vec<Arc<rascal_mcp::client::Pack>>,
    /// The recipe as loaded, so `heal` knows what `--prompt` bound.
    pub recipe: Recipe,
}

/// What `run` did.
pub struct RunReport {
    pub status: Status,
    pub stopped_at: Option<String>,
    pub reason: Option<String>,
    pub mask_dir: Option<PathBuf>,
    pub renders: Vec<(String, PathBuf)>,
}

/// Whether a recipe declares `inputs.prompt` as text, which `--prompt` binds.
pub fn declares_prompt(recipe: &Recipe) -> bool {
    recipe
        .inputs
        .get("prompt")
        .is_some_and(|d| d.kind == InputType::Text)
}

/// Runs a recipe. The run directory must be new or empty. Every declared input is bound from
/// its flag: `image` from the positional path, `location` from `--bbox` or `--location`,
/// `stroke` from `--stroke`, `protect` and `mask` from stored mask directories, `prompt`
/// (text) from `--prompt`; a declared input without its flag, or a flag without its input,
/// is a usage error.
pub fn run(args: &RunArgs) -> Result<RunReport> {
    if args.prompt.is_some() && !declares_prompt(&load_recipe(&args.recipe)?) {
        return Err(Error::usage(format!(
            "recipe {} declares no inputs.prompt; --prompt does not apply (remove-prompt takes one)",
            args.recipe
        )));
    }
    let Prepared { run, outcome, .. } = run_recipe(args)?;
    let mask_dir = match outcome.outputs.get("mask") {
        Some(Artifact::Mask(mask)) => Some(mask.path_in(&run.workspace.root.join("artifacts"))),
        _ => None,
    };
    Ok(RunReport {
        status: outcome.status,
        stopped_at: outcome.stopped_at,
        reason: outcome.reason,
        mask_dir,
        renders: outcome.renders,
    })
}

/// Binds the inputs and runs the recipe; `heal` continues from the outcome. The registry is
/// built here: the shipped ops plus every configured pack's tools, journaling into the run's
/// workspace (so the run directory exists before the recipe is validated).
pub fn run_recipe(args: &RunArgs) -> Result<Prepared> {
    let config = args
        .config
        .as_deref()
        .map(rascal_core::config::Config::read)
        .transpose()?;
    let recipe = load_recipe(&args.recipe)?;
    if declares_prompt(&recipe) && args.prompt.is_none() {
        return Err(Error::usage(format!(
            "recipe {} needs inputs.prompt: give --prompt",
            recipe.name
        )));
    }
    let declared = |name: &str| recipe.inputs.contains_key(name);
    let optional = |name: &str| recipe.inputs.get(name).is_some_and(|d| d.optional);
    for (flag, given, input) in [
        (
            "--bbox or --location",
            args.bbox.is_some() || args.location.is_some(),
            "location",
        ),
        ("--stroke", args.stroke.is_some(), "stroke"),
        ("--protect", args.protect.is_some(), "protect"),
        ("--mask", args.mask.is_some(), "mask"),
    ] {
        if given && !declared(input) {
            return Err(Error::usage(format!(
                "recipe {} declares no inputs.{input}; {flag} does not apply",
                recipe.name
            )));
        }
        if !given && declared(input) && !optional(input) {
            return Err(Error::usage(format!(
                "recipe {} needs inputs.{input}: give {flag}",
                recipe.name
            )));
        }
    }
    if !declared("image") {
        return Err(Error::usage(format!(
            "recipe {} declares no inputs.image; every run takes an image",
            recipe.name
        )));
    }
    let bbox = args.bbox.as_deref().map(Location::parse_bbox).transpose()?;
    if !args.image.is_file() {
        return Err(Error::io(
            &args.image,
            std::io::Error::new(std::io::ErrorKind::NotFound, "no such image"),
        ));
    }
    let out = outdir::prepare(&args.out, OutKind::RunDir)?;
    let run = match &args.workspace {
        Some(dir) => Run::open_in(&out, Arc::new(Workspace::open(dir)?))?,
        None => Run::open(&out)?,
    };
    let (registry, packs) =
        crate::packs::registry_for(config.as_ref(), &run.workspace, args.replay.as_deref())?;
    recipe.validate(&registry).map_err(|e| {
        let no_packs = config.as_ref().is_none_or(|c| c.packs.is_empty());
        match e {
            Error::Recipe {
                recipe,
                step,
                message,
            } if no_packs && message.starts_with("unknown op ") => Error::Recipe {
                recipe,
                step,
                message: format!(
                    "{message}; it is not a shipped op, so it needs a configured pack (--config with a [packs.*] entry that serves it)"
                ),
            },
            other => other,
        }
    })?;

    let (_, frame, image) = run
        .workspace
        .import(&args.image, args.projection.or(recipe.projection))?;
    let mut inputs: BTreeMap<String, Artifact> = BTreeMap::new();
    inputs.insert("image".to_string(), image);
    if let Some(bbox) = bbox {
        let location = Location::bbox(&frame.id, bbox)?;
        location.validate(&frame)?;
        let (_, artifact) = run.workspace.add_location(location)?;
        inputs.insert("location".to_string(), artifact);
    }
    if let Some(path) = &args.location {
        let location = location_from_file(path, &frame, None)?;
        let (_, artifact) = run.workspace.add_location(location)?;
        inputs.insert("location".to_string(), artifact);
    }
    if let Some(path) = &args.stroke {
        let location = location_from_file(path, &frame, Some("stroke"))?;
        let (_, artifact) = run.workspace.add_location(location)?;
        inputs.insert("stroke".to_string(), artifact);
    }
    for (name, path) in [("protect", &args.protect), ("mask", &args.mask)] {
        if let Some(path) = path {
            let mask = Mask::read(path)?;
            // The mask is in the image's frame, or (for protect's second pass) in a frame
            // derived from it that the workspace knows: a cut canvas or a crop of it.
            if mask.record.frame != frame.id {
                let root = run
                    .workspace
                    .frames()?
                    .chain(&mask.record.frame)
                    .ok()
                    .and_then(|chain| chain.last().map(|f| f.id.clone()));
                if root.as_deref() != Some(frame.id.as_str()) {
                    return Err(Error::usage(format!(
                        "{}: the mask is in frame {} but the image is in {}{}",
                        path.display(),
                        mask.record.frame,
                        frame.id,
                        if root.is_none() {
                            "; the workspace does not know the mask's frame (use --workspace from the run that made it)"
                        } else {
                            ""
                        }
                    )));
                }
            }
            inputs.insert(name.to_string(), Artifact::Mask(Arc::new(mask)));
        }
    }

    let mut texts: BTreeMap<String, String> = BTreeMap::new();
    if let Some(prompt) = &args.prompt
        && declares_prompt(&recipe)
    {
        texts.insert("prompt".to_string(), prompt.clone());
    }
    let input_image = inputs["image"].clone();
    let outcome = recipe.run(&registry, inputs, &texts, &run)?;
    Ok(Prepared {
        run,
        outcome,
        image: input_image,
        registry,
        packs,
        recipe,
    })
}

/// A location from a JSON file, validated against the schema and the image's frame. The file
/// may name the frame by its id or leave `frame` out; the image's frame is used.
fn location_from_file(
    path: &Path,
    frame: &rascal_core::frame::Frame,
    kind: Option<&str>,
) -> Result<Location> {
    let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
    let mut value: serde_json::Value =
        serde_json::from_str(&text).map_err(|source| Error::Json {
            path: path.to_path_buf(),
            source,
        })?;
    if let Some(object) = value.as_object_mut() {
        match object.get("frame").and_then(|f| f.as_str()) {
            None => {
                object.insert(
                    "frame".to_string(),
                    serde_json::Value::String(frame.id.clone()),
                );
            }
            Some(named) if named != frame.id => {
                return Err(Error::usage(format!(
                    "{}: names frame {named} but the image is in {}",
                    path.display(),
                    frame.id
                )));
            }
            Some(_) => {}
        }
    }
    let location: Location = serde_json::from_value(value)
        .map_err(|e| Error::usage(format!("{}: not a location: {e}", path.display())))?;
    location.validate(frame)?;
    if let Some(kind) = kind
        && location.kind() != kind
    {
        return Err(Error::usage(format!(
            "{}: expected a {kind} location, got a {}",
            path.display(),
            location.kind()
        )));
    }
    Ok(location)
}

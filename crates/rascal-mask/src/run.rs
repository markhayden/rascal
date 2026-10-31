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
use rascal_core::recipe::{InputType, Override, Recipe, Run, apply_override};
use rascal_core::registry::OpRegistry;
use rascal_core::workspace::Workspace;

pub use rascal_mask::recipes::load_recipe;

/// What `run` was asked to do.
#[derive(Debug, Clone)]
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
    /// Recipe overrides applied before validation (`"<step>.<param>" = value`), with who asked
    /// for them: a filler entry's, set by `heal`; `run` and the lab pass none.
    pub overrides: BTreeMap<String, Override>,
    pub overrides_from: Option<String>,
    /// `--set <step>.<param>=<value>`, applied after the filler's overrides: the command line
    /// wins. Parsed here, so a bad one is a usage error naming it.
    pub sets: Vec<String>,
    /// Set by a caller (a client's cancellation) to stop the run between steps.
    pub cancel: Option<Arc<rascal_core::workspace::Cancel>>,
}

/// One `--set`: `<step>.<param>=<value>`, the value read as TOML (`0.3`, `1`, `true`,
/// `"fan-v2"`, `[1, 2]`) or, when it is not TOML, as the bare string (`fan-v2`).
pub fn parse_set(spec: &str) -> Result<(String, Override)> {
    let Some((key, value)) = spec.split_once('=') else {
        return Err(Error::usage(format!(
            "--set takes <step>.<param>=<value>, got {spec:?}"
        )));
    };
    let key = key.trim();
    if !key.contains('.') || key.starts_with('.') || key.ends_with('.') {
        return Err(Error::usage(format!(
            "--set {spec:?}: the key is <step>.<param> (or <step>.constants)"
        )));
    }
    let value = value.trim();
    let parsed = toml::from_str::<toml::Table>(&format!("v = {value}"))
        .ok()
        .and_then(|mut t| t.remove("v"))
        .and_then(override_of)
        .unwrap_or_else(|| Override::String(value.to_string()));
    Ok((key.to_string(), parsed))
}

fn override_of(value: toml::Value) -> Option<Override> {
    Some(match value {
        toml::Value::Boolean(b) => Override::Bool(b),
        toml::Value::Integer(i) => Override::Integer(i),
        toml::Value::Float(f) => Override::Float(f),
        toml::Value::String(s) => Override::String(s),
        toml::Value::Array(items) => Override::List(
            items
                .into_iter()
                .map(override_of)
                .collect::<Option<Vec<_>>>()?,
        ),
        toml::Value::Table(_) | toml::Value::Datetime(_) => return None,
    })
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
    let mut recipe = load_recipe(&args.recipe)?;
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
    }
    .with_cancel(args.cancel.clone());
    let (registry, packs) =
        crate::packs::registry_for(config.as_ref(), &run.workspace, args.replay.as_deref())?;
    for (key, value) in &args.overrides {
        let from = args.overrides_from.as_deref().unwrap_or("the caller");
        apply_override(&mut recipe, &registry, key, value)
            .map_err(|m| Error::usage(format!("fillers.{from}: override {key}: {m}")))?;
        run.note(format!(
            "override from fillers.{from}: {key} = {}",
            toml::Value::from(value)
        ))?;
    }
    for spec in &args.sets {
        let (key, value) = parse_set(spec)?;
        apply_override(&mut recipe, &registry, &key, &value)
            .map_err(|m| Error::usage(format!("--set {spec:?}: {m}")))?;
        run.note(format!(
            "override from --set: {key} = {}",
            toml::Value::from(&value)
        ))?;
    }
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

    let (image_id, frame, image) = run
        .workspace
        .import(&args.image, args.projection.or(recipe.projection))?;
    // The contract's reference to the caller's file (ADR 0020): a table-manners location must
    // name it.
    let given = rascal_core::contract::image_reference(&run.workspace, &image_id)?;
    let mut inputs: BTreeMap<String, Artifact> = BTreeMap::new();
    inputs.insert("image".to_string(), image);
    if let Some(bbox) = bbox {
        let location = Location::bbox(&frame.id, bbox)?;
        location.validate(&frame)?;
        let (_, artifact) = run.workspace.add_location(location)?;
        inputs.insert("location".to_string(), artifact);
    }
    if let Some(path) = &args.location {
        let location = location_from_file(path, &frame, &given, None)?;
        let (_, artifact) = run.workspace.add_location(location)?;
        inputs.insert("location".to_string(), artifact);
    }
    if let Some(path) = &args.stroke {
        let location = location_from_file(path, &frame, &given, Some("stroke"))?;
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
/// is either rascal's own location, which may name the frame by its id or leave `frame` out
/// (the image's frame is used), or a table-manners location (it has `image`), which must be on
/// the file the run was given (ADR 0020).
fn location_from_file(
    path: &Path,
    frame: &rascal_core::frame::Frame,
    given: &rascal_core::contract::table_manners::images::ImageReference,
    kind: Option<&str>,
) -> Result<Location> {
    let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
    let mut value: serde_json::Value =
        serde_json::from_str(&text).map_err(|source| Error::Json {
            path: path.to_path_buf(),
            source,
        })?;
    if value.get("image").is_some() {
        let contract: rascal_core::contract::table_manners::images::Location =
            serde_json::from_value(value).map_err(|e| {
                Error::usage(format!(
                    "{}: not a table-manners location: {e}",
                    path.display()
                ))
            })?;
        let location = rascal_core::contract::location_in(&contract, given, frame)
            .map_err(|e| Error::usage(format!("{}: {e}", path.display())))?;
        return check_kind(path, location, kind);
    }
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
    check_kind(path, location, kind)
}

/// The location, when it is the kind the flag takes.
fn check_kind(path: &Path, location: Location, kind: Option<&str>) -> Result<Location> {
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

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
    pub protect: Option<MaskIn>,
    pub mask: Option<MaskIn>,
    pub prompt: Option<String>,
    /// `--model`: the segmenter, for a recipe that declares `inputs.model` (remove-prompt).
    pub model: Option<String>,
    /// The model's installation, pinned by a caller that holds it already (a lab cell);
    /// otherwise the run takes its own hold when it is admitted.
    pub installation: Option<String>,
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

/// A Mask a run binds to `inputs.mask` or `inputs.protect`: a stored mask directory a person
/// named on the command line, or the bundle of a Mask `serve` bound by reference in its store
/// (PLAN.md 10.3), which the run imports into its own workspace with its frame chain.
#[derive(Clone)]
pub enum MaskIn {
    Dir(PathBuf),
    Bundle(Arc<rascal_core::workspace::ExportedMask>),
}

impl std::fmt::Debug for MaskIn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MaskIn::Dir(path) => write!(f, "Dir({})", path.display()),
            MaskIn::Bundle(bundle) => write!(f, "Bundle({})", bundle.identity),
        }
    }
}

impl MaskIn {
    /// What the run's input identity digests for it: the directory as given, or the Mask's
    /// identity.
    pub fn digest_part(&self) -> String {
        match self {
            MaskIn::Dir(path) => path.display().to_string(),
            MaskIn::Bundle(bundle) => bundle.identity.clone(),
        }
    }
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
    /// The run's hold on its model's installation, released when this is dropped.
    pub hold: Option<Hold>,
}

/// A run's hold on its segmenter's installation (SPEC.md "Runs hold their installation"):
/// taken through the pack's own `rascal-tools hold` (under the models lock: `current` read,
/// the installation checked, the hold written), released when dropped, on every outcome. A
/// held installation is never deleted, so a reinstall or removal during the run cannot strand
/// its retries.
pub struct Hold {
    launcher: PathBuf,
    models: PathBuf,
    pub model: String,
    run: String,
    pub installation: String,
}

impl Hold {
    /// Holds `model`'s current installation for a run of this process.
    pub fn take(segmenters: &rascal_mask::segmenters::Segmenters, model: &str) -> Result<Hold> {
        let launcher = segmenters.install.join(".venv/bin/rascal-tools");
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let run = format!("rascal-{}-{nanos}", std::process::id());
        let out = std::process::Command::new(&launcher)
            .args(["hold", model, "--run", &run, "--pid"])
            .arg(std::process::id().to_string())
            .env(rascal_mask::readiness::MODELS_ENV, &segmenters.models)
            .output()
            .map_err(|e| Error::io(&launcher, e))?;
        let installation = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !out.status.success() || installation.is_empty() {
            return Err(Error::usage(format!(
                "segmenter {model} could not be held for the run: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(Hold {
            launcher,
            models: segmenters.models.clone(),
            model: model.to_string(),
            run,
            installation,
        })
    }
}

impl Drop for Hold {
    fn drop(&mut self) {
        let _ = std::process::Command::new(&self.launcher)
            .args(["release", &self.model, "--run", &self.run, "--installation"])
            .arg(&self.installation)
            .env(rascal_mask::readiness::MODELS_ENV, &self.models)
            .output();
    }
}

/// Pins the run's installation of `model` (SPEC.md "Choosing a model"): the caller's when it
/// holds one already, else a hold taken now; every `segment` step then names it, so the first
/// call and every retry run the same installation. Nothing is pinned under `--replay` (the
/// recording answers), for a pack whose `segment` takes no `installation`, or without a
/// catalogue to hold through. The installation's `install.json` is copied into the run's
/// directory as `segmenter-install.json`, so removing the model later leaves the evidence.
pub fn pin_installation(
    recipe: &mut Recipe,
    registry: &rascal_core::registry::OpRegistry,
    config: Option<&rascal_core::config::Config>,
    model: Option<&str>,
    given: Option<&str>,
    run_dir: &Path,
    replay: bool,
) -> Result<Option<Hold>> {
    let Some(model) = model else { return Ok(None) };
    let takes = registry
        .get(crate::retry::SEGMENT)
        .is_some_and(|op| op.manifest().param("installation").is_some());
    if replay || !takes {
        return Ok(None);
    }
    let Some(segmenters) = config.and_then(rascal_mask::segmenters::Segmenters::of) else {
        return Ok(None);
    };
    // A caller that holds an installation already names it: the lab's cell (`given`), or a
    // `--set segment.installation=` from a harness that took its own hold.
    let set = recipe
        .step
        .iter()
        .find(|s| s.op == crate::retry::SEGMENT)
        .and_then(|s| s.params.get("installation"))
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
        .map(str::to_string);
    let given = given.map(str::to_string).or(set);
    let (installation, hold) = match given.as_deref() {
        Some(ident) => (ident.to_string(), None),
        None => {
            let hold = Hold::take(&segmenters, model)?;
            (hold.installation.clone(), Some(hold))
        }
    };
    for step in recipe
        .step
        .iter_mut()
        .filter(|s| s.op == crate::retry::SEGMENT)
    {
        step.params.insert(
            "installation".to_string(),
            toml::Value::String(installation.clone()),
        );
    }
    let record = segmenters
        .models
        .join(model)
        .join(&installation)
        .join("install.json");
    if record.is_file() {
        let to = run_dir.join("segmenter-install.json");
        std::fs::copy(&record, &to).map_err(|e| Error::io(&to, e))?;
    }
    Ok(hold)
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

/// Whether a recipe declares `inputs.model`, which `--model` binds (SPEC.md "Choosable
/// segmenters").
pub fn declares_model(recipe: &Recipe) -> bool {
    recipe
        .inputs
        .get("model")
        .is_some_and(|d| d.kind == InputType::Text)
}

/// The segmenter a run of `recipe` uses, resolved once (SPEC.md "Choosable segmenters"): the
/// call's `model`, else `[segment] model`, else the pack's default. A step may not set `model`
/// itself (a literal, or an override): the only way in is `inputs.model`. With a pack whose
/// `segment` takes no `model`, the binding is dropped and naming a model is refused. A model
/// that is not installed is refused before anything runs, naming the command that installs it;
/// under `--replay` nothing is checked on disk, since the recording answers.
pub fn resolve_model(
    recipe: &mut Recipe,
    registry: &rascal_core::registry::OpRegistry,
    config: Option<&rascal_core::config::Config>,
    call: Option<&str>,
    replay: bool,
) -> Result<Option<String>> {
    if !declares_model(recipe) {
        if call.is_some() {
            return Err(Error::usage(format!(
                "recipe {} declares no inputs.model; --model does not apply (remove-prompt takes one)",
                recipe.name
            )));
        }
        return Ok(None);
    }
    for step in recipe.step.iter().filter(|s| s.op == crate::retry::SEGMENT) {
        if let Some(value) = step.params.get("model")
            && value.as_str() != Some("inputs.model")
        {
            return Err(Error::usage(format!(
                "recipe {} step {}: a step cannot set model itself ({value}); pass --model, the call's model or [segment] model",
                recipe.name, step.id
            )));
        }
    }
    let manifest = registry.get(crate::retry::SEGMENT).map(|op| op.manifest());
    let param = manifest.as_ref().and_then(|m| m.param("model").cloned());
    let choices = param.as_ref().map(|p| {
        (
            p.choices.clone(),
            p.default
                .as_ref()
                .and_then(|d| d.as_str())
                .map(str::to_string),
        )
    });
    let resolved = rascal_mask::segmenters::resolve(
        call,
        config,
        choices.as_ref().map(|(c, d)| (c.as_slice(), d.as_deref())),
    )?;
    let Some((model, _)) = resolved else {
        for step in recipe
            .step
            .iter_mut()
            .filter(|s| s.op == crate::retry::SEGMENT)
        {
            step.params.remove("model");
        }
        return Ok(None);
    };
    if !replay
        && let Some(segmenters) = config.and_then(rascal_mask::segmenters::Segmenters::of)
        && let Some(entry) = segmenters.entry(&model)
    {
        use rascal_mask::segmenters::State;
        match segmenters.state(entry) {
            State::Installed => {}
            State::Unavailable => {
                return Err(Error::usage(format!(
                    "segmenter {model} does not run on {}",
                    rascal_mask::segmenters::platform()
                )));
            }
            State::NotInstalled | State::Installing => {
                return Err(Error::usage(format!(
                    "segmenter {model} is not installed in {}: run `rascal tools get {model}` (or Get it in Wafflery)",
                    segmenters.models.display()
                )));
            }
        }
    }
    Ok(Some(model))
}

/// Runs a recipe. The run directory must be new or empty. Every declared input is bound from
/// its flag: `image` from the positional path, `location` from `--bbox` or `--location`,
/// `stroke` from `--stroke`, `protect` and `mask` from stored mask directories, `prompt`
/// (text) from `--prompt`; a declared input without its flag, or a flag without its input,
/// is a usage error.
pub fn run(args: &RunArgs) -> Result<RunReport> {
    check_flags(args, &load_recipe(&args.recipe)?, false)?;
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

/// The checks a run's flags get against its recipe before anything is read or made: a `prompt`
/// it declares is given, every declared input has its flag and every flag its input, and the
/// recipe takes an image. A `run` (`filler` false) also refuses `--prompt` for a recipe that
/// declares none; a `heal` passes it to the filler. The command line's invocation makes these
/// checks first, so flags that do not apply never start a run.
pub fn check_flags(args: &RunArgs, recipe: &Recipe, filler: bool) -> Result<()> {
    if !filler && args.prompt.is_some() && !declares_prompt(recipe) {
        return Err(Error::usage(format!(
            "recipe {} declares no inputs.prompt; --prompt does not apply (remove-prompt takes one)",
            args.recipe
        )));
    }
    if declares_prompt(recipe) && args.prompt.is_none() {
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
    Ok(())
}

/// Binds the inputs and runs the recipe; `heal` continues from the outcome. The registry is
/// built here: the shipped ops plus every configured pack's tools, journaling into the run's
/// workspace (so the run directory exists before the recipe is validated). A pack that does not
/// start is noted and leaves its tools out, so a recipe that needs none of them still runs and
/// one that does fails with the pack's own error.
pub fn run_recipe(args: &RunArgs) -> Result<Prepared> {
    let config = args
        .config
        .as_deref()
        .map(rascal_core::config::Config::read)
        .transpose()?;
    let mut recipe = load_recipe(&args.recipe)?;
    check_flags(args, &recipe, true)?;
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
    let (registry, packs, mut failed_packs) =
        crate::packs::registry_tolerant(config.as_ref(), &run.workspace, args.replay.as_deref())?;
    for (name, reason) in &failed_packs {
        run.note(format!(
            "packs.{name} did not start, so its tools are absent: {reason}"
        ))?;
    }
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
    let model = resolve_model(
        &mut recipe,
        &registry,
        config.as_ref(),
        args.model.as_deref(),
        args.replay.is_some(),
    )?;
    let hold = pin_installation(
        &mut recipe,
        &registry,
        config.as_ref(),
        model.as_deref(),
        args.installation.as_deref(),
        &out,
        args.replay.is_some(),
    )?;
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
            // An op nothing serves while a pack did not start: that pack's own error, as when a
            // pack that does not start failed every run.
            Error::Recipe {
                recipe,
                step,
                message,
            } if message.starts_with("unknown op ") => match failed_packs.pop_first() {
                Some((_, error)) => error,
                None => Error::Recipe {
                    recipe,
                    step,
                    message,
                },
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
    for (name, given) in [("protect", &args.protect), ("mask", &args.mask)] {
        let Some(given) = given else {
            continue;
        };
        let (named, mask) = match given {
            MaskIn::Dir(path) => (path.display().to_string(), Mask::read(path)?),
            // A Mask bound by reference enters the run's workspace with its frame chain, after
            // the image, so a chain rooted in the image's frame joins it.
            MaskIn::Bundle(bundle) => {
                let imported = run.workspace.import_mask_bytes(
                    &bundle.bytes,
                    name,
                    name,
                    Some(&bundle.sha256),
                )?;
                let Artifact::Mask(mask) = run.workspace.load(&imported.id)? else {
                    return Err(Error::usage(format!("{name}: the bundle holds no Mask")));
                };
                (name.to_string(), (*mask).clone())
            }
        };
        // The mask is in the image's frame, or (for protect's second pass) in a frame derived
        // from it that the workspace knows: a cut canvas or a crop of it.
        if mask.record.frame != frame.id {
            let root = run
                .workspace
                .frames()?
                .chain(&mask.record.frame)
                .ok()
                .and_then(|chain| chain.last().map(|f| f.id.clone()));
            if root.as_deref() != Some(frame.id.as_str()) {
                return Err(Error::usage(format!(
                    "{named}: the mask is in frame {} but the image is in {}{}",
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

    let mut texts: BTreeMap<String, String> = BTreeMap::new();
    if let Some(prompt) = &args.prompt
        && declares_prompt(&recipe)
    {
        texts.insert("prompt".to_string(), prompt.clone());
    }
    if let Some(model) = &model {
        texts.insert("model".to_string(), model.clone());
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
        hold,
    })
}

/// A location from a JSON file, rascal's own or a table-manners one, validated against the
/// image's frame (`rascal_core::contract::location_from_json`, shared with
/// `workspace import-location`).
fn location_from_file(
    path: &Path,
    frame: &rascal_core::frame::Frame,
    given: &rascal_core::contract::table_manners::images::ImageReference,
    kind: Option<&str>,
) -> Result<Location> {
    let value = rascal_core::contract::read_location_json(path)?;
    let location =
        rascal_core::contract::location_from_json(value, frame, given).map_err(|e| match e {
            Error::Usage { message } => Error::usage(format!("{}: {message}", path.display())),
            other => other,
        })?;
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

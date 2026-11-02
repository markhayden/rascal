//! The invocation (table-manners 4.8; PLAN.md 10.2): what a call resolved to, recorded in its
//! result so it can be repeated from another directory, by another client or on another machine.
//!
//! - `action` is the action's name (a recipe file run on the command line is `custom`).
//! - `arguments` are the call's as resolved: every default the action's `params_schema` declares
//!   applied, the projection the image ran in, each image as `{"expect": <image reference>}` with
//!   no `path` or `inline`, each location as given, `max_spend_usd` kept, and the delivery
//!   choices (`return_images`, `image_blocks`) left out. `mask` and `protect` are still paths to
//!   stored mask directories (PLAN.md 10.3 makes them references).
//! - `effective` is the estimate's object from the same function ([`estimate::effective`]): the
//!   model, the configuration's identity, and `x-rascal` with the filler and the constant set in
//!   force at each recipe step.
//! - `requirements` names each readiness requirement the call used, by its name, a file or
//!   program as its file name only; never a value.
//!
//! A `serve` call and a command-line run come through the same [`record`]: the command line
//! first writes its flags as the matching action's arguments ([`of_command`]), so a CLI run is
//! callable through `serve` and `rascal call` alike.

use std::path::Path;

use rascal_core::Error;
use rascal_core::config::ConfigState;
use rascal_core::contract::table_manners::images::ImageReference;
use rascal_core::contract::table_manners::{Invocation, Readiness, Status};
use rascal_core::contract::{self, LocationMeta, location_out};
use rascal_core::error::Result;
use rascal_core::frame::{Frame, Projection};
use rascal_core::image::Image;
use rascal_core::location::Location;
use rascal_core::paths::PathRule;
use serde_json::{Map, Value, json};

use crate::estimate::{self, DELIVERY, Price};
use crate::run::{self, RunArgs};
use crate::run_json;
use rascal_mask::describe;
use rascal_mask::readiness::Setup;
use rascal_mask::recipes::load_recipe;

/// The invocation of a `serve` call whose arguments are checked (`estimate::check_arguments`):
/// its images read through `paths` as the run reads them, and `price` the call's own
/// ([`estimate::price`]). A failure is the run's `invalid`: its arguments did not resolve.
pub fn of_call(
    recipe: &str,
    action: &str,
    args: &Map<String, Value>,
    paths: &PathRule,
    price: &Price,
) -> Result<Invocation> {
    resolving(|| {
        let slots = estimate::slots(recipe)?;
        let projection = projection_of(recipe, args)?;
        let mut arguments = Map::new();
        let mut frame = None;
        for (key, value) in args {
            if slots.images.contains(key) {
                let (reference, image_frame) = image_reference(key, value, paths, projection)?;
                arguments.insert(key.clone(), json!({ "expect": reference }));
                frame = Some(image_frame);
            } else {
                arguments.insert(key.clone(), value.clone());
            }
        }
        let frame = frame.ok_or_else(|| Error::usage("image: required"))?;
        record(recipe, action, arguments, &frame, price)
    })
}

/// The invocation of a `rascal run` or `heal` (with `filler`): the flags as the matching action's
/// arguments (the image by its reference; `--bbox`, `--location` and `--stroke` as contract
/// locations on it, a contract location file as given; `--set` as `set`; `--mask` and `--protect`
/// as absolute paths), then [`record`], priced as `estimate::price` prices the call under the
/// run's configuration (no session: no provider has answered anything).
pub fn of_command(args: &RunArgs, filler: Option<&str>) -> Result<Invocation> {
    resolving(|| {
        let action = run_json::action_of(&args.recipe);
        let arguments = command_arguments(args, filler)?;
        let state = ConfigState::read(args.config.as_deref());
        let price = estimate::price(
            &Setup {
                config: &state,
                answers: &Default::default(),
                failed_packs: &Default::default(),
            },
            &args.recipe,
            &action,
            &arguments,
        )?;
        let (reference, frame) = image_reference(
            "image",
            &json!({ "path": absolute(&args.image) }),
            &PathRule::Own,
            projection_of(&args.recipe, &arguments)?,
        )?;
        let mut arguments = arguments;
        arguments.insert("image".to_string(), json!({ "expect": reference.clone() }));
        for (slot, file) in [("location", &args.location), ("stroke", &args.stroke)] {
            if let Some(file) = file {
                let value = contract::read_location_json(file)?;
                arguments.insert(
                    slot.to_string(),
                    location_value(value, &frame, &reference)
                        .map_err(|e| Error::usage(format!("{}: {e}", file.display())))?,
                );
            }
        }
        if let Some(bbox) = &args.bbox {
            let location = Location::bbox(&frame.id, Location::parse_bbox(bbox)?)?;
            arguments.insert(
                "location".to_string(),
                contract_location(location, &reference)?,
            );
        }
        record(&args.recipe, &action, arguments, &frame, &price)
    })
}

/// The flags of a run that need no image read, as an action's arguments: `filler`, `prompt`,
/// `set`, `projection`, `strict`, `mask` and `protect`. The arguments `estimate::price` reads
/// (`filler`, `set`) are here, so a command line prices its call from them.
fn command_arguments(args: &RunArgs, filler: Option<&str>) -> Result<Map<String, Value>> {
    let mut arguments = Map::new();
    if let Some(filler) = filler {
        arguments.insert("filler".to_string(), json!(filler));
    }
    if let Some(prompt) = &args.prompt {
        arguments.insert("prompt".to_string(), json!(prompt));
    }
    if !args.sets.is_empty() {
        let mut set = Map::new();
        for spec in &args.sets {
            let (key, value) = run::parse_set(spec)?;
            set.insert(
                key,
                serde_json::to_value(&value)
                    .map_err(|e| Error::usage(format!("--set {spec:?}: {e}")))?,
            );
        }
        arguments.insert("set".to_string(), Value::Object(set));
    }
    if let Some(projection) = args.projection {
        arguments.insert("projection".to_string(), json!(projection.name()));
    }
    if args.strict {
        arguments.insert("strict".to_string(), json!(true));
    }
    for (slot, dir) in [("mask", &args.mask), ("protect", &args.protect)] {
        if let Some(dir) = dir {
            arguments.insert(slot.to_string(), json!(absolute(dir)));
        }
    }
    Ok(arguments)
}

/// The record from resolved arguments: delivery left out, the defaults applied, the projection
/// the image runs in, the effective choices and the requirements by name.
fn record(
    recipe: &str,
    action: &str,
    mut arguments: Map<String, Value>,
    frame: &Frame,
    price: &Price,
) -> Result<Invocation> {
    for delivery in DELIVERY {
        arguments.remove(delivery);
    }
    for (name, default) in describe::defaults(recipe)? {
        arguments.entry(name).or_insert(default);
    }
    // Without a projection named or declared, the image's shape chose it (2:1 is
    // equirectangular): the resolved call names it, so a replay does not depend on the guess.
    arguments.insert("projection".to_string(), json!(frame.projection.name()));
    Ok(Invocation {
        action: action.to_string(),
        arguments,
        effective: estimate::effective(price),
        requirements: requirement_names(&price.readiness),
    })
}

/// The names of the requirements a call's readiness lists, each once, in order: a name that is
/// a path (a filler command given by path) as its file name, so no path is recorded.
pub fn requirement_names(readiness: &Readiness) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for requirement in &readiness.requirements {
        let name = requirement
            .name
            .rsplit(['/', '\\'])
            .find(|part| !part.is_empty())
            .unwrap_or(&requirement.name)
            .to_string();
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// The projection a run takes for its image: the call's, else the recipe's declared one, else
/// none (the image's shape decides).
fn projection_of(recipe: &str, args: &Map<String, Value>) -> Result<Option<Projection>> {
    match args.get("projection").and_then(|v| v.as_str()) {
        Some("flat") => Ok(Some(Projection::Flat)),
        Some("equirectangular") => Ok(Some(Projection::Equirectangular)),
        Some(other) => Err(Error::usage(format!("projection: {other}"))),
        None => Ok(load_recipe(recipe)?.projection),
    }
}

/// An image slot's value, read as the run reads it, as the reference of the file and the frame
/// it imports in: the same reference the result's `images.taken` gives it, whether the call sent
/// the image by path or inline.
fn image_reference(
    slot: &str,
    value: &Value,
    paths: &PathRule,
    projection: Option<Projection>,
) -> Result<(ImageReference, Frame)> {
    let image = estimate::image_in(slot, value, paths)?;
    let bytes = image.bytes()?;
    estimate::verify_expect(slot, value, &image, &bytes)?;
    let (decoded, frame, _) = Image::import_bytes(&bytes, projection)?;
    let reference = contract::reference(&decoded, &frame, contract::file_sha256(&bytes));
    Ok((reference, frame))
}

/// A location file's JSON as the contract location the run used: a contract location as given
/// (once it is checked against the image), rascal's own written out as one.
fn location_value(value: Value, frame: &Frame, reference: &ImageReference) -> Result<Value> {
    let given = value.get("image").is_some().then(|| value.clone());
    let location = contract::location_from_json(value, frame, reference)?;
    match given {
        Some(given) => Ok(given),
        None => contract_location(location, reference),
    }
}

fn contract_location(location: Location, reference: &ImageReference) -> Result<Value> {
    let out = location_out(&[location], reference.clone(), LocationMeta::default())?;
    serde_json::to_value(out).map_err(|e| Error::usage(format!("location: {e}")))
}

fn absolute(path: &Path) -> String {
    std::path::absolute(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

/// Runs a resolution; whatever stops it is the run's `invalid` (its arguments did not resolve,
/// so its invocation is null, 4.8), never an `error`.
fn resolving(f: impl FnOnce() -> Result<Invocation>) -> Result<Invocation> {
    f().map_err(|error| match run_json::error_of(&error).0 {
        Status::Invalid => error,
        _ => Error::usage(error.to_string()),
    })
}

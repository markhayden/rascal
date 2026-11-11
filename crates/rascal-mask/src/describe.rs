//! The contract's description document (table-manners 3, 3.3, 4.7 and 11.2; ADR 0020): one
//! action of kind `task` per shipped recipe, built from the recipes and the configuration, and one
//! of kind `operation` per built-in op, built from its manifest, so a client that knows only the
//! contract can form a call. `rascal describe --json` prints it; `serve` returns it from the
//! `describe` tool.

use std::collections::BTreeMap;

use rascal_core::artifact::ArtifactType;
use rascal_core::contract::table_manners::images::{
    ImageSlot, Limits, LocationKind, LocationOutput, LocationSlot, Outputs,
};
use rascal_core::contract::table_manners::operations::Ports;
use rascal_core::contract::table_manners::{
    Action, ActionKind, Answers, Cost, DescribedTool, Description, Duration, Profile, Readiness,
    ReadinessState, Render, RenderKind, SpendBasis, Vocabulary,
};
use rascal_core::error::Result;
use rascal_core::op::{Group, Op};
use rascal_core::recipe::{InputType, Recipe};
use serde_json::{Value, json};

use crate::readiness::{self, Setup, Use};
use crate::recipes::{SHIPPED, load_recipe};

/// The largest inline image `serve` takes, in decoded bytes (11.2): 64 MiB.
pub const INLINE_BYTES: u64 = 64 << 20;

/// The contract's artifact reference schema (CONTRACT 3.4), which a Mask argument names.
pub const ARTIFACT_REF: &str = "urn:table-manners:operations:artifact-ref";

/// A Mask argument's schema: the shape of the contract's artifact reference ([`ARTIFACT_REF`],
/// named in `$comment`), written out so the schema stands alone for a client that has no copy
/// of the contract's. The type is any type name, not only `rascal:Mask`: one of the wrong type
/// is the `binding` refusal (`wrong_type`), never a schema error.
pub fn mask_reference_schema() -> Value {
    json!({
        "$comment": ARTIFACT_REF,
        "description": "A rascal:Mask artifact reference (CONTRACT 3.4); it resolves by identity in this server's workspace (rascal serve --workspace).",
        "type": "object",
        "properties": {
            "type": { "type": "string", "pattern": "^(image|location|[a-z][a-z0-9-]*:[A-Z][A-Za-z0-9]*)$" },
            "schema_version": { "type": "integer", "minimum": 1 },
            "tool": { "type": "string", "pattern": "^[a-z][a-z0-9-]*$" },
            "identity": { "type": "string", "pattern": "^sha256:[0-9a-f]{64}$" },
            "frame": { "type": ["string", "null"], "minLength": 1 },
            "on": { "type": "object", "description": "The image reference (CONTRACT 8) of the image it was made on." },
            "resource": { "type": "string", "pattern": "^[a-z][a-z0-9+.-]*://artifact/" }
        },
        "required": ["type", "schema_version", "tool", "identity", "frame", "resource"],
        "additionalProperties": false
    })
}

/// The longest gap between progress notifications on a live run, in seconds (4.1).
pub const PROGRESS_INTERVAL_S: u32 = 10;

/// The longest a host waits for the dash render's URL line, in seconds (3.2).
pub const DASH_READY_TIMEOUT_S: u32 = 10;

/// Every action's renders (3.2), each running `rascal` itself (`tool.name`): `dash`, the page
/// served on loopback for the host's origin to frame, and `page`, the same page as one
/// self-contained file. `{run.dir}` and `{frame_ancestor}` are whole arguments.
fn renders() -> Vec<Render> {
    vec![
        Render {
            name: "dash".to_string(),
            kind: RenderKind::Server,
            ready_timeout_s: Some(DASH_READY_TIMEOUT_S),
            command: [
                "rascal",
                "dash",
                "{run.dir}",
                "--serve",
                "--port",
                "0",
                "--frame-ancestor",
                "{frame_ancestor}",
            ]
            .map(str::to_string)
            .to_vec(),
        },
        Render {
            name: "page".to_string(),
            kind: RenderKind::Document,
            ready_timeout_s: None,
            command: ["rascal", "dash", "{run.dir}", "--document"]
                .map(str::to_string)
                .to_vec(),
        },
    ]
}

/// What the segmenter's detection score is, in a location's `score_kind` (9.2).
pub const SEGMENT_SCORE_KIND: &str = "owlv2-detection";

/// A shipped recipe's name as an action name: lowercase with underscores (3). Every op is an
/// operation of its own name, so a recipe named like an op takes another name: the `rect`
/// recipe (a hard-edged rectangle as a mask) is `mask_rect`, the `rect` op stays.
pub fn action_name(recipe: &str) -> String {
    match recipe {
        "rect" => "mask_rect".to_string(),
        other => other.replace('-', "_"),
    }
}

/// The shipped recipe an action runs.
pub fn recipe_of(action: &str) -> Option<&'static str> {
    SHIPPED
        .iter()
        .map(|(name, _)| *name)
        .find(|name| action_name(name) == action)
}

fn title_and_purpose(recipe: &str) -> (&'static str, &'static str) {
    match recipe {
        "seam" => (
            "Heal a seam",
            "Masks the wrap edge of a panorama as a band, wide on smooth surfaces and thin on detail, with a tone ramp across it; with a filler, heals it.",
        ),
        "remove" => (
            "Remove an object",
            "Masks an object at a location with its shadow, kept off protected regions and feathered; with a filler, removes it.",
        ),
        "remove-prompt" => (
            "Remove what a prompt names",
            "Finds the objects a text prompt names with the segmenter pack and masks them as remove does; with a filler, removes them.",
        ),
        "repair" => (
            "Repair a scratch",
            "Tightens a stroke drawn along a scratch or tear to the defect's edges and feathers it; with a filler, repairs it.",
        ),
        "repair-area" => (
            "Repair a stain",
            "Masks a stain or spot as drawn, feathered; with a filler, repairs it.",
        ),
        "protect" => (
            "Protect an earlier edit",
            "Inverts an earlier mask so a second pass may change everything else.",
        ),
        "rect" => (
            "Mask a rectangle",
            "A hard-edged rectangle as a mask; to soften its edge, feather it afterwards (the feather operation).",
        ),
        _ => ("Run a recipe", "Runs a recipe."),
    }
}

/// The location kinds a recipe's location slot takes.
fn location_kinds(recipe: &str, slot: &str) -> Vec<LocationKind> {
    use LocationKind as K;
    match (recipe, slot) {
        ("rect", _) => vec![K::Bbox],
        ("repair", _) | (_, "stroke") => vec![K::Stroke],
        ("repair-area", _) => vec![K::Bbox, K::Polygon],
        _ => vec![K::Bbox, K::Polygon, K::Stroke],
    }
}

/// `max_spend_usd` as every action declares it (4.5): every action takes a filler, so every
/// action may be paid, and a host can always pass its cap. A cap of zero is not a cap.
pub fn max_spend_usd_schema() -> Value {
    json!({ "type": "number", "exclusiveMinimum": 0,
            "description": "Lower the per-call spend cap for this call (USD, above zero)." })
}

/// The arguments every action takes beside its recipe's inputs: `filler` (from `fillers`, the
/// configured names), `set`, `projection`, `strict` and `max_spend_usd`, each with the default the
/// tool uses when the call leaves it out, where there is one.
fn common_properties(recipe: &Recipe, fillers: &[String]) -> serde_json::Map<String, Value> {
    let mut properties = serde_json::Map::new();
    properties.insert(
        "filler".to_string(),
        if fillers.is_empty() {
            json!({ "type": "string",
                    "description": "A filler from the configuration to paint the mask; without one the run returns the mask alone. None is configured." })
        } else {
            json!({ "type": "string", "enum": fillers,
                    "description": "A filler from the configuration to paint the mask; without one the run returns the mask alone." })
        },
    );
    properties.insert(
        "set".to_string(),
        json!({ "type": "object",
                "description": "Step overrides for this run, `<step>.<param>` to a value (`fan.constants`: `fan-v1`, `grow.px`: 8); journaled.",
                "propertyNames": { "pattern": "^[a-z_][a-z0-9_-]*\\.[a-z_][a-z0-9_]*$" },
                "additionalProperties": { "type": ["string", "number", "boolean"] } }),
    );
    let mut projection = json!({ "type": "string", "enum": ["flat", "equirectangular"],
                "description": "The image's projection; without it, the projection the image's reference states, else an exactly 2:1 image is equirectangular." });
    if let Some(declared) = recipe.projection {
        // A recipe that declares its projection runs with it unless the call names another.
        projection["default"] = json!(declared.name());
        projection["description"] = json!(format!(
            "The image's projection; {} unless the call names another.",
            declared.name()
        ));
    }
    properties.insert("projection".to_string(), projection);
    properties.insert(
        "strict".to_string(),
        json!({ "type": "boolean", "default": false,
                "description": "Fail (exit 1) when the recipe stops at a step that is not applicable." }),
    );
    properties.insert("max_spend_usd".to_string(), max_spend_usd_schema());
    properties
}

/// The defaults an action's `params_schema` declares (3): what a call that leaves the argument
/// out runs with. The invocation applies them (4.8).
pub fn defaults(recipe: &str) -> Result<serde_json::Map<String, Value>> {
    let recipe = load_recipe(recipe)?;
    Ok(common_properties(&recipe, &[])
        .into_iter()
        .filter_map(|(name, schema)| Some((name, schema.get("default")?.clone())))
        .collect())
}

/// The names the contract reserves for its own tools (3, 5): never an action's.
pub const RESERVED: [&str; 6] = [
    "describe",
    "estimate",
    "export",
    "import",
    "install",
    "uninstall",
];

/// `model` as remove_prompt declares it: a string `enum` of the catalogued segmenters that run
/// on this platform, the resolved default (`[segment] model`, else the best-ranked model
/// installed here, else the best-ranked that runs here; `Segmenters::default_model`) first and as
/// `default`, with `x-requirements` naming each choice's requirement. None when no choice runs
/// here or the default is not one of them.
fn model_schema(
    segmenters: &crate::segmenters::Segmenters,
    config: Option<&rascal_core::config::Config>,
) -> Option<Value> {
    use crate::segmenters::State;
    let default = segmenters.default_model(config)?;
    let mut ids: Vec<&str> = segmenters
        .entries
        .iter()
        .filter(|e| segmenters.state(e) != State::Unavailable)
        .map(|e| e.id.as_str())
        .collect();
    let at = ids.iter().position(|id| *id == default)?;
    let first = ids.remove(at);
    ids.insert(0, first);
    let requirements: serde_json::Map<String, Value> = ids
        .iter()
        .map(|id| {
            (
                (*id).to_string(),
                json!([readiness::model_requirement_name(id)]),
            )
        })
        .collect();
    Some(json!({
        "type": "string",
        "enum": ids,
        "default": default,
        "x-requirements": requirements,
        "description": "The segmenter that finds the prompt's objects; one not installed is got in the tool's setup or with `rascal tools get <id>`.",
    }))
}

/// Every catalogued segmenter as an installable (table-manners 3.5), its state read from disk:
/// the installation `current` names, a live installer's status, the platform. Sizes are the
/// files' recorded sizes in the pack's `artifacts.toml`, else the catalogue's megabytes.
pub fn installables(
    segmenters: &crate::segmenters::Segmenters,
) -> Vec<rascal_core::contract::table_manners::Installable> {
    use crate::segmenters::State;
    use rascal_core::contract::table_manners::{
        InstallAction, InstallError, InstallErrorCode, InstallOutcome, Installable,
        InstallableKind, InstallableState, OutcomeState,
    };
    let sizes = artifact_sizes(&segmenters.install);
    segmenters
        .entries
        .iter()
        .map(|entry| {
            let state = segmenters.state(entry);
            let status = segmenters.status(&entry.id);
            let bytes = entry
                .files
                .iter()
                .chain(&entry.licence_files)
                .map(|f| sizes.get(f).copied())
                .sum::<Option<u64>>();
            let last_outcome = status
                .as_ref()
                .and_then(|s| s.get("last_outcome"))
                .filter(|o| !o.is_null())
                .and_then(|o| {
                    let action = match o.get("action")?.as_str()? {
                        "install" => InstallAction::Install,
                        "uninstall" => InstallAction::Uninstall,
                        _ => return None,
                    };
                    let state = match o.get("state")?.as_str()? {
                        "succeeded" => OutcomeState::Succeeded,
                        "failed" => OutcomeState::Failed,
                        "cancelled" => OutcomeState::Cancelled,
                        _ => return None,
                    };
                    Some(InstallOutcome {
                        action,
                        state,
                        error: (state == OutcomeState::Failed).then(|| InstallError {
                            code: InstallErrorCode::Failed,
                            message: o
                                .get("error")
                                .and_then(|e| e.as_str())
                                .unwrap_or("failed")
                                .to_string(),
                        }),
                        at: o.get("at")?.as_str()?.to_string(),
                    })
                });
            let mut extensions = BTreeMap::new();
            if let Some(lab) = &entry.lab
                && let Ok(lab) = serde_json::to_value(lab)
            {
                extensions.insert("x-rascal".to_string(), json!({ "lab": lab }));
            }
            Installable {
                id: entry.id.clone(),
                title: if entry.title.is_empty() {
                    entry.id.clone()
                } else {
                    entry.title.clone()
                },
                kind: InstallableKind::Model,
                download_bytes: bytes.unwrap_or(entry.download_mb * 1_000_000),
                installed_bytes: bytes.unwrap_or(entry.installed_mb * 1_000_000),
                licence: entry.licence.clone(),
                state: match state {
                    State::Installed => InstallableState::Installed,
                    State::NotInstalled => InstallableState::NotInstalled,
                    State::Installing => InstallableState::Installing,
                    State::Unavailable => InstallableState::Unavailable,
                },
                progress: (state == State::Installing).then(|| {
                    status
                        .as_ref()
                        .and_then(|s| s.get("progress"))
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0)
                        .clamp(0.0, 1.0)
                }),
                requirements: Vec::new(),
                notes: (state == State::Unavailable).then(|| {
                    format!(
                        "Does not run on {} (runs on {}).",
                        crate::segmenters::platform(),
                        entry.platforms.join(", ")
                    )
                }),
                last_outcome,
                extensions,
            }
        })
        .collect()
}

/// Each artifact's recorded size, by file name, from the pack's `artifacts.toml` (at the
/// install's root, or under `src/rascal_tools/` in a checkout).
fn artifact_sizes(install: &std::path::Path) -> BTreeMap<String, u64> {
    #[derive(serde::Deserialize)]
    struct Manifest {
        #[serde(default)]
        artifact: Vec<Artifact>,
    }
    #[derive(serde::Deserialize)]
    struct Artifact {
        name: String,
        size: Option<u64>,
    }
    [
        install.join("artifacts.toml"),
        install.join("src/rascal_tools/artifacts.toml"),
    ]
    .into_iter()
    .find(|p| p.is_file())
    .and_then(|p| std::fs::read_to_string(p).ok())
    .and_then(|text| toml::from_str::<Manifest>(&text).ok())
    .map(|m| {
        m.artifact
            .into_iter()
            .filter_map(|a| Some((a.name, a.size?)))
            .collect()
    })
    .unwrap_or_default()
}

/// The longest a built-in operation is expected to take, as a multiple of its manifest's
/// `cost.seconds`, and at least [`OPERATION_MAX_S_FLOOR`] seconds: a large image takes many
/// times the typical call.
pub const OPERATION_MAX_S_FACTOR: f64 = 100.0;

/// The least `duration.max_s` of an operation, in seconds.
pub const OPERATION_MAX_S_FLOOR: f64 = 60.0;

/// A built-in op as the contract's operation (4.7): its manifest's name, purpose (with when to
/// use it and when not) and ports, typed by the contract's names; its group; its parameters with
/// the constant sets ([`Manifest::params_schema`](rascal_core::op::Manifest::params_schema)); no
/// image or location slot (its inputs are ports only), no spend, ready with no requirement (a
/// built-in op reads no configuration) and no renders (its picture is its preview). `outputs`
/// names the roles of the images its out-ports make, `preview` when it makes an artifact whose
/// type has one, and `found` when it makes a location.
fn operation(op: &dyn Op) -> Action {
    let manifest = op.manifest();
    let mut roles: Vec<String> = manifest
        .outputs
        .iter()
        .filter(|p| p.artifact_type == ArtifactType::Image)
        .map(|p| p.name.clone())
        .collect();
    if manifest.outputs.iter().any(|p| p.artifact_type.preview()) {
        roles.push("preview".to_string());
    }
    let found = manifest
        .outputs
        .iter()
        .any(|p| p.artifact_type == ArtifactType::Location);
    let mut title = manifest.name.clone();
    if let Some(first) = title.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    let seconds = manifest.cost.seconds;
    Action {
        name: manifest.name.clone(),
        title,
        kind: ActionKind::Operation,
        group: Some(op.group().name().to_string()),
        purpose: format!(
            "{} When to use: {} When not to use: {}",
            manifest.purpose, manifest.when_to_use, manifest.when_not_to_use
        ),
        answers: None,
        params_schema: manifest.params_schema(),
        images: Some(vec![]),
        locations: Some(vec![]),
        outputs: Some(Outputs {
            images: roles,
            locations: if found {
                vec![LocationOutput::Found]
            } else {
                vec![]
            },
        }),
        ports: Some(Ports {
            inputs: manifest
                .inputs
                .iter()
                .map(rascal_core::contract::port)
                .collect(),
            out: manifest
                .outputs
                .iter()
                .map(rascal_core::contract::port)
                .collect(),
        }),
        cost: Cost {
            may_spend: false,
            basis: None,
        },
        readiness: Readiness {
            state: ReadinessState::Ready,
            requirements: vec![],
        },
        duration: Duration {
            typical_s: seconds,
            max_s: (seconds * OPERATION_MAX_S_FACTOR).max(OPERATION_MAX_S_FLOOR),
        },
        renders: vec![],
    }
}

/// The built-in ops as operations, by group (in [`Group::ALL`] order), then by name; never a
/// pack's (a description is a local inspection, and a pack's ops are known only by starting it).
fn operations() -> Result<Vec<Action>> {
    let registry = rascal_ops::registry()?;
    let mut ops: Vec<&dyn Op> = registry
        .names()
        .into_iter()
        .filter_map(|name| registry.get(name))
        .collect();
    ops.sort_by_cached_key(|op| (op.group(), op.manifest().name));
    Ok(ops.into_iter().map(operation).collect())
}

/// The description under the configuration `setup` holds (its fillers name the spend and the
/// keys; without one no filler is configured and nothing spends): the shipped recipes as
/// `task`s with no `group`, each one's readiness its default use (3.1, [`readiness::for_call`]),
/// then the built-in ops as `operation`s ([`operation`]), with the operations profile's
/// `artifact_types` and the groups in `vocabulary.group`. It never fails on the configuration;
/// the [`RESERVED`] names are tools, never actions.
pub fn description(setup: &Setup<'_>) -> Result<Description> {
    let config = setup.config.config();
    let fillers: Vec<String> = config
        .map(|c| c.fillers.keys().cloned().collect())
        .unwrap_or_default();
    // An action may spend when a configured filler has a price (`spend_usd_per_call`, the
    // run's estimated spend); a filler without one is free by the configuration.
    let remote = config.is_some_and(|c| {
        c.fillers
            .values()
            .any(|f| f.spend_usd_per_call.is_some_and(|p| p > 0.0))
            || c.checker
                .as_ref()
                .is_some_and(|k| k.spend_usd_per_call > 0.0)
    });
    let checker = config.is_some_and(|c| c.checker.is_some());
    let max_s = config
        .and_then(|c| c.fillers.values().map(|f| f.timeout_s).max())
        .unwrap_or(0)
        .max(60) as f64
        + 60.0;
    let segmenters = config.and_then(crate::segmenters::Segmenters::of);
    let mut actions = Vec::new();
    for (name, _) in SHIPPED {
        let recipe = load_recipe(name)?;
        let (title, purpose) = title_and_purpose(name);
        let mut properties = serde_json::Map::new();
        let mut required: Vec<String> = Vec::new();
        let mut images = Vec::new();
        let mut locations = Vec::new();
        for (input, decl) in &recipe.inputs {
            let optional = decl.optional;
            match decl.kind {
                InputType::Image => images.push(ImageSlot {
                    slot: input.clone(),
                    role: if input == "image" {
                        "source".to_string()
                    } else {
                        input.clone()
                    },
                    required: !optional,
                    many: false,
                }),
                InputType::Location => locations.push(LocationSlot {
                    slot: input.clone(),
                    kinds: location_kinds(name, input),
                    required: !optional,
                }),
                InputType::Text => {
                    properties.insert(
                        input.clone(),
                        json!({ "type": "string", "minLength": 1,
                                "description": format!("The recipe's {input} (text).") }),
                    );
                    if !optional {
                        required.push(input.clone());
                    }
                }
                InputType::Mask => {
                    properties.insert(input.clone(), mask_reference_schema());
                    if !optional {
                        required.push(input.clone());
                    }
                }
                _ => {}
            }
        }
        properties.extend(common_properties(&recipe, &fillers));
        // remove_prompt's `model` (SPEC.md "Choosable segmenters"): the catalogued segmenters
        // that run here, the resolved default first and as the default, each naming the
        // requirement its choice needs (table-manners 3.1's `x-requirements`).
        if properties.contains_key(readiness::MODEL_PARAM)
            && let Some(schema) = segmenters.as_ref().and_then(|s| model_schema(s, config))
        {
            properties.insert(readiness::MODEL_PARAM.to_string(), schema);
        }
        // With a checker configured, remove_prompt asks it whether the object is gone after a
        // fill and tries again (ADR 0023); `check` turns that off for one call.
        let checked = checker && action_name(name) == readiness::SEGMENTER_ACTION;
        if checked {
            properties.insert(
                crate::checker::ARGUMENT.to_string(),
                json!({ "type": "boolean", "default": true, "title": "Check the result",
                        "description": format!(
                            "After the fill, ask the configured checker whether any of the prompt's object is still visible, and when it is, add what it points at to the mask and fill again (at most {} tries in all, within the spending limit). Applies when a filler is chosen.",
                            crate::checker::MAX_TRIES) }),
            );
        }
        let found = recipe.step.iter().any(|s| s.op == "segment");
        actions.push(Action {
            name: action_name(name),
            title: title.to_string(),
            kind: ActionKind::Task,
            group: None,
            purpose: purpose.to_string(),
            answers: checked.then_some(Answers::Verdict),
            params_schema: json!({
                "type": "object",
                "properties": Value::Object(properties),
                "required": required,
                "additionalProperties": false,
            }),
            images: Some(images),
            locations: Some(locations),
            outputs: Some(Outputs {
                images: vec!["result".to_string(), "mask".to_string()],
                locations: if found {
                    vec![LocationOutput::Found]
                } else {
                    vec![]
                },
            }),
            cost: Cost {
                may_spend: remote,
                basis: remote.then_some(SpendBasis::Estimated),
            },
            readiness: readiness::for_call(setup, &action_name(name), Use::Configured, None),
            duration: Duration {
                typical_s: if found { 20.0 } else { 5.0 },
                max_s,
            },
            ports: None,
            renders: renders(),
        });
    }
    actions.extend(operations()?);
    Ok(Description {
        contract: rascal_core::contract::CONTRACT_VERSION.to_string(),
        profiles: vec![Profile::Images, Profile::Operations],
        tool: DescribedTool {
            name: "rascal".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            purpose: "Finds, masks and composites regions of an image.".to_string(),
        },
        actions,
        installables: segmenters.as_ref().map(installables),
        vocabulary: Some(Vocabulary {
            group: Some(Group::ALL.map(|g| g.name().to_string()).to_vec()),
            disposition: Some(vec![]),
            confidence: Some(vec![]),
            evidence: Some(vec![]),
            score_kind: Some(vec![SEGMENT_SCORE_KIND.to_string()]),
        }),
        artifact_types: Some(rascal_core::contract::artifact_types()),
        limits: Some(Limits {
            inline_bytes: INLINE_BYTES,
        }),
        progress_interval_s: PROGRESS_INTERVAL_S,
        resources: vec![
            "rascal://runs{?cursor}".to_string(),
            "rascal://run/{id}/run.json".to_string(),
            "rascal://run/{id}/journal".to_string(),
            "rascal://session".to_string(),
            "rascal://artifact/{identity}".to_string(),
            "rascal://artifact/{identity}/preview".to_string(),
        ],
        extensions: BTreeMap::new(),
    })
}

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
    Action, ActionKind, Cost, DescribedTool, Description, Duration, Profile, Readiness,
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
                "description": "The image's projection; without it an exactly 2:1 image is equirectangular." });
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
pub const RESERVED: [&str; 4] = ["describe", "estimate", "export", "import"];

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
    });
    let max_s = config
        .and_then(|c| c.fillers.values().map(|f| f.timeout_s).max())
        .unwrap_or(0)
        .max(60) as f64
        + 60.0;
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
        let found = recipe.step.iter().any(|s| s.op == "segment");
        actions.push(Action {
            name: action_name(name),
            title: title.to_string(),
            kind: ActionKind::Task,
            group: None,
            purpose: purpose.to_string(),
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
            readiness: readiness::for_call(setup, &action_name(name), Use::Configured),
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

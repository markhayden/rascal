//! The contract's description document (table-manners 3 and 11.2; ADR 0020): one action per
//! shipped recipe, built from the recipes and the configuration, so a client that knows only the
//! contract can form a call. `rascal describe --json` prints it; `serve` returns it from the
//! `describe` tool.

use std::collections::BTreeMap;

use rascal_core::config::Config;
use rascal_core::contract::table_manners::images::{
    ImageSlot, Limits, LocationKind, LocationOutput, LocationSlot, Outputs, Vocabulary,
};
use rascal_core::contract::table_manners::{
    Action, Cost, DescribedTool, Description, Duration, Profile, Render, SpendBasis,
};
use rascal_core::error::Result;
use rascal_core::recipe::InputType;
use serde_json::{Value, json};

use crate::recipes::{SHIPPED, load_recipe};

/// The largest inline image `serve` takes, in decoded bytes (11.2): 64 MiB.
pub const INLINE_BYTES: u64 = 64 << 20;

/// The longest gap between progress notifications on a live run, in seconds (4.1).
pub const PROGRESS_INTERVAL_S: u32 = 10;

/// What the segmenter's detection score is, in a location's `score_kind` (9.2).
pub const SEGMENT_SCORE_KIND: &str = "owlv2-detection";

/// A shipped recipe's name as an action name: lowercase with underscores (3). An action is an
/// MCP tool beside the op tools in `serve`, so a recipe named like an op takes another name:
/// the `rect` recipe (a feathered rectangle as a mask) is `mask_rect`, the `rect` op stays.
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
            "Masks the wrap edge of a panorama (or any vertical line) as a band, wide on smooth surfaces and thin on detail, with a tone ramp across it; with a filler, heals it.",
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
        "rect" => ("Mask a rectangle", "A feathered rectangle as a mask."),
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

/// The description for the shipped recipes under `config` (its fillers name the spend and the
/// keys; without one no filler is configured and nothing spends).
pub fn description(config: Option<&Config>) -> Result<Description> {
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
    let mut needs_env: Vec<String> = config
        .map(|c| {
            c.fillers
                .values()
                .filter_map(|f| f.key_env.clone())
                .collect()
        })
        .unwrap_or_default();
    needs_env.sort();
    needs_env.dedup();
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
                    properties.insert(
                        input.clone(),
                        json!({ "type": "string",
                                "description": "A stored rascal mask directory (an earlier run's `mask`), by absolute path." }),
                    );
                    if !optional {
                        required.push(input.clone());
                    }
                }
                _ => {}
            }
        }
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
        properties.insert(
            "projection".to_string(),
            json!({ "type": "string", "enum": ["flat", "equirectangular"],
                    "description": "The image's projection; without it an exactly 2:1 image is equirectangular." }),
        );
        properties.insert(
            "strict".to_string(),
            json!({ "type": "boolean",
                    "description": "Fail (exit 1) when the recipe stops at a step that is not applicable." }),
        );
        let found = recipe.step.iter().any(|s| s.op == "segment");
        actions.push(Action {
            name: action_name(name),
            title: title.to_string(),
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
                images: vec!["mask".to_string(), "result".to_string()],
                locations: if found {
                    vec![LocationOutput::Found]
                } else {
                    vec![]
                },
            }),
            cost: Cost {
                may_spend: remote,
                basis: remote.then_some(SpendBasis::Estimated),
                needs_env: needs_env.clone(),
            },
            duration: Duration {
                typical_s: if found { 20.0 } else { 5.0 },
                max_s,
            },
            renders: vec![Render {
                name: "dash".to_string(),
                command: ["rascal", "dash", "{run.dir}", "--serve", "--port", "0"]
                    .map(str::to_string)
                    .to_vec(),
            }],
        });
    }
    actions.push(Action {
        name: "estimate".to_string(),
        title: "Estimate a call".to_string(),
        purpose: "The worst case a call to an action would spend with a filler, the per-call cap and the session budget left; runs nothing.".to_string(),
        params_schema: json!({
            "type": "object",
            "properties": {
                "action": { "type": "string",
                            "enum": actions.iter().map(|a| a.name.clone()).collect::<Vec<_>>() },
                "filler": { "type": "string" },
                "max_spend_usd": { "type": "number", "minimum": 0 }
            },
            "required": ["action"],
            "additionalProperties": false,
        }),
        images: Some(vec![]),
        locations: Some(vec![]),
        outputs: Some(Outputs {
            images: vec![],
            locations: vec![],
        }),
        cost: Cost {
            may_spend: false,
            basis: None,
            needs_env: vec![],
        },
        duration: Duration {
            typical_s: 0.1,
            max_s: 1.0,
        },
        renders: vec![],
    });
    Ok(Description {
        contract: rascal_core::contract::CONTRACT_VERSION.to_string(),
        profiles: vec![Profile::Images],
        tool: DescribedTool {
            name: "rascal".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            purpose: "Finds, masks and composites regions of an image.".to_string(),
        },
        actions,
        vocabulary: Some(Vocabulary {
            disposition: vec![],
            confidence: vec![],
            evidence: vec![],
            score_kind: vec![SEGMENT_SCORE_KIND.to_string()],
        }),
        limits: Some(Limits {
            inline_bytes: INLINE_BYTES,
        }),
        progress_interval_s: PROGRESS_INTERVAL_S,
        resources: vec![
            "rascal://runs".to_string(),
            "rascal://run/{id}/run.json".to_string(),
            "rascal://run/{id}/journal".to_string(),
        ],
        extensions: BTreeMap::new(),
    })
}

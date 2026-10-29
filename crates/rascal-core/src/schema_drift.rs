//! The boundary-type inventory and the drift harness.
//!
//! A **boundary type** is any type that is written to disk, sent over MCP or read from a file a
//! person edits. Each one is registered here with the [`boundary_type!`] macro, which only
//! compiles for a type that derives `Serialize`, `Deserialize` and `JsonSchema`. For each
//! registered type the harness compares the committed schema with the generated one (as JSON,
//! so formatting is not drift), asserts the committed schema is closed everywhere it describes
//! an object (`deny_unknown_fields`) and versioned (a required `version` or `*_version` field
//! on every shape, or a `-v<N>` suffix on the schema's `$id` for a type versioned by its
//! schema, ADR 0009), validates every serialized fixture against the committed schema and
//! round-trips it through the type. A test greps the crates for the derive and fails on any
//! type not registered.

use std::path::Path;

use crate::schemas::SchemaDir;

/// Where a boundary type's committed schema comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Generated from the type by `schemars` and compared with the committed file.
    Generated,
    /// Written by hand (it references a copied schema); the generated schema is not compared.
    /// The closed and version checks and the fixtures still apply to the committed file.
    Authored,
}

/// How a boundary type carries its version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Versioning {
    /// Every instance has a required `version` or `*_version` property.
    Field,
    /// The schema's `$id` ends in `-v<N>` and instances carry no version of their own, for a
    /// type whose shape is shared with a copied schema that has none (the location, ADR 0009).
    SchemaId,
}

/// One registered boundary type.
#[derive(Debug, Clone)]
pub struct BoundaryType {
    /// The Rust type name, as the derive grep sees it.
    pub name: &'static str,
    /// The committed schema, relative to `schemas/`.
    pub schema: &'static str,
    /// Generated or authored.
    pub source: Source,
    /// By field or by schema id.
    pub versioning: Versioning,
    /// The schema `schemars` generates for the type.
    pub generate: fn() -> serde_json::Value,
    /// Deserializes a value as the type and serializes it again.
    pub roundtrip: fn(&serde_json::Value) -> Result<serde_json::Value, String>,
    /// Serialized fixtures, relative to `fixtures/schemas/`.
    pub fixtures: &'static [&'static str],
    /// Names of `$defs` whose objects are open on purpose, each justified where it is listed
    /// (a recipe step carries op-specific parameters). Everything else must be closed.
    pub open: &'static [&'static str],
}

/// Registers a boundary type, versioned by field. Compiles only for a type with the three
/// derives. A type versioned by its schema id sets `versioning` on the result.
#[macro_export]
macro_rules! boundary_type {
    ($type:ty, $schema:expr, $source:expr, [$($fixture:expr),* $(,)?]) => {
        $crate::schema_drift::BoundaryType {
            name: stringify!($type),
            schema: $schema,
            source: $source,
            versioning: $crate::schema_drift::Versioning::Field,
            generate: || ::schemars::schema_for!($type).to_value(),
            roundtrip: |value| {
                let typed: $type =
                    ::serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                ::serde_json::to_value(typed).map_err(|e| e.to_string())
            },
            fixtures: &[$($fixture),*],
            open: &[],
        }
    };
}

/// Every boundary type in the core. Each task that adds one adds a line here.
pub fn inventory() -> Vec<BoundaryType> {
    use crate::config::Config;
    use crate::frame::{Frame, Plan};
    use crate::image::ImageRecord;
    use crate::journal::JournalLine;
    use crate::location::Location;
    use crate::mask::{InspectReport, MaskRecord};
    use crate::op::{Envelope, Manifest};
    use crate::pack::{PackAnnotation, PackContext};
    use crate::profile::BandProfile;
    use crate::recipe::Recipe;
    use crate::replay::ReplayReport;
    use crate::series::Series;
    use crate::workspace::record::OperationRecord;
    use crate::workspace::registry::Generation;
    use crate::workspace::repair::Report;
    vec![
        boundary_type!(
            Report,
            "rascal/repair-v1.json",
            Source::Generated,
            ["repair-v1/after-crash.json"]
        ),
        boundary_type!(
            InspectReport,
            "rascal/inspect-v1.json",
            Source::Generated,
            ["inspect-v1/rect.json"]
        ),
        boundary_type!(
            ReplayReport,
            "rascal/replay-v1.json",
            Source::Generated,
            ["replay-v1/tampered-core.json"]
        ),
        boundary_type!(
            Frame,
            "rascal/frame-v1.json",
            Source::Generated,
            [
                "frame-v1/raw-flat.json",
                "frame-v1/raw-equirectangular.json",
                "frame-v1/cut.json",
                "frame-v1/crop.json"
            ]
        ),
        boundary_type!(
            Plan,
            "rascal/plan-v1.json",
            Source::Generated,
            ["plan-v1/strip.json"]
        ),
        boundary_type!(
            ImageRecord,
            "rascal/image-v1.json",
            Source::Generated,
            ["image-v1/canonical.json", "image-v1/rendering.json"]
        ),
        boundary_type!(
            MaskRecord,
            "rascal/mask-v1.json",
            Source::Generated,
            ["mask-v1/rect.json", "mask-v1/seam.json"]
        ),
        boundary_type!(
            JournalLine,
            "rascal/journal-v1.json",
            Source::Generated,
            ["journal-v1/op.json", "journal-v1/note.json"]
        ),
        boundary_type!(
            Manifest,
            "rascal/manifest-v1.json",
            Source::Generated,
            ["manifest-v1/rect.json"]
        ),
        boundary_type!(
            Envelope,
            "rascal/envelope-v1.json",
            Source::Generated,
            ["envelope-v1/ok.json", "envelope-v1/not-applicable.json"]
        ),
        boundary_type!(
            Config,
            "rascal/config-v1.json",
            Source::Generated,
            ["config-v1/minimal.json", "config-v1/full.json"]
        ),
        boundary_type!(
            PackAnnotation,
            "rascal/pack-annotation-v1.json",
            Source::Generated,
            ["pack-annotation-v1/segment.json"]
        ),
        boundary_type!(
            PackContext,
            "rascal/pack-context-v1.json",
            Source::Generated,
            ["pack-context-v1/path.json", "pack-context-v1/base64.json"]
        ),
        boundary_type!(
            Generation,
            "rascal/generation-v1.json",
            Source::Generated,
            ["generation-v1/after-rect.json"]
        ),
        boundary_type!(
            OperationRecord,
            "rascal/operation-v1.json",
            Source::Generated,
            ["operation-v1/rasterize.json"]
        ),
        BoundaryType {
            // A step's parameters are the op's own keys, so Step is open on purpose.
            open: &["Step"],
            ..boundary_type!(
                Recipe,
                "rascal/recipe-v1.json",
                Source::Authored,
                ["recipe-v1/rect.json", "recipe-v1/seam.json"]
            )
        },
        boundary_type!(
            BandProfile,
            "rascal/band-profile-v1.json",
            Source::Generated,
            ["band-profile-v1/seam.json"]
        ),
        boundary_type!(
            Series,
            "rascal/series-v1.json",
            Source::Generated,
            ["series-v1/sigma.json", "series-v1/step.json"]
        ),
        BoundaryType {
            versioning: Versioning::SchemaId,
            ..boundary_type!(
                Location,
                "rascal/location-v1.json",
                Source::Authored,
                [
                    "location-v1/valid/absent.json",
                    "location-v1/valid/bbox.json",
                    "location-v1/valid/line-column.json",
                    "location-v1/valid/line-path.json",
                    "location-v1/valid/polygon.json",
                    "location-v1/valid/stroke.json",
                    "location-v1/valid/view.json",
                    "location-v1/valid/whole_image.json",
                ]
            )
        },
    ]
}

/// Types that derive `JsonSchema` only as parts of a registered type and have no schema file
/// of their own. The schema test asserts each appears in the `$defs` of a committed schema,
/// so a type cannot hide here.
pub const COMPONENTS: &[&str] = &[
    "Projection",
    "Topology",
    "FrameKind",
    "PlanEntry",
    "Role",
    "RenderingRole",
    "UseCase",
    "Zones",
    "Source",
    "ProfileSection",
    "Prescription",
    "ProvenanceNode",
    "Windows",
    "WorkspaceConfig",
    "CorpusConfig",
    "FillerConfig",
    "PackConfig",
    "Accepts",
    "ContextImage",
    "InputDecl",
    "RenderOut",
    "Outputs",
    "Step",
    "ArtifactEntry",
    "LocationEntry",
    "RecordEntry",
    "ZoneStatistics",
    "InspectProfile",
    "Refinement",
    "ParamKind",
    "CostTier",
    "Cost",
    "Bounds",
    "Measurement",
    "Region",
    "EnvelopeImage",
    "Transform",
    "Provenance",
    "ArtifactType",
    "Port",
    "Param",
    "Status",
    "ArtifactRef",
    "Event",
];

/// What the harness found wrong with one boundary type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    /// The committed schema file is missing.
    MissingSchema { schema: String },
    /// The committed schema differs from the generated one.
    Drift { schema: String },
    /// The schema accepts unknown fields somewhere it describes an object.
    NotClosed { schema: String },
    /// No version: by field, a shape without a required `version` or `*_version` property; by
    /// schema id, an `$id` that does not end in `-v<N>`.
    MissingVersion { schema: String },
    /// The committed schema cannot be read or compiled.
    Unusable { schema: String, message: String },
    /// A fixture does not validate against the committed schema.
    FixtureInvalid { fixture: String, message: String },
    /// A fixture does not survive deserialize-then-serialize unchanged.
    FixtureRoundTrip { fixture: String, message: String },
    /// A fixture file cannot be read.
    FixtureUnreadable { fixture: String, message: String },
}

/// Checks one boundary type against the committed schema under `schemas` and the fixtures
/// under `fixtures` (the `fixtures/schemas/` directory). An empty list is a pass.
pub fn check(entry: &BoundaryType, schemas: &SchemaDir, fixtures: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    let schema_name = entry.schema.to_string();
    let path = schemas.path(entry.schema);
    if !path.is_file() {
        findings.push(Finding::MissingSchema {
            schema: schema_name,
        });
        return findings;
    }
    let committed = match schemas.read(entry.schema) {
        Ok(value) => value,
        Err(error) => {
            findings.push(Finding::Unusable {
                schema: schema_name,
                message: error.to_string(),
            });
            return findings;
        }
    };

    if entry.source == Source::Generated && !same_json(&(entry.generate)(), &committed) {
        findings.push(Finding::Drift {
            schema: schema_name.clone(),
        });
    }
    if !is_closed_except(&committed, entry.open) {
        findings.push(Finding::NotClosed {
            schema: schema_name.clone(),
        });
    }
    let versioned = match entry.versioning {
        Versioning::Field => has_version(&committed, &committed),
        Versioning::SchemaId => id_is_versioned(&committed),
    };
    if !versioned {
        findings.push(Finding::MissingVersion {
            schema: schema_name.clone(),
        });
    }

    let validator = match schemas.validator_for(&committed) {
        Ok(validator) => validator,
        Err(message) => {
            findings.push(Finding::Unusable {
                schema: schema_name,
                message,
            });
            return findings;
        }
    };
    for fixture in entry.fixtures {
        let path = fixtures.join(fixture);
        let value: serde_json::Value = match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| serde_json::from_str(&text).map_err(|e| e.to_string()))
        {
            Ok(value) => value,
            Err(message) => {
                findings.push(Finding::FixtureUnreadable {
                    fixture: fixture.to_string(),
                    message,
                });
                continue;
            }
        };
        if let Err(error) = validator.validate(&value) {
            findings.push(Finding::FixtureInvalid {
                fixture: fixture.to_string(),
                message: error.to_string(),
            });
        }
        match (entry.roundtrip)(&value) {
            Ok(again) if same_json(&again, &value) => {}
            Ok(_) => findings.push(Finding::FixtureRoundTrip {
                fixture: fixture.to_string(),
                message: "serialized form differs".to_string(),
            }),
            Err(message) => findings.push(Finding::FixtureRoundTrip {
                fixture: fixture.to_string(),
                message,
            }),
        }
    }
    findings
}

/// The text a generated schema is committed as: pretty JSON with one trailing newline.
pub fn render(schema: &serde_json::Value) -> String {
    serde_json::to_string_pretty(schema).unwrap_or_default() + "\n"
}

/// JSON equality where `90` and `90.0` are the same number.
fn same_json(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    use serde_json::Value::{Array, Number, Object};
    match (a, b) {
        (Number(x), Number(y)) => x.as_f64() == y.as_f64(),
        (Array(x), Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same_json(p, q)),
        (Object(x), Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, p)| y.get(k).is_some_and(|q| same_json(p, q)))
        }
        _ => a == b,
    }
}

/// `is_closed`, with the named `$defs` exempt.
fn is_closed_except(schema: &serde_json::Value, open: &[&str]) -> bool {
    if open.is_empty() {
        return is_closed(schema);
    }
    let mut trimmed = schema.clone();
    if let Some(defs) = trimmed.get_mut("$defs").and_then(|d| d.as_object_mut()) {
        for name in open {
            defs.remove(*name);
        }
    }
    is_closed(&trimmed)
}

/// True when every object the schema describes refuses unknown properties. An object with no
/// `properties` and a schema-valued (or `true`) `additionalProperties` is a map and closed by
/// construction: it has no fields to misspell.
fn is_closed(schema: &serde_json::Value) -> bool {
    let Some(object) = schema.as_object() else {
        return true;
    };
    let has_properties = object.contains_key("properties");
    let is_object = object.get("type").and_then(|t| t.as_str()) == Some("object") || has_properties;
    let additional = object.get("additionalProperties");
    if is_object {
        // A map: no named properties, values described by a schema (or anything, `true`).
        let is_map = !has_properties
            && additional.is_some_and(|a| a.is_object() || *a == serde_json::Value::Bool(true));
        if !is_map && additional != Some(&serde_json::Value::Bool(false)) {
            return false;
        }
    }
    let children = ["properties", "$defs"]
        .into_iter()
        .filter_map(|key| object.get(key).and_then(|v| v.as_object()))
        .flat_map(|map| map.values());
    let variants = ["oneOf", "anyOf", "allOf", "prefixItems"]
        .into_iter()
        .filter_map(|key| object.get(key).and_then(|v| v.as_array()))
        .flatten();
    let single = ["items", "additionalProperties"]
        .into_iter()
        .filter_map(|key| object.get(key))
        .filter(|v| v.is_object());
    children.chain(variants).chain(single).all(is_closed)
}

/// True when every shape the schema can take requires a `version` or `*_version` property:
/// an object by its `required`, a `oneOf`/`anyOf` by every branch, an `allOf` by any part, a
/// local `$ref` by its target. A reference into another file cannot be inspected and fails.
fn has_version(schema: &serde_json::Value, root: &serde_json::Value) -> bool {
    let Some(object) = schema.as_object() else {
        return false;
    };
    if let Some(reference) = object.get("$ref").and_then(|r| r.as_str()) {
        return reference
            .strip_prefix("#/$defs/")
            .and_then(|name| root.get("$defs").and_then(|d| d.get(name)))
            .is_some_and(|target| has_version(target, root));
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(branches) = object.get(key).and_then(|v| v.as_array()) {
            return !branches.is_empty() && branches.iter().all(|b| has_version(b, root));
        }
    }
    if let Some(parts) = object.get("allOf").and_then(|v| v.as_array()) {
        return parts.iter().any(|p| has_version(p, root));
    }
    object
        .get("required")
        .and_then(|r| r.as_array())
        .is_some_and(|required| {
            required
                .iter()
                .filter_map(|r| r.as_str())
                .any(|name| name == "version" || name.ends_with("_version"))
        })
}

/// True when the schema's `$id` ends in `-v<N>`.
fn id_is_versioned(schema: &serde_json::Value) -> bool {
    schema
        .get("$id")
        .and_then(|id| id.as_str())
        .and_then(|id| id.rsplit_once("-v"))
        .is_some_and(|(_, digits)| !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()))
}

/// Type names that derive `JsonSchema` in `source` but are not in `registered`. `source` is
/// Rust text. An attribute may span lines (as rustfmt writes a long derive list) and may be a
/// `cfg_attr`; the `struct` or `enum` it decorates follows, possibly after further attributes,
/// doc comments and block comments. Derive text inside `//` and `///` comments is ignored.
pub fn unregistered_types(source: &str, registered: &[&str]) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = false;
    let mut attribute: Option<(String, i32)> = None;
    let mut in_block_comment = false;
    for raw in source.lines() {
        let line = raw.trim();
        if in_block_comment {
            if line.contains("*/") {
                in_block_comment = false;
            }
            continue;
        }
        if line.starts_with("/*") {
            in_block_comment = !line.contains("*/");
            continue;
        }
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        if let Some((text, depth)) = attribute.as_mut() {
            text.push(' ');
            text.push_str(line);
            *depth += bracket_depth(line);
            if *depth <= 0 {
                if is_json_schema_derive(text) {
                    pending = true;
                }
                attribute = None;
            }
            continue;
        }
        if line.starts_with("#[") || line.starts_with("#![") {
            let depth = bracket_depth(line);
            if depth > 0 {
                attribute = Some((line.to_string(), depth));
            } else if is_json_schema_derive(line) {
                pending = true;
            }
            continue;
        }
        if !pending {
            continue;
        }
        pending = false;
        if let Some(name) = item_name(line)
            && !registered.contains(&name.as_str())
        {
            found.push(name);
        }
    }
    found
}

fn bracket_depth(line: &str) -> i32 {
    line.chars().fold(0, |depth, c| match c {
        '[' | '(' => depth + 1,
        ']' | ')' => depth - 1,
        _ => depth,
    })
}

/// True when the attribute text has `derive(` and the token `JsonSchema` (not a longer name).
fn is_json_schema_derive(text: &str) -> bool {
    if !text.contains("derive(") {
        return false;
    }
    let bytes = text.as_bytes();
    text.match_indices("JsonSchema").any(|(start, token)| {
        let end = start + token.len();
        let before = start.checked_sub(1).map(|i| bytes[i] as char);
        let after = bytes.get(end).map(|b| *b as char);
        !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
            && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

/// The name of the `struct` or `enum` an item line declares, with any `pub(...)` stripped.
fn item_name(line: &str) -> Option<String> {
    let mut rest = line;
    if let Some(after_pub) = rest.strip_prefix("pub") {
        rest = if let Some(inner) = after_pub.strip_prefix('(') {
            inner.split_once(')')?.1
        } else {
            after_pub
        };
    }
    let rest = rest.trim_start();
    let rest = rest
        .strip_prefix("struct ")
        .or_else(|| rest.strip_prefix("enum "))?;
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

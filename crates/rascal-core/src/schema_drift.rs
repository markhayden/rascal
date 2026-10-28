//! The boundary-type inventory and the drift harness.
//!
//! A **boundary type** is any type that is written to disk, sent over MCP or read from a file a
//! person edits. Each one is registered here with the [`boundary_type!`] macro, which only
//! compiles for a type that derives `Serialize`, `Deserialize` and `JsonSchema`. For each
//! registered type the harness regenerates the schema and compares it to the committed file,
//! asserts the schema is closed (`deny_unknown_fields`) and carries a version field, and
//! validates every serialized fixture against the committed schema and round-trips it through
//! the type. A test greps the crates for the derive and fails on any type not registered.

use std::path::Path;

use crate::schemas::SchemaDir;

/// Where a boundary type's committed schema comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Generated from the type by `schemars` and compared byte for byte.
    Generated,
    /// Written by hand (it references a copied schema); the type's generated schema is not
    /// compared, but the fixtures must validate against the committed file and round-trip.
    Authored,
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
    /// The schema `schemars` generates for the type.
    pub generate: fn() -> serde_json::Value,
    /// Deserializes a value as the type and serializes it again.
    pub roundtrip: fn(&serde_json::Value) -> Result<serde_json::Value, String>,
    /// Serialized fixtures, relative to `fixtures/schemas/`.
    pub fixtures: &'static [&'static str],
}

/// Registers a boundary type. Compiles only for a type with the three derives.
#[macro_export]
macro_rules! boundary_type {
    ($type:ty, $schema:expr, $source:expr, [$($fixture:expr),* $(,)?]) => {
        $crate::schema_drift::BoundaryType {
            name: stringify!($type),
            schema: $schema,
            source: $source,
            generate: || ::schemars::schema_for!($type).to_value(),
            roundtrip: |value| {
                let typed: $type =
                    ::serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                ::serde_json::to_value(typed).map_err(|e| e.to_string())
            },
            fixtures: &[$($fixture),*],
        }
    };
}

/// Every boundary type in the core. Each task that adds one adds a line here.
pub fn inventory() -> Vec<BoundaryType> {
    Vec::new()
}

/// What the harness found wrong with one boundary type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    /// The committed schema file is missing.
    MissingSchema { schema: String },
    /// The committed schema differs from the generated one.
    Drift { schema: String },
    /// The schema accepts unknown fields somewhere it describes an object.
    NotClosed { schema: String },
    /// No required property named `version` or ending in `_version`.
    MissingVersion { schema: String },
    /// The committed schema cannot be compiled.
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
    let committed = match schemas.read(entry.schema) {
        Ok(value) => value,
        Err(_) => {
            findings.push(Finding::MissingSchema {
                schema: schema_name,
            });
            return findings;
        }
    };

    if entry.source == Source::Generated {
        let generated = (entry.generate)();
        if canonical(&generated) != canonical(&committed) {
            findings.push(Finding::Drift {
                schema: schema_name.clone(),
            });
        }
        if !is_closed(&generated) {
            findings.push(Finding::NotClosed {
                schema: schema_name.clone(),
            });
        }
        if !has_version(&generated) {
            findings.push(Finding::MissingVersion {
                schema: schema_name.clone(),
            });
        }
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
            Ok(again) if again == value => {}
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

/// The text a generated schema is committed as: pretty JSON with a trailing newline.
pub fn render(schema: &serde_json::Value) -> String {
    canonical(schema) + "\n"
}

fn canonical(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

/// True when every object the schema describes refuses unknown properties.
fn is_closed(schema: &serde_json::Value) -> bool {
    let Some(object) = schema.as_object() else {
        return true;
    };
    let is_object = object.get("type").and_then(|t| t.as_str()) == Some("object")
        || object.contains_key("properties");
    if is_object && object.get("additionalProperties") != Some(&serde_json::Value::Bool(false)) {
        return false;
    }
    for key in ["oneOf", "anyOf", "allOf"] {
        if let Some(variants) = object.get(key).and_then(|v| v.as_array())
            && !variants.iter().all(is_closed)
        {
            return false;
        }
    }
    if let Some(defs) = object.get("$defs").and_then(|d| d.as_object())
        && !defs.values().all(is_closed)
    {
        return false;
    }
    true
}

/// True when the schema's top-level object requires a property named `version` or ending in
/// `_version`.
fn has_version(schema: &serde_json::Value) -> bool {
    schema
        .get("required")
        .and_then(|r| r.as_array())
        .is_some_and(|required| {
            required
                .iter()
                .filter_map(|r| r.as_str())
                .any(|name| name == "version" || name.ends_with("_version"))
        })
}

/// Type names that derive `JsonSchema` in `source` but are not in `registered`. `source` is
/// Rust text; a derive attribute naming `JsonSchema` is followed, possibly after further
/// attributes and doc comments, by the `struct` or `enum` it decorates.
pub fn unregistered_types(source: &str, registered: &[&str]) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = false;
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("#[derive(") && trimmed.contains("JsonSchema") {
            pending = true;
            continue;
        }
        if !pending {
            continue;
        }
        if trimmed.starts_with("#[") || trimmed.starts_with("///") || trimmed.is_empty() {
            continue;
        }
        pending = false;
        let rest = trimmed
            .trim_start_matches("pub(crate) ")
            .trim_start_matches("pub ")
            .trim_start_matches("struct ")
            .trim_start_matches("enum ");
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() && !registered.contains(&name.as_str()) {
            found.push(name);
        }
    }
    found
}

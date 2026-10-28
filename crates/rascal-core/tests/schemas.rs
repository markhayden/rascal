//! Task 0.4: the copied schemas match their recorded hashes, the location schema accepts the
//! seven kinds and refuses the rest, and the drift harness fails on what it must.
// Helpers outside `#[test]` functions are still test code: failing loudly is what they are for.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use rascal_core::boundary_type;
use rascal_core::schema_drift::{self, BoundaryType, Finding, Source};
use rascal_core::schemas::{self, SchemaDir};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn schemas() -> SchemaDir {
    SchemaDir::new(repo_root().join("schemas"))
}

fn fixtures() -> PathBuf {
    repo_root().join("fixtures/schemas")
}

fn files(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no files under {}", dir.display());
    paths
}

// --- the copied schemas ---------------------------------------------------------------------

#[test]
fn copied_schemas_match_sources_toml() {
    schemas().verify_copied().unwrap();
    let sources = schemas().sources().unwrap();
    let paths: Vec<&str> = sources.file.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        vec![schemas::COPIED_MANIFEST, schemas::COPIED_ENVELOPE]
    );
    for file in &sources.file {
        assert_eq!(file.licence, "Apache-2.0", "{}", file.path);
        assert_eq!(file.sha256.len(), 64, "{}", file.path);
        assert!(file.origin.contains(&file.commit), "{}", file.path);
    }
}

#[test]
fn a_changed_copied_schema_is_caught() {
    let dir = tempdir();
    copy_dir(&schemas().root().join("copied"), &dir.join("copied"));
    std::fs::copy(
        schemas().root().join("SOURCES.toml"),
        dir.join("SOURCES.toml"),
    )
    .unwrap();
    let planted = dir.join(schemas::COPIED_ENVELOPE);
    let mut text = std::fs::read_to_string(&planted).unwrap();
    text.push('\n');
    std::fs::write(&planted, text).unwrap();
    let error = SchemaDir::new(&dir).verify_copied().unwrap_err();
    assert!(
        matches!(error, rascal_core::Error::CopiedSchemaChanged { .. }),
        "{error}"
    );
    assert!(error.to_string().contains("envelope-v1.json"), "{error}");
}

#[test]
fn copied_schemas_compile() {
    for name in [schemas::COPIED_MANIFEST, schemas::COPIED_ENVELOPE] {
        schemas().validator(name).unwrap();
    }
}

// --- the location schema --------------------------------------------------------------------

#[test]
fn the_seven_location_kinds_validate() {
    let validator = schemas().validator(schemas::LOCATION).unwrap();
    let valid = files(&fixtures().join("location-v1/valid"));
    let mut kinds: Vec<String> = Vec::new();
    for path in &valid {
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        if let Err(error) = validator.validate(&value) {
            panic!("{}: {error}", path.display());
        }
        let kind = value["kind"].as_str().unwrap().to_string();
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    kinds.sort();
    assert_eq!(
        kinds,
        [
            "absent",
            "bbox",
            "line",
            "polygon",
            "stroke",
            "view",
            "whole_image"
        ]
    );
}

#[test]
fn an_unknown_kind_and_malformed_locations_fail() {
    let validator = schemas().validator(schemas::LOCATION).unwrap();
    for path in files(&fixtures().join("location-v1/invalid")) {
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(
            validator.validate(&value).is_err(),
            "{} should not validate",
            path.display()
        );
    }
}

#[test]
fn a_reference_outside_the_schema_directory_is_refused() {
    let schema = serde_json::json!({ "$ref": "urn:rascal:schemas:copied:../LICENSE" });
    let error = schemas().validator_for(&schema).unwrap_err();
    assert!(error.contains("unknown schema reference"), "{error}");
    let schema = serde_json::json!({ "$ref": "https://example.invalid/schema.json" });
    assert!(schemas().validator_for(&schema).is_err());
}

// --- the drift harness ----------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Probe {
    probe_version: u32,
    width: u32,
    label: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ProbeWithoutVersion {
    width: u32,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ProbeOpen {
    probe_version: u32,
}

fn probe_entry() -> BoundaryType {
    boundary_type!(
        Probe,
        "rascal/probe-v1.json",
        Source::Generated,
        ["probe-v1/one.json", "probe-v1/two.json"]
    )
}

/// A scratch `schemas/` and `fixtures/schemas/` pair holding the probe's generated schema and
/// two fixtures, so the harness can be driven to each failure.
fn probe_world() -> (PathBuf, PathBuf) {
    let root = tempdir();
    let schemas_dir = root.join("schemas/rascal");
    let fixtures_dir = root.join("fixtures/probe-v1");
    std::fs::create_dir_all(&schemas_dir).unwrap();
    std::fs::create_dir_all(&fixtures_dir).unwrap();
    std::fs::write(
        schemas_dir.join("probe-v1.json"),
        schema_drift::render(&(probe_entry().generate)()),
    )
    .unwrap();
    std::fs::write(
        fixtures_dir.join("one.json"),
        r#"{"probe_version":1,"width":4,"label":"a"}"#,
    )
    .unwrap();
    std::fs::write(
        fixtures_dir.join("two.json"),
        r#"{"probe_version":1,"width":5,"label":null}"#,
    )
    .unwrap();
    (root.join("schemas"), root.join("fixtures"))
}

#[test]
fn a_registered_type_with_a_current_schema_passes() {
    let (schemas, fixtures) = probe_world();
    let findings = schema_drift::check(&probe_entry(), &SchemaDir::new(schemas), &fixtures);
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn a_planted_schema_change_is_drift() {
    let (schemas, fixtures) = probe_world();
    let path = schemas.join("rascal/probe-v1.json");
    let mut value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    value["properties"]["extra"] = serde_json::json!({ "type": "string" });
    std::fs::write(&path, schema_drift::render(&value)).unwrap();
    let findings = schema_drift::check(&probe_entry(), &SchemaDir::new(schemas), &fixtures);
    assert_eq!(
        findings,
        vec![Finding::Drift {
            schema: "rascal/probe-v1.json".to_string()
        }]
    );
}

#[test]
fn a_missing_schema_file_is_reported() {
    let (schemas, fixtures) = probe_world();
    std::fs::remove_file(schemas.join("rascal/probe-v1.json")).unwrap();
    let findings = schema_drift::check(&probe_entry(), &SchemaDir::new(schemas), &fixtures);
    assert_eq!(
        findings,
        vec![Finding::MissingSchema {
            schema: "rascal/probe-v1.json".to_string()
        }]
    );
}

#[test]
fn a_type_without_a_version_field_is_reported() {
    let entry = boundary_type!(
        ProbeWithoutVersion,
        "rascal/probe-v1.json",
        Source::Generated,
        []
    );
    let (schemas, fixtures) = probe_world();
    std::fs::write(
        schemas.join("rascal/probe-v1.json"),
        schema_drift::render(&(entry.generate)()),
    )
    .unwrap();
    let findings = schema_drift::check(&entry, &SchemaDir::new(schemas), &fixtures);
    assert_eq!(
        findings,
        vec![Finding::MissingVersion {
            schema: "rascal/probe-v1.json".to_string()
        }]
    );
}

#[test]
fn a_type_that_accepts_unknown_fields_is_reported() {
    let entry = boundary_type!(ProbeOpen, "rascal/probe-v1.json", Source::Generated, []);
    let (schemas, fixtures) = probe_world();
    std::fs::write(
        schemas.join("rascal/probe-v1.json"),
        schema_drift::render(&(entry.generate)()),
    )
    .unwrap();
    let findings = schema_drift::check(&entry, &SchemaDir::new(schemas), &fixtures);
    assert_eq!(
        findings,
        vec![Finding::NotClosed {
            schema: "rascal/probe-v1.json".to_string()
        }]
    );
}

#[test]
fn a_fixture_the_schema_rejects_is_reported() {
    let (schemas, fixtures) = probe_world();
    std::fs::write(
        fixtures.join("probe-v1/two.json"),
        r#"{"probe_version":1,"width":5,"colour":"red"}"#,
    )
    .unwrap();
    let findings = schema_drift::check(&probe_entry(), &SchemaDir::new(schemas), &fixtures);
    assert_eq!(findings.len(), 2, "{findings:?}");
    assert!(
        matches!(&findings[0], Finding::FixtureInvalid { fixture, .. } if fixture == "probe-v1/two.json")
    );
    assert!(
        matches!(&findings[1], Finding::FixtureRoundTrip { fixture, .. } if fixture == "probe-v1/two.json")
    );
}

#[test]
fn a_derive_without_a_registration_is_found() {
    let planted = r#"
/// A record.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Planted {
    pub planted_version: u32,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub(crate) enum PlantedKind { A, B }

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct Registered { x: u32 }

#[derive(Debug, Clone)]
pub struct NotABoundary;
"#;
    assert_eq!(
        schema_drift::unregistered_types(planted, &["Registered"]),
        vec!["Planted".to_string(), "PlantedKind".to_string()]
    );
}

#[test]
fn every_boundary_type_in_the_crates_is_registered_and_current() {
    let inventory = schema_drift::inventory();
    let names: Vec<&str> = inventory.iter().map(|entry| entry.name).collect();
    let mut sources = Vec::new();
    rust_sources(&repo_root().join("crates"), &mut sources);
    assert!(!sources.is_empty());
    for path in sources {
        let text = std::fs::read_to_string(&path).unwrap();
        let missing = schema_drift::unregistered_types(&text, &names);
        assert!(
            missing.is_empty(),
            "{}: {missing:?} derive JsonSchema but are not in schema_drift::inventory()",
            path.display()
        );
    }
    for entry in &inventory {
        if std::env::var_os("UPDATE_SCHEMAS").is_some() && entry.source == Source::Generated {
            std::fs::write(
                schemas().path(entry.schema),
                schema_drift::render(&(entry.generate)()),
            )
            .unwrap();
        }
        let findings = schema_drift::check(entry, &schemas(), &fixtures());
        assert!(
            findings.is_empty(),
            "{}: {findings:?} (UPDATE_SCHEMAS=1 regenerates a drifted schema)",
            entry.name
        );
    }
}

/// Every `.rs` file under `crates/*/src`, where boundary types live; test files are not
/// boundary types and may carry probes.
fn rust_sources(crates: &Path, out: &mut Vec<PathBuf>) {
    for krate in files(crates) {
        let src = krate.join("src");
        if src.is_dir() {
            walk(&src, out);
        }
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for path in files(dir) {
        if path.is_dir() {
            walk(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

// --- helpers ----------------------------------------------------------------------------------

/// A fresh directory per call: tests run in parallel, so the name carries a counter as well
/// as the process id and the clock.
fn tempdir() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "rascal-schemas-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for path in files(from) {
        std::fs::copy(&path, to.join(path.file_name().unwrap())).unwrap();
    }
}

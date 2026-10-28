//! Task 0.4: the copied schemas match their recorded hashes, the location schema accepts the
//! seven kinds and refuses the rest, and the drift harness fails on what it must.
// Helpers outside `#[test]` functions are still test code: failing loudly is what they are for.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use rascal_core::boundary_type;
use rascal_core::schema_drift::{self, BoundaryType, Finding, Source, Versioning};
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
    paths
}

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
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

/// A scratch copy of `schemas/copied` and `SOURCES.toml`.
fn copied_world() -> Scratch {
    let scratch = Scratch::new();
    copy_dir(&schemas().root().join("copied"), &scratch.join("copied"));
    std::fs::copy(
        schemas().root().join("SOURCES.toml"),
        scratch.join("SOURCES.toml"),
    )
    .unwrap();
    scratch
}

#[test]
fn a_changed_copied_schema_is_caught() {
    let dir = copied_world();
    let planted = dir.join(schemas::COPIED_ENVELOPE);
    let mut text = std::fs::read_to_string(&planted).unwrap();
    text.push('\n');
    std::fs::write(&planted, text).unwrap();
    let error = SchemaDir::new(dir.path()).verify_copied().unwrap_err();
    assert!(
        matches!(error, rascal_core::Error::CopiedSchemaChanged { .. }),
        "{error}"
    );
    assert!(error.to_string().contains("envelope-v1.json"), "{error}");
}

#[test]
fn a_copied_file_without_provenance_is_caught() {
    let dir = copied_world();
    std::fs::write(dir.join("copied/extra-v1.json"), "{}\n").unwrap();
    let error = SchemaDir::new(dir.path()).verify_copied().unwrap_err();
    assert!(
        matches!(error, rascal_core::Error::UnlistedCopiedSchema { .. }),
        "{error}"
    );
    assert!(error.to_string().contains("extra-v1.json"), "{error}");
}

#[test]
fn sources_toml_refuses_an_unknown_field() {
    let dir = copied_world();
    let path = dir.join("SOURCES.toml");
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str("\n[[file]]\npath = \"copied/x.json\"\norigin = \"o\"\ncommit = \"c\"\nversion = \"v\"\nlicence = \"l\"\nsha256 = \"s\"\nnotes = \"unknown\"\n");
    std::fs::write(&path, text).unwrap();
    let error = SchemaDir::new(dir.path()).sources().unwrap_err();
    assert!(matches!(error, rascal_core::Error::Toml { .. }), "{error}");
    assert!(error.to_string().contains("notes"), "{error}");
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
    assert!(!valid.is_empty());
    let mut kinds: Vec<String> = Vec::new();
    for path in &valid {
        let value = read_json(path);
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
    let invalid = files(&fixtures().join("location-v1/invalid"));
    assert!(invalid.len() >= 13, "{}", invalid.len());
    for path in invalid {
        let value = read_json(&path);
        assert!(
            validator.validate(&value).is_err(),
            "{} should not validate",
            path.display()
        );
    }
}

#[test]
fn the_location_schema_is_closed_and_versioned_by_its_id() {
    // No Rust type yet (M1 task 1.2); the committed file is checked as an authored schema with
    // a roundtrip that leaves JSON alone.
    let entry = BoundaryType {
        name: "Location",
        schema: schemas::LOCATION,
        source: Source::Authored,
        versioning: Versioning::SchemaId,
        generate: || serde_json::Value::Null,
        roundtrip: |value| Ok(value.clone()),
        fixtures: &[
            "location-v1/valid/bbox.json",
            "location-v1/valid/view.json",
            "location-v1/valid/line-path.json",
            "location-v1/valid/stroke.json",
        ],
    };
    let findings = schema_drift::check(&entry, &schemas(), &fixtures());
    assert!(findings.is_empty(), "{findings:?}");
}

// --- the schema directory's reference resolver -----------------------------------------------

#[test]
fn references_resolve_only_inside_the_schema_directory() {
    for bad in [
        "urn:rascal:schemas:copied:../LICENSE",
        "urn:rascal:schemas:cop/ied:envelope-v1",
        "urn:rascal:schemas:copied:envelope-v1.json",
        "urn:rascal:schemas::envelope-v1",
        "urn:rascal:schemas:copied:",
        "urn:rascal:schemas:copied",
        "urn:other:schemas:copied:envelope-v1",
    ] {
        let schema = serde_json::json!({ "$ref": bad });
        let error = schemas().validator_for(&schema).unwrap_err();
        assert!(error.contains("unknown schema reference"), "{bad}: {error}");
    }
    let schema = serde_json::json!({ "$ref": "https://example.invalid/schema.json" });
    assert!(schemas().validator_for(&schema).is_err());
    let schema = serde_json::json!({ "$ref": "urn:rascal:schemas:copied:nope" });
    let error = schemas().validator_for(&schema).unwrap_err();
    assert!(error.contains("nope.json"), "{error}");
    let schema =
        serde_json::json!({ "$ref": "urn:rascal:schemas:copied:envelope-v1#/$defs/LocationKind" });
    let validator = schemas().validator_for(&schema).unwrap();
    assert!(validator.is_valid(&serde_json::json!("bbox")));
    assert!(!validator.is_valid(&serde_json::json!("line")));
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
#[serde(deny_unknown_fields)]
struct ProbeBareVersion {
    version: u32,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ProbeOptionalVersion {
    probe_version: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ProbeOpen {
    probe_version: u32,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ProbeWithFloat {
    probe_version: u32,
    yaw: f64,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Inner {
    depth: u32,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ProbeNested {
    probe_version: u32,
    inner: Inner,
    by_name: std::collections::BTreeMap<String, u32>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", deny_unknown_fields)]
enum TaggedClosed {
    Line { location_version: u32, column: f64 },
    Stroke { location_version: u32, radius: f64 },
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
enum TaggedOpen {
    Line { location_version: u32 },
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
enum ExternalOpen {
    Line { location_version: u32 },
    Unit,
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
struct World {
    scratch: Scratch,
}

impl World {
    fn new() -> Self {
        let scratch = Scratch::new();
        std::fs::create_dir_all(scratch.join("schemas/rascal")).unwrap();
        std::fs::create_dir_all(scratch.join("fixtures/probe-v1")).unwrap();
        let world = World { scratch };
        world.commit(&probe_entry());
        world.fixture(
            "probe-v1/one.json",
            r#"{"probe_version":1,"width":4,"label":"a"}"#,
        );
        world.fixture(
            "probe-v1/two.json",
            r#"{"probe_version":1,"width":5,"label":null}"#,
        );
        world
    }
    fn schemas(&self) -> SchemaDir {
        SchemaDir::new(self.scratch.join("schemas"))
    }
    fn fixtures(&self) -> PathBuf {
        self.scratch.join("fixtures")
    }
    fn schema_path(&self) -> PathBuf {
        self.scratch.join("schemas/rascal/probe-v1.json")
    }
    /// Writes the entry's generated schema as the committed file.
    fn commit(&self, entry: &BoundaryType) {
        std::fs::write(
            self.schema_path(),
            schema_drift::render(&(entry.generate)()),
        )
        .unwrap();
    }
    fn fixture(&self, name: &str, text: &str) {
        std::fs::write(self.fixtures().join(name), text).unwrap();
    }
    fn check(&self, entry: &BoundaryType) -> Vec<Finding> {
        schema_drift::check(entry, &self.schemas(), &self.fixtures())
    }
}

fn drift() -> Finding {
    Finding::Drift {
        schema: "rascal/probe-v1.json".to_string(),
    }
}
fn not_closed() -> Finding {
    Finding::NotClosed {
        schema: "rascal/probe-v1.json".to_string(),
    }
}
fn missing_version() -> Finding {
    Finding::MissingVersion {
        schema: "rascal/probe-v1.json".to_string(),
    }
}

#[test]
fn a_registered_type_with_a_current_schema_passes() {
    let world = World::new();
    let findings = world.check(&probe_entry());
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn a_planted_schema_change_is_drift() {
    let world = World::new();
    let mut value = read_json(&world.schema_path());
    value["properties"]["extra"] = serde_json::json!({ "type": "string" });
    std::fs::write(world.schema_path(), schema_drift::render(&value)).unwrap();
    assert_eq!(world.check(&probe_entry()), vec![drift()]);
}

#[test]
fn formatting_is_not_drift_and_render_ends_with_one_newline() {
    let world = World::new();
    let rendered = schema_drift::render(&(probe_entry().generate)());
    assert!(rendered.ends_with('}') || rendered.ends_with("}\n"));
    assert!(rendered.ends_with('\n') && !rendered.ends_with("\n\n"));
    let compact = serde_json::to_string(&read_json(&world.schema_path())).unwrap();
    std::fs::write(world.schema_path(), compact.replace('\n', "\r\n")).unwrap();
    assert!(world.check(&probe_entry()).is_empty());
}

#[test]
fn a_missing_schema_file_is_reported() {
    let world = World::new();
    std::fs::remove_file(world.schema_path()).unwrap();
    assert_eq!(
        world.check(&probe_entry()),
        vec![Finding::MissingSchema {
            schema: "rascal/probe-v1.json".to_string()
        }]
    );
}

#[test]
fn an_unparseable_or_uncompilable_schema_is_unusable() {
    let world = World::new();
    std::fs::write(world.schema_path(), "{ not json").unwrap();
    let findings = world.check(&probe_entry());
    assert!(
        matches!(&findings[..], [Finding::Unusable { message, .. }] if message.contains("JSON")),
        "{findings:?}"
    );
    std::fs::write(world.schema_path(), r#"{"type": 5}"#).unwrap();
    let findings = world.check(&probe_entry());
    assert!(
        findings
            .iter()
            .any(|f| matches!(f, Finding::Unusable { .. })),
        "{findings:?}"
    );
}

#[test]
fn version_fields_by_shape() {
    let world = World::new();
    let cases: [(BoundaryType, Vec<Finding>); 6] = [
        (
            boundary_type!(
                ProbeWithoutVersion,
                "rascal/probe-v1.json",
                Source::Generated,
                []
            ),
            vec![missing_version()],
        ),
        (
            boundary_type!(
                ProbeBareVersion,
                "rascal/probe-v1.json",
                Source::Generated,
                []
            ),
            vec![],
        ),
        (
            boundary_type!(
                ProbeOptionalVersion,
                "rascal/probe-v1.json",
                Source::Generated,
                []
            ),
            vec![missing_version()],
        ),
        (
            boundary_type!(TaggedClosed, "rascal/probe-v1.json", Source::Generated, []),
            vec![],
        ),
        (
            boundary_type!(TaggedOpen, "rascal/probe-v1.json", Source::Generated, []),
            vec![not_closed()],
        ),
        (
            boundary_type!(ExternalOpen, "rascal/probe-v1.json", Source::Generated, []),
            vec![not_closed(), missing_version()],
        ),
    ];
    for (entry, expected) in cases {
        world.commit(&entry);
        assert_eq!(world.check(&entry), expected, "{}", entry.name);
    }
}

#[test]
fn closedness_by_shape() {
    let world = World::new();
    let open = boundary_type!(ProbeOpen, "rascal/probe-v1.json", Source::Generated, []);
    world.commit(&open);
    assert_eq!(world.check(&open), vec![not_closed()]);

    let nested = boundary_type!(ProbeNested, "rascal/probe-v1.json", Source::Generated, []);
    world.commit(&nested);
    assert!(
        world.check(&nested).is_empty(),
        "a map field is closed by construction"
    );
    let mut value = read_json(&world.schema_path());
    value["$defs"]["Inner"]
        .as_object_mut()
        .unwrap()
        .remove("additionalProperties");
    std::fs::write(world.schema_path(), schema_drift::render(&value)).unwrap();
    assert_eq!(world.check(&nested), vec![drift(), not_closed()]);
}

#[test]
fn an_authored_schema_is_not_compared_but_is_still_checked() {
    let world = World::new();
    let authored = BoundaryType {
        source: Source::Authored,
        ..probe_entry()
    };
    // A committed schema that differs from the generated one: no drift for an authored entry.
    std::fs::write(
        world.schema_path(),
        serde_json::to_string_pretty(&serde_json::json!({
            "type": "object", "additionalProperties": false,
            "required": ["probe_version", "width"],
            "properties": {
                "probe_version": {"type": "integer"},
                "width": {"type": "integer"},
                "label": {"type": ["string", "null"]}
            }
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(world.check(&authored).is_empty());
    assert_eq!(world.check(&probe_entry()), vec![drift()]);
    // Still closed and versioned.
    let mut value = read_json(&world.schema_path());
    value
        .as_object_mut()
        .unwrap()
        .remove("additionalProperties");
    value["required"] = serde_json::json!(["width"]);
    std::fs::write(world.schema_path(), schema_drift::render(&value)).unwrap();
    assert_eq!(
        world.check(&authored),
        vec![not_closed(), missing_version()]
    );
    // And its fixtures still validate.
    world.commit(&probe_entry());
    world.fixture(
        "probe-v1/two.json",
        r#"{"probe_version":1,"width":5,"colour":"red"}"#,
    );
    let findings = world.check(&authored);
    assert!(
        matches!(&findings[..], [Finding::FixtureInvalid { fixture, .. }, Finding::FixtureRoundTrip { .. }] if fixture == "probe-v1/two.json"),
        "{findings:?}"
    );
}

#[test]
fn versioning_by_schema_id() {
    let world = World::new();
    let by_id = BoundaryType {
        versioning: Versioning::SchemaId,
        ..boundary_type!(
            ProbeWithoutVersion,
            "rascal/probe-v1.json",
            Source::Authored,
            []
        )
    };
    let mut value = (by_id.generate)();
    std::fs::write(world.schema_path(), schema_drift::render(&value)).unwrap();
    assert_eq!(world.check(&by_id), vec![missing_version()], "no $id");
    value["$id"] = serde_json::json!("urn:rascal:schemas:rascal:probe-v1");
    std::fs::write(world.schema_path(), schema_drift::render(&value)).unwrap();
    assert!(world.check(&by_id).is_empty());
    value["$id"] = serde_json::json!("urn:rascal:schemas:rascal:probe-vx");
    std::fs::write(world.schema_path(), schema_drift::render(&value)).unwrap();
    assert_eq!(
        world.check(&by_id),
        vec![missing_version()],
        "non-numeric suffix"
    );
}

#[test]
fn fixtures_are_validated_and_round_tripped() {
    let world = World::new();
    world.fixture(
        "probe-v1/two.json",
        r#"{"probe_version":1,"width":5,"colour":"red"}"#,
    );
    let findings = world.check(&probe_entry());
    assert!(
        matches!(&findings[..], [Finding::FixtureInvalid { fixture, .. }, Finding::FixtureRoundTrip { .. }] if fixture == "probe-v1/two.json"),
        "{findings:?}"
    );
    // Valid but not in serialized form: the Option field is omitted.
    world.fixture("probe-v1/two.json", r#"{"probe_version":1,"width":5}"#);
    let findings = world.check(&probe_entry());
    assert!(
        matches!(&findings[..], [Finding::FixtureRoundTrip { fixture, .. }] if fixture == "probe-v1/two.json"),
        "{findings:?}"
    );
    // Missing and unparseable fixtures.
    std::fs::remove_file(world.fixtures().join("probe-v1/two.json")).unwrap();
    world.fixture("probe-v1/one.json", "{ not json");
    let findings = world.check(&probe_entry());
    assert!(
        matches!(
            &findings[..],
            [
                Finding::FixtureUnreadable { .. },
                Finding::FixtureUnreadable { .. }
            ]
        ),
        "{findings:?}"
    );
}

#[test]
fn an_integer_valued_float_round_trips() {
    let world = World::new();
    let entry = boundary_type!(
        ProbeWithFloat,
        "rascal/probe-v1.json",
        Source::Generated,
        ["probe-v1/one.json"]
    );
    world.commit(&entry);
    world.fixture("probe-v1/one.json", r#"{"probe_version":1,"yaw":90}"#);
    assert!(world.check(&entry).is_empty());
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

#[derive(
    Debug,
    Clone,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct Wrapped {
    pub wrapped_version: u32,
}

#[cfg_attr(feature = "schemas", derive(JsonSchema))]
pub(super) struct Conditional;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
/* a block comment between the derive and the item */
pub(in crate::x) struct AfterComment;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct Registered { x: u32 }

#[derive(Debug, Clone)]
pub struct NotABoundary;

// #[derive(JsonSchema)] in a line comment
/// `#[derive(JsonSchema)]` in a doc comment
pub struct Commented;

#[derive(Debug, MyJsonSchemaLike)]
pub struct LookAlike;
"#;
    assert_eq!(
        schema_drift::unregistered_types(planted, &["Registered"]),
        vec![
            "Planted".to_string(),
            "PlantedKind".to_string(),
            "Wrapped".to_string(),
            "Conditional".to_string(),
            "AfterComment".to_string(),
        ]
    );
}

#[test]
fn every_boundary_type_in_the_crates_is_registered_and_current() {
    let inventory = schema_drift::inventory();
    let mut names: Vec<&str> = inventory.iter().map(|entry| entry.name).collect();
    names.extend_from_slice(schema_drift::COMPONENTS);
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
    // Every component is a `$defs` entry of some committed schema: none is a boundary type in
    // hiding.
    let committed: Vec<serde_json::Value> = inventory
        .iter()
        .filter_map(|entry| schemas().read(entry.schema).ok())
        .collect();
    for component in schema_drift::COMPONENTS {
        assert!(
            committed
                .iter()
                .any(|schema| schema["$defs"].get(component).is_some()),
            "{component} is listed as a component but no committed schema defines it"
        );
    }
}

/// Every `.rs` file under `crates/*/src`, where boundary types live; test files are not
/// boundary types and may carry probes. An empty directory is tolerated.
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

/// A scratch directory removed when dropped. Tests run in parallel, so the name carries a
/// counter as well as the process id and the clock.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
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
        Scratch(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
    fn join(&self, rel: &str) -> PathBuf {
        self.0.join(rel)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for path in files(from) {
        std::fs::copy(&path, to.join(path.file_name().unwrap())).unwrap();
    }
}

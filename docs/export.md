# Compatibility export

What `rascal export --compat` projects, what it drops, and what it refuses by name. The export
itself arrives with M1 task 1.9; this page already documents the schemas it targets.

## The copied schemas

`schemas/copied/` holds two files copied **unchanged** from the evaluation harness in the same
family (ADR 0009): `manifest-v1.json`, the tool manifest, and `envelope-v1.json`, the result
envelope whose `$defs/Location` is the shared location shape. `schemas/SOURCES.toml` records,
for each file, its origin URL at the exact commit, the origin's version and licence
(Apache-2.0), and the file's SHA-256. A test hashes the files on every run and fails on any
difference, so updating a copied schema is a deliberate commit that changes `SOURCES.toml` too.
rascal has no code dependency on the origin.

## rascal's schemas

`schemas/rascal/` holds rascal's own versioned schemas. The first is `location-v1.json`: a
location is **one of** the copied `Location` definition (the kinds `bbox`, `polygon`, `view`,
`whole_image`, `absent`, referenced by `$ref` and never redefined) or rascal's `line` (a column
or a path with a full width in pixels) or `stroke` (a path with a radius in pixels). Inside a
schema, another schema file is referenced as `urn:rascal:schemas:<dir>:<name>`, which the
validator resolves to `schemas/<dir>/<name>.json` and to nothing else.

`fixtures/schemas/location-v1/valid/` holds one instance of each of the seven kinds and
`invalid/` the shapes the schema must refuse (an unknown kind, a line with both a column and a
path, a stroke without a radius, extra fields on any kind).

## The drift harness

Every type that crosses a boundary (written to disk, sent over MCP, read from a file a person
edits) is registered in `rascal_core::schema_drift::inventory()` with the `boundary_type!` macro,
which compiles only for a type deriving `Serialize`, `Deserialize` and `JsonSchema`. For each
registered type the test suite:

- compares the committed schema with the generated one as JSON (formatting is not drift;
  `UPDATE_SCHEMAS=1 cargo test -p rascal-core --test schemas` rewrites a drifted file) for a
  `Source::Generated` entry; a `Source::Authored` entry (one whose schema is written by hand
  because it references a copied schema, such as the location) is not compared;
- asserts the committed schema is closed everywhere it describes an object
  (`deny_unknown_fields`; a map with a schema-valued `additionalProperties` and no
  `properties` is closed by construction), for both sources;
- asserts the type is versioned, for both sources: by field (every shape the schema can take,
  through `oneOf`, `anyOf`, `allOf` and local `$ref`s, requires a `version` or `*_version`
  property) or, for a type whose shape is shared with a copied schema that has none, by schema
  id (`$id` ends in `-v<N>`; ADR 0009, refined 2026-10-28);
- validates every serialized fixture against the committed schema and round-trips it through
  the type. Fixtures are written in serialized form: an `Option` field that is `None` is
  present as `null`, and an integer-valued float may be written either way.

A grep over `crates/*/src` for the `JsonSchema` derive (across wrapped attribute lines and
`cfg_attr`) fails the suite on any type that is not registered.

## Where the schemas live

`schemas/` is at the repository root so people find it. When the registry (M1 task 1.4) needs
the schemas at run time it must embed them, and the packaging check refuses an `include_str!`
that reaches outside a crate; that task decides between moving the directory under
`crates/rascal-core/` with a root symlink and copying it in a build script.

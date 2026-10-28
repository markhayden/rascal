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

- regenerates the schema and compares it to the committed file (`UPDATE_SCHEMAS=1 cargo test
  -p rascal-core --test schemas` rewrites a drifted one);
- asserts the schema is closed everywhere it describes an object (`deny_unknown_fields`) and
  requires a `version` or `*_version` property;
- validates every serialized fixture against the committed schema and round-trips it through
  the type.

A type whose schema is written by hand because it references a copied schema (such as the
location) is registered as `Source::Authored`: its fixtures are validated and round-tripped,
and the generated schema is not compared. A grep over `crates/*/src` for the `JsonSchema`
derive fails the suite on any type that is not registered.

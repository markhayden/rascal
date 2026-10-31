# Contract fixtures

Files from [table-manners](https://github.com/markhayden/table-manners), the shared contract rascal
conforms to (ADR 0020), at the release pinned in the workspace `Cargo.toml`.

- `pixels/`: the pixel-digest test vectors (`vectors/pixels/` at v0.1.0), copied unchanged with
  their `digests.json`. `crates/rascal-core/tests/contract.rs` reproduces every `file_sha256` and
  `pixel_sha256` through rascal's own import.

- `locations/`: the contract's valid location examples (`examples/location/valid/` at v0.1.0),
  which `crates/rascal-core/tests/contract.rs` sends across rascal's boundary and back.
- `describe.json`: rascal's description document without a configuration, compared by
  `crates/rascal-mask/tests/describe.rs` (`UPDATE_FIXTURES=1` rewrites it).
- `results/`: rascal's conformance fixtures (table-manners 7), one `run.json` per outcome from
  the command line (`completed` for a heal and for a remove with a location used, `error` for a
  failing filler command, `invalid` for a finding on another image), with times, ids, the
  version and machine paths masked; `crates/rascal-mask/tests/run_json.rs` regenerates and
  compares them. These are the files the contract repository's `fixtures/rascal/` takes.

Moving the pin to a newer release copies the contract's files again in the same commit.

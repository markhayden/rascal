# Contract fixtures

Files from [table-manners](https://github.com/markhayden/table-manners), the shared contract rascal
conforms to (ADR 0020), at the release pinned in the workspace `Cargo.toml`.

- `pixels/`: the pixel-digest test vectors (`vectors/pixels/` at v0.1.0), copied unchanged with
  their `digests.json`. `crates/rascal-core/tests/it/contract.rs` reproduces every `file_sha256` and
  `pixel_sha256` through rascal's own import.

- `locations/`: the contract's valid location examples (`examples/location/valid/` at v0.1.0),
  which `crates/rascal-core/tests/it/contract.rs` sends across rascal's boundary and back.
- `describe.json`: rascal's description document without a configuration, compared by
  `crates/rascal-mask/tests/it/describe.rs` (`UPDATE_FIXTURES=1` rewrites it).
- `results/`: rascal's conformance fixtures (table-manners 7), one `run.json` per outcome from
  the command line (`completed` for a heal and for a remove with a location used, `error` for a
  failing filler command, `invalid` for a finding on another image), with times, ids, the
  version and machine paths masked; `crates/rascal-mask/tests/it/run_json.rs` regenerates and
  compares them.

`fixtures/table-manners/` holds what the contract repository's `fixtures/rascal/` takes: rascal's
fixtures at the pinned version (M11's set and v0.3's, task 12.9: the description with no
configuration and in each credential state, the estimates, the runs failed `stale_estimate`,
`cap`, `budget` and `credential`, the `not_ready` refusal and the retry under the id it released;
at v0.4, task 10.2, with `kind` on every action, `allowed_dirs` in the session and `invocation`
on every result)
from `rascal serve` and `rascal estimate` through the independent client, with machine values
replaced by valid stand-ins and each file held to the contract as the contract's own fixtures
test holds it (`crates/rascal-mask/tests/it/table_manners_fixtures.rs`; `docs/contract.md`,
"Conformance", lists every file).

Moving the pin to a newer release copies the contract's files again in the same commit.

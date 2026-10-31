# Contract fixtures

Files from [table-manners](https://github.com/markhayden/table-manners), the shared contract rascal
conforms to (ADR 0020), at the release pinned in the workspace `Cargo.toml`.

- `pixels/`: the pixel-digest test vectors (`vectors/pixels/` at v0.1.0), copied unchanged with
  their `digests.json`. `crates/rascal-core/tests/contract.rs` reproduces every `file_sha256` and
  `pixel_sha256` through rascal's own import.

Moving the pin to a newer release copies these again in the same commit.

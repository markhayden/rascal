# table-manners: rascal's side of the shared contract

rascal conforms to [table-manners](https://github.com/markhayden/table-manners) v0.1.0, the shared
contract of the owner's tools (ADR 0020, decision 13): what an agent, a script or the playground
sees from outside is the same for squint, rascal and the tools after them. The contract's text is
the specification; this page is what rascal does under it, task by task as milestone M8 lands
(PLAN.md).

## The pin

`table-manners = "=0.1.0"` in the workspace `Cargo.toml`: the crate's serde types and embedded
schemas, no logic. `scripts/check-contract.sh` warns in CI when crates.io has a newer release;
moving the pin is a deliberate commit that also refreshes `fixtures/contract/`.

## Image references (8.1)

Every import records the SHA-256 of the caller's file (`file_sha256` on the `import` record), so
`rascal_core::contract::image_reference` can give the contract's reference for any image in a
workspace: an imported image names the caller's file, an image rascal made names the PNG it
wrote. Projection and topology come from the image's frame; `pixel_sha256` is rascal's own pixel
digest, which is the contract's recipe byte for byte (the twelve vectors in
`fixtures/contract/pixels/` reproduce).

```
rascal workspace import ws/ photo.jpg     # img-1 raw:4032x3024:flat file_sha256:3f9a…
```

## Locations (8.2)

`--location` and `--stroke` take a table-manners location (recognised by its `image` field) as
well as rascal's own. Its image reference must name the file the run was given (the digest and
the dimensions); a location on another image exits 64. rascal holds one shape per location, so in
v0.1 it refuses, with the reason: a shape in several parts (split at the wrap edge), a polygon over
64 vertices (rascal's location schema is the copied envelope's, ADR 0009), and a `mask` in a
location slot. Outbound, `contract::location_out` turns rascal's locations into one contract
location: a split location leaves in parts with `straddles_wrap`, a `view` never leaves, and an
`absent` always carries a reason.

## The description (8.3)

```
rascal describe                          # one line per action
rascal describe --json [--config rascal.toml]
```

The contract's description document: one action per shipped recipe (`seam`, `remove`,
`remove_prompt`, `repair`, `repair_area`, `protect`, `rect`), with the image slots (role
`source`), the location slots and the kinds each takes, the text and mask inputs and the common
arguments (`filler`, `set`, `projection`, `strict`) as `params_schema`, the outputs (`mask`,
`result`; `remove_prompt` fills `locations.found`), the cost, the duration and the `dash` render.
With `--config`, the configured fillers are the `filler` enum, and any configured filler makes an
action `may_spend` with its key's variable named (never its value).
`fixtures/contract/describe.json` pins the document without a configuration. The `describe` MCP
tool returns the same document once `serve` conforms (8.5).

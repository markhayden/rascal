# 0016: Canonical serialization and the identity order

Date: 2026-10-28. Status: accepted.

## Context

An artifact's identity is the SHA-256 of its canonical payload (SPEC.md "Artifacts and
identity"). For two workspaces with different allocation histories to agree, and for a
committed test vector to stay true, the bytes that are hashed must be defined exactly: how a
record becomes text, how pixels become bytes, how one artifact refers to another, and in which
order the dependent identities are computed. Task 1.1 hashed length-prefixed byte parts in an
ad hoc order; this record replaces that.

## Decision

- **Records** are hashed as the canonical JSON of a payload object: keys sorted at every
  level, no whitespace, numbers in `serde_json`'s shortest round-trip form. The payload is
  prefixed by the record kind (`frame`, `plan`, `image`, `location`, `mask`) and a zero byte,
  so two kinds with the same fields cannot collide. Every reference to another artifact or
  frame in a payload is that thing's identity, never a local id, path, file name or timestamp.
- **Pixels** are hashed as the pixel digest: width, height, channel count and the samples in
  row-major order, prefixed by `pixels` and a zero byte. It depends on nothing else.
- **Order**: pixel digest, then frame (no image is involved: kind, size, projection, topology,
  and for a derived frame its parent's identity and mapping parameters), then image (pixel
  digest, frame identity, role, rendering role), then plan (parent frame identity, size, strip,
  spans), location (kind, frame identity, geometry), mask (the three planes' pixel digests,
  use case, frame identity, source identities). There is no cycle.
- **Stored PNGs** are 8-bit, not interlaced, with the `image` crate's default compression and
  adaptive filtering. "Byte-identical" images are images whose decoded samples agree; the
  encoder settings make files reproducible but are not part of any identity.
- **Vectors**: `fixtures/identity/vectors.json` holds the identities the core computes for
  known inputs (the floor fixture, a raw and a wrapped frame, a plan, a cut and a crop frame,
  a bbox, the rect mask, its grey rendering). A test reproduces them; changing them is a
  deliberate commit that amends this record.
- **Mappings** between frames are affine per region in `f64`; an unclipped mapping and its
  inverse agree to within floating-point rounding (the tests assert `1e-9`), which is what
  "round-trip exactly" means for normalised coordinates.

## Alternatives

- **Hash the on-disk files.** Encoder settings, TOML formatting and key order would then
  change identities without changing content.
- **Hash with local ids.** Two workspaces could never agree, and renaming would change
  identity.

## Consequences

- Any new boundary type that enters identity must say which of its fields are references and
  substitute identities for them before hashing.
- A change to canonical JSON or to the pixel digest is a format change: a new vectors file
  and a dated refinement here.

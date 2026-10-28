# 0009: Shared contracts by copied schema, exported on request

Date: 2026-10-28. Status: accepted.

## Context

An evaluation harness in the same family already defines a tool manifest, a result envelope
and a location shape. rascal's output should be readable by it without translation, but rascal
must be fully standalone, with no code dependency on it, and must be free to carry its own
bookkeeping (artifacts, identities, a mask zone) that the shared shape does not know.

## Decision

`schemas/copied/` holds the manifest and envelope schemas **unchanged**, with `SOURCES.toml`
recording origin, commit, version, licence and the SHA-256 of each file. The location
definition embedded in the copied envelope schema is referenced from rascal's own location
schema by `$ref`, which adds the `line` and `stroke` kinds. Native output follows
`schemas/rascal/`. `rascal export --compat` projects a native envelope onto the copied
envelope's fields, dropping native bookkeeping, and fails only on content the copied schema
cannot express, naming it. A drift harness regenerates every boundary type's schema in tests
and compares it to the committed file.

## Alternatives

- **Depend on the other crate.** Couples release cycles and breaks the standalone promise.
- **Invent incompatible shapes.** Forces an adapter on every consumer that uses both tools.

## Consequences

- Updating a copied schema is a deliberate commit that changes `SOURCES.toml` and reruns the
  harness.
- The export is a projection, not the native truth; the native envelope is what a run stores.

## Refined 2026-10-28

The M0 review asked how a location carries the version field every boundary type must have.
It cannot: the copied `Location` definition refuses unknown properties, so a `bbox` or `polygon`
instance has no room for one. The rule is therefore refined, not reversed: a boundary type is
versioned **by field** (a required `version` or `*_version` property on every shape) unless its
shape is shared with a copied schema that has none, in which case it is versioned **by schema
id** (the schema's `$id` ends in `-v<N>`, instances carry no version, and the record that
embeds the instance carries its own). The location is the one such type in v1. The drift
harness checks both forms and applies the closed and version checks to authored schemas too.

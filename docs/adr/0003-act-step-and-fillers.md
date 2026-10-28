# 0003: rascal owns the act step up to the composite; the filler is a plugin

Date: 2026-10-28. Status: accepted.

## Context

The quality of a heal is decided as much by what is done to the pixels before the model sees
them (cutting a wrapped image so its seam is in the middle, a tone ramp across a step) and by
how the result is put back (exactly inside the core, blended across the blend zone, gradient
matched when there is a tone step) as by the model itself. Those pre-passes and the composite
belong with the mask, because only the mask knows the zones. The painting does not: it is
whatever model the pipeline has.

## Decision

rascal owns everything up to and including the composite: the prep ops (`cut`, `put`, `ramp`,
`apply`, `window`, `unwindow`), per-consumer renderings, and `paste` with feather, hard and
Poisson modes. The painter is a **filler plugin**: a command or an HTTP endpoint declared in
`rascal.toml`, PNG in and PNG out, exact dimensions, the key from a named environment variable.
rascal bundles no weights for painting and has no vendor adapters.

## Alternatives

- **Stop at the mask.** Leaves the composite to every consumer, where the zone semantics are
  lost and each one reinvents the blend.
- **Vendor adapters for hosted models.** Convenient for a week and a maintenance burden for
  ever; a thin command or endpoint contract covers every model, hosted or local.

## Consequences

- A filler's output must match the request's dimensions exactly; anything else is an
  operational error, never a silent resize.
- In v1 a filler's `max_side` is met by an integer crop only; a mask whose support does not fit
  is `not_applicable`. Scaled windows are a later milestone.
- The composite never touches a pixel whose effective weight is zero, and a test proves it.

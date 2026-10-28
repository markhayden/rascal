# 0006: A mask is a record: zones, planes and prescriptions, with one effective weight

Date: 2026-10-28. Status: accepted.

## Context

A greyscale PNG cannot say which pixels must never change, which may be painted and which only
blend, nor what should be done to the pixels first, nor where it came from. Models and
consumers each want a different picture of the same intent, and a picture read back as a mask
loses the intent.

## Decision

A mask is a directory: `mask.toml` plus lossless planes beside it. **Core** and **protect** are
binary; **blend** holds the fractional weights outside the core as 8-bit values. The **effective
weight** of a pixel is one rule: protect gives 0, else core gives 1, else blend over 255.
Prescriptions (a tone ramp across a line, a transform for an inject) are data on the record, not
pixels. **Renderings** (feathered greyscale, a binary at a threshold, the tinted photo, an alpha
cut-out) are `Image` artifacts projected from the record for one consumer and are never
accepted where a `Mask` is expected. Every mask carries the frame it lives in and the provenance
of each step that made it.

## Alternatives

- **One greyscale plane.** Cannot express protect, cannot tell a hard core from a soft blend,
  and is lossy under re-encoding.
- **A vector description only.** Cannot carry a segmenter's output or a feathered band without
  rasterizing anyway.

## Consequences

- Every op that changes pixels inside a mask is a provenance node with resolved parameters and
  input and output identities.
- Ops that apply prescriptions never touch a pixel whose weight is 0, and a test proves it for
  each.
- Type confusion between a rendering and a mask is a compile-time error in the library and a
  validation error at every other surface.

# 0004: v1 use cases: seam, remove, repair, protect

Date: 2026-10-28. Status: accepted.

## Context

The use cases on the table were seam healing for wrapped images and any vertical line, object
removal with shadows and reflections, scratch and stain repair for scans and prints, the
inverse mask for a second pass, replacing screens and signs, retouching, extending, isolating a
subject, and wholesale grades and upscales. Each needs a different finder and a different mask
shape, and the first release has to prove the method on a few before it spreads.

## Decision

v1 ships **seam**, **remove**, **repair** (stroke and area) and **protect**, each as a shipped
recipe and an acceptance fixture. **Replace** (with a homography), **retouch**, **extend**,
**isolate**, the **path fan** and **scaled windows** are named later milestones. Whole-frame
work with no region (grades, upscales, denoise) is out for good: there is no mask in it.

## Alternatives

- **Seam only.** The deepest prior knowledge is there, but a tool with one recipe is a script.
- **Everything at once.** Spreads the lab thin; no use case would be proven by the first
  release.

## Consequences

- The mask record (ADR 0006) must already express what the later cases need (prescriptions
  such as a transform), so adding them is new ops and recipes, not a new record.
- Examples, fixtures and docs span varied subjects (scans, product shots, prints, interiors,
  landscapes, screens, signs); wrapped images are one supported case among many, not the
  project's identity.

# 0002: rascal finds and masks; it does not judge

Date: 2026-10-28. Status: accepted.

## Context

A pipeline has three kinds of work around an edit: deciding what is wrong (evaluation), deciding
which pixels may change and how the edge is handled (masking), and painting (a model). The
evaluator and the painter both exist. What was missing is the middle, and it was being done
with hand-made masks and one-off scripts.

## Decision

rascal owns finding and masking. It accepts a location from anywhere (a person, a file, an
evaluator's finding, an agent) and ships its own finders as tools that return locations: the
geometric ones (`line`, `rect`, `stroke`) in Rust, the learned ones (a segmenter, later OCR and
detectors) in the Python tool pack. It never decides whether an image is good, and it never
hides a model's output behind its own judgement. Measures are evidence, not verdicts (ADR 0013).

## Alternatives

- **Masking inside the evaluator.** Couples two concerns that evolve at different speeds and
  makes the mask unavailable to a pipeline that has no evaluator.
- **Masking inside the painter's wrapper.** Ties the mask's quality to one model and loses it
  when the model changes.

## Consequences

- Locations are a first-class contract with a schema, and a finder is just an op that returns
  one.
- The same tools serve a scripted pipeline, an agent over MCP and the owner's lab.

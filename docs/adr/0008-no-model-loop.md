# 0008: No model-driven loop inside rascal

Date: 2026-10-28. Status: accepted.

## Context

An agent could, in principle, live inside rascal: find, mask, fill, measure, retry. But the
pipelines rascal serves already have a controller (an agent, a harness, a person), and a second
controller inside the tool would fight the first, hide decisions, and make runs depend on a
model's mood.

## Decision

rascal contains no model-driven controller and no in-process inference in v1. Agents drive its
tools one by one; a recipe is a fixed, typed list. Models run only in explicitly configured
finder packs and filler plugins, out of process, and their outputs are recorded as data. The
door stays open: if a loop is ever needed it is an ADR, and nothing in the workspace model
forbids it.

## Alternatives

- **A built-in retry loop over measures.** Tempting for seam healing, but it turns evidence into
  a verdict (contrary to ADR 0013) and makes the tool non-deterministic.

## Consequences

- Every op in `rascal-ops` is a pure function of its inputs and resolved parameters; the same
  artifacts give the same identities.
- Diagnostics (the run page, tracing) show what happened, never decide what happens next.

# 0013: Measures are evidence; the lab decides under a frozen protocol

Date: 2026-10-28. Status: accepted.

## Context

Whether a heal looks right is a judgement. Numbers (continuity across the line, the tone step,
the transition width, texture match, drift) describe what changed, and they are useful for
spotting regressions and for an evaluator's finding, but a tool that turns them into pass or
fail would be judging with a ruler. Real photographs cannot live in a public repository.

## Decision

`measure` reports numbers and never a verdict. Synthetic fixtures with known answers live in
git; real photographs live in a private corpus outside the repository, referenced by SHA-256.
`rascal lab` runs a matrix of recipes over the corpus, records every cell's identity before
execution, and writes blind pairs pages; a person (or an evaluator) picks under a protocol
frozen before any result is inspected, and `lab decode` reveals the arms afterwards. Success for
v1 is the fan-and-ramp mask beating the fixed mask it replaces on the owner's corpus this way.

## Alternatives

- **Thresholds on the measures.** Fast, and wrong in exactly the cases that matter.
- **A learned judge.** Out of scope for a mask tool; an evaluator can use rascal's measures as
  input.

## Consequences

- The protocol, arms and picks are files the lab directory keeps; a lab is resumable and a
  decoded lab cannot be re-blinded.
- Measures feed the run page as charts beside the strips they measured.

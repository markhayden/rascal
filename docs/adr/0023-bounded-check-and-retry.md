# 0023: A bounded check-and-retry for remove

Date: 2026-11-04. Status: accepted. Amends ADR 0008.

## Context

ADR 0008 keeps any model-driven loop out of rascal: the caller (an agent, a host, a person)
drives the tools. The owner's hand test of `remove-prompt` (a dog's collar: the buckle, ring
and tongue left out of the mask, the fill showing them as a ghost) showed that a remove at scale
has to fix itself, without painting and without a large model, and the same way from the
command line, over MCP and in Wafflery. M13 track A fixes most of it with fixed rules in
`segment` and `plug`; what is left is noticing that the object is still visible after the fill.
Making every caller build that loop would put the same logic in every host.

## Decision

- `heal remove-prompt` may ask one configured **checker** about its result, and retry on a
  fixed ladder. This is the one exception to ADR 0008, Assumption 4 and Decision 7.
- The checker is any table-manners tool action that answers with a verdict (contract 0.5.0,
  2.4); squint's `source_fidelity` is the first. It is configured by hand in `[checker]`, like a
  filler, and called as `<command> --arguments <file> --allow-dir <dir>`; rascal holds no
  checker-specific code.
- The ladder is fixed: at most three tries; on `fail`, or `unsure` that names a place, `segment`
  runs again inside each place the checker named (padded, half the threshold) and its planes join
  the mask, a place where it finds nothing joins as itself, a `fail` that names no place widens
  the mask once; an `unsure` that names no place ends the tries (amended 2026-11-05: in the first
  hand check such an answer came from a tentative call about the subject, not the removed item,
  and the widened retry repeated the same fill at the same price); the mask is
  rebuilt from the planes through the same shaping and filled again. Only the verdict comes from
  a model, and it is recorded with each try, so a run explains every step it took and replays
  without a call.
- The check is on whenever a checker is configured and a filler is chosen, and a call can turn
  it off (`check: false`, `--no-check`). A missing checker key never blocks the remove: it runs
  once, unchecked, and says why. Tries are reserved up front within the spending limit.

## Alternatives

- **Outside rascal** (ADR 0008 as it stood): the caller chains remove, check and remove again.
  Rejected: every host would rebuild the same loop, and the command line would not have it.
- **A squint-only link**: rascal reads squint's own result. Rejected: it ties rascal to one
  checker; the contract's verdict lets the owner bring another.
- **An open-ended loop**: retry until a pass. Rejected: unbounded spend and time, and a model
  deciding when to stop.

## Consequences

- table-manners 0.5.0 adds the verdict, optional requirements and image out labels; rascal,
  squint and Wafflery move to it together (PLAN.md 13.15).
- `rascal.toml` gains `[checker]`; `remove_prompt` gains `check` and `answers: verdict` when one
  is configured, and the checker's key is an optional requirement, for checking the result.
- Determinism holds for everything but the verdict, which is recorded; `heal --replay` answers
  checks from the recording.

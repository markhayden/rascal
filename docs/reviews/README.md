# External reviews

Verbatim output of Codex CLI (`codex exec --sandbox read-only`, model `gpt-6-astra`), one file
per round. From round two on, each round's prompt asks for a resolution table over the previous
findings plus new findings, most severe first.

## Spec

| Round | Reviewed | Verdict | Answered by |
|---|---|---|---|
| 1 | SPEC.md draft 1 (2026-10-28) | not ready: 6 blockers, 14 important | draft 2 |
| 2 | draft 2 | not ready: 2 of 20 resolved, 18 partly; 4 new blockers, 14 important | draft 3 |
| 3 | draft 3 | not ready: 10 of 18 resolved, 8 partly; 2 new blockers, 8 important | draft 4 |
| 4 | draft 4 | not ready: 5 of 10 resolved, 5 partly; 1 new blocker, 3 important, 1 suggestion | draft 5 |
| 5 | draft 5 | not ready: 3 of 5 resolved, 2 partly; 0 new blockers, 2 important | draft 6 (for the owner's approval) |

## Plan

| Round | Reviewed | Verdict | Answered by |
|---|---|---|---|
| 1 | PLAN.md draft 1 (2026-10-28) | not ready: 3 blockers, 7 important, 1 suggestion; 16 coverage gaps | draft 2 |
| 2 | draft 2 | not ready: 7 of 11 and 11 of 16 gaps resolved; 1 new blocker (the spec additions had not landed), 5 important | draft 3 and spec draft 7 |
| 3 | draft 3 | **ready to begin M0**: 0 blockers, 6 important to fix in the owning tasks | draft 4 and spec draft 8 (all six made) |

## Checkpoints

| Milestone | File | Verdict |
|---|---|---|
| M0 | [m0-checkpoint.md](m0-checkpoint.md) | two reviews; every must-have fixed the same day; open only the CI run after the first push |
| M1 | [m1-checkpoint.md](m1-checkpoint.md) | two reviews; the must-fix and every should-fix landed in two commits; four nits recorded with owners |
| M2 | [m2-checkpoint.md](m2-checkpoint.md) | two reviews; no must-fix; six should-fix and the eight proposed tests landed in one commit; three nits recorded |
| M3 | [m3-checkpoint.md](m3-checkpoint.md) | two reviews; two must-fix (the transport's line limit, a byte-index panic) and twelve should-fix landed in one commit with the proposed tests; three items recorded |
| M4 | [m4-checkpoint.md](m4-checkpoint.md) | two reviews; one must-fix (closing eroded from the image border) and seven should-fix landed in one commit with the proposed tests; three items recorded |

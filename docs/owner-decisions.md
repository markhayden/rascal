# Owner decisions

Decisions only the owner can make, each dated when made and tied to the task that needs it.
The plan's "Owner decisions" table is the index; this file holds the answers.

| # | Decision | Needed before | Status |
|---|---|---|---|
| 1 | Confirmation of no conflicting obligation | the bootstrap commit | made 2026-10-28 |
| 2 | The forbidden-word list location (`RASCAL_FORBIDDEN_WORDS`) | 0.1's first commit | made 2026-10-28 |
| 3 | A real local filler and a real panorama on the owner's machine | 2.8's by-hand run | pending |
| 4 | The MCP transport | 3.1 (by ADR) | pending |
| 5 | The segmenter, its branch and the export's redistribution terms | 5.2 (decided in 4.5) | pending |
| 6 | The private corpus location, contents and groups | 6.1 | pending |
| 7 | The path fan before or after v1 | after M7 unless 4.3 shows the need | pending |

## 1. No conflicting obligation

2026-10-28. The owner confirms that no conflicting obligation applies: all of this work is the
owner's own, from the owner's own ideas and from extensions of earlier work the owner also owns
and built from scratch.

## 2. The forbidden-word list

2026-10-28. The list lives outside the tree at `~/rascal/forbidden-words.txt`, named by
`RASCAL_FORBIDDEN_WORDS` in the owner's shell. It is short and literal: a handful of names that
must never appear in the repo. The clean-room scan in `just check` reads it when set, matches
case-insensitively, and passes trivially when unset (as in CI).

The owner also asks for breadth of framing. Wrapped images and 360 panoramas are one supported
case among many, not the project's identity. Examples, fixtures and docs draw on varied subjects
(stitched scans, product shots, archival prints, interiors, landscapes, screens and signs) rather
than any single industry.

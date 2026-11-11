# M13 checkpoint

The tag `m13` is on `661bd68`, "Merge m13d-handcheck". That is the last M13 merge, and the commit
M14's restore drill rebuilt. It was tagged on 2026-11-11, after M14, as part of freezing rascal for
the merge into whizbang.

- **Tasks 13.0 to 13.15** are done and merged:
  - track A (the outline and plug) and track B (the lab round), tagged `m13.1` to `m13.4`;
  - track C (the checker loop, `2e688d7`), with table-manners 0.5.0 published from crates.io.
- **13.16, merge and the by-hand check.**
  - The four mains are merged and green.
  - The first by-hand pass, on the owner's own install, found five problems. They were fixed on
    `m13d-handcheck` and merged as `661bd68`:
    - results' notes were not written as sentences;
    - an unsure verdict that names no place now keeps the try and ends the tries;
    - a recipe now takes the image's described projection;
    - the projection refusal now says what to change, and why for a seam;
    - previews' labels.
  - The checker loop's by-hand pass ran on the command line. It is recorded in
    `m14-checkpoint.md`, "The owner's check", second attempt, Part B. There, rascal (built from
    `e02a273`, a superset of `661bd68`) called squint `source_fidelity`, got a verdict, and kept
    the try as specified.
  - The restore drill ran against `661bd68`; see `m14-checkpoint.md`, "The restore drill".
- **The by-hand check in Wafflery moved to whizbang milestone 5.**
  - Under Wafflery the checker cannot run when squint's config names an account. Wafflery
    passes a tool only the variables its description declares, and `[checker]` declares only
    `key_env` (`m14-checkpoint.md` R11).
  - The merge into one CLI removes that boundary, so the check is done there.

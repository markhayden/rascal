# The lab

The lab compares two ways of making a mask on the owner's private corpus, blind: every arm
heals every image through a filler, a pairs page shows two results side by side in a drawn
order, and a decode of the picks says whether one arm is `preferred`, `not_worse` or the
round was `inconclusive`, group by group, under a protocol frozen before the run. Three files
drive it, each a boundary type with a generated schema under `schemas/rascal/` and an example
under `examples/lab/`.

## The corpus manifest

`corpus.toml` lists images by id with the SHA-256 of the file, a group and a licence note. The
files live outside the repository, under `RASCAL_CORPUS` (default `~/rascal/corpus`) or the
directory `rascal corpus verify --root` names; no real photograph is ever committed, the
manifest is. Ids are unique, files are plain relative paths, digests are `sha256:<64 hex>`.

```
rascal corpus verify corpus.toml [--root <dir>]
```

prints one line per image (`ok`, `missing`, or `changed` with both digests) and a count, and
fails with exit 3 after the whole list when any image is missing or changed. The example's
files are the repository's fixtures: `rascal corpus verify examples/lab/corpus.toml --root
fixtures/images` passes.

The `[corpus]` section of `rascal.toml` names a manifest relative to the configuration file;
the `[workspace]`, `[fillers]` and `[packs]` sections are described in `docs/workspace.md`,
`docs/fillers.md` and `docs/tools.md`.

## The protocol

`protocol.toml` is committed before `lab run` is invoked; the run records the digest of its
bytes, and a change after a run is a new lab name. It names the lab, the seed `lab pairs`
draws from, the two arms compared (`arms = [a, b]`; wins are a's), `min_decided` (default 10)
and the rules as fractions of the decided pairs (`preferred = [2, 3]`: wins over decided pairs
at least two thirds; `not_worse = [1, 3]`: losses over decided pairs below one third), and the
groups, each a name and corpus ids; an image is in one group at most. `same` and `neither`
picks count toward neither side, and a group with fewer than `min_decided` decided pairs is
`inconclusive`.

## The matrix

`matrix.toml` names its corpus and protocol (paths relative to the file), repeats the seed, and
lists the images (corpus ids, with per-image `inputs` binding recipe inputs to files such as a
location, and per-image `overrides`; a `Text` input such as `remove-prompt`'s prompt has no
matrix binding yet, so such a recipe cannot be an arm) and the arms: a `recipe` (shipped by name or a file
relative to the matrix), `overrides` (`"<step>.<param>" = value`, with `"<step>.constants"`
selecting a constant set), the `filler` entry in the configuration the run is given, and
`spend_usd_per_call`, which the ledger sums. `fan-thin-v1` is the spec's fixed-width arm:
`fan-v1` with `wide_max 0.0`, so every row of the band is the minimum width at any image width.

A matrix is validated before anything runs: the protocol and the matrix share one name and
one seed; every arm the protocol compares is a matrix arm; every image is in the corpus and
every image the protocol names is in the matrix; every override names a step of its recipe and
a parameter of that step's op (or a constant set it has), and its value reaches the step; the
recipe validates against the catalogue. The **dependent-finder rule**: a finder (a pack tool)
whose input binds, through any chain of steps, to another finder's output is refused naming
the binding (`finder b (detect) takes plane = a.plane, which derives from finder a`), because
a cell's identity, finder invocation descriptors included, must be computable before any
finder runs; such a recipe still runs under `rascal run`, just not in the lab. Every refusal
names the thing that is wrong. `crates/rascal-mask/tests/lab_config.rs` has the cases.

## Cells and attempts: `lab run`

```
rascal lab run matrix.toml --config rascal.toml --out lab/<name>/ [--corpus-root <dir>] [--redo <arm>] [--no-dash]
```

A **cell** is one arm on one image. The run has two phases. First the matrix is validated
(every per-image input is one `heal` binds from a file, and every required input of every
arm's recipe is bound), the corpus images the matrix uses are verified (a missing or changed
file stops the run), and every cell's **identity** is computed and its record written, so
nothing that could be refused is found after a filler has been paid for: the SHA-256 over the
image's digest, every bound input file's digest, the overrides as applied, the recipe as it
runs, the constant set each step selects (the op's default when the step names none), the
version of every op involved, the filler entry (command or URL, profile, paste mode,
`max_side`, `send_mask`, `revision`; never its name in the configuration and never the key),
and, for every finder the arm would invoke, its invocation
descriptor (the pack name, the bundle digest when the pack was installed from a bundle, the
pack entry's `revision`, its declared `env` names, the tool, the resolved arguments and the
semantic identity of each input: an initial input's digest, or the binding of a deterministic
op over the initial inputs). Both `revision` fields are required for lab use. Changing only a
prompt, a finder parameter or a `revision` is a new cell; nothing else changing is the same
cell; a bound file's name is not in it, its bytes are. Then every cell runs. The lab
directory holds `lab.json` (name, protocol digest, matrix, the current cells by arm and
image), `ledger.jsonl` (one
line per external call: cell, attempt, arm, image, filler or pack, outcome, duration, the
arm's `spend_usd_per_call`), `workspace/` (the packs' journal), and `cells/<identity>/` with
`cell.json` (the record with every component), `attempts/<n>/` (the resolved `recipe.toml`,
the `run/` directory of the `heal` that ran it, with `result.png` and, unless `--no-dash`, the
run page, and `attempt.json`: `ok`, `stopped` or `failed`, with the reason) and `latest`, the
attempt in use.

A second `lab run --out` on the same directory resumes: a cell whose `latest` attempt is `ok`
is reused and makes no external call (the configured packs are still started for their
listings, which is not a call); one that stopped or failed runs again as a new attempt.
`--redo <arm>` adds an attempt to every cell of that arm and never overwrites one. The run
refuses a directory holding another lab, or this lab under a changed protocol (a protocol
change after a run is a new lab name), and any other non-empty directory. A failed cell does
not stop the others; the run ends with exit 3 and the attempt records say why. The example
lab (`examples/lab/`, the fake filler, `--corpus-root fixtures/images`) runs eight cells
offline in a few seconds; `crates/rascal-mask/tests/lab_run.rs` is the scenario.

## Pairs and decode

```
rascal lab pairs lab/<name>/ --arms a,b
rascal lab decode lab/<name>/ picks.json
```

`lab pairs` takes the two arms' current cells (those of the last `lab run`) for every image
the protocol's groups name, each with a complete `latest` attempt, and writes
`pairs/assignment.json` (the lab, the protocol digest, the arms, the seed, and per pair the
image id, the group, the cell and attempt on each side and which side arm `a` took) and
`pairs/index.html`, a static page with both results embedded as PNG data and nothing that
names an arm, a side's cell or the configuration; the command prints the page's size, which on
full panoramas runs to hundreds of megabytes (the spec asks for embedding and forbids a silent
resize). The page order and each pair's sides are
drawn from the protocol's seed (splitmix64; the order by Fisher–Yates, then one bit per pair),
so the same lab draws the same page. The judge picks `left`, `right`, `same` or `neither` per
pair and saves `picks.json` (version, the assignment file's digest, one pick per pair id);
the page offers the file and shows its text.

A whole panorama scaled to a browser window hides the very pixels a seam lives in, so each
pair has three views. The **focus** view, open by default, shows the region the two results
changed at full size, both sides in boxes that scroll together, opened at the middle of the
region; holding `f` swaps the sides in place, so one spot can be compared without moving the
eye. The region is the union over both sides of the columns and rows where the result differs
from the source the heal started from (`rascal_core::lab::focus`), with context of half its
extent on each axis, at least 64 and at most 512 pixels on each side, clamped at the edge of a
bounded frame and continuing through the edge of a wrap frame (the two edge strips of a
panorama come out side by side, the wrap edge in the middle). Both sides share the one region,
so a thin mask and a wide one are judged in the same window; a pair whose results do not
differ from the source anywhere shows the whole images. The **whole** view is the images
entire. The **look around** view, for an image whose run frame is equirectangular (the
recipe's declaration, or the 2:1 assumption), draws both results from inside the sphere in
WebGL2, aimed at the focus, the two sides sharing one orientation: drag to turn, wheel to
zoom, `aim at the focus` to return. An image wider than the GPU's largest texture is drawn
from a scaled copy and the page says so; detail belongs to the focus view. For this the
attempt record names the `source` image beside the `result`, and `lab pairs` refuses an
attempt without one.

`lab decode` reads the picks against `pairs/assignment.json` (which must be this lab's, under
this protocol) and the lab's `protocol.toml` (kept beside `lab.json` by the run; an edited one
is refused). Picks whose assignment digest
differs (stale or foreign), a pair with no pick, a pair picked twice, a pair not in the
assignment or a pick that is not one of the four are refused naming the pair. Per group it
prints pairs, decided (`left` or `right`), wins (arm `a`'s side), losses, `same`, `neither`,
and the outcome: `inconclusive` under `min_decided` decided pairs, else `preferred` when wins
over decided pairs reach the protocol's fraction (two thirds by default, inclusive),
`not_worse` when losses over decided pairs stay below the protocol's fraction (one third by
default), else `worse`. Under the default fractions `not_worse` without `preferred` cannot
occur (wins and losses sum to the decided pairs); a protocol whose fractions leave a gap can
reach it. Success criterion 6 reads: the sky group `preferred` and the structured group
`not_worse` (which `preferred` satisfies) for `fan-ramp` against `thin`. The results of a
round go in `docs/results/<lab>.md` with the protocol digest beside the outcome.

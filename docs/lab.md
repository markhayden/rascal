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
location, and per-image `overrides`) and the arms: a `recipe` (shipped by name or a file
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

A **cell** is one arm on one image. Before anything runs, the matrix is validated, the
corpus images the matrix uses are verified (a missing or changed file stops the run), and
every cell's **identity** is computed: the SHA-256 over the image's digest, every bound input
file's digest, the overrides as applied, the recipe as it runs, the constant set each step
selects, the version of every op involved, the filler entry (command or URL, profile, paste
mode, `revision`; never the key), and, for every finder the arm would invoke, its invocation
descriptor (the pack name, the bundle digest when the pack was installed from a bundle, the
pack entry's `revision`, its declared `env` names, the tool, the resolved arguments and the
semantic identity of each input: an initial input's digest, or the binding of a deterministic
op over the initial inputs). Both `revision` fields are required for lab use. Changing only a
prompt, a finder parameter or a `revision` is a new cell; nothing else changing is the same
cell. The lab directory holds `lab.json` (name, protocol digest, matrix), `ledger.jsonl` (one
line per external call: cell, attempt, arm, image, filler or pack, outcome, duration, the
arm's `spend_usd_per_call`), `workspace/` (the packs' journal), and `cells/<identity>/` with
`cell.json` (the record with every component), `attempts/<n>/` (the resolved `recipe.toml`,
the `run/` directory of the `heal` that ran it, with `result.png` and, unless `--no-dash`, the
run page, and `attempt.json`: `ok`, `stopped` or `failed`, with the reason) and `latest`, the
attempt in use.

A second `lab run --out` on the same directory resumes: a cell whose `latest` attempt is `ok`
is reused and makes no external call; one that stopped or failed runs again as a new attempt.
`--redo <arm>` adds an attempt to every cell of that arm and never overwrites one. The run
refuses a directory holding another lab, or this lab under a changed protocol (a protocol
change after a run is a new lab name), and any other non-empty directory. A failed cell does
not stop the others; the run ends with exit 3 and the attempt records say why. The example
lab (`examples/lab/`, the fake filler, `--corpus-root fixtures/images`) runs eight cells
offline in a few seconds; `crates/rascal-mask/tests/lab_run.rs` is the scenario.

## Pairs and decode

`lab pairs` and `lab decode` arrive with task 6.3; this page grows with them.

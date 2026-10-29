# Workspaces

A workspace is a directory that holds every artifact by identity, a registry of generations,
the operation records and the journal. Every op reads and writes a workspace; a run directory
is a workspace too.

```
images/                 canonical PNGs by identity, each with its record beside it
artifacts/<identity>    every other artifact file or directory, named by identity
ops/<identity>.json     immutable operation records
registry/<n>.json       one registry generation: frames, locations, artifacts, the id counter, the records
CURRENT                 the generation number, written by atomic rename
staging/<token>/        one directory per in-flight op, holding a `lock` its owner keeps locked
journal.jsonl           observations, appended after each commit
lock                    the writer lock, held only across a commit
```

## Ids and identities

Every artifact has an identity (`sha256:…`, ADR 0016) and a short local id for the command
line: `img-1`, `loc-2`, `mask-3`, `band-4`, `series-5`. Ids are allocated under the writer lock
from the generation's counter, so two concurrent ops never take the same one. Records and
identities never use ids; a command accepts either.

## The commit protocol

An op stages its outputs in `staging/<token>/`, then: syncs the staged tree; takes the writer
lock; reads `CURRENT`, allocates ids and the sequence number, writes its record into staging
and syncs it; renames the artifacts and the record into place (an identical identity already
there is left alone) and syncs those directories; writes the next generation and syncs it and
`registry/`; writes the pointer aside, syncs it, renames it over `CURRENT`, syncs the root;
releases the lock; appends to the journal. Visibility is the rename; `committed` is reported
only after the last sync. A crash before the pointer rename publishes nothing (orphans are
collected by `repair`); after it, the generation is complete and the journal is rebuilt from
the records. ADR 0017 states the primitive per platform and what is and is not promised.

Readers read `CURRENT` once and then the named generation, so they see a consistent snapshot.
Ops stage in parallel and commit in turn.

## Staging and locks

A staging directory's `lock` is held for the op's lifetime. On open, a staging directory whose
lock can be taken has no owner and is removed; one that cannot is active and left alone. A
generation that names an artifact missing on disk refuses the workspace until `rascal workspace
repair` has run (task 1.7b).

## Commands

```
rascal workspace import ws/ photo.jpg [--projection flat|equirectangular]   # prints the id and the frame
rascal workspace list ws/ [--type Mask] [--json]
rascal <op> --workspace ws/ --<port> <id> [--<param> <value>] [--constants <set>] [--strict]
rascal run <recipe> <image> --out run/ [--workspace ws/]
```

Every op in `rascal ops list` is a subcommand that takes `--workspace` and its ports by id. A
run without `--workspace` uses its run directory as a private workspace; with it, artifacts and
the journal go to the shared workspace and the run directory keeps the named outputs and the
`frames.json` and `locations.json` projections.

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
line: `img-1`, `loc-2`, `mask-3`, `band-4`, `series-5`, `plan-6`. Locations and plans are
registry entries; the rest are files under `images/` or `artifacts/`. Ids are allocated under the writer lock
from the generation's counter, so two concurrent ops never take the same one. Records and
identities never use ids; a command accepts either.

## Resolving by identity

Outside the workspace an artifact is named by its artifact reference (table-manners 3.4, task
10.3; `rascal_core::contract::artifact_ref`): its contract type, schema version 1, its identity
(an image's is `sha256:` and the digest of the file it was imported from), its frame's identity,
`on` (the root image it was made on, by file digest and the root frame) and its resource; never
an id. `Workspace::resolve(type, identity)` finds what the current generation holds under an
identity: a Mask, band profile, series, location or plan by its own, an image by the
`file_sha256` its `import` record carries (an image rascal made itself does not resolve). The
lookup reads an index built from the registry and the records once per generation, so a call
does not scan the workspace. Nothing in it depends on ids or on where the directory is: a
workspace reopened after a restart, or moved and reopened, resolves the same identities.
`rascal serve` resolves its callers' references in its own workspace, the session's store
(`docs/contract.md`); a Mask that store lacks goes in with `workspace import-mask` or the
`import` tool. Its artifact resource, `rascal://artifact/{identity}` (task 10.7), reads by
identity alone, trying each type in turn, so a host that kept references reads and binds them
again after `serve` restarts on the same workspace, or starts with `--workspace` on the directory
after it moved. rascal never searches folders for an artifact: a store that does not hold the
identity refuses the read.

## The commit protocol

An op stages its outputs in `staging/<token>/`, then: syncs the staged tree; takes the writer
lock; reads `CURRENT`, refuses a frame id the generation already binds to another identity
(nothing moved yet, so nothing is left behind), allocates ids and the sequence number, writes its record into staging
and syncs it; renames the artifacts and the record into place (an identical identity already
there is left alone) and syncs those directories; writes the next generation and syncs it and
`registry/`; writes the pointer aside, syncs it, renames it over `CURRENT`, syncs the root;
releases the lock; appends to the journal. Visibility is the rename; `committed` is reported
only after the last sync. A crash before the pointer rename publishes nothing (orphans are
collected by `repair`); after it, the generation is complete and the journal is rebuilt from
the records. ADR 0017 states the primitive per platform and what is and is not promised.

The record file under `ops/` is content-addressed, so an invocation identical to an earlier
one shares its file; the generation's `records` list carries each invocation's sequence number
and duration, the two observations that differ between them.

A journal append takes the journal file's own lock, assigns the sequence number from the lines
already in the file and writes the line in one call, so two processes sharing a workspace never
interleave lines or share a number. A line is complete only with its newline: an unterminated
tail is the torn line a crash mid-append leaves, skipped on read and rebuilt by `repair`.

Readers read `CURRENT` once and then the named generation, so they see a consistent snapshot.
Ops stage in parallel and commit in turn.

## Staging and locks

A staging directory's `lock` is held for the op's lifetime. On open, a staging directory whose
lock can be taken has no owner and is removed (never one this process owns, nor one younger
than a few seconds); one that cannot is active and left alone. A generation that names an
artifact missing on disk refuses the workspace until `rascal workspace repair` has run.

## Cancellation

A cancellation request is honoured once, before the writer lock is taken: nothing is published,
the owner removes its staging directory, and the journal notes it. Once the lock is held the
commit runs to completion and the op reports `committed` together with `cancelled`.

## Repair

`rascal workspace repair ws/ [--json]` puts right what a crash or a kill can leave behind, and
reports what it did (`schemas/rascal/repair-v1.json`). It holds the writer lock throughout, so
no commit is in flight while orphans are judged and the generation rewritten:

- abandoned staging directories are removed;
- an entry whose payload is missing on disk is dropped from a new, repaired generation (its
  record stays, so the history says what happened);
- files under `images/`, `artifacts/` and `ops/` that the current generation does not name are
  orphans (a commit that never reached its pointer rename) and are removed;
- the journal is rebuilt as a projection of the committed records in commit order, with a
  first line saying it was rebuilt; a rebuilt journal holds only committed invocations.

A run killed between the pointer rename and the journal append therefore repairs to a journal
whose identities equal those of an uninterrupted run of the same recipe; a run killed earlier
repairs to the previous generation with the failed commit's files gone.

## Portable masks

A Mask leaves a workspace as a **mask bundle** and enters another with the same identity (task
10.5, ADR 0022). The bundle is one uncompressed tar, readable with `tar tf`:

```
bundle.json          mask_bundle_version, the mask's identity, its frame chain from the root
                     raw frame to the mask's frame, every other member's path, size and SHA-256,
                     and the image the mask was made on when known
mask/blend.png       the files mask.toml names, sorted by name
mask/core.png
mask/mask.toml
mask/protect.png
...                  for a band: the profile and the sigma series; each prescription's series
```

Every header is fixed (mode 0644, uid and gid 0, mtime 0), so the same stored mask always gives
the same bytes, and exporting an imported mask gives the bundle it came from. `export-mask` prints
the bundle's SHA-256; `import-mask --sha256` checks it before anything else.

Export holds itself to the limits import reads within (below): a mask whose bundle would pass
one is refused with exit 64, naming the size and the limit, and nothing is written. The bundle is
built and checked in memory, then written beside `--out` and renamed over it, so `--out` is never
a partial bundle. The planes are 8-bit grey PNGs, so a mask up to about 9,400 px square can never
reach 256 MiB. At the largest image rascal takes (32,768 px a side) a plane is 1 GiB raw: smooth
zones (a box, a band, a feathered blend) make a bundle of a few to about 25 MiB, but planes of
per-pixel detail (a ragged instance union across the frame) do not compress and can pass the cap.

Import reads the two versions first, `mask_bundle_version` in `bundle.json` and the mask
record's `version` in `mask/mask.toml`, before any field it might not know. A bundle from a newer
rascal is refused with `made by a newer rascal (bundle version N); this rascal reads version 1:
upgrade rascal` (and the same for the mask record version); one from an older rascal whose format
this one has dropped, with `made by an older rascal (bundle version N) that this rascal no longer
reads`.

Import trusts nothing in the file. It reads at most 256 MiB (a larger file is refused from its
size), at most 16 members, a `bundle.json` of at most 1 MiB, member names `mask/<plain name>` of
at most 64 bytes, and checks each member's declared size against what remains before reading it.
Then: each member against the manifest's size and digest, both ways (a missing, an extra or a
renamed member is named); the archive against the canonical bytes rebuilt from its members (a
changed header, padding or anything after the end); `bundle.json` in its canonical form; each
frame of the chain rebuilt from its fields and its parent, ending at a raw frame, the mask's frame
its last; every mask invariant; and the identity recomputed from the pixels against the record
and the manifest. Any fault refuses the whole bundle with exit 64, naming it, and nothing is
published. What the digest cannot vouch for is a bundle rebuilt whole with fresh member digests;
`--sha256` from the sender closes that.

On success one `import_mask` record publishes the mask and the frames of its chain the workspace
does not know. A frame the workspace has (by identity) keeps the workspace's id, and a `crop:<n>`
another frame holds moves to the next free number; the record's `frame` follows, since ids are not
identity. A mask whose identity is already present publishes nothing and the command prints the
existing id with `(already present)`. A PNG, a JPEG, a directory, a gzip file or any other archive
is refused naming what it is: a rendering of a mask is an `Image`, and an image goes in with
`workspace import`.

Images are not bundled; a mask's frame is coordinates, not pixels. `bundle.json` names the image
the mask was made on, when the exporting workspace can tell (the nearest image in the root frame
among the mask's inputs, followed back through the records): `image.file_sha256`, the digest
`workspace import` prints, and its identity. It is advice: nothing in the bundle can vouch for it,
it gates nothing, and the import record keeps it so the copy exports the same bytes. An op that
needs the pixels of an imported mask whose image the workspace does not hold fails (64) naming it:

```
$ rascal window --workspace ws2/ --image img-1 --mask mask-1
rascal window: img-1: no such artifact in ws2/; mask-1 was made on an image ws2/ does not hold, the
file with sha256 40ec72…7a50 (320x200, flat): import it with `rascal workspace import ws2/ <path>`
and give the id it prints; mask-1 needs no new import
```

(with `--projection` added when the image is 2:1 and flat, or equirectangular and not 2:1). Import
that file; it lands in the mask's raw frame (a raw frame's identity is its size and projection),
and the same mask id then works: `rascal window --workspace ws2/ --image img-2 --mask mask-1`.

A **location** travels as JSON: `rascal workspace import-location ws/ finding.json --image img-1`
takes rascal's own location (which may leave out `frame` or must name the image's) or a
table-manners location (its `image` reference must name the same file, ADR 0020), the two shapes
`run --location` takes and through the same function. It is validated against the image's frame
(the kind's rules, the seam rule) and registered like any location; the same file twice gives the
same id. The file is read within 1 MiB; one that is not JSON, or not a location, is refused (64).

**Through the contract** (CONTRACT 5.1, PLAN.md 10.10), `serve`'s reserved `export` and `import`
tools, and `rascal export --json` and `rascal import --json`, which read the same request and
print the same document, move the same two forms by artifact reference instead of by id. Export
writes a Mask's bundle (the bytes `export-mask` gives) or a location's contract JSON into rascal's
export area, `$XDG_DATA_HOME/rascal/exports/<identity with : as ->.mask.tar` or
`.location.json`, never at a caller's path, and answers with the file's path, SHA-256, size and
media type. Import takes a file by path (`serve`'s, or `rascal import`'s, `--allow-dir` rule) or
its bytes inline, tells a bundle from a location by its content, checks it whole (a location
against the image the store holds under its `image.file_sha256`) and answers with the reference
and `already_present`. Wrong requests are refusals: on the command line, the refusal JSON on
stdout and exit 64 (`docs/contract.md` "Export and import").

## Commands

```
rascal workspace import ws/ photo.jpg [--projection flat|equirectangular]   # prints the id, the frame and file_sha256:<the file's SHA-256>
rascal workspace list ws/ [--type Mask] [--json]
rascal workspace repair ws/ [--json]
rascal workspace export-mask ws/ mask-3 --out seam.mask.tar    # prints the identity and sha256:<the bundle's digest>
rascal workspace import-mask ws2/ seam.mask.tar [--sha256 <digest>]   # prints the id, the frame and the identity
rascal workspace import-location ws/ finding.json --image img-1  # prints the id, the frame and the identity
rascal export --json <request.json | -> [--workspace ws/]          # the contract's export of a Mask or a location (below)
rascal import --json <request.json | -> [--workspace ws/] [--allow-dir <abs>]...   # and its import
rascal <op> --workspace ws/ --<port> <id> [--<param> <value>] [--constants <set>] [--strict]
rascal run <recipe> <image> --out run/ [--workspace ws/]
```

Every op in `rascal ops list` is a subcommand that takes `--workspace` and its ports by id. A
run without `--workspace` uses its run directory as a private workspace; with it, artifacts and
the journal go to the shared workspace and the run directory keeps the named outputs and the
`frames.json` and `locations.json` projections.

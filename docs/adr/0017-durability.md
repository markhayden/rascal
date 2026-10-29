# 0017: Durability: the sync primitive per platform and what rascal promises

Date: 2026-10-28. Status: accepted.

## Context

A workspace publishes a generation by renaming a pointer over `CURRENT`. The spec (SPEC.md
"Workspace") requires that everything `CURRENT` names is durable before `CURRENT` is, so a
crash publishes nothing or a complete generation, never a pointer to a missing payload. That
needs a sync primitive for files and for directory entries on each supported platform, and a
statement of what the sequence guarantees.

## Decision

- The primitive is `std::fs::File::sync_all` on the file, and on an opened directory for its
  entries. On Linux it is `fsync(2)` for both. On macOS the standard library issues
  `fcntl(F_FULLFSYNC)`, which asks the drive to flush its own cache as well; plain `fsync` on
  macOS does not. No system library is linked for this.
- The order, in `rascal_core::workspace::commit`: every staged file and every directory of the
  staged trees; the record with its sequence number; the renames into `artifacts/`, `images/`
  and `ops/`, then those directories; the new generation file, then `registry/`; the pointer
  written aside and synced, renamed over `CURRENT`, then the workspace directory. An op
  reports `committed` only after the last sync.
- Supported: a power loss or kill at any point leaves either the previous `CURRENT` with every
  payload it names intact, or the new one likewise. Files under `artifacts/`, `images/` and
  `ops/` that no generation names are orphans, harmless and collected by `repair`. The journal
  is appended after the commit and is never the only copy of anything: `repair` rebuilds it from
  the records.
- Not promised: durability across a filesystem that lies about `fsync` (some network and
  virtual filesystems), or across a drive whose cache ignores a flush. Atomic rename within one
  filesystem is assumed, so a workspace is one directory tree on one filesystem.
- The testkit's `RecordingFs` records the order of writes, syncs and renames, and can crash at
  any point, dropping what was written or renamed but never synced and truncating a crashed
  append to its length before it; the tests assert the order above and the all-or-nothing
  outcome at each boundary, for a location-only commit and for a mask commit whose staged tree
  is a directory. A platform test performs a real sync on every CI runner.
- Amended at the M1 checkpoint (2026-10-29): `repair` holds the writer lock for its whole run,
  since judging orphans against a generation while a commit renames files in could remove a
  payload the next generation names; and a journal append is one write under the journal
  file's own lock, with the sequence number taken from the file, since two processes appending
  to one journal could otherwise interleave a line or share a number. Both have tests.

## Alternatives

- **`fdatasync` or no directory sync.** Faster, and exactly the gap that loses a rename.
- **A write-ahead log.** More machinery than a pointer rename for a registry this small.

## Consequences

- Commits are a few syncs each; concurrent ops stage in parallel and queue only across the
  short locked section.
- A new platform needs its sync behaviour checked and this record amended.

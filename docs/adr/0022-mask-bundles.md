# 0022: A Mask travels as a canonical tar bundle, checked whole before it is imported

Date: 2026-11-01. Status: accepted (2026-11-01).

## Context

A mask is a directory in one workspace: `mask.toml`, three planes, and for a band the profile
and series it references, all named by its identity (SPEC.md "The mask record"). Milestone M10
lets a client test one op at a time, which needs a saved Mask to reach another workspace, on
another machine, and come back as the same artifact: same identity, its frame chain registered,
nothing taken on trust. Until now the only ways in were an image import and `--mask <dir>` on
`run`, which reads a directory without committing it and needs the workspace that made it to
know the mask's frame. A PNG of a mask is a rendering, an `Image`; taking one as a Mask would
lose the zones, the profile and the prescriptions and invent an identity (ADR 0006).

## Decision

- **One file, a ustar archive, uncompressed.** Members in a fixed order: `bundle.json`, then
  `mask/mask.toml` and every file it names, sorted by name. Every header is fixed (regular
  file, mode 0644, uid and gid 0, mtime 0, no user or group name), the archive ends with the two
  zero blocks and nothing after. The same stored mask therefore gives the same bytes, and the
  bundle's SHA-256 names it the way `file_sha256` names an imported image (ADR 0020). The planes
  are PNGs already, so compression would buy little and cost a second decoder on hostile input.
- **`bundle.json` is the manifest**: `mask_bundle_version`, the artifact type, the mask's
  identity, the frame chain from the root raw frame to the mask's frame (each frame record as
  `frames.json` holds it), each other member's path, size and SHA-256, and, when the exporting
  workspace can tell, the image the mask was made on (its `file_sha256` and identity). It is a boundary type
  (`schemas/rascal/mask-bundle-v1.json`).
- **Import checks everything, then commits once.** Bounded reads (256 MiB a file, 16 members, a
  1 MiB manifest, member names at most 64 bytes and plain, a member's declared size checked
  against what remains before it is read); the manifest against the members, both ways; the
  archive against the canonical bytes rebuilt from the members, so no byte outside them can
  differ; each frame's identity recomputed from its fields and its parent's identity, the chain
  ending at a raw frame and the record's frame its last; the mask's invariants; the identity
  recomputed from the pixels against the record and the manifest. Any fault refuses the bundle
  whole, naming the fault, and publishes nothing.
- **Export holds itself to the reader's limits.** The writer checks every bound import enforces
  (the size, the members, the manifest, the names, the frame chain) and refuses a bundle outside
  them, naming its size and the limit, before a byte is written; an export never makes a file
  import refuses. The CLI writes beside `--out` and renames over it, so `--out` is never partial.
  How close a mask comes: the planes are 8-bit grey PNGs, a byte a pixel before compression, so
  no mask up to about 9,400 px square can reach 256 MiB (three incompressible planes of that size
  are 253 MiB). At the 32,768 px side rascal accepts (`image::MAX_SIDE`) a plane is 1 GiB raw;
  smooth zones (a box, a band, a feathered blend) deflate to roughly 1 to 8 MiB a plane, a bundle
  of a few to about 25 MiB, while a plane of per-pixel detail (a ragged instance union, noise)
  stays near its raw size. The cap is hit only by a large mask whose planes average more than
  about 0.7 bits a pixel after compression; a band's profile and series (a value a row) add at
  most a few MiB.
- **Ids are not identity.** A frame the target already has, by identity, keeps the target's id;
  a `crop:<n>` id taken there by another frame takes the next free number; the record's `frame`
  follows. The commit refuses to bind a frame id to a second identity, which also closes two
  concurrent `window`s taking the same `crop:<n>`.
- **Idempotent.** A mask whose identity the workspace holds publishes nothing; the import
  answers with the existing id.
- **Never a PNG.** `import-mask` names what a file is when it is not a bundle (a PNG, a JPEG, a
  directory, another archive) and refuses it.
- **`tar` joins the core's dependencies.** The crate is already in the lock for `tools setup`'s
  bundle archive; the core reads and writes archives in memory with it and never unpacks to disk.

## Alternatives

- **A directory copy.** Nothing to hash as one thing, no single file to hand over, and every
  tool that copies directories decides differently about metadata and extra files.
- **zip.** A new dependency, a central directory that can disagree with the local headers, and
  compression by default.
- **A JSON document with the planes in base64.** One parser instead of two, but a third larger
  and every plane decoded twice; `tar tf` is a better window for a person.
- **Rewrite member bytes canonically on import and accept any archive.** The bundle's digest
  would then not name the file, and a byte that changed nothing visible would pass unremarked.

## Consequences

- A bundle made by one rascal version imports into another while the mask record version and
  the bundle version agree; a change to either is a new version, refused by older readers.
  Both versions are read before anything else, so the refusal says what to do: a newer bundle
  "made by a newer rascal (bundle version N); this rascal reads version 1: upgrade rascal", an
  older one "made by an older rascal (bundle version N) that this rascal no longer reads".
- The MCP tools for export and import wait for the operations profile (table-manners v0.4's
  reserved `export` and `import`, PLAN.md 10.10); the bundle does not presume its artifact
  reference.
- Images are not bundled: a mask's frame is coordinates, not pixels, so the target needs the
  image only to apply the mask, and it imports the image by its own path. The manifest names that
  image (the nearest image in the root frame among the mask's inputs, through the records; none
  when there are several), and an op given an imported mask whose image the workspace lacks
  fails naming it by file digest with the exact `rascal workspace import <ws> <path>` remedy;
  once imported the image lands in the mask's raw frame and the same mask id applies. The image
  is advice: nothing in the bundle can vouch for it and it gates nothing, so a flipped byte inside
  it is the one change import cannot see; `--sha256` covers it with the rest.
- Accepted 2026-11-01 after review, with three fixes made before merge: export holds itself to
  every limit import reads within (above), so no rascal writes a bundle a rascal refuses; a version
  mismatch tells the person what to do (upgrade, or the format is gone); and a missing image is
  named with its remedy (above).

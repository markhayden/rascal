# Fillers

A filler is the plugin that paints: a command or an HTTP endpoint named in `rascal.toml`. rascal
bundles no weights. It sends the filler an image, and for a mask-input profile a mask, of
exactly the dimensions it chose, and takes back a PNG of those dimensions; anything else, a
non-zero exit, a timeout or a non-PNG body, is an operational error. There is no resize of a
filler's answer and no scaled request in v1 (scaled windows are a later milestone).

## Declaring one

```toml
[fillers.local-inpaint]
command = ["python", "fill.py", "{image}", "{mask}", "{out}"]   # placeholders substituted; exit 0 and a PNG at {out}
profile = "grey"                                                # grey | binary (mask input), alpha | tinted (image input)
paste = "feather"                                               # feather (default) | hard | poisson (M7)
timeout_s = 120                                                 # default 120
revision = "fill.py 2026-10-28"                                 # the lab hashes it into a cell's identity

[fillers.hosted]
url = "https://example.invalid/inpaint"   # multipart POST: image, mask (when the profile has one), prompt (when given)
profile = "binary"
paste = "hard"
key_env = "HOSTED_FILLER_KEY"             # Authorization: Bearer; read at call time, never stored, logged or journaled
timeout_s = 180
max_side = 2048                           # the heal graph windows to it
```

Exactly one of `command` and `url`; `key_env` goes with `url`; `send_mask = true` on an
image-input profile also sends the `grey` rendering as `mask`. `examples/rascal.toml` has both
entries; the schema is `schemas/rascal/config-v1.json`.

## What a filler receives (G9)

| profile role | `image` field | `mask` field |
|---|---|---|
| mask input (`grey`, `binary`) | the working image | the rendering |
| image input (`alpha`, `tinted`) | the rendering | nothing, or the `grey` rendering with `send_mask = true` |

A command sees them as files at `{image}` and `{mask}` and writes `{out}`; `{prompt}` is the
prompt text or empty. An endpoint receives them as multipart parts `image` and `mask` (PNG) and
`prompt` (text). A command naming `{mask}` for a profile that sends none is a usage error before
anything runs.

## The call in the journal

Every call is a journal note, `filler call: {…}`, with the entry's name, the command as
configured (placeholders shown) or the URL, the request's dimensions and bytes, the answer's
bytes, the duration, the exit code or HTTP status, and the outcome. The key is handed to the
`Authorization` header and appears in no file, message or span; a wrong key is reported as the
server's status, and an unset `key_env` as a usage error naming the variable.

## `heal`: the typed graph

```
rascal heal seam pano.jpg --filler local-inpaint --config rascal.toml --out run/
rascal heal seam pano.jpg --filler local-inpaint --config rascal.toml --out run2/ --replay run/ --no-dash
```

`heal` runs the recipe and then the fixed graph of SPEC.md "Recipes" over its declared outputs
(`mask`, `image`, and `plan` when the mask's frame is a `cut:`), every step an op invocation in
the journal: `apply` (the prescriptions; a mask without one leaves the image as is), `window`
when the filler has `max_side` (a support larger than it is not applicable), `render` with the
filler's profile (and `grey` when `send_mask`), the filler call (a journal note, never the
key), `paste` with the filler's mode, `unwindow` when windowed, `put` back onto the panorama
when the frame was a cut, the preservation check, and `measure` in the mask's frame with the
cut canvas as `before` and the composite as `after`. It writes `result.png`, the recipe's
renders, `frames.json` and `locations.json`, and the run page (`--no-dash` skips it).

The preservation check is in raw coordinates: every raw pixel the mask never reaches
(`w = 0`, mapped through the crop bbox and the cut plan) and every protected pixel must equal
this invocation's input image, or the heal is an operational error (exit 3). A filler's
failure, timeout, mis-sized or non-PNG answer is the same.

The fill is stored as an artifact with a `fill stored:` note carrying the request's identity
(a hash of the bytes sent), so `--replay <previous run>` reuses it when the request is the same
and makes no call: a healed run replays with the filler absent (G4).

## The fake fillers

The testkit's fake filler is one implementation with six behaviours, reachable as a command
(`rascal fake-filler <behaviour> <image> [<mask>] <out>` in a `--features testkit` build, or the
`fake-filler` binary of the testkit crate) and as a local HTTP server (`fake-filler serve
<behaviour>`), so the real command and HTTP fillers are exercised without a model:

| behaviour | outcome |
|---|---|
| `exact` | a deterministic box blur of the image where the mask is set; the request's size |
| `mis-sized` | an answer one pixel wider than the request: refused, naming both sizes |
| `re-encoding` | every channel of every pixel changed by one level: accepted; a paste still leaves `w = 0` pixels as the base |
| `slow` | three seconds of silence: the timeout, and a command is killed |
| `failing` | exit 2 (HTTP 500) with a message: an operational error carrying it |
| `non-png` | text where a PNG was expected: refused |

`crates/rascal-mask/tests/fillers.rs` drives the port with each behaviour both ways.

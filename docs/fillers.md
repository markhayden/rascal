# Fillers

A filler is the plugin that paints: a command or an HTTP endpoint named in `rascal.toml`. rascal
bundles no weights. It sends the filler an image, and for a mask-input profile a mask, of
exactly the dimensions it chose, and takes back a PNG of those dimensions; anything else, a
non-zero exit, a timeout or a non-PNG body, is an operational error. The one resize is the
scaled window a filler asks for with `fill_max_side` (below): the request goes down to fit and
the fill comes back up to the support, both said in the journal.

## Declaring one

```toml
[fillers.local-inpaint]
command = ["python", "fill.py", "{image}", "{mask}", "{out}"]   # placeholders substituted; exit 0 and a PNG at {out}
profile = "grey"                                                # grey | binary (mask input), alpha | tinted (image input)
paste = "feather"                                               # feather (default) | hard | poisson
timeout_s = 120                                                 # default 120
revision = "fill.py 2026-10-28"                                 # the lab hashes it into a cell's identity
overrides = { "fan.constants" = "fan-thin-v1", "fan.wide_max" = 0.05 }   # the band this filler can carry (heal only)
fill_max_side = 1024                                            # send the request scaled to this longest side; the fill is scaled back
spend_usd_per_call = 0.04                                       # the price of one call: run.json's spend, serve's limits (unset: free)

[fillers.hosted]
url = "https://example.invalid/inpaint"   # multipart POST: image, mask (when the profile has one), prompt (when given)
profile = "binary"
paste = "hard"
key_env = "HOSTED_FILLER_KEY"             # the key's variable (a command filler may name one too); read at call time, never stored, logged or journaled
timeout_s = 180
max_side = 2048                           # the heal graph windows to it
```

Three sections beside the fillers (ADR 0020, `docs/contract.md`):

```toml
[cache]
dir = ".rascal/fills"          # keep every fill by its request, filler and revision; an identical
                               # request is answered from here and the filler is not called

[limits]                       # rascal serve only
call_cap_usd = 0.50            # the most one call may spend; a call may lower it (max_spend_usd), never raise it
session_budget_usd = 5.00      # the most one serve process may spend

[serve]                        # rascal serve and rascal call
allow_dirs = ["/srv/images"]   # narrows --allow-dir to the overlap, never widens it; [] allows nothing
runs = "runs"                  # where action runs go without --runs, relative to this file
```

With `[workspace] dir` and `[serve] runs`, the configuration alone says where the store and the
runs are. A host can start `rascal serve --config rascal.toml`, and `rascal call <action>
--config rascal.toml` from a terminal then lands in the same store and runs: a host's command
handoff carries `--config`, never flags it does not understand.

A fill is cached under a key over the request's bytes (the image, the mask, the prompt) with the
filler's name and `revision`, so one filler's pixels are never another's. A hit writes `fill from
the cache` in the journal, and the run's `run.json` says `cache: hit` with spend 0; a call says
`miss` with the filler's `spend_usd_per_call`. The limits refuse a call whose price would pass
the per-call cap (an `error` with code `cap`) or the session's budget (code `budget`) before any
paid step, and `rascal estimate` or the `estimate` tool says so beforehand
(`docs/contract.md`, "The fill cache, prices and limits" and "Estimates").

Exactly one of `command` and `url`; `send_mask = true` on an image-input profile also sends
the `grey` rendering as `mask`.

**`key_env`** names the variable holding the filler's key, for either kind. For `url` it is
sent as `Authorization: Bearer`. A `command` inherits rascal's environment, so the script
reads the variable itself (a script that calls a hosted model, such as Bria on fal, names
`key_env = "FAL_KEY"`). Either way readiness lists it as a `credential` requirement, so a host
that passes a tool only the names it declares (Wafflery does) passes the key; a command filler
that reads a key it never declares works from a shell and fails under such a host. The key is
never stored, logged or journaled, and no pack sees the variable. `examples/rascal.toml` has both
entries; the schema is `schemas/rascal/config-v1.json`.

**`overrides`** are recipe overrides in the matrix's form (`"<step>.<param>" = value`,
`"<step>.constants" = "<set>"`, `docs/lab.md`) that `heal` applies to the recipe before it
runs when this filler paints. The lab rounds showed the band a filler can carry is a property
of the filler: a model that smears a wide band across smooth sky wants `fan-thin-v1`, one that
carries it wants the ramp (`docs/results/seam-02.md`). So the entry says it once, and `heal
seam --filler lama` runs the band LaMa needs while `--filler bria-erase` runs the wide ramp.
Each applied override is a journal note (`override from fillers.<name>: <key> = <value>`) and
the step's journaled parameters are the resolved ones, so a run says what it ran; an override
naming a step, parameter or set the recipe lacks is refused naming the entry before anything
runs. `rascal run` applies none (no filler is involved), and the lab ignores them: an arm says
everything it runs, so the rounds that find the right band stay explicit. `--set` on the
command line is applied after them and wins (`docs/recipes.md`).

**`fill_max_side`** is the scaled window (owner decision 10): a model trained on small images,
such as LaMa at a few hundred pixels, smears a band it is handed at ten times that. With
`fill_max_side` set (at least 1; 0 is refused when the configuration is read, as for
`max_side`), a request whose longest side exceeds it is sent downscaled to fit (the
image resampled with Lanczos, the mask with a triangle filter and, when binary, thresholded
back to binary) and the fill comes back scaled to the support with Lanczos before the paste.
The journal notes the two sizes, the call records the scaled request's dimensions and bytes,
the request identity is over the bytes sent, so a replay reproduces the call, and the lab's
cell identity carries the setting. The lab answered what it costs (`docs/results/lama-scale-01.md`):
for LaMa at 1024, twenty of twenty-one sky seams judged better than at full size and the
structured seams a near draw (twelve to fourteen), with the call faster by more than half; so
a filler trained small wants it where the seam crosses smooth content and can do without it
on detail. `max_side` (the window, a crop) is unchanged and the two combine.

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

Every call is two journal notes. Before it, `filler call started: {…}` with the entry's name and
its price; after it, `filler call: {…}` with the entry's name, the command as configured
(placeholders shown) or the URL, the request's dimensions and bytes, the answer's bytes, the
duration, the exit code or HTTP status, the outcome, and `charge`: `not_sent`, `known` or
`unresolved` (`docs/contract.md`, "The fill cache, prices and limits"); a 401 or 403 is `known`
and counts zero. A run's spend is read from
these notes, so a failed, cancelled or interrupted run keeps what it may have been charged. The key is handed to the
`Authorization` header and appears in no file, message or span. A key the server rejects (HTTP
401 or 403) ends the run `credential`, never retryable (exit 3): the call is known (the
provider answered) and counts zero, with `spend.basis: measured`; a key that is
not set never reaches the filler, since the call is refused as not ready before its run
(`docs/contract.md`, "Readiness").

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
(a hash of the bytes sent), the filler's name and its `revision`, so `--replay <previous run>`
reuses it when all three are the same and makes no call: a healed run replays with the filler
absent (G4). The replayed artifact's record names the run it came from. `--replay` takes the
directory holding the previous journal: the run directory, or the workspace when that run used
`--workspace`. A heal that stops at a step that is not ok still writes the tables and the run
page; every filler call, failed ones included, keeps its status, duration and bytes in the
journal.

## The checker (ADR 0023)

`heal remove-prompt` can ask a **checker** whether the object is still visible after the fill and
try again when it is (SPEC.md "Self-correcting remove", part 3). A checker is any table-manners
tool action that answers with a verdict (contract 0.5.0, 2.4); squint's `source_fidelity` is the
first. At most one, written by hand like a filler:

```toml
[checker]
command = ["/Users/me/.local/bin/squint", "call", "source_fidelity"]   # by absolute path; rascal adds --arguments and --allow-dir
arguments = { source = { path = "{source}" }, candidate = { path = "{candidate}" }, requirements = [{ id = "RQ1", kind = "must_not", text = "Any part of {prompt} is visible." }] }
key_env = "ANTHROPIC_API_KEY"   # declared so readiness lists it and a host passes it; never read or stored
spend_usd_per_call = 0.01       # the most one check costs: reserved per try, sent as the call's max_spend_usd
timeout_s = 120
revision = "squint 0.1, claude-haiku-5-5"
```

- `arguments` are the checker's own call arguments; rascal replaces `{source}` (the input image),
  `{candidate}` (the try's result) and `{prompt}` in their string values and adds
  `max_spend_usd`. `{source}` and `{candidate}` are required; any other `{…}` is refused.
- The check runs for `remove_prompt` with a filler when a checker is configured, its key is
  present, and the call has not turned it off (`"check": false`; `--no-check` on the command
  line, `rascal heal … --no-check`). The description then offers `check` (default true) and says
`answers: verdict`.
- The checker's program and key are **optional** requirements of `remove_prompt`, `for` "checking
  the result": missing, the action stays `ready` and a call fills once, unchecked, saying why.
- A checked call reserves its tries up front: the most of three whose total (each try the
  filler's price plus the checker's) fits the call's cap and the session's balance, at least one.
  The estimate's `worst_case_usd` is that total; `effective["x-rascal"].check` says whether the
  result is checked, or why not. The run's notes say "Checked: up to N tries within the spend
  limits.", or why it was not checked ("Not checked: the checker needs ANTHROPIC_API_KEY.").

**One check.** Each try's result is written as `tries/<k>/result.png` in the run directory, and
its check is staged in `tries/<k>/check/`: the input image as `source.png`, the result as
`candidate.png` and the rendered arguments as `arguments.json`. The checker runs there as
`<command> --arguments <that file> --allow-dir <that directory>`, its working directory the same,
so it reads nothing else wherever the workspace lives; its environment is rascal's (the key
passes through), its stderr is dropped. rascal reads the table-manners result it prints (kept as
`result.json`), never its exit code (squint exits 1 on `fail` and 2 on `unsure`): a `completed`
result with a `verdict` is the answer, and the run's `verdict` is the selected try's. Anything
else is a **check failure**, which ends the checking with the tries made and a note "The check
could not run: …": another status (with its error's reason), a completed result with no verdict,
output that is not JSON, output over 1 MiB, or no answer within `timeout_s` (the checker is
stopped).

**The journal** gets `checker call started: {"spend_usd": …}` before a check, `checker call:
{…}` after it (the command as configured, the duration, the exit code, the bytes, the outcome,
whether it may have been charged, and the spend the result reports; never a key), and, when it
answered, `checker stored: {…}`: the try, the request's identity (over the checker's command and
revision, the source's and the candidate's file digests and the arguments with digests in place
of the paths), the checker's `revision`, both digests, the arguments as sent and the verdict.
A check's spend is what its result reports; a check that printed none counts its price as
unresolved (`docs/contract.md`, "Spend").

**The scripted checker.** The testkit's `rascal fake-checker <script>` (a `--features testkit`
build) is a checker command that answers from a JSON script: `{"answers": [{"verdict": "fail",
"where": [[x0, y0, x1, y1]]}, {"verdict": "pass"}], "spend_usd": 0.004}`, the `n`th call taking
the `n`th answer (the last once they run out). An answer may add `delay_ms`, `foreign` (its places
name another image) or a `failure`: `status`, `no-verdict`, `garbage`, `oversize` or `outside`
(it reads a path outside `--allow-dir` and refuses itself, as a real tool refuses one). Like
squint it reads both images and exits 1 on `fail` and 2 on `unsure`. The tests in
`crates/rascal-mask/tests/it/checker.rs` drive it, and one runs the real `squint` with its
scripted provider when `../squint/target/debug/squint` is a testkit build.

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

`crates/rascal-mask/tests/it/fillers.rs` drives the port with each behaviour both ways.

# Tools

Every operation is a tool: one manifest, one envelope per invocation, callable as a subcommand,
from a recipe step, and (from M3) over MCP with the same arguments. `rascal ops list` prints the
catalogue; `rascal ops describe <op> [--json]` prints one manifest as a client sees it. From task
1.9 a check proves every op is documented on this page.

## The manifest

| field | meaning |
|---|---|
| `name`, `version` | the op and its semantic version; a behaviour change bumps it |
| `purpose`, `when_to_use`, `when_not_to_use`, `output_guidance` | the four sentences an agent reads before calling |
| `cost` | `tier` (cheap, moderate, expensive) and rough `seconds` per call |
| `inputs`, `outputs` | typed ports: name, artifact type (`Image`, `Location`, `Mask`, `BandProfile`, `Series`, `Plan`), required, purpose |
| `params` | each with a kind (number, integer, boolean, string, number array, choice), required or a default, bounds, length or choices |
| `returns` | bounds on the envelope: images, regions, measurements, transforms, summary bytes |
| `input_schema`, `output_schema` | JSON Schemas derived from `params` and `outputs` |

The registry checks every manifest when it registers it (`Manifest::check`): empty texts, a
version that is not `major.minor.patch`, duplicate ports, an optional parameter without a
default, a choice without choices, an array without a length, or schemas out of step with the
fields are refused at start and in tests.

## The envelope

`status` (`ok`, `measured_nothing`, `not_applicable`, `invalid_args`), `summary`, `reason` (when
not `ok`), `measurements`, `regions`, `images`, `transforms`, `provenance` (tool, version,
duration), `artifacts` (rascal's extension: every typed output by name, id and identity) and
`refinement` (what a refinement cleared). An envelope that is not `ok` publishes nothing and
needs a reason; an `ok` envelope has every required output of the declared type and no
undeclared one; counts stay within the manifest's bounds. The runner checks each envelope
before accepting it.

## Shared rules

- Inputs are validated against the manifest before the op runs: every required port bound,
  every bound port of the declared type (a rendering is an `Image` and is refused where a
  `Mask` is declared, naming its role), no unknown port, and a mask and an image in one
  invocation in one frame.
- Parameters are resolved before the op runs: defaults filled, kinds and bounds checked. A bad
  value is an `invalid_args` envelope in the journal and a usage error (exit 64), with or
  without `--strict`.
- An op that cannot act on its input returns `not_applicable` and publishes nothing: no
  artifact, no journal output, one journal line. `rascal run` exits 1 under `--strict` and 0
  without. A later step that binds the refused step's output does not run; the run stops
  naming the requirement.
- Every invocation appends one journal line, publishing or not, successful or not.

## The catalogue

| op | group | in | out | since |
|---|---|---|---|---|
| `rect` | find | Image; `bbox` | Location (`bbox`) | 1.1 |
| `rasterize` | refine | Location, Image; `use_case` | Mask (bbox and whole_image; the rest in 1.8b) | 1.1 |
| `render` | out | Mask [, Image]; `for`, `threshold`, `colour`, `strength` | Image: `grey`, `binary` (`mask_input`), `alpha`, `tinted` (`image_input`), `overlay` (inspection) | 1.1, 1.5 |
| `cut` | prep | Image (wrapped raw); `strip` | Image in a new `cut:` frame (the two edge strips side by side, the wrap edge at the centre) and the Plan | 1.8a |
| `put` | prep | Image (parent), Image (canvas), Plan | Image in the parent frame with the canvas's halves on the edge strips; cut then put is byte-identical | 1.8a |

The pixel primitives every op is built from live in `rascal_ops::primitives`: row smoothing
(a Gaussian truncated at 3σ, renormalised at the edges), the 3×3 Sobel magnitude with edge
replication, disc dilation and erosion, the exact Euclidean distance transform, sRGB to CIE Lab
under D65, and the standard normal CDF; each is tested against brute force or tabulated values.

An op subcommand's image port takes a workspace id or a path (a path is imported first), a mask
port takes an id or a mask directory, and `--out <file>` also writes a lone image output as a
PNG.

The remaining ops arrive with their tasks (PLAN.md): `line` and `stroke` in M1;
`fan`, `profile`, `feather`, `ramp`, `apply`, `window`, `unwindow`, `paste`, `measure` in M2;
`grow`, `shrink`, `close`, `subtract`, `select`, `invert`, `tighten` in M4; `segment` from the
Python pack in M5.

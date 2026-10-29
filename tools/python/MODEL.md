# The segmenter: the models, their export and their provenance

`segment` (SPEC.md "Operations") takes an image and a text prompt with `max_instances` and
returns one polygon region per instance with a score plus one plane per instance, in the
image's frame; `select` keeps or drops instances and `remove-prompt` grows, closes and
feathers the union. Branch A of ADR 0012 asked for a permissively licensed public segmenter
with an exportable checkpoint and text-prompt input; ADR 0018 (owner decision 5, 2026-10-29)
chose two models, a detector and a mask decoder, both Apache-2.0, both exported to ONNX so the
pack runs on `onnxruntime` with no PyTorch in the bundle.

## The models

| role | model | source | licence | export | size |
|---|---|---|---|---|---|
| text to boxes | OWLv2 base, patch 16 | [`google/owlv2-base-patch16`](https://huggingface.co/google/owlv2-base-patch16), revision `2a1560802f8cf3c408fec9b809d705f56a2f7146` | `apache-2.0` (the card's licence tag, read 2026-10-29) | `owlv2-base-patch16.onnx`, with the CLIP tokenizer as `owlv2-base-patch16.tokenizer.json` | 622 MB and 3.6 MB |
| box to plane | SAM 2.1 Hiera-Tiny | [`facebook/sam2.1-hiera-tiny`](https://huggingface.co/facebook/sam2.1-hiera-tiny), revision `de431c4043854a71d8101e17995dfe596bf101a5` | `apache-2.0` (the card's licence tag, read 2026-10-29) | `sam2.1-hiera-tiny.encoder.onnx`, `sam2.1-hiera-tiny.decoder.onnx` | 110 MB and 17 MB |

The ADR named MobileSAM as the decoder with SAM 2.1 Hiera-Tiny as its drop-in. The export
takes the drop-in: MobileSAM's code is installable only from its git repository, and the export
environment (`tools/python/export/`) is locked to PyPI packages so the export reproduces from
its lock; SAM 2.1 comes through Transformers like the detector. It is four times the size and
gives tighter planes. ADR 0018 records the refinement.

The digests in `artifacts.toml` are the exported files'; `rascal-tools fetch` verifies them
after a download and the pack verifies them once per process before an artifact is used. The
files are published as release assets of the repository (`models-2026-10`), and a bundle
carries them under `models/`.

## The export

`uv run --project tools/python/export python tools/python/export/export.py` on a connected
machine: it downloads both checkpoints from the Hub, exports the graphs below with
`torch.onnx.export` (the dynamo exporter, opset 18, one file each, no external data), checks
each graph against PyTorch on `fixtures/images/chair-blob.png`, and writes `export.json`
beside the artifacts with every digest, the Hub revisions and the versions used. The last
export, 2026-10-29: Python 3.12.15, torch 2.15.0, transformers 4.57.6, onnx 1.23.2,
onnxruntime 1.31.1.

| graph | inputs | outputs |
|---|---|---|
| `owlv2-base-patch16.onnx` | `pixel_values` [1, 3, 960, 960] float32; `input_ids`, `attention_mask` [Q, 16] int64, Q dynamic | `logits` [1, 3600, Q]; `pred_boxes` [1, 3600, 4] as centre x, centre y, width, height, fractions of the padded square; `objectness` [1, 3600] |
| `sam2.1-hiera-tiny.encoder.onnx` | `pixel_values` [1, 3, 1024, 1024] float32 | `feat0` [1, 256, 256, 256], `feat1` [1, 256, 128, 128], `feat2` [1, 256, 64, 64] |
| `sam2.1-hiera-tiny.decoder.onnx` | the three features; `boxes` [1, B, 4] float32 as x0, y0, x1, y1 in 1024-pixel coordinates, B dynamic | `masks` [1, B, 256, 256] logits (inside where > 0); `iou` [1, B], the decoder's own estimate |

Export parity on the fixture (ONNX against PyTorch, same preprocessed tensors): OWLv2 logits
within 5.1e-3 and boxes within 1.7e-4; SAM 2.1 features within 1.3e-5 and the thresholded
mask identical (IoU 1.000; the logits differ by up to 0.21 away from the threshold).

## Preprocessing in the pack

The pack mirrors the upstream processors in numpy and Pillow (`rascal_tools.models`):

- **OWLv2**: the image is padded to a square at the bottom and right with mid-grey (127),
  resized to 960 by 960 with Pillow's bilinear filter (antialiased), scaled to [0, 1] and
  normalised with the CLIP mean and deviation. Upstream resizes with a Gaussian anti-alias and
  a bilinear zoom from scipy; on the fixture the two give the same best box to the pixel and
  scores within 0.001. The prompt is split into queries on `.`, `,` and `;`, lower-cased,
  tokenised with the CLIP tokenizer (`tokenizers`), padded to 16 with id 0 and truncated. A
  box's score is the sigmoid of its logit for the query; boxes over the threshold are kept per
  query after greedy suppression at IoU 0.5, then sorted by score across the queries and cut
  to `max_instances`. Boxes come back in image pixels, clipped to the image.
- **SAM 2.1**: the image is resized to 1024 by 1024 (bilinear, antialiased), scaled and
  normalised with the ImageNet mean and deviation; each box is scaled into those coordinates.
  The decoder's 256 by 256 logits are resized to the image (bilinear) and thresholded at zero,
  as upstream's post-processing does.

## Parity and the tests

`tools/python/tests/test_segment.py` covers the query parsing, the box arithmetic, the
preprocessing and the argument checks without a model, and under the `segmenter` marker (run
only when every artifact is fetched and verified) the chair fixture through both models with
parity against `fixtures/parity/segment/chair/`: the envelope must match exactly (scores to
three decimals, polygon corners to four) and the plane may differ in at most 0.5% of its
pixels. The fixture's chair is a drawn blob; the detector puts its box on it exactly but
scores it 0.07 for "a wooden chair" (0.02 for "chair"), as PyTorch does on the same pixels, so
the recorded case uses threshold 0.05. On photographs the default threshold, 0.2, is the
right one. No test downloads anything.

`crates/rascal-mask/tests/packs_real.rs` drives the pack through rascal's client and calls
`segment` when the artifacts are present; `crates/rascal-mask/tests/remove_prompt.rs` runs
`remove-prompt` through the fake pack. The bundle acceptance (`scripts/bundle-acceptance.sh`)
calls `segment` from a bundle that carries the models.

## Redistribution

Apache-2.0 permits redistributing the exports with the licence text and a notice of changes;
the export is the change. `tools/python/NOTICE` carries both for each model and travels with
the bundle and the release assets.

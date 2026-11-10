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
| box to outlines | SAM 2.1 Hiera-Tiny | [`facebook/sam2.1-hiera-tiny`](https://huggingface.co/facebook/sam2.1-hiera-tiny), revision `de431c4043854a71d8101e17995dfe596bf101a5` | `apache-2.0` (the card's licence tag, read 2026-10-29) | `sam2.1-hiera-tiny.encoder.onnx` (2026-10-29), `sam2.1-hiera-tiny.decoder3.onnx` (2026-11-04) | 110 MB and 17 MB |

The ADR named MobileSAM as the decoder with SAM 2.1 Hiera-Tiny as its drop-in. The export
takes the drop-in: MobileSAM's code is installable only from its git repository, and the export
environment (`tools/python/export/`) is locked to PyPI packages so the export reproduces from
its lock; SAM 2.1 comes through Transformers like the detector. It is four times the size and
gives tighter planes. ADR 0018 records the refinement.

The digests in `artifacts.toml` are the exported files'; `rascal-tools get` checks them after a
download and the pack verifies an installation once per process before it is used. The files
are published as release assets of the repository (`models-2026-10` for OWLv2 and the SAM
encoder, `models-2026-11` for the three-outline decoder); a bundle carries none. Each model's licence texts are artifacts too, installed beside its graphs: Tiny's,
`LICENSE.owlv2-base-patch16.txt` and `LICENSE.sam2.1-hiera-tiny.txt` (the Apache-2.0 text with a
header naming the model, its revision and the export), are the release `models-2026-11-tiny`,
and their sources are `export/licences/`. Every artifact records its size, which bounds a
download.

## The catalogue

`segment` runs the model its `model` argument names from the segmenter catalogue,
`src/rascal_tools/segmenters.toml` (SPEC.md "Choosable segmenters"): per model its id, the
adapter that runs it (`rascal_tools/models/adapters.py`), the `segment` parameters it uses (a
call that supplies another is `invalid_args`, even at its default), its licence and licence
files, its files by name in `artifacts.toml`, its sizes, the platforms it runs on and the lab
round's figures. Exactly one entry is the default (`tiny`). The catalogue holds five models:
`tiny`, the two models above as one adapter, `owlv2+sam2`, which carries M13's pick and retry and
reports `outline_<n>`, `retried_<n>`, `retry_kept_<n>` and `mask_iou_predicted`; and the four
below, `hqsam`, `birefnet`, `sam3` and `evfsam2`. Every adapter's instances come back best score
first, ties by query then by the box's top-left, row-major, cut to `max_instances`; region `n` is
paired with plane `n`. One model is loaded at a time.

## HQ-SAM ViT-L (`hqsam`, task 14.6)

- **Source.** [`lkeab/hq-sam`](https://huggingface.co/lkeab/hq-sam) `sam_hq_vit_l.pth`, revision
  `09b02a333b37772133eff3997bdba997867374b7`; `apache-2.0` (the card's licence tag, read
  2026-11-09); the code, SysCV/sam-hq (PyPI `segment-anything-hq` 0.3), Apache-2.0. The
  checkpoint is a pickle and is read only by the export, with `weights_only=True`.
- **Export.** `export.py --only hqsam [--references]` (`export/hqsam.py`; torch 2.15.0, the
  TorchScript exporter, opset 17): `sam-hq-vit-l.encoder.onnx` (1,234 MB: pixel_values to
  image_embeddings and the ViT's first global-attention output, the one the HQ decoder reads) and
  `sam-hq-vit-l.decoder.onnx` (22 MB: SAM-HQ's own `SamOnnxModel` with `hq_token_only=True` and
  `multimask_output=False`, its mask post-processing included, so the output is in image pixels),
  and `LICENSE.sam-hq-vit-l.txt` (the repository's Apache-2.0 text). Published
  as the release `models-2026-11-hqsam`; the digests are in `artifacts.toml`.
- **Preprocessing** (`rascal_tools/models/hqsam.py`), as `SamPredictor`: the longest side resized
  to 1024 (Pillow bilinear, `ResizeLongestSide`'s rounding), normalised with SAM's mean and
  deviation on the 0-255 scale, padded with zeros at the bottom and right; positive points (label
  1) then the box's corners (labels 2, 3) in the resized frame; the output thresholded at 0.
- **Adapter** `owlv2+hqsam`: OWLv2 as Tiny's, one outline per padded box, M13's retry (it takes
  points), no pick and no `outline_<n>`. Parameters: `threshold`, `max_instances`, `pad`,
  `min_cover`.
- **Parity** (2026-11-09). The export against the maker's predictor on the chair fixture: plane
  IoU 1.0 for a box alone and with 1 and 3 points, predicted IoU within 2.4e-7. Instance parity
  (`tests/test_model_parity.py`, references in `fixtures/models/hqsam/`, positive case at
  threshold 0.01, the first of 0.2, 0.1, 0.05, 0.01 to find the chair on both images; "giraffe"
  at 0.2 finds nothing): passes from an installation made with `get hqsam --from` the export, with
  `HF_HOME` empty and the network denied (`sandbox-exec`). The corpus gate (`segment-lab
  pack-round` with the branch's own rascal and pack, then `compare` against the lab's
  `hqsam-vitl-hqonly` cell), offline from the same installation: 20 of 20 images at IoU ≥ 0.999,
  lowest 0.999996 (`thin-02`), the other 19 at 1.000000.
- **Release download** (2026-11-10, macOS arm64): `rascal-tools get hqsam` from the release into a
  fresh models directory (installation `6be1130e…`), then `RASCAL_TOOLS_REQUIRE_MODELS=hqsam pytest
  -m model_hqsam` with `HF_HOME` empty and the network denied: passed. **Linux x86_64:** passed 2026-11-10: `scripts/model-gate.sh hqsam` (linux/amd64 under emulation, Docker's VM at 32 GB; it ran out of memory at 8 GB) installed from the release with the same identity (`6be1130e…`) and passed its 1 test with the network off in 42 s.

## BiRefNet (`birefnet`)

| | |
|---|---|
| adapter | `owlv2+birefnet`: OWLv2 as Tiny's, then BiRefNet in each padded box, one outline per box (no pick, no retry); `segment` parameters `threshold`, `max_instances`, `pad` |
| source | [`ZhengPeng7/BiRefNet`](https://huggingface.co/ZhengPeng7/BiRefNet), revision `e2bf8e4460fc8fa32bba5ea4d94b3233d367b0e4` (the lab's), the maker's own code loaded with `trust_remote_code` on the exporting machine only |
| licence | MIT (the card's licence tag, read 2026-11-09); the text is the maker's GitHub `LICENSE` at `2713d1293ce8a923d5878f1f30177b78770bf24c` (the Hub repository has none), installed as `LICENSE.birefnet.txt` |
| export | `export.py --only birefnet` (`export/birefnet.py`): the last and finest output through the dynamo exporter at **opset 19**, which writes torchvision's `deform_conv2d` as ONNX's `DeformConv` (new in opset 19; opset 18 fails the checker); onnxruntime 1.31.1 runs it on the CPU. `birefnet.onnx`: `input_image` [1, 3, 1024, 1024] to `output_image` [1, 1, 1024, 1024] logits, 897 MB, one file |
| release | `models-2026-11-birefnet` (the graph and the licence); with OWLv2's two files, 1.5 GB to download |
| preprocessing | the padded box rounded outward to whole pixels and clamped, cropped, resized to 1024² (Pillow bilinear), [0, 1], the ImageNet mean and deviation; the output's sigmoid quantised to 8 bits (truncating), resized to the crop as an 8-bit image (Pillow bilinear), kept above 127, placed at the crop. A crop under 2 px on a side gives an empty plane. Exactly the lab's `birefnet` cell |
| export parity | on the chair fixture's padded box: probabilities within 1.9e-4 of PyTorch, the kept plane identical (IoU 1.000000) |
| instance parity | `fixtures/models/birefnet/references.json` (`export.py --only birefnet --references`): the maker's PyTorch model through this adapter on `chair-blob.png` and `chair-wide.png` (the chair mirrored on a wider canvas, made for the purpose; a version rotated to portrait gave no detection at any threshold), "chair. table" at 0.01 (the highest of 0.2, 0.1, 0.05, 0.01 with an instance on each image) and "giraffe" at 0.2; `tests/test_model_parity.py` holds the installed graph to it |
| instance parity, installed | passed 2026-11-09: `get birefnet --from` the export into an isolated models directory, `HF_HOME` empty, `HF_HUB_OFFLINE=1`; `RASCAL_TOOLS_REQUIRE_MODELS=birefnet pytest -m model_birefnet tests/test_model_parity.py`, 1 passed |
| corpus gate | passed 2026-11-09: `segment-lab pack-round --model birefnet` with the branch's own rascal and pack (rascal forwarding only `threshold`, `max_instances` and `pad`, d798d91), then `compare` against the lab's `birefnet` cell: 20 of 20 images at IoU 1.000000 |
| release download | passed 2026-11-10 on macOS arm64: `rascal-tools get birefnet` from the release into a fresh models directory (installation `62824792…`), then `RASCAL_TOOLS_REQUIRE_MODELS=birefnet pytest -m model_birefnet` with `HF_HOME` empty and the network denied: passed |
| Linux x86_64 | passed 2026-11-10: `scripts/model-gate.sh birefnet` (linux/amd64 under emulation, Docker's VM at 32 GB; it ran out of memory at 8 GB) installed from the release with the same identity (`62824792…`) and passed its 1 test with the network off in 118 s |
| lab | segment-01: IoU 0.736, remove score 0.731, the least spill of the five (0.066) |

## SAM 3 (`sam3`, task 14.8)

- **Source.** [`facebook/sam3`](https://huggingface.co/facebook/sam3) (Transformers `Sam3Model`,
  safetensors), revision `3c879f39826c281e95690f02c7821c4de09afae7`; the **SAM License** (Meta,
  custom; gated on the Hub, accepted by the owner 2026-11-05). The export is a derivative of SAM
  Materials and is redistributed under that licence, whose text, `LICENSE.sam3.txt` (the
  repository's `LICENSE` as downloaded), goes with every copy (owner decision 15).
- **Export.** `export.py --only sam3 [--references]` runs `export/sam3/export_sam3.py` in its
  own environment, `export/sam3/` (Transformers 5.22.1, torch 2.15.0: `Sam3Model` is not in the
  Transformers 4 line the shared export environment pins). Dynamo exporter, opset 18, three
  graphs: `sam3.vision.onnx` with its external data `sam3.vision.onnx.data` (7 MB + 1,816 MB:
  one file would pass protobuf's 2 GB; each release asset is under GitHub's 2 GB per file),
  pixel_values [1,3,1008,1008] to three FPN levels and their position encodings;
  `sam3.text.onnx` (1,416 MB), input_ids and attention_mask [1,32] to text_features [1,32,256];
  `sam3.head.onnx` (134 MB), the six vision tensors, text_features and attention_mask to
  pred_logits [1,200], pred_boxes [1,200,4] (xyxy fractions), pred_masks [1,200,288,288]
  (logits) and presence_logits [1,1]. The head carries the lab's text-only workaround: one box
  labelled -10, which makes Transformers run the geometry encoder as Meta's own code does
  (segment-lab `adapters/sam3_hf.py`). With them `sam3.tokenizer.json` (the checkpoint's CLIP
  tokenizer) and the licence: 3,377 MB in all. Published as the release `models-2026-11-sam3`; the digests are in
  `artifacts.toml`.
- **Preprocessing** (`rascal_tools/models/sam3.py`), as `Sam3Processor`: the image resized to
  1008x1008 exactly as torchvision resizes a uint8 image (bilinear, antialiased, int16
  fixed-point weights, horizontal then vertical, each pass rounded; `resize_uint8`, checked bit
  for bit against torchvision 0.29.2 by `tests/test_sam3.py`), then `(x - 127.5) / 127.5`. Pillow's
  own bilinear resize differs by one level in about 0.5% of values, enough to move the corpus
  gate's `hole-01` to IoU 0.990, so it is not used. The query is tokenised with the CLIP
  tokenizer, padded with `<|endoftext|>` to 32 tokens with its attention mask, cut at 32.
- **Post-processing**, as `post_process_instance_segmentation`: score sigmoid(logit) times
  sigmoid(presence logit), kept at `threshold` or above; the mask the sigmoid of the 288x288
  logits resized to the image (bilinear, `align_corners=False`) and kept above 0.5; an empty mask
  gives no instance; the region the predicted box in image pixels, clipped to the image. No
  NMS, as the lab's cell.
- **Adapter** `sam3`, end to end: one vision pass per image, one text and one head pass per
  query; no OWLv2, no outliner, no pick, no retry, no diagnostics. Parameters: `threshold`,
  `max_instances`.
- **Parity** (2026-11-09). The graphs against PyTorch on the chair fixture: vision features
  within 7.7e-4, text features 2.2e-5, head logits 8.3e-5, mask logits 4.1e-3, presence 7.7e-7;
  through the adapter the same one instance, score within 6e-6, plane IoU 1.0. Instance parity
  (`tests/test_model_parity.py`, references in `fixtures/models/sam3/`, positive case at
  threshold 0.2, the first of 0.2, 0.1, 0.05, 0.01 to find the chair on both images, one
  instance each; "giraffe" at 0.2 finds nothing): passes from an installation made with
  `get sam3 --from` the export, with `HF_HOME` empty and the network denied (`sandbox-exec`).
  The corpus gate (`segment-lab pack-round --model sam3` with the branch's own rascal and pack,
  then `compare` against the lab's `sam3-own` cell, which ran PyTorch on the Apple GPU), offline
  from the same installation: 20 of 20 images at IoU >= 0.999, lowest 0.999987 (`hole-01`),
  eight at 1.000000. With Pillow's resize in place of torchvision's it was 19 of 20 (`hole-01`
  0.990382), which is why the pack carries the port.
- **Release download** (2026-11-10): `rascal-tools get sam3` from the release into a fresh models
  directory, then `RASCAL_TOOLS_REQUIRE_MODELS=sam3 pytest -m model_sam3` offline (`HF_HOME`
  empty, network denied): installed with the same identity as from the export (`593e11a3…`), and
  instance parity passes. The Linux x86_64 gate passed 2026-11-10: `scripts/model-gate.sh sam3` (linux/amd64 under emulation, Docker's VM at 32 GB; it ran out of memory at 8 GB) installed from the release with the same identity (`593e11a3…`) and passed its 1 test with the network off in 84 s.

## EVF-SAM2 (`evfsam2`)

| | |
|---|---|
| adapter | `evfsam2`: end to end, no detector. Per query BEIT-3 Large reads the image and the query and its first token, projected, prompts SAM 2.1 Large's decoder (no box, no points); of the three outlines the one with the highest predicted IoU is kept. One instance per query, score 1.0, region the plane's bounding box; a query whose plane is empty gives no instance. `segment` parameter `max_instances` alone: `threshold`, `pad`, `outline` and `min_cover` are refused, and a checker-directed crop retry sends none of them (rascal's crop pad is 0.05, SPEC.md "Choosable segmenters") |
| source | code [`hustvl/EVF-SAM`](https://github.com/hustvl/EVF-SAM) at `9935dc52571678b806830821b3ddeacb30e7af07` (GitHub's archive, SHA-256 `37d84f8c…aff13`; not on PyPI); weights [`YxZhang/evf-sam2-multitask`](https://huggingface.co/YxZhang/evf-sam2-multitask), revision `df3644009016a4f62d13c02f3f8fb6c9bbf94310` (safetensors; only the -multitask checkpoints load at this commit), as the lab's `evfsam2` cell |
| licence | Apache-2.0: the card's licence tag (read 2026-11-09) and the code repository's `LICENSE`, installed as `LICENSE.evf-sam2.txt` |
| export | `export.py --only evfsam2` runs `export/evfsam2.py` in its own environment, `export/evfsam2/` (Transformers 4.51.3, torchscale 0.2.0, hydra, timm: the maker's code predates the main export environment's Transformers). TorchScript exporter, opset 17, float32. **`evf-sam2.image.onnx`** (852 MB): `image` [1, 3, H, W] (RGB 0 to 255, any size) to `image_embed` [1, 256, 64, 64], `high_res_0` [1, 32, 256, 256], `high_res_1` [1, 64, 128, 128] (stretched to 1024² bilinear, SAM 2's mean and deviation, `no_mem_embed` added) and `beit_image` [1, 3, 224, 224] (over 255, bicubic without antialiasing, normalised to [-1, 1]); both resizes are inside the graph. **`evf-sam2.prompted.onnx`** (2.7 GB of weights in two external-data parts, `evf-sam2.prompted.0.data` and `.1.data`, each under GitHub's 2 GiB limit on a release asset): `beit_image`, `input_ids` [1, T] and the three feature levels to `low_res_mask` [1, 1, 256, 256] (the kept outline's logits) and `iou` [1, 3]. **`evf-sam2.tokenizer.json`**: the checkpoint's XLM-R sentencepiece model converted to `tokenizers` JSON, its ids checked equal to the slow tokenizer's on fifteen queries |
| release | `models-2026-11-evfsam2` (the two graphs, the two weight parts, the tokenizer, the licence); 3.6 GB to download |
| preprocessing | in the pack, the image as float32 planes (the graph resizes and normalises); the query tokenised with `<s>` and `</s>`; the kept outline's 256² logits resized to the image bilinear with half-pixel centres, a negative source clamped to 0, no antialiasing (`F.interpolate`'s `align_corners=False`, in numpy: `adapters.bilinear`), kept above 0. The image graph runs once per image, the prompted graph once per query. Exactly the lab's `evfsam2` cell (`EvfSam2Model.inference`) |
| export parity | the graphs against the lab cell's own path (the maker's `inference`) on `chair-blob.png` and `chair-wide.png`, "chair", "table" and "giraffe": the planes identical in all six (IoU 1.000000) |
| instance parity | `fixtures/models/evfsam2/references.json` (`export.py --only evfsam2 --references`): the maker's PyTorch model through this adapter on the two fixtures, "chair. table" at 0.2 (the first of 0.2, 0.1, 0.05, 0.01; EVF-SAM2 ignores the threshold) and "giraffe" at 0.2, which EVF-SAM2 answers with a plane on each image (it always answers, even on a blank image), recorded and compared like any other; `tests/test_model_parity.py` holds the installed graphs to it. Passed 2026-11-09 from an installation made with `get evfsam2 --from` the export, `HF_HOME` empty, `HF_HUB_OFFLINE=1`, the network denied (`sandbox-exec`) |
| corpus gate | passed 2026-11-09: `segment-lab pack-round --model evfsam2` with the branch's own rascal and pack, offline from the same installation, then `compare` against the lab's `evfsam2` cell: 20 of 20 images at IoU ≥ 0.999, lowest 0.999989 (`parts-03`) |
| download | passed 2026-11-10: `rascal-tools get evfsam2` from the release into a fresh models directory gave the gated installation's identity (`7931bdfe…`), and `RASCAL_TOOLS_REQUIRE_MODELS=evfsam2 pytest -m model_evfsam2` passed offline from it on macOS arm64 |
| Linux gate | passed 2026-11-10: `scripts/model-gate.sh evfsam2` (linux/amd64 under emulation, Docker's VM at 32 GB; it ran out of memory at 8 GB) installed from the release with the same identity (`7931bdfe…`) and passed its 4 tests with the network off in 76 s |
| lab | segment-01: IoU 0.664, remove score 0.581; 4 to 9 s an image on a CPU for three queries |

## Installations

`rascal_tools.installs` lays models out in the models directory as installations (SPEC.md
"Installing a model"): `<models>/<id>/<identity>/` holds a model's files by name and its
`install.json` (the model, the catalogue revision, each file's name, size and SHA-256); the
identity is the SHA-256 of that record; `<models>/<id>/current` names the installation in use and
is replaced in one rename; every file is kept once under `<models>/files/<sha256>` as a hard link;
`.lock` serialises publishing, deleting and the holds runs take (`.held/<run id>`, the run's
process id inside; a hold whose process is gone is ignored); a held installation is never
deleted. An installation is verified once per process per identity (sizes, digests, regular
files), and every external-data file a graph names must be a file of the same installation
(`rascal_tools.onnx_refs` reads the locations from the graph's protobuf). The pack reads every
model through its installation (`rascal_tools.models.cache`); a `segment` call may name the
installation it expects, and the pack runs that one even when it is no longer `current`.
`rascal-tools get <model>` makes installations (`rascal_tools.installer`, `docs/tools.md`).

## The export

`uv run --project tools/python/export python tools/python/export/export.py` on a connected
machine: it downloads both checkpoints from the Hub, exports the graphs below with
`torch.onnx.export` (the dynamo exporter, opset 18, one file each, no external data), checks
each graph against PyTorch on `fixtures/images/chair-blob.png`, and writes `export.json`
beside the artifacts with every digest, the Hub revisions and the versions used. `--only
sam_decoder` exports and checks the decoder alone, leaving the other files as published. The
full export ran 2026-10-29; the decoder alone was re-exported 2026-11-04 (PLAN.md 13.1) from
the same SAM revision: Python 3.12.15, torch 2.15.0, transformers 4.57.6, onnx 1.23.2,
onnxruntime 1.31.1.

| graph | inputs | outputs |
|---|---|---|
| `owlv2-base-patch16.onnx` | `pixel_values` [1, 3, 960, 960] float32; `input_ids`, `attention_mask` [Q, 16] int64, Q dynamic | `logits` [1, 3600, Q]; `pred_boxes` [1, 3600, 4] as centre x, centre y, width, height, fractions of the padded square; `objectness` [1, 3600] |
| `sam2.1-hiera-tiny.encoder.onnx` | `pixel_values` [1, 3, 1024, 1024] float32 | `feat0` [1, 256, 256, 256], `feat1` [1, 256, 128, 128], `feat2` [1, 256, 64, 64] |
| `sam2.1-hiera-tiny.decoder3.onnx` | the three features; `points` [1, P, 2] float32 as x, y in 1024-pixel coordinates and `point_labels` [1, P] int64, P dynamic from 2 to 10: the box as its two corners (labels 2 and 3, the way SAM 2's own predictor passes a box), then up to eight positive points (label 1). One box per call; the pack reuses the features | `masks` [1, 3, 256, 256]: three candidate outlines as logits (inside where > 0); `iou` [1, 3], the decoder's predicted IoU of each |

Export parity on the fixture (ONNX against PyTorch, same preprocessed tensors): OWLv2 logits
within 5.1e-3 and boxes within 1.7e-4; SAM 2.1 features within 1.3e-5. The decoder (gate: each
outline's IoU ≥ 0.99 after the threshold, each predicted IoU within 0.02):

| prompt | outline IoUs | predicted IoU max difference |
|---|---|---|
| the chair's box alone | 1.000, 1.000, 1.000 | 6.0e-8 |
| the box and 1 point | 1.000, 1.000, 1.000 | 6.0e-8 |
| the box and 3 points | 1.000, 1.000, 1.000 | 4.2e-7 |

The box goes in as its two corner points rather than through Transformers' box input. On the
fixture the two give the same three outlines (IoU 0.999, 0.999 and 1.000). Unused point slots
are never padded: the graph's point axis is dynamic, because padding with "not a point" labels
moved the outlines by up to 11% IoU in a trial.

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
  normalised with the ImageNet mean and deviation; a box's corners and its points are scaled
  into those coordinates. Each of the three 256 by 256 outlines is resized to the image
  (bilinear) and thresholded at zero, as upstream's post-processing does.

## Parity and the tests

`tools/python/tests/test_segment.py` covers the query parsing, the box arithmetic, the
preprocessing and the argument checks without a model, and under the `segmenter` marker (run
only when Tiny is installed, required under RASCAL_TOOLS_REQUIRE_MODELS=tiny) the chair fixture through both models with
parity against `fixtures/parity/segment/chair/`: the envelope must match exactly (scores to
three decimals, polygon corners to four) and the plane may differ in at most 0.5% of its
pixels. The fixture's chair is a drawn blob; the detector puts its box on it exactly but
scores it 0.07 for "a wooden chair" (0.02 for "chair"), as PyTorch does on the same pixels, so
the recorded case uses threshold 0.05. On photographs the default threshold, 0.2, is the
right one. `tests/test_sam2.py` covers the decoder's prompt tensors without a model and, with
the artifacts, the three outlines for a box alone and with 1 and 8 points. The chair's stored
case was re-recorded with the three-outline decoder and `segment`'s outline rules (2026-11-04):
the box padded by 0.05 of its sides, the whole-object pick keeps candidate 0, 8338 pixels at
predicted IoU 0.944 (the single-outline graph gave 8033 at 0.983); no retry runs.
No test downloads anything.

`crates/rascal-mask/tests/it/packs_real.rs` drives the pack through rascal's client and calls
`segment` when the artifacts are present; `crates/rascal-mask/tests/it/remove_prompt.rs` runs
`remove-prompt` through the fake pack. The bundle acceptance (`scripts/bundle-acceptance.sh`)
calls `segment` from a bundle that carries the models.

## Redistribution

Apache-2.0 permits redistributing the exports with the licence text and a notice of changes;
the export is the change. `tools/python/NOTICE` carries both for each model and travels with
the bundle and the release assets.

SAM 3 is not Apache-2.0: its export is redistributed under the SAM License (owner decision 15),
whose copy, `LICENSE.sam3.txt`, is a release asset beside the graphs and is installed with them;
its use restrictions bind whoever uses the model. The release notes name the licence and the
source.

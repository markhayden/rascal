# The segmenter: survey and recommendation (draft for the owner's decision)

Task 4.5 (PLAN.md), owner decision 5 (`docs/owner-decisions.md`), ADR 0018 to follow. The
question: is there a permissively licensed public segmenter with **text-prompt input** and an
**exportable checkpoint** that rascal's Python pack may redistribute in its bundle (branch A), or
does v1 ship the pack without `segment` (branch B)? SPEC.md "Open questions" 1; success
criterion 5 or 5b hangs on it.

Surveyed 2026-10-29. Licence tags were read from the primary pages named in each row (the
model card's `license` field, or the repository's LICENSE file); where only secondary sources
were available this says so. The owner should open the linked pages before signing.

## Candidates

| model | what it does | prompt | licence (source) | size | export | fit |
|---|---|---|---|---|---|---|
| **CLIPSeg** `CIDAS/clipseg-rd64-refined` | one model, text (or image) prompt → a coarse segmentation logit map | text, one phrase per call | Apache-2.0 on the Hugging Face card ([card](https://huggingface.co/CIDAS/clipseg-rd64-refined)); the original code is MIT and that repository says nothing about its weights, so the Hub copy is the one to ship | 0.2 B parameters | a single Transformers model; ONNX through `optimum` | simplest pipeline; masks are coarse (a 352-pixel input, a low-resolution logit map upsampled), good enough for `remove` after `grow` and `close`, poor for thin or precise instances; no instance separation (one plane per prompt) |
| **OWLv2** `google/owlv2-base-patch16` + a SAM-family decoder | text queries → boxes with scores; boxes → planes | text, several queries per call | Apache-2.0 on the card ([card](https://huggingface.co/google/owlv2-base-patch16)) | 0.2 B | Transformers; ONNX through `optimum` (the export task must be checked for this architecture) | real instances with scores, which `select`'s keep/drop needs; two models |
| **Grounding DINO** `IDEA-Research/grounding-dino-tiny` + a SAM-family decoder | text → boxes with scores; boxes → planes | text; the card requires lowercase queries ending in a dot | Apache-2.0 on the card ([card](https://huggingface.co/IDEA-Research/grounding-dino-tiny)); a mirror states the weights are published under Apache-2.0 | 0.2 B | Transformers; no documented ONNX path on the card (TAO's export is a different workflow) | the strongest open-vocabulary detector of the three; heavier to export |
| **MobileSAM** (`ChaoningZhang/MobileSAM`) | points or boxes → planes | not text; the decoder for the two above | Apache-2.0 for the code ([repository](https://github.com/ChaoningZhang/MobileSAM)); the README does not state a licence for the weights separately | 9.7 M parameters in all | `scripts/export_onnx_model.py` in the repository | small enough to bundle; one encoder pass per image, one decoder pass per box |
| **SAM 2 / 2.1** (`facebookresearch/sam2`) | points or boxes → planes (and video) | not text | Apache-2.0 LICENSE file ([repository](https://github.com/facebookresearch/sam2/blob/main/LICENSE)); third-party ONNX repackagings say the weights carry the same licence ([samexporter](https://github.com/vietanhdev/samexporter), [an ONNX export](https://huggingface.co/vietanhdev/segment-anything-2.1-onnx-models)) | Hiera-Tiny 39 M | `samexporter` (MIT) exports encoder and decoder to ONNX | better masks than MobileSAM at four times the size; still bundle-sized |
| EfficientViT-SAM (`mit-han-lab/efficientvit`) | boxes → planes | not text | reported Apache-2.0 by redistributors; the repository's LICENSE was not read | L1 about 35 M | ONNX published by the authors | an alternative decoder if MobileSAM's quality disappoints; verify the licence first |

Not considered: YOLO-World and the Ultralytics stack (AGPL-3.0), Florence-2 (MIT, but a
generative model whose grounding output needs its own parsing and whose ONNX export is a
community effort), anything behind an API.

## What rascal needs from `segment`

`segment` (SPEC.md "Operations") takes an image and a text prompt (points later) with
`max_instances`, and returns one polygon region per instance with a score plus one plane per
instance, in the image's frame; `select` keeps or drops instances by index and `remove` grows,
closes and feathers the union. So the pack needs **instances with scores**, not one blended map:
a detector that proposes boxes per instance, and a mask decoder that turns each box into a plane.
CLIPSeg alone gives a single relevance map per prompt and would have to be thresholded and split
into connected components to fake instances; it is the fallback, not the design.

## Recommendation: branch A with OWLv2 + MobileSAM

- **OWLv2 base** for text → boxes: Apache-2.0 on its card, a plain Transformers model, several
  queries per call (`prompt = "chair. table."` becomes a list), scores per box, which maps onto
  `segment`'s regions and `max_instances` directly.
- **MobileSAM** for box → plane: Apache-2.0, 9.7 M parameters (a 40 MB bundle rather than 2.5 GB
  for SAM-H), a documented ONNX export, one encoder pass per image and a cheap decoder pass per
  box. If its masks prove too loose on the lab corpus, SAM 2.1 Hiera-Tiny (also Apache-2.0,
  39 M) drops in behind the same interface.
- Both export to ONNX, so the pack runs on `onnxruntime` without PyTorch in the bundle, which
  is what SPEC.md "The Python pack and its bundle" asks for (a wheelhouse per platform, offline
  install, digests over every file).
- Grounding DINO is the stronger detector but has no documented ONNX path for the Hub
  checkpoint; it is the upgrade to try in the lab if OWLv2's recall on interiors disappoints,
  not the first choice.

**Redistribution terms to confirm before signing.** Apache-2.0 permits redistribution of the
exported weights with the licence text and a notice of changes (the export is a change). Two
things the owner should check on the live pages: that `ChaoningZhang/MobileSAM`'s weights are
covered by the repository's Apache-2.0 (the README states the licence for the code), and that
`google/owlv2-base-patch16`'s card still reads `apache-2.0`. SAM 2's weights are covered by the
repository's Apache-2.0 LICENSE per the repackagers; Meta's own README was not read here.

**If the owner prefers branch B**: the pack ships with provisioning, `echo_region` and the
parity harness; `segment` becomes a named later milestone in the README; success criterion 5b is
in force. Nothing in M4 changes either way: `select` and the recipes are built for instances.

## The decision row

The owner signs in `docs/owner-decisions.md` (decision 5) with the branch, the model pair and
the date; ADR 0018 then records it before task 5.2 starts.

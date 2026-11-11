# segment-01 candidates

A licence and capability survey of segmenters for rascal's text-prompted object removal (PLAN.md task 13.4). rascal ships OWLv2 base (text to boxes) and SAM 2.1 Hiera-Tiny (box to 3 outlines), both Apache-2.0, on onnxruntime CPU. The rule: AGPL or other copyleft that binds distribution is out; non-commercial-only weights are out for shipping. All pages were read on 2026-11-04. Where a page does not state something, the cell says "not stated". Sizes are from the pages; "est." marks my arithmetic (params x 4 bytes, fp32).

| candidate | ids | code licence | weights licence | commercial / redistribution | in → out | size | ONNX path | verdict |
|---|---|---|---|---|---|---|---|---|
| SAM 2.1 Small | facebook/sam2.1-hiera-small (repo facebookresearch/sam2) | Apache 2.0 | Apache 2.0 (repo README: checkpoints under Apache 2.0); HF tag `apache-2.0` | yes / yes, Apache notice | points, box, mask → masks + `iou_scores` (multimask 3) | 46 M params (README "Size (M)"); ~184 MB est. | not stated in sam2 README; rascal already exports Tiny, same graph | candidate |
| SAM 2.1 Base+ | facebook/sam2.1-hiera-base-plus | Apache 2.0 | Apache 2.0 (repo README) | yes / yes | as above | 80.8 M; ~323 MB est. | as above | candidate |
| SAM 3 | facebook/sam3 (repo facebookresearch/sam3) | "SAM License" (LICENSE, updated 2025-11-19) | same "SAM License"; HF tag `other`, gated (accept terms, Meta privacy policy) | grant is royalty-free, worldwide, allows use/reproduce/distribute/modify; no commercial ban in the text read; redistribution must ship a copy of the agreement; acceptable-use bans (military, nuclear, espionage, weapons, ITAR), trade-control terms, no reverse engineering, terminates on breach or patent suit against Meta; research publications must acknowledge use | text and/or points/boxes/masks → instance masks + boxes + scores (open-vocabulary, replaces OWLv2+SAM in one model) | 0.9B params (HF); ~3.6 GB est. fp32 | not stated; Transformers `Sam3Model` exists | candidate if the owner accepts the SAM License terms (custom, not OSI) and the size/CPU cost |
| HQ-SAM / HQ-SAM 2 | repo SysCV/sam-hq (`sam-hq2` folder, HQ-SAM 2 beta); PyPI `segment-anything-hq` | Apache-2.0 | not stated separately (README) | code yes; weights not stated | points, box (demo also text via Grounded SAM) → masks (+ two unnamed returns) | not stated for ViT-B/L/H/Tiny; Light HQ-SAM 40.3 MB | `scripts/export_onnx_model.py` exports the mask decoder only | candidate if weights licence is confirmed (ask or assume Apache from repo) |
| Grounding DINO (open) | IDEA-Research/grounding-dino-tiny, -base (HF); repo IDEA-Research/GroundingDINO | Apache-2.0 | HF tag `apache-2.0`; GitHub weights not stated | yes / yes (HF ports) | text (lowercase, end with ".") → boxes + scores only | tiny 0.2B params (HF); ~690 MB est. | not stated | candidate (box stage, OWLv2 replacement) |
| Grounding DINO 1.5 / 1.6 | Grounding-DINO-1.5-API (DeepDataSpace) | Apache 2.0 (API client) | no weights released; API only with token, paid calls | n/a | text → boxes via API | n/a | n/a | out: API only, no weights to ship |
| Florence-2 | microsoft/Florence-2-base, -large | MIT | MIT (HF tag) | yes / yes | prompt task tokens → captions, boxes, phrase grounding; referring segmentation format not stated on card | large 0.77B params (F16 ~1.5 GB est.) | not stated | candidate if trust_remote_code and its seg output are acceptable (needs custom code) |
| MobileSAM | repo ChaoningZhang/MobileSAM | Apache-2.0 | not stated separately | code yes | points, box → masks | 9.66M params | `scripts/export_onnx_model.py` documented | candidate (speed baseline, likely below SAM 2.1 Tiny quality) |
| EfficientSAM | repo yformer/EfficientSAM (S, Ti) | Apache-2.0 | not stated separately | code yes | points, box → masks | not stated | `export_to_onnx.py` in repo; ONNX on HF Space | candidate |
| EVF-SAM / EVF-SAM2 | repo hustvl/EVF-SAM | Apache-2.0 | not stated separately | code yes | text (referring phrase) → mask | 232M to 1.32B params | not stated | candidate if weights licence confirmed and size acceptable |
| OWLv2 large | google/owlv2-large-* | not read today | not read today | not read | text → boxes | not read | as rascal's OWLv2 base | not surveyed; read before adding |
| YOLO-World / Ultralytics | — | AGPL-3.0 (known) | — | — | — | — | — | out: AGPL |

## Sources (read 2026-11-04)

- https://github.com/facebookresearch/sam2
- https://huggingface.co/facebook/sam2.1-hiera-small
- https://huggingface.co/facebook/sam3
- https://github.com/facebookresearch/sam3/blob/main/LICENSE
- https://github.com/SysCV/sam-hq
- https://github.com/IDEA-Research/GroundingDINO
- https://huggingface.co/IDEA-Research/grounding-dino-tiny
- https://github.com/IDEA-Research/Grounding-DINO-1.5-API
- https://huggingface.co/microsoft/Florence-2-large
- https://github.com/ChaoningZhang/MobileSAM
- https://github.com/yformer/EfficientSAM
- https://github.com/hustvl/EVF-SAM

Not read today (row values for these are not from a page): facebook/sam2.1-hiera-base-plus card, IDEA-Research/grounding-dino-base card, microsoft/Florence-2-base card, OWLv2 large card, the Ultralytics licence.

## Notes for the owner

1. SAM 3 is under Meta's custom "SAM License", gated on Hugging Face. The text read has no commercial ban, but shipping exported weights means shipping the agreement with them, passing on its acceptable-use and trade-control terms, and accepting termination on breach. It is not OSI-approved. Decide whether that is acceptable before it enters the lab; at 0.9B params it is also far heavier on CPU than the current pair. **Decided 2026-11-05:** the owner accepted the SAM License for the lab; whether to ship it is decided only if it wins.
2. HQ-SAM, MobileSAM, EfficientSAM, EVF-SAM and the GitHub Grounding DINO checkpoints state an Apache-2.0 code licence but no separate weights licence. Decide whether repo licence covers the weights, or confirm with authors, before shipping any winner. **Decided 2026-11-05 for HQ-SAM:** taken as the repo's Apache-2.0 for the lab; confirmed with the authors before shipping if it wins.
3. Florence-2 requires `trust_remote_code=True`; the lab would run its custom code. Its segmentation output format is not stated on the card. **Superseded by round 2:** Transformers 5.x runs `florence-community/Florence-2-large` natively, and its segmentation output is polygons.
4. Grounding DINO and OWLv2 large give boxes only; they pair with a SAM decoder, they do not replace it.
5. Checkpoint MB figures were not on the pages; measure the files when downloading for the lab.

## Round 2 survey (2026-11-05)

Round 1 assumed every candidate had to run on CPU in rascal's pack. On 2026-11-05 the owner set
two new terms. A production segmenter may be hosted on a GPU (Modal or similar). And the study
should be comprehensive enough to write up. So size and CPU cost no longer exclude a candidate:
the lab measures them instead. This survey covers what round 1 left out.

Sources were read on 2026-11-05. Licence tags, parameter counts and file sizes come from the
Hugging Face API (`/api/models/<id>?blobs=true`).

### Findings that change round 1

- **HQ-SAM 2:** the checkpoints are licensed explicitly. The sam-hq2 README says "The HQ-SAM 2,
  SAM 2 model checkpoints … are licensed under Apache 2.0".
- **HQ-SAM v1** (ViT-B/L/H): the weights carry only Hugging Face `apache-2.0` tags, on
  lkeab/hq-sam and the syscv-community ports, with no licence text. PyPI's
  `segment-anything-hq` 0.3 lists "MIT", while the repo LICENSE is Apache-2.0. The 2026-11-05
  lab decision covers all three sizes.
- **EVF-SAM:** all four weight repos (evf-sam, evf-sam2 and their -multitask variants) state
  `license: apache-2.0`.
- **MobileSAM:** the authors state no weights licence. The Hugging Face mirror
  (dhkim2810/MobileSAM) is tagged `mit`, while the repo LICENSE is Apache-2.0.
- **EfficientSAM:** the weights sit in the repo under its Apache-2.0 LICENSE, with no separate
  statement.
- **Unreliable `apache-2.0` tags:** some tags contradict the stated provenance.
  - SAM3-LiteText keeps SAM 3's image encoder (SAM License), and its text encoder is
    MobileCLIP-based (Apple's weights are research-only).
  - Sa2VA-4B builds on a Qwen research-licensed model.
  - Sa2VA-Qwen3-VL-4B-SAM3 vendors SAM 3.

### Candidates

| candidate | ids | code licence | weights licence | commercial / redistribution | in → out | size | verdict |
|---|---|---|---|---|---|---|---|
| SAM 2.1 Large | facebook/sam2.1-hiera-large | Apache 2.0 | Apache 2.0 (sam2 README) | yes / yes | box, points → 3 masks + IoU | 224M; 898 MB | candidate (outliner) |
| HQ-SAM ViT-L / ViT-H | lkeab/hq-sam `sam_hq_vit_{l,h}.pth`; syscv-community/sam-hq-vit-{large,huge} | Apache-2.0 | HF tag `apache-2.0` only | yes / yes, taking the tags as the licence | box, points → best mask + IoU | L 314M / 1255 MB; H 643M / 2571 MB | candidate (outliner; lab decision as ViT-B) |
| HQ-SAM 2 | lkeab/hq-sam `sam2.1_hq_hiera_large.pt` (code SysCV/sam-hq/sam-hq2) | Apache 2.0 | Apache 2.0 (sam-hq2 README) | yes / yes | box, points → 3 masks + IoU | 225M; 899 MB | candidate (outliner; its package is also named `sam2`, so it needs its own environment) |
| EdgeTAM | facebook/EdgeTAM; yonigozlan/EdgeTAM-hf | Apache 2.0 | Apache 2.0 (README) | yes / yes | box, points → 3 masks + IoU | 14M | candidate (outliner, speed floor) |
| MobileSAM | ChaoningZhang/MobileSAM `weights/mobile_sam.pt` | Apache-2.0 | not stated by the authors; HF mirror tagged `mit` | code yes; weights unclear | box, points → 3 masks + scores | 9.7M; 41 MB | candidate (outliner, speed floor) |
| EfficientSAM Ti / S | yformer/EfficientSAM `weights/` | Apache-2.0 | not stated separately | code yes | box (as two points), points → 3 masks + IoU | Ti 41 MB; S 98 MB | candidate (outliner, speed floor) |
| EfficientTAM | yunyangx/efficient-track-anything | Apache 2.0 | Apache 2.0 (README) | yes / yes | box, points → masks | not stated | candidate, low priority |
| Grounding DINO base | IDEA-Research/grounding-dino-base | Apache-2.0 | HF tag `apache-2.0` | yes / yes | text → boxes + scores | 233M; 933 MB | candidate (box finder) |
| OWLv2 large | google/owlv2-large-patch14-ensemble | Apache-2.0 | HF tag `apache-2.0` | yes / yes ("research output" on the card is not a licence term) | text → boxes + scores | 438M; 1751 MB | candidate (box finder) |
| MM-Grounding-DINO large / LLMDet large | openmmlab-community/mm_grounding_dino_large_all; iSEE-Laboratory/llmdet_large | Apache-2.0 | HF tag `apache-2.0` | yes / yes | text → boxes + scores | 344M | candidate (box finder) |
| Florence-2 large | florence-community/Florence-2-large (native in Transformers 5.x) | MIT | MIT | yes / yes | text → polygons (referring segmentation); box → polygons (region segmentation); no scores | 777M; 1554 MB | candidate (end to end, and as an outliner) |
| EVF-SAM2 / EVF-SAM | YxZhang/evf-sam2, evf-sam (and -multitask) | Apache-2.0 | `apache-2.0` (cards) | yes / yes | text → 1 mask (best of 3); no score; no point input | 898M / 1.32B | candidate (end to end; repo code is CUDA-bound and needs patching for CPU) |
| Sa2VA-Qwen3-VL-4B | ByteDance/Sa2VA-Qwen3-VL-4B | Apache-2.0 | `apache-2.0`; base Qwen3-VL-4B `apache-2.0` | yes / yes | chat text → masks; no scores | 5.1B | candidate (end to end; heavy; `trust_remote_code`; written for Transformers 4.57) |
| SAM3-LiteText | vil-uob/sam3-litetext-l | Apache-2.0 tag | conflicting (see above) | conflicting | text, box → instance masks | 611M | lab only, under the SAM License decision; out for shipping |
| BiRefNet (general, HR, matting) | ZhengPeng7/BiRefNet, _HR, -matting | MIT | MIT | yes / yes | image or box crop → soft foreground map | 221M; 444 MB | candidate (refiner after the outliner) |
| CascadePSP | PyPI `segmentation-refinement` | MIT | not stated separately | code yes | image + coarse mask → refined mask | 259 MB | candidate (refiner) |
| ViTMatte | hustvl/vitmatte-small-composition-1k | MIT | HF tag `apache-2.0` | yes / yes | image + trimap → alpha | 26M; 103 MB | candidate (refiner, trimap from the mask) |
| SegRefiner | MengyuWang826/SegRefiner | Apache-2.0 | not stated | not stated | image + mask → mask | not stated | out: needs Python 3.8, torch 1.12 and CUDA 11.3 |
| EVF-Effi-SAM | (README table) | Apache-2.0 | not stated | not stated | text → mask | 232M to 700M | out: no checkpoint location found |
| Sa2VA (older Qwen2.5 / InternLM variants) | ByteDance/Sa2VA-4B and others | Apache-2.0 | base models research-only or not read | inherits research terms | chat text → masks | 4B+ | out: base-model licences |
| X-SAM | hao9610/X-SAM | Apache-2.0 | `apache-2.0`; Phi-3 not read | not stated | text → masks | not stated | out for this round: a CUDA training stack, no CPU path |
| LISA / LISA++ | xinlai/LISA-7B-v1 | Apache-2.0 | not stated | not stated | text → mask | 7B / 13B | out: weights licence not stated |
| Moondream 2 / 3 | vikhyatk/moondream2; moondream/moondream3-preview | Apache-2.0 / BSL 1.1 | as code | 3: restricted | text → boxes or points; no segmentation | 1.9B / 9B | out: no segmentation (and BSL for 3) |
| YOLOE | THU-MIG/yoloe | AGPL-3.0 | `agpl-3.0` | copyleft | text → boxes + masks | — | out: AGPL |
| DINO-X | DINO-X-API | API client | no weights | n/a | text → masks via API | n/a | out: API only |
| Rex-Omni | IDEA-Research/Rex-Omni | IDEA License 1.0 | research (Qwen2.5-VL base) | research | text → boxes | 4.1B | out: research licence, no masks |
| Grounded-SAM 2 | IDEA-Research/Grounded-SAM-2 | Apache-2.0 | as its parts | — | text → boxes → SAM 2 masks | — | not separate: it is the Grounding DINO + SAM 2.1 recipe the lab already runs |

**Decided 2026-11-05** (owner):
- MobileSAM, EfficientSAM and CascadePSP are cleared for the lab on their repos' licences, the
  HQ-SAM rule. Their authors confirm the weights' licence before any of them ships.
- Candidates that run their own code are run in the lab, each pinned to an exact revision whose
  code was read before it ran, and in its own environment where packages clash. These are
  BiRefNet and Sa2VA (`trust_remote_code`), and EVF-SAM and HQ-SAM 2 (cloned repos, EVF-SAM
  patched to run without CUDA).
- SAM3-LiteText runs in the lab under the SAM License decision.

The run recipe for each candidate (Python package, class, input and output format, and gotchas)
is in the lab's adapters, `rascal-lab/segment-lab/src/segment_lab/adapters/`.

### Sources (read 2026-11-05)

- **Hugging Face cards and API:**
  - SAM 2.1, HQ-SAM and EdgeTAM:
    - facebook/sam2.1-hiera-large
    - lkeab/hq-sam
    - syscv-community/sam-hq-vit-{base,large,huge}
    - facebook/EdgeTAM; yonigozlan/EdgeTAM-hf
  - Box finders:
    - IDEA-Research/grounding-dino-base
    - google/owlv2-large-patch14{,-ensemble,-finetuned}
    - openmmlab-community/mm_grounding_dino_{large,base}_all
    - iSEE-Laboratory/llmdet_{tiny,base,large}
  - End to end:
    - YxZhang/evf-sam, evf-sam2, evf-sam-multitask, evf-sam2-multitask
    - microsoft/Florence-2-large; florence-community/Florence-2-large
    - ByteDance/Sa2VA-4B, Sa2VA-Qwen3-VL-4B, Sa2VA-Qwen3-VL-4B-SAM3
    - vil-uob/sam3-litetext-{l,s0}
  - Small SAMs: dhkim2810/MobileSAM
  - Refiners: ZhengPeng7/BiRefNet, BiRefNet_HR, BiRefNet-matting; hustvl/vitmatte-{base,small}-composition-1k
  - Others read for exclusion or licence checks:
    - hao9610/X-SAM; xinlai/LISA-7B-v1; jameslahm/yoloe-v8l-seg; IDEA-Research/Rex-Omni
    - vikhyatk/moondream2; moondream/moondream3-preview; moondream/moondream3.1-9B-A2B
    - apple/MobileCLIP2-L-14
    - Qwen/Qwen2.5-3B-Instruct; Qwen/Qwen2.5-VL-3B-Instruct; Qwen/Qwen3-VL-4B-Instruct; internlm/internlm2_5-7b-chat
- **GitHub READMEs and LICENSE files:**
  - SAM family: facebookresearch/sam2; SysCV/sam-hq (and sam-hq2/README.md); facebookresearch/EdgeTAM
  - End to end: hustvl/EVF-SAM; magic-research/Sa2VA; wanghao9610/X-SAM; dvlab-research/LISA
  - Small SAMs: ChaoningZhang/MobileSAM; yformer/EfficientSAM; yformer/EfficientTAM
  - Refiners: ZhengPeng7/BiRefNet (issue #285); hustvl/ViTMatte; hkchengrex/CascadePSP; MengyuWang826/SegRefiner
  - Box finders and pipelines: THU-MIG/yoloe; IDEA-Research/Grounded-SAM-2; IDEA-Research/DINO-X-API
- **Transformers v5.22.1:** the model directory and the model docs for florence2, sam_hq, owlv2,
  vitmatte, edgetam, sam3_lite_text and mm-grounding-dino.
- **PyPI:** transformers, segment-anything-hq, segmentation-refinement.
- **Not read**, so not used for any value: the Phi-3, InternLM 2.5, LLaVA/LLaMA and Moondream
  Model License texts, and the onnx-community ports.

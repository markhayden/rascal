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

1. SAM 3 is under Meta's custom "SAM License", gated on Hugging Face. The text read has no commercial ban, but shipping exported weights means shipping the agreement with them, passing on its acceptable-use and trade-control terms, and accepting termination on breach. It is not OSI-approved. Decide whether that is acceptable before it enters the lab; at 0.9B params it is also far heavier on CPU than the current pair.
2. HQ-SAM, MobileSAM, EfficientSAM, EVF-SAM and the GitHub Grounding DINO checkpoints state an Apache-2.0 code licence but no separate weights licence. Decide whether repo licence covers the weights, or confirm with authors, before shipping any winner.
3. Florence-2 requires `trust_remote_code=True`; the lab would run its custom code. Its segmentation output format is not stated on the card.
4. Grounding DINO and OWLv2 large give boxes only; they pair with a SAM decoder, they do not replace it.
5. Checkpoint MB figures were not on the pages; measure the files when downloading for the lab.

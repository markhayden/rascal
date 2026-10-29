# 0018: The segmenter: branch A with OWLv2 and MobileSAM, exported to ONNX

Date: 2026-10-29. Status: accepted.

## Context

ADR 0012 left the Python pack's `segment` tool on two branches: A, if a permissively licensed
public segmenter with an exportable checkpoint and text-prompt input exists, or B, the pack
without `segment` in v1. `segment` (SPEC.md "Operations") takes an image and a text prompt with
`max_instances` and returns one polygon region per instance with a score plus one plane per
instance; `select` keeps or drops instances by index and `remove` grows, closes and feathers
the union. The survey is `tools/python/MODEL.md` (task 4.5): six candidates, with the licence
tag or file read on each primary page. The owner signed decision 5 on 2026-10-29.

## Decision

- **Branch A.** `segment` ships in v1 and success criterion 5 applies.
- **Two models.** OWLv2 base (`google/owlv2-base-patch16`, Apache-2.0) turns the text prompt
  into boxes with scores, several queries per call; MobileSAM (`ChaoningZhang/MobileSAM`,
  Apache-2.0, 9.7 M parameters) turns each box into a plane, one encoder pass per image and one
  decoder pass per box. Instances with scores are what `segment`'s contract needs; one blended
  map (CLIPSeg) is not.
- **ONNX only in the bundle.** Both models are exported to ONNX and the pack runs them on
  `onnxruntime`; PyTorch is a tool of the export, never a dependency of the bundle. The export
  is a change under Apache-2.0 and ships with the licence text and a notice saying so.
- **The drop-in.** If MobileSAM's planes prove loose on the lab corpus, SAM 2.1 Hiera-Tiny
  (Apache-2.0, 39 M) replaces it behind the same interface; if OWLv2's recall disappoints,
  Grounding DINO is the detector to try once it has an export path. Either is a new ADR
  section, not a new ADR.
- **Redistribution terms.** Task 5.2 records the licence tag of each page as read on the day of
  the export in the bundle's notice, so the terms the owner signed on are the terms shipped.

## Alternatives

- **CLIPSeg alone.** One model, simplest pipeline, but one relevance map per prompt: instances
  would be faked from connected components, and thin or precise objects are lost.
- **Grounding DINO + a SAM decoder.** The strongest open-vocabulary detector, but no documented
  ONNX path for the Hub checkpoint; an upgrade candidate, not the first choice.
- **Branch B.** Ships the pack without `segment`; rejected because the models exist on the terms
  the branch asked for.

## Consequences

- Task 5.2 starts: the export scripts, the two models in the pack, `segment` over MCP, the
  bundle gaining the model artifacts' bytes, and the real-model test gated by the bundle.
- The bundle grows by the two exports (tens of megabytes); `scripts/build-bundle.sh` digests
  them like every other file.
- Model weights never enter the repository; `fetch` brings them to the owner's machine and the
  bundle carries them to a user's.

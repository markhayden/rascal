| Round 6 finding | Resolution in draft 7 |
|---|---|
| EVF-SAM2 retry threshold | **Resolved.** Retries preserve applicable settings and exclude `threshold` and `pad`; an explicit retry test is required ([SPEC.md:1101](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:1101), [SPEC.md:1414](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:1414)). |
| BiRefNet post-processing versus lab | **Resolved.** Outward crop rounding, uint8 quantization, bilinear resizing and `>127` thresholding now match the lab ([SPEC.md:1189](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:1189)). |
| Lost instance-parity gate | **Resolved.** Counts, ordering, scores, regions, planes and empty cases are restored; corpus acceptance requires actual recipe execution ([SPEC.md:1232](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:1232)). |

1. **Major — The restored gate requires a positive detection that its fixture and settings cannot produce.** [SPEC.md:1234](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:1234)  
   The gate requires a nonempty instance on `chair-blob.png` using `"chair. table"` at threshold **0.2**. Running the existing OWLv2 export with those exact settings returns **zero detections**: the best table score is **0.024251**, and chair **0.019171**. Tiny, BiRefNet and HQ-SAM therefore fail before their outliners run, regardless of export correctness. This limitation is already documented in [MODEL.md:88](/Users/markhayden/go/src/github.com/markhayden/rascal/tools/python/MODEL.md:88).

   **Fix:** Use a positive fixture verified to produce detections at 0.2 for each model, or freeze model-specific positive-case prompts and thresholds. Keep the negative case separately specified; simply lowering the shared threshold can also make `"giraffe"` detect something.

**Verdict: not ready to build.**

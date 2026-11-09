1. **Major — EVF-SAM2 has no defined checker-retry behavior.** [SPEC.md:1202](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:1202)  
   EVF-SAM2 excludes `threshold`, and applicability validation rejects it. However, M13 requires crop retries to halve the threshold ([SPEC.md:972](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:972)); [retry.rs:135](/Users/markhayden/go/src/github.com/markhayden/rascal/crates/rascal-mask/src/retry.rs:135) unconditionally resolves and forwards it. Unlike `pad`, draft 6 supplies no exception. An EVF-SAM2 run can therefore fail on its first checker-directed retry.

   **Fix:** Halve and forward `threshold` only for models declaring it. EVF-SAM2 retries should reuse its applicable settings, with the specified host-side `crop_pad`. Require an EVF-SAM2 retry test asserting that neither `threshold` nor `pad` reaches the adapter.

2. **Major — BiRefNet’s specified postprocessing differs from the parity reference.** [SPEC.md:1189](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:1189)  
   The draft resizes sigmoid probabilities and thresholds at 0.5. The lab instead computes `(soft * 255).astype(uint8)`, resizes that image, then thresholds at `>127` ([refine.py:172](/Users/markhayden/go/src/github.com/markhayden/rascal-lab/segment-lab/src/segment_lab/adapters/refine.py:172)). Its ONNX adapter inherits that exact path. These operations are not equivalent: even a uniform probability of 0.501 survives the draft’s threshold but disappears in the lab’s quantization. Following the spec therefore introduces mask differences unrelated to export accuracy into the mandatory ≥0.999 gate.

   **Fix:** Specify the lab’s quantization, resize, threshold, and outward crop rounding exactly, and use them in the shipped adapter. Add a boundary-value postprocessing fixture.

3. **Major — The replacement export gate loses the settled instance-parity guarantees.** [SPEC.md:1225](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:1225)  
   Draft 5 checked per-query instance counts, scores, regions, and matched planes with defined tolerances and empty cases. Draft 6 removes those checks and requires only final-core IoU. Incorrect labels, scores, regions, or merged instances can preserve the core while breaking indexed `select` and the segment contract. Also, the cited SAM 3 check does not compute the shipped recipe’s actual core: [check.py:149](/Users/markhayden/go/src/github.com/markhayden/rascal-lab/segment-lab/onnxcheck/sam3/check.py:149) substitutes changed union pixels into the old core.

   **Fix:** Keep these checks as **release-time** acceptance, without restoring local conversion: frozen reference envelopes and planes matching the specified adapter semantics, explicit matching/tolerances/empty cases, and actual recipe execution for the corpus core gate. Treat the existing SAM 3 check as supporting evidence, not completion of that gate.

**Verdict: not ready to build.**

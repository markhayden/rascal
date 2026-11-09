| Positive-case threshold | Tiny instance suite |
|---|---|
| **Resolved:** model-specific threshold selection, separate negative case ([SPEC.md:1235](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:1235)). | **Resolved:** task 14.2 requires frozen references and required-model parity before `m14.1` ([PLAN.md:4141](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4141)). |

Verified using digest-checked shipped OWLv2 and Tiny artifacts on `chair-blob.png`: maximum scores were **chair 0.019171**, **table 0.024251**. Thresholds 0.2, 0.1 and 0.05 produce no detections; **0.01 produces five instances, all with nonempty Tiny masks**. The separate “giraffe” case at 0.2 produces none. This supports the revised rule; full PyTorch, non-square and corpus gates were not run.

**New blocker/major findings: none.**

Spec: **ready to build.**  
Plan: **ready to begin M14.**

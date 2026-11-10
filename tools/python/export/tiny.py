"""Tiny's instance-parity references (SPEC.md "The export gate", check 1; PLAN.md 14.2).

    uv run --project tools/python/export python tools/python/export/export.py \
        --only tiny --references

Tiny's graphs are exported by `export.py` itself; this freezes their references: the maker's
PyTorch models (`google/owlv2-base-patch16`, `facebook/sam2.1-hiera-tiny`) run through the
pack's own adapter (`adapters.Owlv2Sam2`, with the pack's own preprocessing and tokenizer, from
the installed `tiny`), with PyTorch standing in for onnxruntime behind the same graph inputs and
outputs `export.py` exports. Written under `fixtures/models/tiny/` for
`tests/test_model_parity.py`."""

from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path

import numpy as np
import torch
from PIL import Image
from transformers import Owlv2ForObjectDetection, Sam2Model

REPO_ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO_ROOT / "tools/python/src"))

from export import OWL, SAM, OwlGraph, SamDecoderGraph, SamEncoderGraph  # noqa: E402
from rascal_tools import catalogue  # noqa: E402
from rascal_tools.models import adapters, cache  # noqa: E402

FIXTURES = REPO_ROOT / "fixtures"
IMAGES = ["chair-blob.png", "chair-wide.png"]
POSITIVE = "chair. table"
NEGATIVE = "giraffe"
THRESHOLDS = [0.2, 0.1, 0.05, 0.01]


class TorchSession:
    """An onnxruntime session's `run`, answered by the PyTorch module the graph was exported
    from, with the graph's input names in order."""

    def __init__(self, module: torch.nn.Module, inputs: list[str]) -> None:
        self.module = module.eval()
        self.inputs = inputs

    def run(self, _outputs, feeds: dict[str, np.ndarray]) -> list[np.ndarray]:
        with torch.no_grad():
            tensors = [torch.from_numpy(np.ascontiguousarray(feeds[n])) for n in self.inputs]
            out = self.module(*tensors)
        return [o.numpy() for o in out]


def adapter() -> adapters.Owlv2Sam2:
    """The pack's own `tiny` adapter with its three sessions swapped for PyTorch."""
    entry = catalogue.by_id(catalogue.read(), "tiny")
    assert entry is not None
    found = cache.installation("tiny")
    tiny = adapters.BUILDERS[entry.adapter]({n: found.path(n) for n in entry.files})
    assert isinstance(tiny, adapters.Owlv2Sam2), type(tiny)
    owl = Owlv2ForObjectDetection.from_pretrained(OWL).eval()
    sam = Sam2Model.from_pretrained(SAM).eval()
    tiny.detector.session = TorchSession(
        OwlGraph(owl), ["pixel_values", "input_ids", "attention_mask"]
    )
    tiny.segmenter.encoder = TorchSession(SamEncoderGraph(sam), ["pixel_values"])
    tiny.segmenter.decoder = TorchSession(
        SamDecoderGraph(sam), ["feat0", "feat1", "feat2", "points", "point_labels"]
    )
    return tiny


def queries_of(prompt: str) -> list[str]:
    """`segment.queries_of` (the pack's own split), without importing the MCP tool module."""
    seen: list[str] = []
    for part in re.split(r"[.,;]", prompt):
        query = " ".join(part.lower().split())
        if query and query not in seen:
            seen.append(query)
    return seen


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(tiny, pixels: np.ndarray, prompt: str, threshold: float) -> list[adapters.Instance]:
    settings = adapters.Settings(threshold=threshold, max_instances=8)
    return adapters.order(tiny.find(pixels, queries_of(prompt), settings), 8)


def references() -> dict:
    """Freeze the maker's models' answers through the pack's adapter (the spec's check 1)."""
    tiny = adapter()
    images = {n: np.asarray(Image.open(FIXTURES / "images" / n).convert("RGB")) for n in IMAGES}
    positive = None
    for threshold in THRESHOLDS:
        if all(
            any(i.plane.any() for i in run(tiny, px, POSITIVE, threshold)) for px in images.values()
        ):
            positive = threshold
            break
    if positive is None:
        raise SystemExit("no positive-case threshold gives an instance on every image")
    target = FIXTURES / "models" / "tiny"
    target.mkdir(parents=True, exist_ok=True)
    for stale in target.glob("*.png"):
        stale.unlink()
    cases = []
    for case, prompt, threshold in (("positive", POSITIVE, positive), ("negative", NEGATIVE, 0.2)):
        for name, px in images.items():
            found = []
            for n, inst in enumerate(run(tiny, px, prompt, threshold)):
                plane_name = f"{case}-{Path(name).stem}-{n}.png"
                Image.fromarray(inst.plane).convert("1").save(target / plane_name)
                found.append({
                    "query": inst.query, "score": inst.score, "box": list(inst.box),
                    "plane": plane_name, "sha256": sha256(target / plane_name),
                })  # fmt: skip
            cases.append({"case": case, "image": name, "prompt": prompt, "threshold": threshold,
                          "instances": found})  # fmt: skip
    record = {"model": "tiny", "source": {"repos": [OWL, SAM]}, "max_instances": 8,
              "positive_threshold": positive, "cases": cases}  # fmt: skip
    (target / "references.json").write_text(json.dumps(record, indent=2) + "\n")
    print(json.dumps({k: v for k, v in record.items() if k != "cases"}, indent=2))
    for c in cases:
        print(f"  {c['case']} {c['image']}: {len(c['instances'])} instance(s)")
    return record

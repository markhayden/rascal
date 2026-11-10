"""Instance parity (SPEC.md "The export gate", check 1): the shipped adapter over the installed
model against the frozen references the maker's PyTorch model gave (`fixtures/models/<id>/`,
written by `export.py --only <id> --references`). Per case and image: the same number of
instances, matched in `segment`'s order, the same query, each score within 1e-3, each box within
2 px, each plane at IoU ≥ 0.999 (two empty planes count as 1). Runs only with the model
installed, so `RASCAL_TOOLS_REQUIRE_MODELS=<id>` makes it a gate."""

from __future__ import annotations

import json
from pathlib import Path

import numpy as np
import pytest
from PIL import Image

from rascal_tools import catalogue
from rascal_tools.models import adapters, cache

ROOT = Path(__file__).resolve().parents[3]
FIXTURES = ROOT / "fixtures"
MODELS = sorted(
    p.name for p in (FIXTURES / "models").glob("*") if (p / "references.json").is_file()
)


def _queries(prompt: str) -> list[str]:
    parts = prompt.replace(";", ".").replace(",", ".").split(".")
    return [q.strip().lower() for q in parts if q.strip()]


def _iou(a: np.ndarray, b: np.ndarray) -> float:
    union = (a | b).sum()
    return 1.0 if union == 0 else float((a & b).sum() / union)


def test_every_model_with_references_is_catalogued():
    ids = {e.id for e in catalogue.read()}
    assert set(MODELS) <= ids


@pytest.mark.parametrize(
    "model", [pytest.param(m, marks=getattr(pytest.mark, f"model_{m}")) for m in MODELS]
)
def test_instance_parity(model: str):
    entry = catalogue.by_id(catalogue.read(), model)
    try:
        found = cache.installation(model)
    except cache.MissingModel as e:
        pytest.skip(str(e))
    adapter = adapters.BUILDERS[entry.adapter]({n: found.path(n) for n in entry.files})
    refs = json.loads((FIXTURES / "models" / model / "references.json").read_text())
    positives = {c["image"] for c in refs["cases"] if c["case"] == "positive" and c["instances"]}
    assert positives, "the positive case must give an instance on an image"
    for case in refs["cases"]:
        pixels = np.asarray(Image.open(FIXTURES / "images" / case["image"]).convert("RGB"))
        settings = adapters.Settings(threshold=case["threshold"], max_instances=8)
        got = adapters.order(adapter.find(pixels, _queries(case["prompt"]), settings), 8)
        where = f"{model} {case['case']} {case['image']}"
        assert len(got) == len(case["instances"]), where
        for inst, want in zip(got, case["instances"], strict=True):
            assert inst.query == want["query"], where
            assert abs(inst.score - want["score"]) <= 1e-3, (where, inst.score, want["score"])
            assert max(abs(a - b) for a, b in zip(inst.box, want["box"], strict=True)) <= 2, where
            plane = np.asarray(Image.open(FIXTURES / "models" / model / want["plane"])) > 0
            assert _iou(inst.plane, plane) >= 0.999, (where, _iou(inst.plane, plane))

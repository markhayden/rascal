"""The export gate's instance parity (SPEC.md "The export gate", check 1): each catalogued model's
installed graphs, through the pack's own adapter, against the references the maker's PyTorch model
gave through the same adapter, frozen under `fixtures/models/<model>/` by `export.py --only
<model> --references`. Per case: the same number of instances, matched in the adapter's order;
the same query; each score within 1e-3; each box within 2 px; each plane at IoU >= 0.999 (two
empty planes count as 1)."""

from __future__ import annotations

import json
from pathlib import Path

import numpy as np
import pytest
from PIL import Image

from rascal_tools import catalogue
from rascal_tools.models import adapters
from rascal_tools.tools import segment
from tests.parity import requires_model

FIXTURES = Path(__file__).resolve().parents[3] / "fixtures"
MODELS = sorted(
    p.name for p in (FIXTURES / "models").glob("*") if (p / "references.json").is_file()
)


def plane_iou(a: np.ndarray, b: np.ndarray) -> float:
    union = np.count_nonzero(a | b)
    return 1.0 if union == 0 else np.count_nonzero(a & b) / union


@pytest.mark.parametrize(
    "model",
    [
        pytest.param(m, marks=[pytest.mark.segmenter, getattr(pytest.mark, f"model_{m}")])
        for m in MODELS
    ],
)
def test_the_installed_model_matches_its_references(model: str) -> None:
    requires_model(model)
    directory = FIXTURES / "models" / model
    record = json.loads((directory / "references.json").read_text())
    entry = catalogue.by_id(catalogue.read(), model)
    assert entry is not None
    adapter = segment.load(entry)
    positives = 0
    for case in record["cases"]:
        pixels = np.asarray(Image.open(FIXTURES / "images" / case["image"]).convert("RGB"))
        settings = adapters.Settings(
            threshold=case["threshold"], max_instances=record["max_instances"]
        )
        found = adapters.order(
            adapter.find(pixels, segment.queries_of(case["prompt"]), settings),
            record["max_instances"],
        )
        where = f"{model} {case['case']} {case['image']}"
        assert len(found) == len(case["instances"]), f"{where}: {len(found)} instances"
        for n, (got, want) in enumerate(zip(found, case["instances"], strict=True)):
            assert got.query == want["query"], f"{where} #{n}: query"
            assert abs(got.score - want["score"]) <= 1e-3, f"{where} #{n}: score {got.score}"
            assert max(abs(g - w) for g, w in zip(got.box, want["box"], strict=True)) <= 2, (
                f"{where} #{n}: box {got.box}"
            )
            with Image.open(directory / want["plane"]) as im:
                expected = np.asarray(im.convert("L")) > 0
            iou = plane_iou(got.plane, expected)
            assert iou >= 0.999, f"{where} #{n}: plane IoU {iou:.6f}"
            positives += int(case["case"] == "positive" and expected.any())
    assert positives > 0, f"{model}: no non-empty positive instance to compare"

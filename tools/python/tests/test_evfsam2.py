"""The `evfsam2` adapter without its graphs (SPEC.md "The adapters"): the bilinear resize against
`F.interpolate`'s formula, the logit-0 threshold, one instance per query with score 1.0 and the
plane's bounding box as its region, an empty plane giving no instance, threshold and pad never
reaching the model; and the catalogue's applicable parameters. With the model installed
(`model_evfsam2`): the chair fixture and a non-square image, several queries, and the refusal of
every parameter EVF-SAM2 does not use. EVF-SAM2 always answers (even "giraffe" on a blank image
gives a plane), so the query that finds nothing is the fake backend's empty plane."""

from __future__ import annotations

from pathlib import Path

import numpy as np
import pytest
from PIL import Image

from rascal_tools import catalogue
from rascal_tools import harness as harness_module
from rascal_tools.models import adapters
from rascal_tools.tools import segment
from tests.conftest import harness_dict
from tests.parity import requires_model

IMAGES = Path(__file__).resolve().parents[3] / "fixtures" / "images"


class FakeBackend:
    """`EvfSam2`'s backend with a fixed low-resolution mask per query."""

    def __init__(self, masks: dict[str, np.ndarray]) -> None:
        self.masks = masks
        self.embedded: list[tuple[int, ...]] = []
        self.asked: list[str] = []

    def embed(self, image: np.ndarray) -> tuple[np.ndarray, ...]:
        self.embedded.append(image.shape)
        return ("features",)  # type: ignore[return-value]

    def ids(self, query: str) -> np.ndarray:
        self.asked.append(query)
        return np.asarray([[0, len(query), 2]], dtype=np.int64)

    def mask(self, features, ids):  # noqa: ANN001, ANN201
        assert features == ("features",)
        return self.masks[self.asked[-1]]


def low(value: float) -> np.ndarray:
    return np.full((1, 1, 256, 256), value, dtype=np.float32)


def test_bilinear_matches_half_pixel_centres() -> None:
    # 2 -> 4 on one axis: sources -0.25 (clamped to 0), 0.25, 0.75, 1.25 (clamped at the edge)
    x = np.asarray([[0.0, 4.0]], dtype=np.float32)
    assert np.allclose(adapters.bilinear(x, 1, 4)[0], [0.0, 1.0, 3.0, 4.0])
    # identity at the same size; a downscale takes the half-pixel source, no antialiasing
    y = np.arange(16, dtype=np.float32).reshape(4, 4)
    assert np.array_equal(adapters.bilinear(y, 4, 4), y)
    assert np.allclose(adapters.bilinear(y, 2, 2), [[2.5, 4.5], [10.5, 12.5]])


def test_the_input_is_the_image_as_float_planes() -> None:
    pixels = np.zeros((3, 5, 3), dtype=np.uint8)
    pixels[..., 0] = 255
    x = adapters.evfsam2_input(pixels)
    assert x.shape == (1, 3, 3, 5) and x.dtype == np.float32
    assert x[0, 0].min() == 255.0 and x[0, 1:].max() == 0.0


def test_logits_above_zero_are_kept_at_the_image_size() -> None:
    m = low(-1.0)
    m[0, 0, 64:128, 32:96] = 1.0
    plane = adapters.evfsam2_plane(m, (512, 256))
    assert plane.shape == (512, 256)
    assert not adapters.evfsam2_plane(low(0.0), (10, 10)).any()  # exactly 0 is not kept
    ys, xs = np.nonzero(plane)
    assert ys.min() >= 126 and ys.max() <= 258 and xs.min() >= 30 and xs.max() <= 97


def test_one_instance_per_query_scored_one_with_the_planes_box() -> None:
    chair = low(-5.0)
    chair[0, 0, 100:150, 50:100] = 5.0
    backend = FakeBackend({"chair": chair, "giraffe": low(-5.0), "table": low(5.0)})
    adapter = adapters.EvfSam2(backend)
    pixels = np.zeros((256, 256, 3), dtype=np.uint8)
    found = adapter.find(pixels, ["chair", "giraffe", "table"], adapters.Settings())
    assert backend.embedded == [(1, 3, 256, 256)]  # one image pass for all queries
    assert backend.asked == ["chair", "giraffe", "table"]
    assert [i.query for i in found] == [0, 2]  # giraffe's plane is empty: no instance
    assert all(i.score == 1.0 and i.iou is None for i in found)
    assert found[0].box == (50.0, 100.0, 100.0, 150.0)
    assert found[1].box == (0.0, 0.0, 256.0, 256.0)
    assert found[0].plane.sum() == 50 * 50


def test_threshold_and_pad_change_nothing() -> None:
    chair = low(-5.0)
    chair[0, 0, 10:20, 10:20] = 5.0
    pixels = np.zeros((256, 256, 3), dtype=np.uint8)
    answers = []
    for settings in (
        adapters.Settings(),
        adapters.Settings(threshold=1.0, pad=0.5),
        adapters.Settings(threshold=0.0, pad=0.0),
    ):
        found = adapters.EvfSam2(FakeBackend({"chair": chair})).find(pixels, ["chair"], settings)
        answers.append([(i.query, i.score, i.box, i.plane.sum()) for i in found])
    assert answers[0] == answers[1] == answers[2] and len(answers[0]) == 1


def test_the_catalogue_entry_uses_max_instances_alone() -> None:
    entry = catalogue.by_id(catalogue.read(), "evfsam2")
    assert entry is not None
    assert entry.adapter == "evfsam2" and entry.params == ("max_instances",)
    assert entry.licence == "Apache-2.0"
    assert "LICENSE.evf-sam2.txt" in entry.licence_files
    assert adapters.BUILDERS["evfsam2"] is not None


@pytest.mark.parametrize("param", ["threshold", "pad", "outline", "min_cover"])
def test_a_call_setting_an_unused_parameter_is_refused(param: str) -> None:
    pixels = np.zeros((48, 64, 3), dtype=np.uint8)
    harness = harness_module.parse({"_harness": harness_dict(pixels)})
    value = "whole" if param == "outline" else 0.2
    result = segment.run({"prompt": "thing", "model": "evfsam2", param: value}, harness)
    assert result.structuredContent["status"] == "invalid_args"
    assert param in result.structuredContent["summary"]


@pytest.mark.segmenter
@pytest.mark.model_evfsam2
@pytest.mark.parametrize("image", ["chair-blob.png", "chair-wide.png"])
def test_the_installed_model_answers_each_query_with_one_plane(image: str) -> None:
    requires_model("evfsam2")
    pixels = np.asarray(Image.open(IMAGES / image).convert("RGB"))
    height, width = pixels.shape[:2]
    harness = harness_module.parse({"_harness": harness_dict(pixels, f"raw:{width}x{height}:flat")})
    result = segment.run({"prompt": "chair. table; lamp", "model": "evfsam2"}, harness)
    assert result.isError is False, result.content[0]
    envelope = result.structuredContent
    assert envelope["status"] == "ok", envelope
    assert [r["label"] for r in envelope["regions"]] == ["chair", "table", "lamp"]
    assert all(r["score"] == 1.0 for r in envelope["regions"])
    planes = [b for b in result.content if b.type == "image"]
    assert len(planes) == len(envelope["regions"]) == 3


@pytest.mark.segmenter
@pytest.mark.model_evfsam2
def test_max_instances_cuts_the_answer() -> None:
    requires_model("evfsam2")
    pixels = np.asarray(Image.open(IMAGES / "chair-blob.png").convert("RGB"))
    harness = harness_module.parse({"_harness": harness_dict(pixels, "raw:480x320:flat")})
    result = segment.run(
        {"prompt": "chair. table; lamp", "model": "evfsam2", "max_instances": 2}, harness
    )
    assert [r["label"] for r in result.structuredContent["regions"]] == ["chair", "table"]

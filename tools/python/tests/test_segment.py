"""`segment`: the prompt's queries, the box arithmetic and the plane resize without a model;
the argument checks; and, when the artifacts are fetched, the chair fixture through both
models with parity against the stored answer (`MODEL.md` states the tolerance)."""

from __future__ import annotations

from pathlib import Path

import numpy as np
import pytest
from PIL import Image

from rascal_tools.models import cache, owlv2, sam2
from rascal_tools.tools import segment
from tests.conftest import harness_dict
from tests.parity import assert_parity, requires_artifact

FIXTURE = Path(__file__).resolve().parents[3] / "fixtures" / "images" / "chair-blob.png"


def test_the_prompt_splits_into_trimmed_lower_cased_queries() -> None:
    assert segment.queries_of("Chair. side table,  LAMP ; chair.") == [
        "chair",
        "side table",
        "lamp",
    ]
    assert segment.queries_of(" . , ; ") == []


def test_a_box_becomes_a_polygon_of_fractions() -> None:
    assert segment.polygon_of((10.0, 20.0, 110.0, 70.0), 200, 100) == [
        [0.05, 0.2],
        [0.55, 0.2],
        [0.55, 0.7],
        [0.05, 0.7],
    ]


def test_iou_and_suppression() -> None:
    boxes = np.array([[0, 0, 10, 10], [1, 1, 11, 11], [20, 20, 30, 30]], dtype=np.float64)
    overlap = owlv2.iou(boxes[0], boxes[1:])
    assert overlap[0] == pytest.approx(81 / 119)
    assert overlap[1] == 0.0
    assert owlv2.nms(boxes, np.array([0.9, 0.8, 0.7]), 0.5) == [0, 2]
    assert owlv2.nms(boxes, np.array([0.5, 0.9, 0.7]), 0.5) == [1, 2]


def test_owlv2_preprocessing_pads_to_a_square_and_normalises() -> None:
    pixels = np.zeros((30, 50, 3), dtype=np.uint8)
    pixels[:, :, 0] = 255
    values, side = owlv2.preprocess(pixels)
    assert side == 50
    assert values.shape == (1, 3, 960, 960) and values.dtype == np.float32
    # The image occupies the top 30/50 of the square; below it is the grey pad.
    red_top_left = values[0, :, 0, 0]
    assert red_top_left[0] == pytest.approx((1.0 - owlv2.MEAN[0]) / owlv2.STD[0], abs=1e-5)
    grey = values[0, :, 900, 900]
    assert grey[1] == pytest.approx((127 / 255 - owlv2.MEAN[1]) / owlv2.STD[1], abs=1e-5)


def test_sam2_preprocessing_and_the_plane_resize() -> None:
    pixels = np.full((40, 60, 3), 128, dtype=np.uint8)
    values = sam2.preprocess(pixels)
    assert values.shape == (1, 3, 1024, 1024) and values.dtype == np.float32
    logits = np.full((256, 256), -1.0, dtype=np.float32)
    logits[:, 128:] = 1.0
    plane = sam2.plane_of(logits, 60, 40)
    assert plane.shape == (40, 60) and plane.dtype == np.bool_
    assert not plane[:, :28].any() and plane[:, 32:].all()


def test_bad_arguments_are_invalid_args(harness) -> None:
    for arguments in (
        {},
        {"prompt": ""},
        {"prompt": " . "},
        {"prompt": "chair", "max_instances": 0},
        {"prompt": "chair", "max_instances": 99},
        {"prompt": "chair", "threshold": 2},
        {"prompt": ". ".join(str(n) for n in range(20))},
        {"prompt": "x" * 2000},
    ):
        result = segment.run(arguments, harness)
        assert result.isError is True, arguments
        assert result.structuredContent["status"] == "invalid_args", arguments


def test_a_missing_artifact_names_where_it_looked(harness, monkeypatch, tmp_path) -> None:
    monkeypatch.setenv("RASCAL_TOOLS_MODELS", str(tmp_path))
    monkeypatch.setattr(cache, "_verified", {})
    monkeypatch.setattr(segment, "_models", None)
    with pytest.raises(cache.MissingArtifact) as error:
        segment.run({"prompt": "chair"}, harness)
    assert str(tmp_path) in str(error.value) and "rascal-tools fetch" in str(error.value)


def test_the_manifest_holds_the_bounds_rascal_ceilings_allow() -> None:
    manifest = segment.SPEC.manifest()
    assert manifest["returns"]["max_images"] == segment.MAX_INSTANCES
    assert manifest["model_schema"]["required"] == ["prompt"]
    assert manifest["image_args"] == ["image"]


@pytest.mark.segmenter
def test_the_chair_is_found_with_parity() -> None:
    for name in (segment.GRAPH, segment.TOKENIZER, segment.ENCODER, segment.DECODER):
        requires_artifact(name)
    from rascal_tools import harness as harness_module

    pixels = np.asarray(Image.open(FIXTURE).convert("RGB"))
    harness = harness_module.parse({"_harness": harness_dict(pixels, "raw:480x320:flat")})
    # The fixture's chair is a drawn blob: the detector puts its box exactly on it but scores
    # it far below the default threshold (0.07 for "a wooden chair", 0.02 for "chair"), as
    # PyTorch does on the same pixels (MODEL.md); the test lowers the threshold.
    result = segment.run(
        {"prompt": "a wooden chair", "max_instances": 1, "threshold": 0.05}, harness
    )
    assert result.isError is False, result.content[0]
    envelope = result.structuredContent
    assert envelope["status"] == "ok", envelope
    region = envelope["regions"][0]
    assert region["label"] == "a wooden chair" and 0.05 < region["score"] < 0.2
    polygon = region["location"]["polygon"]
    # The chair stands roughly in the middle third of the fixture.
    assert 0.2 < polygon[0][0] < 0.4 and 0.4 < polygon[1][0] < 0.65
    assert_parity("segment", "chair", result, tolerance=0.005)


@pytest.mark.segmenter
def test_several_instances_decode_one_box_per_call() -> None:
    for name in (segment.GRAPH, segment.TOKENIZER, segment.ENCODER, segment.DECODER):
        requires_artifact(name)
    from rascal_tools import harness as harness_module

    pixels = np.asarray(Image.open(FIXTURE).convert("RGB"))
    harness = harness_module.parse({"_harness": harness_dict(pixels, "raw:480x320:flat")})
    # A threshold low enough to keep the whole-frame proposal beside the chair: two boxes,
    # two planes, each decoded in its own call (the graph takes one box at a time).
    result = segment.run(
        {"prompt": "a wooden chair", "max_instances": 4, "threshold": 0.03}, harness
    )
    assert result.isError is False, result.content[0]
    envelope = result.structuredContent
    assert envelope["status"] == "ok", envelope
    assert len(envelope["regions"]) >= 2
    assert len(envelope["images"]) == len(envelope["regions"])
    assert sum(1 for b in result.content if b.type == "image") == len(envelope["regions"])

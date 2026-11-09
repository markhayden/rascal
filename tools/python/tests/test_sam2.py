"""The SAM 2.1 decoder's runtime (`rascal_tools.models.sam2`, MODEL.md): the prompt tensors a
box and its points become, and, with the artifacts fetched, the three candidate outlines."""

from __future__ import annotations

from pathlib import Path

import numpy as np
import pytest
from PIL import Image

from rascal_tools.models import sam2
from tests.parity import requires_model

FIXTURE = Path(__file__).resolve().parents[3] / "fixtures" / "images" / "chair-blob.png"
ENCODER = "sam2.1-hiera-tiny.encoder.onnx"
DECODER = "sam2.1-hiera-tiny.decoder3.onnx"


def test_a_box_alone_is_its_two_corners_in_the_1024_frame() -> None:
    points, labels = sam2.prompt((100.0, 50.0, 300.0, 250.0), [], 400, 500)
    assert points.dtype == np.float32 and points.shape == (1, 2, 2)
    assert labels.dtype == np.int64 and labels.tolist() == [[2, 3]]
    np.testing.assert_allclose(points[0], [[256.0, 102.4], [768.0, 512.0]])


def test_positive_points_follow_the_corners() -> None:
    points, labels = sam2.prompt((0.0, 0.0, 10.0, 10.0), [(5.0, 5.0), (1.0, 9.0)], 10, 10)
    assert points.shape == (1, 4, 2)
    assert labels.tolist() == [[2, 3, 1, 1]]
    np.testing.assert_allclose(points[0, 2:], [[512.0, 512.0], [102.4, 921.6]])


def test_more_than_eight_points_is_refused() -> None:
    with pytest.raises(ValueError, match="at most 8"):
        sam2.prompt((0.0, 0.0, 10.0, 10.0), [(1.0, 1.0)] * 9, 10, 10)


@pytest.mark.segmenter
@pytest.mark.model_tiny
def test_three_outlines_for_a_box_and_for_a_box_with_points() -> None:
    tiny = requires_model("tiny")
    pixels = np.asarray(Image.open(FIXTURE).convert("RGB"))
    height, width = pixels.shape[:2]
    model = sam2.Segmenter(tiny.path(ENCODER), tiny.path(DECODER))
    features = model.embed(pixels)
    box = (130.0, 70.0, 250.0, 250.0)
    for points in ([], [(190.0, 160.0)], [(190.0, 160.0)] * 8):
        planes, ious = model.outlines(features, box, points, width, height)
        assert len(planes) == len(ious) == 3
        assert all(p.shape == (height, width) and p.dtype == bool for p in planes)
        assert all(0.0 <= i <= 1.5 for i in ious)
        # The chair is inside its box: every outline has pixels there.
        assert all(p[70:250, 130:250].any() for p in planes)

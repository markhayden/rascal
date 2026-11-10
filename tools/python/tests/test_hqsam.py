"""HQ-SAM's preprocessing and prompt (`rascal_tools.models.hqsam`), and the `owlv2+hqsam` adapter
with fake graphs: one outline per box, so no `outline` index is reported, and the retry still
runs."""

from __future__ import annotations

import numpy as np
import pytest

from rascal_tools.models import adapters, hqsam


def test_resized_shape_rounds_like_resize_longest_side():
    assert hqsam.resized_shape(1792, 2400) == (765, 1024)
    assert hqsam.resized_shape(640, 480) == (1024, 768)
    assert hqsam.resized_shape(320, 480) == (683, 1024)


def test_preprocess_normalises_and_pads_bottom_right():
    pixels = np.full((320, 480, 3), 200, dtype=np.uint8)
    out = hqsam.preprocess(pixels)
    assert out.shape == (1, 3, 1024, 1024) and out.dtype == np.float32
    want = (200 - hqsam.MEAN) / hqsam.STD
    np.testing.assert_allclose(out[0, :, 0, 0], want, rtol=1e-6)
    np.testing.assert_allclose(out[0, :, 682, 1023], want, rtol=1e-6)
    assert not out[0, :, 683:, :].any()


def test_prompt_puts_points_first_then_the_box_corners():
    coords, labels = hqsam.prompt((10, 20, 110, 220), [(50, 60)], 480, 320)
    sx, sy = 1024 / 480, 683 / 320
    np.testing.assert_allclose(
        coords[0], [[50 * sx, 60 * sy], [10 * sx, 20 * sy], [110 * sx, 220 * sy]], rtol=1e-6
    )
    assert labels.tolist() == [[1.0, 2.0, 3.0]] and labels.dtype == np.float32
    with pytest.raises(ValueError):
        hqsam.prompt((0, 0, 1, 1), [(0, 0)] * 9, 10, 10)


class _Detection:
    def __init__(self, box, score, query=0):
        self.box, self.score, self.query = box, score, query


class _Detector:
    def detect(self, pixels, queries, threshold, max_instances):
        return [_Detection((20.0, 20.0, 80.0, 80.0), 0.5)]


class _OneOutline:
    """A thin outline first (so the retry runs), then a full one."""

    def __init__(self):
        self.calls = []

    def embed(self, pixels):
        return "features"

    def outlines(self, features, box, points, width, height):
        self.calls.append(points)
        plane = np.zeros((height, width), dtype=bool)
        if points:
            plane[25:75, 25:75] = True
        else:
            plane[50, 50] = True
        return [plane], [0.9]


def test_the_hqsam_adapter_reports_no_outline_index_and_retries():
    segmenter = _OneOutline()
    adapter = adapters.Owlv2Sam2(_Detector(), segmenter, three_outlines=False)
    [inst] = adapter.find(np.zeros((100, 100, 3), np.uint8), ["chair"], adapters.Settings())
    assert "outline" not in inst.diagnostics
    assert inst.diagnostics["retried"] == (1.0, "flag")
    assert inst.diagnostics["retry_kept"] == (1.0, "flag")
    assert len(segmenter.calls) == 2 and segmenter.calls[1]

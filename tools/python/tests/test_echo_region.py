from __future__ import annotations

import numpy as np

from rascal_tools.tools import echo_region
from tests.parity import assert_parity


def test_the_box_comes_back_as_one_region_and_one_plane(harness) -> None:
    result = echo_region.run({"bbox": [0.25, 0.5, 0.75, 1.0]}, harness)
    assert result.isError is False
    envelope = result.structuredContent
    assert envelope["status"] == "ok"
    assert envelope["regions"][0]["location"] == {
        "kind": "bbox",
        "frame": "raw:64x48:flat",
        "bbox": [0.25, 0.5, 0.75, 1.0],
    }
    assert envelope["regions"][0]["score"] == 1.0
    assert len(envelope["images"]) == 1
    assert (
        envelope["images"][0]["width"] == 64 and envelope["images"][0]["media_type"] == "image/png"
    )
    assert sum(1 for b in result.content if b.type == "image") == 1
    plane = echo_region.plane_of_box(64, 48, [0.25, 0.5, 0.75, 1.0])
    assert plane[30, 40] == 255 and plane[10, 40] == 0 and plane[30, 10] == 0
    assert int(np.count_nonzero(plane)) == 32 * 24
    assert envelope["measurements"][0] == {
        "name": "plane_px",
        "value": 768.0,
        "unit": "px",
        "frame": "raw:64x48:flat",
    }


def test_a_bad_box_is_invalid_args_and_an_empty_one_not_applicable(harness) -> None:
    bad = echo_region.run({"bbox": [0.9, 0.9, 0.1, 0.1]}, harness)
    assert bad.isError is True
    assert bad.structuredContent["status"] == "invalid_args"
    short = echo_region.run({"bbox": [0.1, 0.1]}, harness)
    assert short.structuredContent["status"] == "invalid_args"
    empty = echo_region.run({"bbox": [2.0, 2.0, 3.0, 3.0]}, harness)
    assert empty.isError is False
    assert empty.structuredContent["status"] == "not_applicable"
    assert "images" not in empty.structuredContent


def test_parity_with_the_stored_answer(harness) -> None:
    result = echo_region.run({"bbox": [0.125, 0.25, 0.5, 0.75]}, harness)
    assert_parity("echo_region", "box", result, tolerance=0.0)

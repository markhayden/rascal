"""The `owlv2+birefnet` adapter without its graphs: the crop, the 8-bit post-processing at its
boundary, the placement in the image and the empty plane for a crop under 2 px (SPEC.md "The
adapters"); and the catalogue's applicable parameters."""

from __future__ import annotations

import numpy as np
import pytest

from rascal_tools import catalogue
from rascal_tools import harness as harness_module
from rascal_tools.models import adapters
from rascal_tools.models.owlv2 import Detection
from rascal_tools.tools import segment
from tests.conftest import harness_dict


class FakeDetector:
    def __init__(self, detections: list[Detection]) -> None:
        self.detections = detections

    def detect(self, pixels, queries, threshold, max_instances):  # noqa: ANN001, ANN201
        return [d for d in self.detections if d.score >= threshold][:max_instances]


def test_the_crop_rounds_outward_and_clamps() -> None:
    assert adapters.crop_of((10.4, 5.6, 20.2, 30.0), 25, 28) == (10, 5, 21, 28)
    assert adapters.crop_of((-3.0, 0.0, 9.0, 9.0), 100, 100) == (0, 0, 9, 9)


def test_the_8_bit_threshold_keeps_only_values_above_127() -> None:
    size = adapters.BIREFNET_SIDE
    # 0.5 and 0.501 both quantise to 127 (truncating) and are dropped; 0.51 is 130 and kept
    for p, kept in ((0.5, False), (0.501, False), (0.51, True), (0.499, False)):
        plane = adapters.birefnet_plane(np.full((size, size), p, dtype=np.float32), (7, 5))
        assert plane.shape == (5, 7)
        assert bool(plane.all()) is kept and bool(plane.any()) is kept, p


def test_the_input_is_normalised_1024_square() -> None:
    crop = np.full((10, 20, 3), 255, dtype=np.uint8)
    x = adapters.birefnet_input(crop)
    assert x.shape == (1, 3, 1024, 1024) and x.dtype == np.float32
    expected = (1.0 - adapters.MEAN_IMAGENET) / adapters.STD_IMAGENET
    assert np.allclose(x[0, :, 0, 0], expected)


def test_the_plane_lands_at_the_crop_and_the_box_is_padded() -> None:
    pixels = np.zeros((100, 200, 3), dtype=np.uint8)
    detector = FakeDetector([Detection((50.0, 20.0, 150.0, 70.0), 0.9, 0)])
    adapter = adapters.Owlv2BiRefNet(detector, lambda x: np.ones((1024, 1024), np.float32))
    [instance] = adapter.find(pixels, ["thing"], adapters.Settings(pad=0.1))
    assert instance.box == (40.0, 15.0, 160.0, 75.0)
    ys, xs = np.nonzero(instance.plane)
    assert (xs.min(), xs.max(), ys.min(), ys.max()) == (40, 159, 15, 74)
    assert instance.iou is None and instance.diagnostics == {}


def test_a_crop_under_2_px_gives_an_empty_plane_without_running_the_model() -> None:
    pixels = np.zeros((50, 50, 3), dtype=np.uint8)
    detector = FakeDetector([Detection((10.0, 10.0, 11.0, 30.0), 0.5, 0)])

    def never(x: np.ndarray) -> np.ndarray:
        raise AssertionError("the model ran on a crop under 2 px")

    [instance] = adapters.Owlv2BiRefNet(detector, never).find(
        pixels, ["thing"], adapters.Settings(pad=0.0)
    )
    assert not instance.plane.any()


def test_birefnet_takes_no_outline_or_min_cover() -> None:
    entry = catalogue.by_id(catalogue.read(), "birefnet")
    assert entry is not None and entry.adapter == "owlv2+birefnet"
    assert set(entry.params) == {"threshold", "max_instances", "pad"}
    assert entry.licence == "MIT" and not entry.default


@pytest.mark.parametrize("param", ["outline", "min_cover"])
def test_a_call_setting_an_unused_parameter_is_refused(param: str) -> None:
    pixels = np.zeros((48, 64, 3), dtype=np.uint8)
    harness = harness_module.parse({"_harness": harness_dict(pixels)})
    value = "whole" if param == "outline" else 0.2
    result = segment.run({"prompt": "thing", "model": "birefnet", param: value}, harness)
    assert result.structuredContent["status"] == "invalid_args"
    assert param in result.structuredContent["summary"]

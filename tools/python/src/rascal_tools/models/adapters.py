"""The adapters behind `segment` (SPEC.md "Choosable segmenters", "The adapters"): one class
per kind of model, one interface. `find` takes the image's pixels, the prompt's queries and the
settings, and returns instances: the query each answers, its score, its box (the region, in
image pixels), its plane and the diagnostics the adapter reports as measurements. `order` puts
every adapter's instances in the one order `segment` answers in: best score first, ties by
query, then by the box's top-left, row-major; cut to `max_instances`."""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Protocol

import numpy as np

from rascal_tools import outline

Box = tuple[float, float, float, float]


@dataclass(frozen=True)
class Settings:
    threshold: float = 0.2
    max_instances: int = 8
    pad: float = 0.05
    outline: str = "whole"
    min_cover: float = 0.2


@dataclass(frozen=True)
class Instance:
    """One instance: `diagnostics` are per-instance measurements as name to (value, unit),
    emitted as `<name>_<n>`; `iou` is the decoder's predicted IoU when the model gives one."""

    query: int
    score: float
    box: Box
    plane: np.ndarray
    diagnostics: dict[str, tuple[float, str]] = field(default_factory=dict)
    iou: float | None = None


class Adapter(Protocol):
    def find(
        self, pixels: np.ndarray, queries: list[str], settings: Settings
    ) -> list[Instance]: ...


def order(instances: list[Instance], max_instances: int) -> list[Instance]:
    ranked = sorted(instances, key=lambda i: (-i.score, i.query, i.box[1], i.box[0]))
    return ranked[:max_instances]


@dataclass(frozen=True)
class Traced:
    """One instance's kept outline: its plane, the padded box it was traced in, the candidate
    index kept, its predicted IoU and whether a retry ran and was kept."""

    plane: np.ndarray
    box: Box
    index: int
    iou: float
    retried: bool
    retry_kept: bool


def trace(
    segmenter: Any,
    features: Any,
    found: Box,
    pad: float,
    rule: str,
    min_cover: float,
    width: int,
    height: int,
) -> Traced:
    """The detector's box to one outline: padded, traced, picked, and retried once with the box
    padded by twice `pad` and a point on each piece when the outline is thin or in pieces."""
    box = outline.pad_box(found, pad, width, height)
    planes, ious = segmenter.outlines(features, box, [], width, height)
    k = outline.pick(planes, ious, box, rule)
    traced = Traced(planes[k], box, k, ious[k], False, False)
    if not traced.plane.any() or not outline.needs_retry(traced.plane, box, min_cover):
        return traced
    wider = outline.pad_box(found, 2 * pad, width, height)
    planes, ious = segmenter.outlines(features, wider, outline.points(traced.plane), width, height)
    k = outline.pick(planes, ious, wider, rule)
    if outline.accept_retry(traced.plane, planes[k], wider):
        return Traced(planes[k], wider, k, ious[k], True, True)
    return Traced(traced.plane, box, traced.index, traced.iou, True, False)


class Owlv2Sam2:
    """OWLv2 proposes boxes per query; SAM 2.1 traces each padded box with M13's pick and
    retry (`rascal_tools.outline`). `three_outlines` False (HQ-SAM, one outline per box) leaves
    the pick with nothing to choose, so the kept outline's index is not reported."""

    def __init__(self, detector: Any, segmenter: Any, three_outlines: bool = True) -> None:
        self.detector = detector
        self.segmenter = segmenter
        self.three_outlines = three_outlines

    def find(self, pixels: np.ndarray, queries: list[str], settings: Settings) -> list[Instance]:
        height, width = pixels.shape[:2]
        detections = self.detector.detect(
            pixels, queries, settings.threshold, settings.max_instances
        )
        if not detections:
            return []
        features = self.segmenter.embed(pixels)
        out = []
        for detection in detections:
            traced = trace(
                self.segmenter,
                features,
                detection.box,
                settings.pad,
                settings.outline,
                settings.min_cover,
                width,
                height,
            )
            diagnostics = {
                "retried": (float(traced.retried), "flag"),
                "retry_kept": (float(traced.retry_kept), "flag"),
            }
            if self.three_outlines:
                diagnostics = {"outline": (float(traced.index), "index"), **diagnostics}
            out.append(
                Instance(
                    query=detection.query,
                    score=detection.score,
                    box=traced.box,
                    plane=traced.plane,
                    diagnostics=diagnostics,
                    iou=traced.iou,
                )
            )
        return out


def _owlv2_sam2(files: dict[str, Path]) -> Adapter:
    from rascal_tools.models.owlv2 import Detector
    from rascal_tools.models.sam2 import Segmenter

    detector = Detector(
        files["owlv2-base-patch16.onnx"], files["owlv2-base-patch16.tokenizer.json"]
    )
    segmenter = Segmenter(
        files["sam2.1-hiera-tiny.encoder.onnx"], files["sam2.1-hiera-tiny.decoder3.onnx"]
    )
    return Owlv2Sam2(detector, segmenter)


def _owlv2_hqsam(files: dict[str, Path]) -> Adapter:
    from rascal_tools.models.hqsam import Segmenter
    from rascal_tools.models.owlv2 import Detector

    detector = Detector(
        files["owlv2-base-patch16.onnx"], files["owlv2-base-patch16.tokenizer.json"]
    )
    segmenter = Segmenter(files["sam-hq-vit-l.encoder.onnx"], files["sam-hq-vit-l.decoder.onnx"])
    return Owlv2Sam2(detector, segmenter, three_outlines=False)


# The adapter for each catalogue `adapter` name, built from the model's files by name.
BUILDERS: dict[str, Callable[[dict[str, Path]], Adapter]] = {
    "owlv2+sam2": _owlv2_sam2,
    "owlv2+hqsam": _owlv2_hqsam,
}

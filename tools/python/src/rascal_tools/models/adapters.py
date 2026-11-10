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


MEAN_IMAGENET = np.array([0.485, 0.456, 0.406], dtype=np.float32)
STD_IMAGENET = np.array([0.229, 0.224, 0.225], dtype=np.float32)
BIREFNET_SIDE = 1024


def crop_of(box: Box, width: int, height: int) -> tuple[int, int, int, int]:
    """The padded box rounded outward to whole pixels and clamped to the image (SPEC.md,
    `owlv2+birefnet`): x0, y0 floored, x1, y1 ceiled."""
    x0, y0 = max(0, int(np.floor(box[0]))), max(0, int(np.floor(box[1])))
    x1, y1 = min(width, int(np.ceil(box[2]))), min(height, int(np.ceil(box[3])))
    return x0, y0, x1, y1


def birefnet_input(crop: np.ndarray) -> np.ndarray:
    """A crop's pixels as BiRefNet's [1, 3, 1024, 1024] input: Pillow bilinear, [0, 1], the
    ImageNet mean and deviation."""
    from PIL import Image

    resized = Image.fromarray(crop).resize(
        (BIREFNET_SIDE, BIREFNET_SIDE), Image.Resampling.BILINEAR
    )
    x = (np.asarray(resized, dtype=np.float32) / 255.0 - MEAN_IMAGENET) / STD_IMAGENET
    return np.ascontiguousarray(x.transpose(2, 0, 1)[None])


def birefnet_plane(probabilities: np.ndarray, size: tuple[int, int]) -> np.ndarray:
    """BiRefNet's [1024, 1024] probabilities back to a crop of `size` (width, height): quantised
    to 8 bits (truncating), resized as an 8-bit image (Pillow bilinear), kept above 127."""
    from PIL import Image

    quantised = (probabilities * 255).astype(np.uint8)
    back = Image.fromarray(quantised).resize(size, Image.Resampling.BILINEAR)
    return np.asarray(back) > 127


class Owlv2BiRefNet:
    """OWLv2 proposes boxes per query; BiRefNet cuts out the most salient object in each padded
    box, one outline per box (no pick, no retry: it takes no points). `soft` maps BiRefNet's
    input to its probabilities, [1024, 1024]: the ONNX graph in the pack, the maker's PyTorch
    model when the export's references are made."""

    def __init__(self, detector: Any, soft: Callable[[np.ndarray], np.ndarray]) -> None:
        self.detector = detector
        self.soft = soft

    def find(self, pixels: np.ndarray, queries: list[str], settings: Settings) -> list[Instance]:
        height, width = pixels.shape[:2]
        detections = self.detector.detect(
            pixels, queries, settings.threshold, settings.max_instances
        )
        out = []
        for detection in detections:
            box = outline.pad_box(detection.box, settings.pad, width, height)
            x0, y0, x1, y1 = crop_of(box, width, height)
            plane = np.zeros((height, width), dtype=bool)
            if x1 - x0 >= 2 and y1 - y0 >= 2:
                probabilities = self.soft(birefnet_input(pixels[y0:y1, x0:x1]))
                plane[y0:y1, x0:x1] = birefnet_plane(probabilities, (x1 - x0, y1 - y0))
            out.append(Instance(detection.query, detection.score, box, plane))
        return out


def onnx_soft(path: Path) -> Callable[[np.ndarray], np.ndarray]:
    """BiRefNet's ONNX graph (`input_image` to `output_image` logits) as probabilities."""
    import onnxruntime as ort

    session = ort.InferenceSession(str(path), providers=["CPUExecutionProvider"])

    def soft(x: np.ndarray) -> np.ndarray:
        logits = session.run(None, {"input_image": x})[0][0, 0]
        return 1.0 / (1.0 + np.exp(-logits))

    return soft


def _owlv2_birefnet(files: dict[str, Path]) -> Adapter:
    from rascal_tools.models.owlv2 import Detector

    detector = Detector(
        files["owlv2-base-patch16.onnx"], files["owlv2-base-patch16.tokenizer.json"]
    )
    return Owlv2BiRefNet(detector, onnx_soft(files["birefnet.onnx"]))


# The adapter for each catalogue `adapter` name, built from the model's files by name.
BUILDERS: dict[str, Callable[[dict[str, Path]], Adapter]] = {
    "owlv2+sam2": _owlv2_sam2,
    "owlv2+birefnet": _owlv2_birefnet,
    "owlv2+hqsam": _owlv2_hqsam,
}

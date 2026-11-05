"""`segment`: a text prompt to instances. OWLv2 proposes boxes for each query in the prompt;
each kept box is padded, SAM 2.1 gives three candidate outlines for it, and the rules in
`rascal_tools.outline` keep one: the whole object by default, retried once with a wider box and
a point on each piece when the outline is thin or in pieces (SPEC.md "Self-correcting remove").
The answer is one polygon region (the padded box) with its score per instance and one plane per
region, in the image's frame, best score first (`MODEL.md`). The models load on the first call
and stay loaded."""

from __future__ import annotations

import re
import threading
from dataclasses import dataclass
from typing import Any

import numpy as np
from mcp import types

from rascal_tools import __version__, outline
from rascal_tools.envelope import Answer
from rascal_tools.harness import Harness
from rascal_tools.manifest import Bounds, Param, Port, ToolSpec
from rascal_tools.models import cache
from rascal_tools.models.owlv2 import Detector
from rascal_tools.models.sam2 import Segmenter

GRAPH = "owlv2-base-patch16.onnx"
TOKENIZER = "owlv2-base-patch16.tokenizer.json"
ENCODER = "sam2.1-hiera-tiny.encoder.onnx"
DECODER = "sam2.1-hiera-tiny.decoder3.onnx"

MAX_INSTANCES = 16
MAX_QUERIES = 16
MAX_PROMPT_BYTES = 1024

SPEC = ToolSpec(
    name="segment",
    version=__version__,
    purpose=(
        "Finds the instances a text prompt names and returns a box region with its score and "
        "a plane for each."
    ),
    when_to_use=(
        "When the region to mask is an object named in words rather than drawn: remove with a "
        "prompt binds its planes through select."
    ),
    when_not_to_use=(
        "When the region is a known box or a drawn stroke; use rect, stroke or a location."
    ),
    output_guidance=(
        "One polygon region (the detector's box, padded) per instance labelled with the query it "
        "answered and scored by the detector, best first, and one plane per region in the "
        "image's frame; select makes a mask of the kept planes. Per instance, outline_<n> is "
        "the candidate kept (0 to 2), retried_<n> whether a thin or broken outline was traced "
        "again and retry_kept_<n> whether the retry replaced it. not_applicable when nothing "
        "scores over the threshold."
    ),
    cost_tier="expensive",
    cost_seconds=4.0,
    inputs=(Port("image", "Image", "The image to search."),),
    outputs=(
        Port("region", "Location", "One box per instance as a polygon, with its score.", many=True),
        Port("plane", "Image", "One instance plane per region, in the image's frame.", many=True),
    ),
    params=(
        Param(
            "prompt",
            "What to find: one or more queries separated by '.', ',' or ';' "
            '("chair. side table").',
            "string",
            required=True,
        ),
        Param(
            "max_instances",
            "The most instances to return across the queries, best score first.",
            "integer",
            default=8,
            min=1,
            max=MAX_INSTANCES,
        ),
        Param(
            "threshold",
            "The detector score a box needs to be kept.",
            "number",
            default=0.2,
            min=0.0,
            max=1.0,
        ),
        Param(
            "pad",
            "How far each found box is widened before the object is traced, as a share of the "
            "box's width and height on each side, so an object's ends are not cut off.",
            "number",
            default=0.05,
            min=0.0,
            max=0.5,
        ),
        Param(
            "outline",
            "Which traced outline to keep: 'whole', the largest that stays inside the box, so "
            "the whole object and not just one part of it; or 'best', the one the model is "
            "most sure of.",
            "choice",
            default="whole",
            choices=outline.RULES,
        ),
        Param(
            "min_cover",
            "An outline covering less than this share of its box, or one in pieces, is traced "
            "once more with a wider box and a point on each piece.",
            "number",
            default=0.2,
            min=0.0,
            max=1.0,
        ),
    ),
    returns=Bounds(
        max_images=MAX_INSTANCES,
        max_long_edge=8192,
        max_image_bytes=8 << 20,
        max_regions=MAX_INSTANCES,
        # `queries`, `mask_iou_predicted`, and per instance kept `plane_px_<n>`, `outline_<n>`,
        # `retried_<n>` and `retry_kept_<n>`.
        max_measurements=4 * MAX_INSTANCES + 2,
    ),
)

_lock = threading.Lock()
_models: tuple[Detector, Segmenter] | None = None


def models() -> tuple[Detector, Segmenter]:
    """The two models, loaded on first use; a missing artifact raises `MissingArtifact`."""
    global _models
    with _lock:
        if _models is None:
            detector = Detector(cache.artifact(GRAPH), cache.artifact(TOKENIZER))
            segmenter = Segmenter(cache.artifact(ENCODER), cache.artifact(DECODER))
            _models = (detector, segmenter)
        return _models


def queries_of(prompt: str) -> list[str]:
    """The prompt split into queries: on '.', ',' and ';', trimmed, lower-cased, empties
    dropped, duplicates kept once in order."""
    seen: list[str] = []
    for part in re.split(r"[.,;]", prompt):
        query = " ".join(part.lower().split())
        if query and query not in seen:
            seen.append(query)
    return seen


def polygon_of(
    box: tuple[float, float, float, float], width: int, height: int
) -> list[list[float]]:
    """The box's corners as fractions of the frame, four decimals."""
    x0, y0, x1, y1 = box
    fx = lambda v: round(v / width, 4)  # noqa: E731
    fy = lambda v: round(v / height, 4)  # noqa: E731
    return [[fx(x0), fy(y0)], [fx(x1), fy(y0)], [fx(x1), fy(y1)], [fx(x0), fy(y1)]]


@dataclass(frozen=True)
class Traced:
    """One instance's kept outline: its plane, the padded box it was traced in, the candidate
    index kept, its predicted IoU and whether a retry ran and was kept."""

    plane: np.ndarray
    box: tuple[float, float, float, float]
    index: int
    iou: float
    retried: bool
    retry_kept: bool


def trace(
    segmenter: Segmenter,
    features: list[np.ndarray],
    found: tuple[float, float, float, float],
    pad: float,
    rule: str,
    min_cover: float,
    image: Any,
) -> Traced:
    """The detector's box to one outline: padded, traced, picked, and retried once with the box
    padded by twice `pad` and a point on each piece when the outline is thin or in pieces."""
    width, height = image.width, image.height
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


def run(arguments: dict[str, Any], harness: Harness) -> types.CallToolResult:
    answer = Answer(SPEC.name, SPEC.version)
    prompt = arguments.get("prompt")
    if not isinstance(prompt, str) or not prompt.strip():
        return answer.invalid_args("prompt must be a non-empty string")
    if len(prompt.encode("utf-8")) > MAX_PROMPT_BYTES:
        return answer.invalid_args(f"prompt is over {MAX_PROMPT_BYTES} bytes")
    queries = queries_of(prompt)
    if not queries:
        return answer.invalid_args("prompt names no query")
    if len(queries) > MAX_QUERIES:
        return answer.invalid_args(f"prompt names {len(queries)} queries; at most {MAX_QUERIES}")
    max_instances = arguments.get("max_instances", 8)
    if not isinstance(max_instances, int) or not 1 <= max_instances <= MAX_INSTANCES:
        return answer.invalid_args(f"max_instances must be an integer in 1..{MAX_INSTANCES}")
    threshold = arguments.get("threshold", 0.2)
    if not isinstance(threshold, int | float) or not 0.0 <= threshold <= 1.0:
        return answer.invalid_args("threshold must be a number in 0..1")
    pad = arguments.get("pad", 0.05)
    if not isinstance(pad, int | float) or not 0.0 <= pad <= 0.5:
        return answer.invalid_args("pad must be a number in 0..0.5")
    rule = arguments.get("outline", "whole")
    if rule not in outline.RULES:
        return answer.invalid_args(f"outline must be one of {', '.join(outline.RULES)}")
    min_cover = arguments.get("min_cover", 0.2)
    if not isinstance(min_cover, int | float) or not 0.0 <= min_cover <= 1.0:
        return answer.invalid_args("min_cover must be a number in 0..1")
    image = harness.image("image")
    detector, segmenter = models()
    detections = detector.detect(image.pixels, queries, float(threshold), max_instances)
    answer.measure("queries", float(len(queries)), "count")
    if not detections:
        return answer.not_applicable(
            f"no instance of {prompt!r} scores over {float(threshold):g} in {image.id}"
        )
    features = segmenter.embed(image.pixels)
    kept = 0
    for n, detection in enumerate(detections):
        traced = trace(
            segmenter, features, detection.box, float(pad), rule, float(min_cover), image
        )
        if not traced.plane.any():
            continue
        answer.region(
            f"seg-{n}",
            {
                "kind": "polygon",
                "frame": image.frame,
                "polygon": polygon_of(traced.box, image.width, image.height),
            },
            queries[detection.query],
            round(detection.score, 3),
        )
        answer.plane(image.frame, traced.plane)
        answer.measure(f"plane_px_{n}", float(np.count_nonzero(traced.plane)), "px", image.frame)
        answer.measure(f"outline_{n}", float(traced.index), "index")
        answer.measure(f"retried_{n}", float(traced.retried), "flag")
        answer.measure(f"retry_kept_{n}", float(traced.retry_kept), "flag")
        kept += 1
        if kept == 1:
            answer.measure("mask_iou_predicted", round(traced.iou, 3), "ratio")
    if kept == 0:
        return answer.not_applicable(f"every instance of {prompt!r} decoded to an empty plane")
    labels = ", ".join(f"{queries[d.query]} {d.score:.2f}" for d in detections)
    return answer.ok(f"{kept} instance{'s' if kept != 1 else ''} of {prompt!r}: {labels}")

"""`segment`: a text prompt to instances. OWLv2 proposes boxes for each query in the prompt,
SAM 2.1 turns each kept box into a plane; the answer is one polygon region (the box) with its
score per instance and one plane per region, in the image's frame, best score first
(`MODEL.md`). The models load on the first call and stay loaded."""

from __future__ import annotations

import re
import threading
from typing import Any

import numpy as np
from mcp import types

from rascal_tools import __version__
from rascal_tools.envelope import Answer
from rascal_tools.harness import Harness
from rascal_tools.manifest import Bounds, Param, Port, ToolSpec
from rascal_tools.models import cache
from rascal_tools.models.owlv2 import Detector
from rascal_tools.models.sam2 import Segmenter

GRAPH = "owlv2-base-patch16.onnx"
TOKENIZER = "owlv2-base-patch16.tokenizer.json"
ENCODER = "sam2.1-hiera-tiny.encoder.onnx"
DECODER = "sam2.1-hiera-tiny.decoder.onnx"

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
        "One polygon region (the detector's box) per instance labelled with the query it "
        "answered and scored by the detector, best first, and one plane per region in the "
        "image's frame; select makes a mask of the kept planes. not_applicable when nothing "
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
    ),
    returns=Bounds(
        max_images=MAX_INSTANCES,
        max_long_edge=8192,
        max_image_bytes=8 << 20,
        max_regions=MAX_INSTANCES,
        max_measurements=4,
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
    image = harness.image("image")
    detector, segmenter = models()
    detections = detector.detect(image.pixels, queries, float(threshold), max_instances)
    answer.measure("queries", float(len(queries)), "count")
    if not detections:
        return answer.not_applicable(
            f"no instance of {prompt!r} scores over {float(threshold):g} in {image.id}"
        )
    features = segmenter.embed(image.pixels)
    planes, ious = segmenter.planes(
        features, [d.box for d in detections], image.width, image.height
    )
    kept = 0
    for n, (detection, plane, predicted_iou) in enumerate(
        zip(detections, planes, ious, strict=True)
    ):
        if not plane.any():
            continue
        answer.region(
            f"seg-{n}",
            {
                "kind": "polygon",
                "frame": image.frame,
                "polygon": polygon_of(detection.box, image.width, image.height),
            },
            queries[detection.query],
            round(detection.score, 3),
        )
        answer.plane(image.frame, plane)
        answer.measure(f"plane_px_{n}", float(np.count_nonzero(plane)), "px", image.frame)
        kept += 1
        if kept == 1:
            answer.measure("mask_iou_predicted", round(predicted_iou, 3), "ratio")
    if kept == 0:
        return answer.not_applicable(f"every instance of {prompt!r} decoded to an empty plane")
    labels = ", ".join(f"{queries[d.query]} {d.score:.2f}" for d in detections)
    return answer.ok(f"{kept} instance{'s' if kept != 1 else ''} of {prompt!r}: {labels}")

"""`echo_region`: the parity tool. It takes a box and returns it as one region and one instance
plane, so the whole path from rascal's call through `_harness` to a stored plane is exercised
without a model, and the parity harness has a tool whose right answer is known."""

from __future__ import annotations

from typing import Any

import numpy as np
from mcp import types

from rascal_tools import __version__
from rascal_tools.envelope import Answer
from rascal_tools.harness import Harness
from rascal_tools.manifest import Bounds, Param, Port, ToolSpec

SPEC = ToolSpec(
    name="echo_region",
    version=__version__,
    purpose="Returns the box it was given as one region and one instance plane: the parity tool.",
    when_to_use=(
        "To prove the pack is reachable and its answers land as artifacts, before trusting a "
        "finder."
    ),
    when_not_to_use="To find anything: it finds nothing, it echoes.",
    output_guidance=(
        "One bbox region labelled echo with score 1, and one plane set inside the box, in the "
        "image's frame; select or rasterize makes a mask."
    ),
    cost_tier="cheap",
    cost_seconds=0.01,
    inputs=(Port("image", "Image", "The image whose frame the box is in."),),
    outputs=(
        Port("region", "Location", "The box as a bbox location, with score 1.", many=True),
        Port("plane", "Image", "The box as an instance plane in the image's frame.", many=True),
    ),
    params=(
        Param(
            "bbox",
            "The box as fractions of the frame: x0, y0, x1, y1.",
            "number_array",
            required=True,
            length=4,
        ),
    ),
    returns=Bounds(
        max_images=1,
        max_long_edge=32_768,
        max_image_bytes=64 << 20,
        max_regions=1,
    ),
)


def plane_of_box(width: int, height: int, bbox: list[float]) -> np.ndarray:
    """The pixels whose centres fall inside the box, by rascal's pixel-centre rule."""
    xs = (np.arange(width) + 0.5) / width
    ys = (np.arange(height) + 0.5) / height
    inside_x = (xs >= bbox[0]) & (xs < bbox[2])
    inside_y = (ys >= bbox[1]) & (ys < bbox[3])
    return (np.outer(inside_y, inside_x) * 255).astype(np.uint8)


def run(arguments: dict[str, Any], harness: Harness) -> types.CallToolResult:
    answer = Answer(SPEC.name, SPEC.version)
    bbox = arguments.get("bbox")
    if not (
        isinstance(bbox, list) and len(bbox) == 4 and all(isinstance(v, int | float) for v in bbox)
    ):
        return answer.invalid_args("bbox must be four numbers")
    bbox = [float(v) for v in bbox]
    if not (bbox[0] < bbox[2] and bbox[1] < bbox[3]):
        return answer.invalid_args(f"bbox needs x0 < x1 and y0 < y1, got {bbox}")
    image = harness.image("image")
    plane = plane_of_box(image.width, image.height, bbox)
    if not plane.any():
        return answer.not_applicable("the box covers no pixel of the frame")
    answer.region("echo-0", {"kind": "bbox", "frame": image.frame, "bbox": bbox}, "echo", 1.0)
    answer.plane(image.frame, plane)
    answer.measure("plane_px", float(np.count_nonzero(plane)), "px", image.frame)
    return answer.ok(f"echoed the box {bbox} as one region and one plane")

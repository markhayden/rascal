"""`echo_env`: a debug tool (served with `--debug-tools`) that names the environment variables
the pack can see, so rascal's allowlist can be proven from the pack's side."""

from __future__ import annotations

import os
from typing import Any

from mcp import types

from rascal_tools import __version__
from rascal_tools.envelope import Answer
from rascal_tools.harness import Harness
from rascal_tools.manifest import Bounds, Port, ToolSpec

SPEC = ToolSpec(
    name="echo_env",
    version=__version__,
    purpose="Names the environment variables the pack process can see; a debug tool.",
    when_to_use="To check what reaches the pack: rascal passes a short allowlist and nothing else.",
    when_not_to_use="In a recipe; it finds nothing.",
    output_guidance=(
        "One region covering the whole image (so the tool has an output), with the variable "
        "names, sorted and comma-separated, in the summary."
    ),
    cost_tier="cheap",
    cost_seconds=0.01,
    inputs=(Port("image", "Image", "Any image; only its frame is used."),),
    outputs=(Port("region", "Location", "The whole image.", many=True),),
    returns=Bounds(max_regions=1, max_summary_bytes=8192),
)


def run(arguments: dict[str, Any], harness: Harness) -> types.CallToolResult:
    answer = Answer(SPEC.name, SPEC.version)
    image = harness.image("image")
    answer.region("env-0", {"kind": "whole_image", "frame": image.frame}, "env", 1.0)
    names = ",".join(sorted(os.environ))
    return answer.ok(f"env: {names}")

"""The answer rascal reads: the envelope as structured content (`schemas/rascal/envelope-v1.json`
without `artifacts`, which rascal fills), a text block with the summary, and one PNG image
block per entry of `images`, in order. Regions and planes land on the tool's `many` ports."""

from __future__ import annotations

import base64
import io
import time
from typing import Any

import numpy as np
from mcp import types
from PIL import Image

PNG = "image/png"


class Answer:
    """Builds one tool result."""

    def __init__(self, tool: str, version: str) -> None:
        self.tool = tool
        self.version = version
        self.started = time.monotonic()
        self.regions: list[dict[str, Any]] = []
        self.images: list[dict[str, Any]] = []
        self.blocks: list[types.ContentBlock] = []
        self.measurements: list[dict[str, Any]] = []

    def region(self, id: str, location: dict[str, Any], label: str, score: float) -> None:
        self.regions.append({"id": id, "location": location, "label": label, "score": float(score)})

    def plane(self, frame: str, plane: np.ndarray) -> None:
        """A binary or 8-bit instance plane, H × W, as one image entry and one PNG block."""
        if plane.dtype == np.bool_:
            plane = plane.astype(np.uint8) * 255
        elif plane.dtype != np.uint8:
            raise ValueError(f"a plane is bool or uint8, not {plane.dtype}; no silent threshold")
        height, width = plane.shape[:2]
        buffer = io.BytesIO()
        Image.fromarray(np.ascontiguousarray(plane), mode="L").save(buffer, format="PNG")
        self.images.append(
            {
                "frame": frame,
                "parent": frame,
                "width": int(width),
                "height": int(height),
                "media_type": PNG,
                "identity": "",
            }
        )
        self.blocks.append(
            types.ImageContent(
                type="image",
                data=base64.b64encode(buffer.getvalue()).decode("ascii"),
                mimeType=PNG,
            )
        )

    def measure(self, name: str, value: float, unit: str, frame: str | None = None) -> None:
        entry: dict[str, Any] = {"name": name, "value": float(value), "unit": unit}
        if frame:
            entry["frame"] = frame
        self.measurements.append(entry)

    def envelope(self, status: str, summary: str, reason: str | None = None) -> dict[str, Any]:
        out: dict[str, Any] = {
            "envelope_version": 1,
            "status": status,
            "summary": summary,
            "provenance": {
                "tool": self.tool,
                "version": self.version,
                "duration_ms": int((time.monotonic() - self.started) * 1000),
            },
        }
        if reason:
            out["reason"] = reason
        if self.measurements:
            out["measurements"] = self.measurements
        if status == "ok":
            if self.regions:
                out["regions"] = self.regions
            if self.images:
                out["images"] = self.images
        return out

    def ok(self, summary: str) -> types.CallToolResult:
        envelope = self.envelope("ok", summary)
        return types.CallToolResult(
            content=[types.TextContent(type="text", text=summary), *self.blocks],
            structuredContent=envelope,
            isError=False,
        )

    def invalid_args(self, reason: str) -> types.CallToolResult:
        summary = f"invalid arguments: {reason}"
        return types.CallToolResult(
            content=[types.TextContent(type="text", text=summary)],
            structuredContent=self.envelope("invalid_args", summary, reason),
            isError=True,
        )

    def not_applicable(self, reason: str) -> types.CallToolResult:
        summary = f"not applicable: {reason}"
        return types.CallToolResult(
            content=[types.TextContent(type="text", text=summary)],
            structuredContent=self.envelope("not_applicable", summary, reason),
            isError=False,
        )

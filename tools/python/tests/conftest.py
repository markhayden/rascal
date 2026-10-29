"""Shared pieces: a synthetic image and the `_harness` rascal would send for it."""

from __future__ import annotations

import base64
import io
from typing import Any

import numpy as np
import pytest
from PIL import Image

from rascal_tools import harness as harness_module


def png_bytes(pixels: np.ndarray) -> bytes:
    buffer = io.BytesIO()
    Image.fromarray(pixels, mode="RGB").save(buffer, format="PNG")
    return buffer.getvalue()


def harness_dict(pixels: np.ndarray, frame: str = "raw:64x48:flat") -> dict[str, Any]:
    height, width = pixels.shape[:2]
    return {
        "context_version": 1,
        "run_id": "test",
        "call_id": "test-1",
        "images": [
            {
                "port": "image",
                "id": "img-1",
                "identity": "sha256:" + "0" * 64,
                "frame": frame,
                "width": width,
                "height": height,
                "base64": base64.b64encode(png_bytes(pixels)).decode("ascii"),
                "record": {},
            }
        ],
        "frames": [
            {
                "frame_version": 1,
                "id": frame,
                "kind": "raw",
                "width": width,
                "height": height,
                "projection": "flat",
                "topology": "bounded",
                "identity": "sha256:" + "0" * 64,
            }
        ],
    }


@pytest.fixture
def pixels() -> np.ndarray:
    rng = np.random.default_rng(7)
    return rng.integers(0, 255, size=(48, 64, 3), dtype=np.uint8)


@pytest.fixture
def harness(pixels: np.ndarray) -> harness_module.Harness:
    return harness_module.parse({"_harness": harness_dict(pixels)})

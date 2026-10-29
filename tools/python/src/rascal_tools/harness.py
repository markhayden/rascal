"""`_harness`: rascal's client-side context on every call (`schemas/rascal/pack-context-v1.json`):
the run and call ids, every referenced image with its record and its pixels by path or as
base64, and the frames those images are in. The pack reads pixels through this module only."""

from __future__ import annotations

import base64
import io
from dataclasses import dataclass
from typing import Any

import numpy as np
from PIL import Image


class HarnessError(ValueError):
    """The call's context is missing or malformed: an invalid-arguments answer."""


@dataclass(frozen=True)
class ContextImage:
    port: str
    id: str
    identity: str
    frame: str
    width: int
    height: int
    pixels: np.ndarray  # H × W × 3, uint8, RGB


@dataclass(frozen=True)
class Harness:
    run_id: str
    call_id: str
    images: tuple[ContextImage, ...]
    frames: dict[str, dict[str, Any]]

    def image(self, port: str) -> ContextImage:
        for image in self.images:
            if image.port == port:
                return image
        raise HarnessError(f"no image bound to port {port}")


def parse(arguments: dict[str, Any]) -> Harness:
    """The harness out of the call's arguments, every image decoded and checked against its
    declared size."""
    raw = arguments.get("_harness")
    if not isinstance(raw, dict):
        raise HarnessError("the call carries no _harness")
    if raw.get("context_version") != 1:
        raise HarnessError(f"context_version {raw.get('context_version')!r} is not 1")
    images = []
    for entry in raw.get("images", []):
        for key in ("port", "id", "identity", "frame", "width", "height"):
            if key not in entry:
                raise HarnessError(f"image {entry.get('id')!r} lacks {key}")
        pixels = _decode(entry)
        height, width = pixels.shape[:2]
        images.append(
            ContextImage(
                port=str(entry["port"]),
                id=str(entry["id"]),
                identity=str(entry["identity"]),
                frame=str(entry["frame"]),
                width=width,
                height=height,
                pixels=pixels,
            )
        )
    frames = {f["id"]: f for f in raw.get("frames", []) if isinstance(f, dict) and "id" in f}
    return Harness(
        run_id=str(raw.get("run_id", "")),
        call_id=str(raw.get("call_id", "")),
        images=tuple(images),
        frames=frames,
    )


def _decode(entry: dict[str, Any]) -> np.ndarray:
    """The image's pixels: the declared size is checked against the header before any sample
    is decoded, so a wrong or oversized image never costs its decode."""
    path = entry.get("path")
    data = entry.get("base64")
    if path:
        source: Any = path
    elif data:
        try:
            source = io.BytesIO(base64.b64decode(data, validate=True))
        except (ValueError, TypeError) as error:
            raise HarnessError(f"image {entry.get('id')}: base64 is malformed ({error})") from error
    else:
        raise HarnessError(f"image {entry.get('id')} has neither path nor base64")
    try:
        with Image.open(source) as im:
            if im.size != (entry["width"], entry["height"]):
                raise HarnessError(
                    f"image {entry.get('id')} is {im.size[0]}x{im.size[1]} but the context "
                    f"says {entry['width']}x{entry['height']}"
                )
            return np.asarray(im.convert("RGB"))
    except (OSError, Image.DecompressionBombError) as error:
        raise HarnessError(f"image {entry.get('id')} cannot be decoded ({error})") from error

"""The parity harness: a tool's answer against stored outputs within a stated tolerance.

A case is a directory `fixtures/parity/<tool>/<case>/` at the repository root (rasters live
under `fixtures/`) holding `envelope.json` (the envelope
without provenance) and `plane_<n>.png` for each image block. `assert_parity` compares the
regions and measurements exactly (as JSON) and each plane as the fraction of pixels that differ,
which must not exceed the tolerance. `UPDATE_PARITY=1` rewrites a case. `requires_artifact`
skips a `segmenter`-marked test when the model artifact is not in the cache, so `pytest` is
green offline without it and checks parity when it is there."""

from __future__ import annotations

import base64
import io
import json
import os
from pathlib import Path

import numpy as np
import pytest
from mcp import types
from PIL import Image

from rascal_tools import fetch

PARITY = Path(__file__).resolve().parents[3] / "fixtures" / "parity"


def requires_artifact(name: str) -> None:
    """Skips the test unless the named artifact is fetched and verified."""
    artifacts = {
        a.name: a for a in fetch.read_artifacts(Path(__file__).parents[1] / "artifacts.toml")
    }
    artifact = artifacts.get(name)
    if artifact is None or not fetch.fetched(artifact):
        pytest.skip(f"artifact {name} is not fetched; run `rascal-tools fetch`")


def _planes(result: types.CallToolResult) -> list[np.ndarray]:
    planes = []
    for block in result.content:
        if isinstance(block, types.ImageContent):
            with Image.open(io.BytesIO(base64.b64decode(block.data))) as im:
                planes.append(np.asarray(im.convert("L")))
    return planes


def assert_parity(tool: str, case: str, result: types.CallToolResult, tolerance: float) -> None:
    directory = PARITY / tool / case
    envelope = dict(result.structuredContent or {})
    envelope.pop("provenance", None)
    planes = _planes(result)
    if os.environ.get("UPDATE_PARITY"):
        directory.mkdir(parents=True, exist_ok=True)
        for stale in directory.glob("plane_*.png"):
            stale.unlink()
        (directory / "envelope.json").write_text(
            json.dumps(envelope, indent=2, sort_keys=True) + "\n"
        )
        for n, plane in enumerate(planes):
            Image.fromarray(plane, mode="L").save(directory / f"plane_{n}.png")
    stored = json.loads((directory / "envelope.json").read_text())
    assert envelope == stored, f"{tool}/{case}: the envelope differs from the stored one"
    stored_planes = sorted(directory.glob("plane_*.png"))
    assert len(planes) == len(stored_planes), (
        f"{tool}/{case}: {len(planes)} planes, stored {len(stored_planes)}"
    )
    for n, (plane, path) in enumerate(zip(planes, stored_planes, strict=True)):
        with Image.open(path) as im:
            expected = np.asarray(im.convert("L"))
        assert plane.shape == expected.shape, f"{tool}/{case}: plane_{n} size"
        differing = float(np.count_nonzero(plane != expected)) / plane.size
        assert differing <= tolerance, (
            f"{tool}/{case}: plane_{n} differs in {differing:.4%} of pixels, over {tolerance:.4%}"
        )

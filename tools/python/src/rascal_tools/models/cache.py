"""Where the model artifacts live and how a tool gets one: `RASCAL_TOOLS_MODELS`, else the
`models/` directory beside the pack's environment (where `rascal tools setup` puts a bundle's
models), else `~/.cache/rascal-tools` (where `rascal-tools fetch` downloads). An artifact is
checked against `artifacts.toml`'s digest once per process before it is used."""

from __future__ import annotations

import os
import sys
from pathlib import Path

from rascal_tools import fetch

ARTIFACTS_TOML = fetch.ARTIFACTS


class MissingArtifact(RuntimeError):
    """A model artifact is absent or does not match its digest."""


def models_dir() -> Path:
    """The directory the artifacts are read from."""
    env = os.environ.get("RASCAL_TOOLS_MODELS")
    if env:
        return Path(env)
    beside = Path(sys.prefix).parent / "models"
    if beside.is_dir():
        return beside
    return fetch.cache_dir()


_verified: dict[str, Path] = {}


def artifact(name: str) -> Path:
    """The verified path of the artifact `name` from `artifacts.toml`."""
    if name in _verified:
        return _verified[name]
    by_name = {a.name: a for a in fetch.read_artifacts(ARTIFACTS_TOML)}
    entry = by_name.get(name)
    if entry is None:
        raise MissingArtifact(f"{name} is not an artifact artifacts.toml names")
    directory = models_dir()
    path = directory / name
    if not path.is_file():
        raise MissingArtifact(
            f"{name} is not in {directory}; run `rascal-tools fetch`, or `rascal tools setup` "
            "with a bundle that holds the models, or set RASCAL_TOOLS_MODELS"
        )
    got = fetch.digest(path)
    if got != entry.sha256:
        raise MissingArtifact(
            f"{path}: sha256 {got} is not artifacts.toml's {entry.sha256}; fetch it again"
        )
    _verified[name] = path
    return path

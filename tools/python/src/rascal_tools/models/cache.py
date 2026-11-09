"""Where the models live and how a tool gets one (SPEC.md "Installing a model"): the models
directory is `RASCAL_TOOLS_MODELS`, else the `models/` directory beside the pack's environment
(where an install keeps them), else `~/.cache/rascal-tools`. Each model is an installation in it
(`rascal_tools.installs`), put there by `rascal-tools get <model>`; a model is read through its
installation, verified once per process."""

from __future__ import annotations

import os
import sys
from pathlib import Path

from rascal_tools import installs


class MissingModel(installs.InstallError):
    """A model is not installed, or its installation is gone or does not verify."""


def models_dir() -> Path:
    """The directory the models are read from."""
    env = os.environ.get("RASCAL_TOOLS_MODELS")
    if env:
        return Path(env)
    beside = Path(sys.prefix).parent / "models"
    if beside.is_dir():
        return beside
    return Path.home() / ".cache" / "rascal-tools"


_models: dict[Path, installs.Models] = {}


def models() -> installs.Models:
    """The models directory's `Models`, one per directory per process (it remembers which
    installations it has verified)."""
    root = models_dir()
    if root not in _models:
        _models[root] = installs.Models(root)
    return _models[root]


def installation(model: str, identity: str | None = None) -> installs.Installation:
    """The model's installation: `identity` when named, else the current one."""
    try:
        return models().resolve(model, identity)
    except installs.InstallError as e:
        raise MissingModel(
            f"{e}; run `rascal-tools get {model}` (or `rascal tools get {model}`), or set "
            "RASCAL_TOOLS_MODELS"
        ) from e

"""The artifacts manifest, `artifacts.toml`: every file a catalogued model needs (graphs,
tokenizers, licence texts) with its release URL, SHA-256, size and licence. The installer
(`rascal_tools.installer`) downloads and checks them; the catalogue names them by `name`."""

from __future__ import annotations

import tomllib
from dataclasses import dataclass
from pathlib import Path

# The manifest ships inside the package, so an installed pack reads the same file.
ARTIFACTS = Path(__file__).resolve().parent / "artifacts.toml"


class ArtifactsError(RuntimeError):
    """The manifest does not read or an entry is incomplete."""


@dataclass(frozen=True)
class Artifact:
    name: str
    url: str
    sha256: str
    licence: str
    size: int


def read(path: Path = ARTIFACTS) -> list[Artifact]:
    try:
        data = tomllib.loads(path.read_text())
    except (OSError, tomllib.TOMLDecodeError) as e:
        raise ArtifactsError(f"{path}: {e}") from e
    out = []
    for entry in data.get("artifact", []):
        for key in ("name", "url", "sha256", "licence", "size"):
            if key not in entry:
                raise ArtifactsError(f"{path}: an artifact lacks {key}")
        if "/" in entry["name"] or entry["name"] in ("", ".", ".."):
            raise ArtifactsError(
                f"{path}: artifact name {entry['name']!r} is not a plain file name"
            )
        out.append(
            Artifact(
                entry["name"],
                entry["url"],
                entry["sha256"].lower(),
                entry["licence"],
                int(entry["size"]),
            )
        )
    return out

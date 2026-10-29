"""`rascal-tools fetch`: the one network step. Downloads every artifact `artifacts.toml` names
into the cache and verifies its SHA-256; a digest that does not match is refused and the file
removed. The cache is `RASCAL_TOOLS_MODELS` or `~/.cache/rascal-tools`. Launch never fetches."""

from __future__ import annotations

import hashlib
import os
import shutil
import tomllib
import urllib.request
from dataclasses import dataclass
from pathlib import Path

# The artifacts manifest ships inside the package, so an installed pack reads the same file.
ARTIFACTS = Path(__file__).resolve().parent / "artifacts.toml"


class FetchError(RuntimeError):
    pass


@dataclass(frozen=True)
class Artifact:
    name: str
    url: str
    sha256: str
    licence: str


def cache_dir() -> Path:
    env = os.environ.get("RASCAL_TOOLS_MODELS")
    return Path(env) if env else Path.home() / ".cache" / "rascal-tools"


def read_artifacts(path: Path) -> list[Artifact]:
    data = tomllib.loads(path.read_text())
    out = []
    for entry in data.get("artifact", []):
        for key in ("name", "url", "sha256", "licence"):
            if key not in entry:
                raise FetchError(f"{path}: an artifact lacks {key}")
        if "/" in entry["name"] or entry["name"] in ("", ".", ".."):
            raise FetchError(f"{path}: artifact name {entry['name']!r} is not a plain file name")
        out.append(Artifact(entry["name"], entry["url"], entry["sha256"].lower(), entry["licence"]))
    return out


def digest(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def fetch_one(artifact: Artifact, cache: Path) -> Path:
    """The artifact in the cache, downloaded unless already present with the right digest."""
    cache.mkdir(parents=True, exist_ok=True)
    target = cache / artifact.name
    if target.is_file() and digest(target) == artifact.sha256:
        return target
    if not artifact.url.startswith(("https://", "http://")):
        raise FetchError(f"{artifact.name}: url {artifact.url!r} is not http(s)")
    partial = cache / (artifact.name + ".partial")
    try:
        with urllib.request.urlopen(artifact.url) as response, partial.open("wb") as out:  # noqa: S310
            shutil.copyfileobj(response, out)
        got = digest(partial)
        if got != artifact.sha256:
            raise FetchError(
                f"{artifact.name}: downloaded sha256 {got} does not match artifacts.toml's "
                f"{artifact.sha256}; the file was discarded"
            )
        partial.replace(target)
    finally:
        partial.unlink(missing_ok=True)
    return target


def fetched(artifact: Artifact, cache: Path | None = None) -> bool:
    """Whether the artifact is in the cache with the right digest; what the parity harness
    asks before a `segmenter` test runs."""
    target = (cache or cache_dir()) / artifact.name
    return target.is_file() and digest(target) == artifact.sha256


def main(artifacts: Path, cache: Path | None = None) -> int:
    cache = cache or cache_dir()
    try:
        for artifact in read_artifacts(artifacts):
            path = fetch_one(artifact, cache)
            print(f"{artifact.name}: {path} ({artifact.licence})")
    except (FetchError, OSError) as e:
        print(f"rascal-tools fetch: {e}", file=os.sys.stderr)
        return 3
    return 0

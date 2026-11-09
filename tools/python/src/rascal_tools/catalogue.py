"""The segmenter catalogue (`segmenters.toml`, SPEC.md "Choosable segmenters"): which models
`segment` can run, the adapter that runs each, the parameters each uses, and the files each needs
by reference to `artifacts.toml`. Read and checked once; a broken catalogue is a packaging error,
so it raises rather than answering."""

from __future__ import annotations

import platform
import re
import tomllib
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from rascal_tools import fetch

CATALOGUE = Path(__file__).resolve().parent / "segmenters.toml"

# The `segment` parameters a model may use; `prompt` (and `model`) are always taken.
SEGMENT_PARAMS = ("threshold", "max_instances", "pad", "outline", "min_cover")
PLATFORMS = ("macos-arm64", "linux-x86_64")
ADAPTERS = ("owlv2+sam2",)
_ID = re.compile(r"^[a-z0-9][a-z0-9-]*$")


class CatalogueError(RuntimeError):
    """The catalogue does not read or does not hold together."""


@dataclass(frozen=True)
class Entry:
    id: str
    title: str
    adapter: str
    params: tuple[str, ...]
    default: bool
    licence: str
    licence_files: tuple[str, ...]
    files: tuple[str, ...]
    download_mb: int
    installed_mb: int
    platforms: tuple[str, ...]
    lab: dict[str, Any] = field(default_factory=dict)


def read(path: Path = CATALOGUE, artifacts: Path = fetch.ARTIFACTS) -> list[Entry]:
    """Every entry of the catalogue at `path`, checked against `artifacts`."""
    try:
        data = tomllib.loads(path.read_text())
    except (OSError, tomllib.TOMLDecodeError) as e:
        raise CatalogueError(f"{path}: {e}") from e
    known = {a.name for a in fetch.read_artifacts(artifacts)}
    entries: list[Entry] = []
    for raw in data.get("segmenter", []):
        entries.append(_entry(path, raw, known))
    ids = [e.id for e in entries]
    if len(set(ids)) != len(ids):
        raise CatalogueError(f"{path}: an id appears twice in {ids}")
    defaults = [e.id for e in entries if e.default]
    if len(defaults) != 1:
        raise CatalogueError(f"{path}: exactly one entry must be the default, not {defaults}")
    return entries


def _entry(path: Path, raw: dict[str, Any], known: set[str]) -> Entry:
    def need(key: str) -> Any:
        if key not in raw:
            raise CatalogueError(f"{path}: an entry lacks {key}")
        return raw[key]

    ident = need("id")
    if not isinstance(ident, str) or not _ID.match(ident):
        raise CatalogueError(f"{path}: id {ident!r} is not lower-case letters, digits and '-'")
    where = f"{path}: {ident}"
    adapter = need("adapter")
    if adapter not in ADAPTERS:
        raise CatalogueError(f"{where}: no adapter {adapter!r} (have {', '.join(ADAPTERS)})")
    params = tuple(need("params"))
    unknown = [p for p in params if p not in SEGMENT_PARAMS]
    if unknown:
        raise CatalogueError(f"{where}: {unknown} are not segment parameters")
    files = tuple(need("files"))
    licence_files = tuple(raw.get("licence_files", ()))
    missing = [f for f in files + licence_files if f not in known]
    if missing:
        raise CatalogueError(f"{where}: {missing} are not in artifacts.toml")
    if not files:
        raise CatalogueError(f"{where}: no files")
    platforms = tuple(need("platforms"))
    if not platforms or any(p not in PLATFORMS for p in platforms):
        raise CatalogueError(f"{where}: platforms {list(platforms)} are not among {PLATFORMS}")
    return Entry(
        id=ident,
        title=str(need("title")),
        adapter=adapter,
        params=params,
        default=bool(raw.get("default", False)),
        licence=str(need("licence")),
        licence_files=licence_files,
        files=files,
        download_mb=int(need("download_mb")),
        installed_mb=int(need("installed_mb")),
        platforms=platforms,
        lab=dict(raw.get("lab", {})),
    )


def default(entries: list[Entry]) -> Entry:
    return next(e for e in entries if e.default)


def by_id(entries: list[Entry], ident: str) -> Entry | None:
    return next((e for e in entries if e.id == ident), None)


def current_platform() -> str | None:
    """This machine's platform as the catalogue names it, or None when it names none."""
    system, machine = platform.system(), platform.machine().lower()
    if system == "Darwin" and machine in ("arm64", "aarch64"):
        return "macos-arm64"
    if system == "Linux" and machine in ("x86_64", "amd64"):
        return "linux-x86_64"
    return None

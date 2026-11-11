"""The default segmenter (SPEC.md "Choosable segmenters"): the catalogue ranks its models best
first, and a call that names no model runs the best-ranked one that is installed, complete and
runs on this platform; with none installed, the best-ranked one that runs here, so it is the
one reported missing. A model a call names is run as named, never swapped for another."""

from __future__ import annotations

import hashlib
from pathlib import Path

import pytest

from rascal_tools import catalogue, installer, installs
from rascal_tools.tools import segment

# The owner's preference, best first, written to the file in another order so the rank (and not
# the file's order) is what decides.
RANKED = ("sam3", "birefnet", "hqsam", "evfsam2", "tiny")
FILE_ORDER = ("tiny", "birefnet", "sam3", "evfsam2", "hqsam")
IDENTITY = "abc"


def _artifact(name: str) -> str:
    return (
        f'[[artifact]]\nname = "{name}"\nurl = "https://example.invalid/{name}"\n'
        f'sha256 = "{hashlib.sha256(name.encode()).hexdigest()}"\nsize = {len(name)}\n'
        'licence = "Apache-2.0"\n\n'
    )


def _segmenter(ident: str) -> str:
    return (
        f'[[segmenter]]\nid = "{ident}"\ntitle = "{ident.upper()}"\nadapter = "sam3"\n'
        f'params = []\nrank = {RANKED.index(ident) + 1}\nlicence = "Apache-2.0"\n'
        f'licence_files = ["LICENSE.{ident}"]\nfiles = ["{ident}.onnx", "{ident}.json"]\n'
        'download_mb = 1\ninstalled_mb = 1\nplatforms = ["macos-arm64", "linux-x86_64"]\n\n'
    )


@pytest.fixture
def ranked(tmp_path: Path):
    """The five-model catalogue, an empty models directory and `install(id, complete=True)`,
    which lays an installation out as the installer does (`<id>/<identity>/` and `current`),
    leaving one of its files out when not `complete`."""
    artifacts = tmp_path / "artifacts.toml"
    artifacts.write_text(
        "".join(
            _artifact(f"{i}.onnx") + _artifact(f"{i}.json") + _artifact(f"LICENSE.{i}")
            for i in RANKED
        )  # fmt: skip
    )
    cat = tmp_path / "segmenters.toml"
    cat.write_text("".join(_segmenter(i) for i in FILE_ORDER))
    entries = catalogue.read(cat, artifacts)
    models = installs.Models(tmp_path / "models")

    def install(ident: str, complete: bool = True) -> None:
        directory = models.root / ident / IDENTITY
        directory.mkdir(parents=True)
        names = [f"{ident}.onnx", f"{ident}.json", f"LICENSE.{ident}"]
        for name in names[: None if complete else -1]:
            (directory / name).write_bytes(b"weights")
        (models.root / ident / "current").write_text(IDENTITY + "\n")

    return entries, models, install


def _default(entries, models, platform=lambda e: True) -> str:
    return installer.default(models, entries, platform=platform).id


def test_nothing_installed_gives_the_best_ranked_model_not_installed(ranked) -> None:
    entries, models, _ = ranked
    assert _default(entries, models) == "sam3"
    listed = {m["id"]: m for m in installer.listing(models, entries, platform=lambda e: True)}
    assert listed["sam3"]["default"] is True and listed["sam3"]["state"] == "not_installed"
    assert [m["id"] for m in listed.values() if m["default"]] == ["sam3"]


def test_only_tiny_installed_gives_tiny(ranked) -> None:
    entries, models, install = ranked
    install("tiny")
    assert _default(entries, models) == "tiny"


def test_the_best_ranked_installed_model_wins(ranked) -> None:
    entries, models, install = ranked
    install("tiny")
    install("birefnet")
    assert _default(entries, models) == "birefnet"
    listed = {m["id"]: m for m in installer.listing(models, entries, platform=lambda e: True)}
    assert listed["birefnet"]["default"] is True and listed["tiny"]["default"] is False


def test_an_incomplete_installation_is_skipped(ranked) -> None:
    entries, models, install = ranked
    install("sam3", complete=False)
    install("tiny")
    assert _default(entries, models) == "tiny"
    listed = {m["id"]: m for m in installer.listing(models, entries, platform=lambda e: True)}
    assert listed["sam3"]["state"] == "not_installed"


def test_a_model_that_does_not_run_here_is_skipped(ranked) -> None:
    entries, models, install = ranked
    elsewhere = lambda e: e.id != "sam3"  # noqa: E731
    assert _default(entries, models, elsewhere) == "birefnet"
    install("sam3")
    install("tiny")
    assert _default(entries, models, elsewhere) == "tiny"


def test_a_call_naming_no_model_runs_the_default_and_a_named_one_runs_as_named(
    ranked, harness, monkeypatch
) -> None:
    entries, models, install = ranked
    install("tiny")
    loaded: list[str] = []

    def load(entry, installation=None):
        loaded.append(entry.id)
        raise segment.cache.MissingModel(f"{entry.id} is not installed")

    monkeypatch.setattr(segment, "ENTRIES", entries)
    monkeypatch.setattr(segment.cache, "models", lambda: models)
    monkeypatch.setattr(installer, "platform_ok", lambda e: True)
    monkeypatch.setattr(segment, "load", load)
    with pytest.raises(segment.cache.MissingModel):
        segment.run({"prompt": "chair"}, harness)
    # A named model that is not installed is that model, missing: never the installed default.
    with pytest.raises(segment.cache.MissingModel, match="birefnet"):
        segment.run({"prompt": "chair", "model": "birefnet"}, harness)
    assert loaded == ["tiny", "birefnet"]

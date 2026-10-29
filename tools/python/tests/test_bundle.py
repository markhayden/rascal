"""The bundle a build made (`scripts/build-bundle.sh`), when `RASCAL_BUNDLE` names one: the
manifest lists every file with its digest and nothing else; the wheel is the pack at the
manifest's version; the lock's wheels are the wheelhouse's, no more and no fewer; the
provisioning project pins the wheel by relative path for one platform. Skipped otherwise:
building needs the network, and this suite does not."""

from __future__ import annotations

import hashlib
import os
import re
import tomllib
from pathlib import Path

import pytest

BUNDLE = os.environ.get("RASCAL_BUNDLE")
requires_bundle = pytest.mark.skipif(not BUNDLE, reason="RASCAL_BUNDLE is not set")

IGNORED = {".venv", ".cache"}


def manifest() -> tuple[Path, dict]:
    root = Path(BUNDLE or "").resolve()
    return root, tomllib.loads((root / "bundle.toml").read_text())


@requires_bundle
def test_the_manifest_lists_every_file_with_its_digest_and_nothing_else() -> None:
    root, bundle = manifest()
    assert bundle["bundle_version"] == 1
    assert bundle["pack"] == "rascal-tools"
    assert bundle["platform"] in {"macos-arm64", "linux-x86_64"}
    present = {
        p.relative_to(root).as_posix()
        for p in root.rglob("*")
        if p.is_file()
        and p.name != "bundle.toml"
        and not (set(p.relative_to(root).parts) & IGNORED)
    }
    listed = bundle["files"]
    assert set(listed) == present
    for rel, digest in listed.items():
        assert digest == "sha256:" + hashlib.sha256((root / rel).read_bytes()).hexdigest(), rel
    for required in (
        "pyproject.toml",
        "uv.lock",
        "MODEL.md",
        "artifacts.toml",
        "NOTICE",
        "LICENSE",
        bundle["wheel"],
    ):
        assert required in listed
    # The models are in, every one of them, or none is.
    named = [a["name"] for a in tomllib.loads((root / "artifacts.toml").read_text())["artifact"]]
    bundled = {rel for rel in listed if rel.startswith("models/")}
    if bundle["models"]:
        assert bundled == {f"models/{name}" for name in named}
    else:
        assert not bundled


@requires_bundle
def test_the_wheel_the_lock_and_the_wheelhouse_agree() -> None:
    root, bundle = manifest()
    wheel = Path(bundle["wheel"])
    assert wheel.parts[0] == "wheel"
    name, version = wheel.name.split("-")[:2]
    assert name == "rascal_tools" and version == bundle["pack_version"]
    project = tomllib.loads((root / "pyproject.toml").read_text())
    assert project["tool"]["uv"]["sources"]["rascal-tools"]["path"] == bundle["wheel"]
    assert project["project"]["requires-python"] == f"=={bundle['python']}.*"
    assert len(project["tool"]["uv"]["environments"]) == 1
    lock = (root / "uv.lock").read_text()
    locked = set(re.findall(r'\{ path = "([^"]+\.whl)" \}', lock))
    wheelhouse = {p.name for p in (root / "wheelhouse").iterdir() if p.suffix == ".whl"}
    # The pack wheel is a path source; every dependency wheel is a wheelhouse path.
    assert locked == wheelhouse | {bundle["wheel"]}, locked ^ (wheelhouse | {bundle["wheel"]})
    assert 'registry = "wheelhouse"' in lock
    assert "https://" not in lock, "the lock reaches for an index"

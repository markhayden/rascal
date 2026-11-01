"""Where the pack reads its model files, as rascal's readiness looks for them (PLAN.md 12.2):
both suites check the one table in `fixtures/readiness/models-dir.json`, so `rascal describe`
names the directory the pack would read."""

from __future__ import annotations

import json
import sys
from pathlib import Path

import pytest

from rascal_tools.models import cache

TABLE = Path(__file__).resolve().parents[3] / "fixtures" / "readiness" / "models-dir.json"
CASES = json.loads(TABLE.read_text())["cases"]


@pytest.mark.parametrize("case", CASES, ids=lambda c: f"env={c['env']}-beside={c['beside']}")
def test_the_models_directory_is_the_one_readiness_names(
    case: dict, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    install = tmp_path / "install"
    (install / ".venv").mkdir(parents=True)
    if case["beside"]:
        (install / "models").mkdir()
    home = tmp_path / "home"
    home.mkdir()
    variable = tmp_path / "from-env"
    monkeypatch.setattr(sys, "prefix", str(install / ".venv"))
    monkeypatch.setenv("HOME", str(home))
    if case["env"] is None:
        monkeypatch.delenv("RASCAL_TOOLS_MODELS", raising=False)
    elif case["env"] == "":
        monkeypatch.setenv("RASCAL_TOOLS_MODELS", "")
    else:
        monkeypatch.setenv("RASCAL_TOOLS_MODELS", str(variable))
    expected = {
        "env": variable,
        "beside": install / "models",
        "cache": home / ".cache" / "rascal-tools",
    }[case["expect"]]
    assert cache.models_dir() == expected

"""`RASCAL_TOOLS_REQUIRE_MODELS=<id>[,<id>]` (PLAN.md M14, "Model-required runs"): select exactly
the tests marked `model_<id>` for those models, and fail rather than skip. A selected test that
skips (its model absent) fails the run, and a selection with no test fails too, so a gate that
names a model cannot pass without running that model's tests."""

from __future__ import annotations

import os

import pytest

VARIABLE = "RASCAL_TOOLS_REQUIRE_MODELS"


def required() -> list[str]:
    return [m.strip() for m in os.environ.get(VARIABLE, "").split(",") if m.strip()]


def pytest_collection_modifyitems(config: pytest.Config, items: list[pytest.Item]) -> None:
    wanted = required()
    if not wanted:
        return
    keep = [i for i in items if any(i.get_closest_marker(f"model_{m}") for m in wanted)]
    dropped = [i for i in items if i not in keep]
    if dropped:
        config.hook.pytest_deselected(items=dropped)
    items[:] = keep
    if not keep:
        raise pytest.UsageError(f"{VARIABLE}={','.join(wanted)}: no test is marked for them")


@pytest.hookimpl(hookwrapper=True)
def pytest_runtest_makereport(item: pytest.Item, call: pytest.CallInfo):
    outcome = yield
    report = outcome.get_result()
    if required() and report.skipped:
        report.outcome = "failed"
        report.longrepr = f"skipped under {VARIABLE}, which requires it to run: {report.longrepr}"

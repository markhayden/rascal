"""The model-required switch (`tests/require_models.py`): a selected model's skipped test fails,
other tests are left out, and naming a model no test is marked for fails the run."""

from __future__ import annotations

import pytest

SUITE = """
import pytest

@pytest.mark.model_tiny
def test_tiny_runs():
    pass

@pytest.mark.model_tiny
def test_tiny_without_its_files():
    pytest.skip("not installed")

def test_unmarked():
    pass
"""


def test_without_the_variable_skips_stay_skips(pytester: pytest.Pytester, monkeypatch) -> None:
    monkeypatch.delenv("RASCAL_TOOLS_REQUIRE_MODELS", raising=False)
    pytester.makepyfile(SUITE)
    pytester.runpytest("-p", "tests.require_models").assert_outcomes(passed=2, skipped=1)


def test_a_required_model_s_skip_fails_and_others_are_left_out(
    pytester: pytest.Pytester, monkeypatch
) -> None:
    monkeypatch.setenv("RASCAL_TOOLS_REQUIRE_MODELS", "tiny")
    pytester.makepyfile(SUITE)
    result = pytester.runpytest("-p", "tests.require_models")
    result.assert_outcomes(passed=1, failed=1, deselected=1)


def test_a_model_no_test_is_marked_for_fails(pytester: pytest.Pytester, monkeypatch) -> None:
    monkeypatch.setenv("RASCAL_TOOLS_REQUIRE_MODELS", "sam3")
    pytester.makepyfile(SUITE)
    result = pytester.runpytest("-p", "tests.require_models")
    assert result.ret != 0
    result.stderr.fnmatch_lines(["*no test is marked for them*"])

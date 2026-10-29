"""The tools the pack serves. Each module exposes `SPEC` (its manifest and annotation) and
`run(arguments, harness) -> CallToolResult`."""

from __future__ import annotations

from collections.abc import Callable
from typing import Any

from mcp import types

from rascal_tools.harness import Harness
from rascal_tools.manifest import ToolSpec

Runner = Callable[[dict[str, Any], Harness], types.CallToolResult]


def catalogue(debug: bool = False) -> dict[str, tuple[ToolSpec, Runner]]:
    """Every tool by name; `debug` adds the tools that exist to test the pack itself."""
    from rascal_tools.tools import echo_region

    tools: dict[str, tuple[ToolSpec, Runner]] = {
        echo_region.SPEC.name: (echo_region.SPEC, echo_region.run)
    }
    if debug:
        from rascal_tools.tools import echo_env

        tools[echo_env.SPEC.name] = (echo_env.SPEC, echo_env.run)
    return tools

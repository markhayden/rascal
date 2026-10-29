"""The server over an in-process session: the listing carries both `_meta` documents, a call
with `_harness` answers an envelope with its image block, a call without one is invalid_args,
and an unknown tool is a JSON-RPC error."""

from __future__ import annotations

import numpy as np
import pytest
from mcp import types
from mcp.shared.memory import create_connected_server_and_client_session as connect

from rascal_tools import PROTOCOL, server
from tests.conftest import harness_dict


@pytest.mark.anyio
async def test_listing_calls_and_errors_over_a_session(anyio_backend) -> None:
    async with connect(server.build(debug=True)) as session:
        assert session.get_server_capabilities() is not None
        listed = await session.list_tools()
        names = [t.name for t in listed.tools]
        assert names == ["echo_region", "segment", "echo_env"]
        echo = listed.tools[0]
        assert echo.meta is not None and "manifest" in echo.meta and "rascal" in echo.meta
        assert echo.meta["manifest"]["name"] == "echo_region"
        assert echo.meta["rascal"]["outputs"][1]["name"] == "plane"
        pixels = np.zeros((48, 64, 3), dtype=np.uint8)
        result = await session.call_tool(
            "echo_region",
            {"image": "img-1", "bbox": [0.0, 0.0, 0.5, 0.5], "_harness": harness_dict(pixels)},
        )
        assert result.isError is False
        assert result.structuredContent["status"] == "ok"
        assert len(result.structuredContent["images"]) == 1
        assert [b.type for b in result.content] == ["text", "image"]
        assert isinstance(result.content[1], types.ImageContent)
        assert result.content[1].mimeType == "image/png"
        missing = await session.call_tool("echo_region", {"image": "img-1", "bbox": [0, 0, 1, 1]})
        assert missing.isError is True
        assert missing.structuredContent["status"] == "invalid_args"
        env = await session.call_tool(
            "echo_env", {"image": "img-1", "_harness": harness_dict(pixels)}
        )
        assert env.structuredContent["summary"].startswith("env: ")
        unknown = await session.call_tool("nothing", {"_harness": harness_dict(pixels)})
        assert unknown.isError is True


def test_the_pack_speaks_the_pinned_revision() -> None:
    assert types.LATEST_PROTOCOL_VERSION == PROTOCOL


@pytest.fixture
def anyio_backend() -> str:
    return "asyncio"

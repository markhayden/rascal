"""The MCP server over stdio: tools listed with their two `_meta` documents, every call parsed
for `_harness`, every answer an envelope with its image blocks. A harness error is an
`invalid_args` envelope; anything else the tool raises is a JSON-RPC error with its class."""

from __future__ import annotations

import sys
from typing import Any

import anyio
from mcp import types
from mcp.server.lowlevel import NotificationOptions, Server
from mcp.server.models import InitializationOptions
from mcp.server.stdio import stdio_server

from rascal_tools import __version__
from rascal_tools.envelope import Answer
from rascal_tools.harness import HarnessError, parse
from rascal_tools.manifest import META_MANIFEST, META_RASCAL
from rascal_tools.tools import catalogue


def build(debug: bool = False) -> Server:
    tools = catalogue(debug)
    server: Server = Server("rascal-tools", version=__version__)

    @server.list_tools()
    async def list_tools() -> list[types.Tool]:
        # `_meta` is the wire name of the Tool's `meta` field; it is set by alias so it
        # serialises as `_meta` and the client reads it back as `meta`.
        return [
            types.Tool.model_validate(
                {
                    "name": spec.name,
                    "description": spec.purpose,
                    "inputSchema": spec.input_schema(),
                    "_meta": {META_MANIFEST: spec.manifest(), META_RASCAL: spec.annotation()},
                }
            )
            for spec, _ in tools.values()
        ]

    @server.call_tool(validate_input=False)
    async def call_tool(name: str, arguments: dict[str, Any] | None) -> types.CallToolResult:
        entry = tools.get(name)
        if entry is None:
            raise ValueError(f"no tool {name}")
        spec, run = entry
        answer = Answer(spec.name, spec.version)
        try:
            harness = parse(arguments or {})
        except HarnessError as e:
            return answer.invalid_args(str(e))
        return await anyio.to_thread.run_sync(run, arguments or {}, harness)

    return server


async def serve(debug: bool = False) -> None:
    server = build(debug)
    options = InitializationOptions(
        server_name="rascal-tools",
        server_version=__version__,
        capabilities=server.get_capabilities(
            notification_options=NotificationOptions(), experimental_capabilities={}
        ),
        instructions="rascal's tool pack: the learned finders over MCP",
    )
    async with stdio_server() as (read, write):
        await server.run(read, write, options)


def main(debug: bool = False) -> int:
    try:
        anyio.run(serve, debug)
    except KeyboardInterrupt:
        return 130
    except Exception as e:  # noqa: BLE001 - the last line on stderr before exit
        print(f"rascal-tools: {e}", file=sys.stderr)
        return 3
    return 0

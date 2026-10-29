"""The command line: `rascal-tools serve [--debug-tools]` and
`rascal-tools fetch [--artifacts FILE] [--cache DIR]`."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from rascal_tools import __version__
from rascal_tools import fetch as fetch_module


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="rascal-tools", description=__doc__)
    parser.add_argument("--version", action="version", version=f"rascal-tools {__version__}")
    sub = parser.add_subparsers(dest="command", required=True)
    serve = sub.add_parser("serve", help="the MCP server over stdio")
    serve.add_argument(
        "--debug-tools",
        action="store_true",
        help="also serve the tools that exist to test the pack (echo_env)",
    )
    fetch = sub.add_parser("fetch", help="download and verify the model artifacts")
    fetch.add_argument(
        "--artifacts",
        type=Path,
        default=fetch_module.ARTIFACTS,
        help="the artifacts.toml to read (default: the pack's own)",
    )
    fetch.add_argument("--cache", type=Path, default=None, help="the cache directory")
    args = parser.parse_args(argv)
    if args.command == "serve":
        from rascal_tools import server

        return server.main(debug=args.debug_tools)
    if args.command == "fetch":
        return fetch_module.main(args.artifacts, args.cache)
    return 64


if __name__ == "__main__":
    sys.exit(main())

"""The command line: `rascal-tools serve [--debug-tools]`; the installer's `get <model> [--from
DIR]`, `remove <model>`, `models [--json]`, `hold <model> --run ID` and `release <model> --run ID`
(SPEC.md "Installing a model"); and `fetch`, until the cutover (PLAN.md 14.4)."""

from __future__ import annotations

import argparse
import json
import signal
import sys
from pathlib import Path

from rascal_tools import __version__
from rascal_tools import fetch as fetch_module

EXIT_ERROR = 3
EXIT_USAGE = 64
EXIT_CANCELLED = 130


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
    models_help = "the models directory (default: RASCAL_TOOLS_MODELS, beside the pack, the cache)"
    get = sub.add_parser("get", help="install a segmenter from the catalogue")
    get.add_argument("model", nargs="?", help="the catalogue id (default: the default model)")
    get.add_argument("--from", dest="source", type=Path, help="a directory of its release assets")
    get.add_argument("--no-wait", action="store_true", help="refuse when another install runs")
    get.add_argument("--models", type=Path, help=models_help)
    remove = sub.add_parser("remove", help="remove an installed segmenter")
    remove.add_argument("model")
    remove.add_argument("--no-wait", action="store_true")
    remove.add_argument("--models", type=Path, help=models_help)
    listing = sub.add_parser("models", help="the catalogue with each model's state")
    listing.add_argument("--json", action="store_true")
    listing.add_argument("--models", type=Path, help=models_help)
    for name, purpose in (("hold", "pin a model's installation for a run"),
                          ("release", "release a run's hold")):  # fmt: skip
        p = sub.add_parser(name, help=purpose)
        p.add_argument("model")
        p.add_argument("--run", required=True)
        p.add_argument("--pid", type=int, help="the process the hold belongs to (hold only)")
        p.add_argument("--installation", help="the identity held (release only)")
        p.add_argument("--models", type=Path, help=models_help)
    args = parser.parse_args(argv)
    if args.command == "serve":
        from rascal_tools import server

        return server.main(debug=args.debug_tools)
    if args.command == "fetch":
        return fetch_module.main(args.artifacts, args.cache)
    return _installer(args)


def _models(args: argparse.Namespace):
    from rascal_tools import installs
    from rascal_tools.models import cache

    return installs.Models(args.models or cache.models_dir())


def _installer(args: argparse.Namespace) -> int:
    from rascal_tools import catalogue, installer, installs

    models = _models(args)
    try:
        entries = catalogue.read()
        if args.command == "get":
            model = args.model or catalogue.default(entries).id
            cancel = installer.Cancel()
            for sig in (signal.SIGINT, signal.SIGTERM):
                signal.signal(sig, lambda *_: cancel.set())
            last = [-1]

            def report(fraction: float) -> None:
                percent = int(fraction * 100)
                if percent // 10 != last[0] // 10:
                    last[0] = percent
                    print(f"{model}: {percent}%", file=sys.stderr, flush=True)

            identity = installer.get(
                model, models, entries=entries, source=args.source, cancel=cancel,
                wait=not args.no_wait, report=report,
            )  # fmt: skip
            print(f"{model}: installed {identity} in {models.root}")
            return 0
        if args.command == "remove":
            kept = installer.remove(args.model, models, wait=not args.no_wait)
            print(
                f"{args.model}: removed" + (f" (held, kept until released: {kept})" if kept else "")
            )
            return 0
        if args.command == "models":
            rows = installer.listing(models, entries)
            if args.json:
                print(json.dumps({"models_dir": str(models.root), "models": rows}, indent=2))
            else:
                for row in rows:
                    mark = " (default)" if row["default"] else ""
                    print(f"{row['id']:<10} {row['state']:<14} {row['title']}{mark}")
            return 0
        if args.command == "hold":
            print(models.hold(args.model, args.run, pid=args.pid))
            return 0
        if args.command == "release":
            held = [args.installation] if args.installation else models.installations(args.model)
            for identity in held:
                models.release(args.model, identity, args.run)
            return 0
    except installer.Cancelled as e:
        print(f"rascal-tools {args.command}: {e}", file=sys.stderr)
        return EXIT_CANCELLED
    except (installs.InstallError, installs.Busy, catalogue.CatalogueError, OSError) as e:
        print(f"rascal-tools {args.command}: {e}", file=sys.stderr)
        return EXIT_ERROR
    return EXIT_USAGE


if __name__ == "__main__":
    sys.exit(main())

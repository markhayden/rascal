"""The installer (SPEC.md "Installing a model"): `rascal-tools get <model> [--from DIR]`,
`remove <model>`, `models [--json]`, `hold <model> --run ID` and `release <model> --run ID`.
It runs as its own process, never inside a pack, with no credential.

`get` stages a model's files (its graphs and licence files, by name in `artifacts.toml`) in the
models directory, from the release over HTTPS or from a local directory, each bounded by its
recorded size and checked against its SHA-256 before anything reads it; files already installed
for another model are linked, not fetched again. It then publishes the installation
(`rascal_tools.installs`). One install or remove of a model runs at a time (its own lock);
publishing, deleting and holds take the models directory's lock only briefly, so runs are never
kept waiting on a download. Cancel (SIGINT, SIGTERM, or the token) deletes the attempt's staging
directory and nothing else. The status file records `installing` with progress, and the last
outcome when it ends."""

from __future__ import annotations

import contextlib
import errno
import fcntl
import hashlib
import json
import os
import shutil
import threading
import urllib.request
from collections.abc import Callable, Iterator
from pathlib import Path
from typing import IO, Any, Protocol

from rascal_tools import catalogue, fetch, installs

CHUNK = 1 << 20
MB = 1 << 20


class Unavailable(installs.InstallError):
    """The model cannot run on this platform."""


class NotReady(installs.InstallError):
    """Something installing needs is missing (free disk)."""


class Cancelled(installs.InstallError):
    """The install was cancelled."""


class Transport(Protocol):
    def open(self, url: str) -> contextlib.AbstractContextManager[IO[bytes]]: ...


class HttpsTransport:
    """urllib over HTTPS only; redirects are followed by urllib (GitHub's release assets
    redirect to their storage), and the bytes are checked by size and digest whatever answers."""

    def open(self, url: str) -> contextlib.AbstractContextManager[IO[bytes]]:
        if not url.startswith("https://"):
            raise installs.InstallError(f"{url!r} is not https")
        return urllib.request.urlopen(url, timeout=60)  # noqa: S310


class Cancel:
    """A cancel token: `set` from a signal handler or another thread; the install checks it
    between chunks and files."""

    def __init__(self) -> None:
        self._event = threading.Event()

    def set(self) -> None:
        self._event.set()

    def check(self) -> None:
        if self._event.is_set():
            raise Cancelled("cancelled")


def revision(entry: catalogue.Entry) -> str:
    """The catalogue revision an installation records: the digest of what decides its files
    and how they run (not the lab's figures, which change without a reinstall)."""
    decided = {
        "id": entry.id,
        "adapter": entry.adapter,
        "files": list(entry.files),
        "licence_files": list(entry.licence_files),
    }
    return hashlib.sha256(json.dumps(decided, sort_keys=True).encode()).hexdigest()


@contextlib.contextmanager
def model_lock(models: installs.Models, model: str, wait: bool) -> Iterator[None]:
    """The per-model lock: one install or remove of a model at a time."""
    models.root.mkdir(parents=True, exist_ok=True)
    with (models.root / f".install-{model}.lock").open("a+") as handle:
        try:
            fcntl.flock(handle, fcntl.LOCK_EX | (0 if wait else fcntl.LOCK_NB))
        except OSError as e:
            if e.errno in (errno.EAGAIN, errno.EACCES, errno.EWOULDBLOCK):
                raise installs.Busy(f"{model} is being installed or removed") from e
            raise
        try:
            yield
        finally:
            fcntl.flock(handle, fcntl.LOCK_UN)


def _copy_bounded(
    source: IO[bytes],
    target: Path,
    size: int,
    name: str,
    cancel: Cancel,
    progress: Callable[[int], None],
) -> None:
    done = 0
    with target.open("wb") as out:
        while True:
            cancel.check()
            chunk = source.read(min(CHUNK, size + 1 - done))
            if not chunk:
                break
            done += len(chunk)
            if done > size:
                raise installs.InstallError(f"{name}: more than its recorded {size} bytes")
            out.write(chunk)
            progress(len(chunk))
    if done != size:
        raise installs.InstallError(f"{name}: {done} bytes, its record says {size} (truncated)")


def platform_ok(entry: catalogue.Entry) -> bool:
    return catalogue.current_platform() in entry.platforms


def get(
    model: str,
    models: installs.Models,
    *,
    entries: list[catalogue.Entry] | None = None,
    artifacts: Path = fetch.ARTIFACTS,
    transport: Transport | None = None,
    source: Path | None = None,
    cancel: Cancel | None = None,
    wait: bool = True,
    platform: Callable[[catalogue.Entry], bool] = platform_ok,
    free_bytes: Callable[[Path], int] = lambda p: shutil.disk_usage(p).free,
    report: Callable[[float], None] = lambda fraction: None,
) -> str:
    """Installs `model` and returns its installation identity."""
    entries = entries if entries is not None else catalogue.read(artifacts=artifacts)
    entry = catalogue.by_id(entries, model)
    if entry is None:
        raise installs.InstallError(f"no model {model!r} (have {', '.join(e.id for e in entries)})")
    if not platform(entry):
        raise Unavailable(f"{model} does not run on this platform ({', '.join(entry.platforms)})")
    by_name = {a.name: a for a in fetch.read_artifacts(artifacts)}
    wanted = [by_name[n] for n in entry.files + entry.licence_files]
    total = sum(a.size for a in wanted)
    models.root.mkdir(parents=True, exist_ok=True)
    if free_bytes(models.root) < total:
        raise NotReady(
            f"{model} needs {total / MB:.0f} MB free in {models.root}; "
            f"{free_bytes(models.root) / MB:.0f} MB is"
        )
    cancel = cancel or Cancel()
    transport = transport or HttpsTransport()
    with model_lock(models, model, wait):
        previous = models.status(model).get("last_outcome")
        models.write_status(
            model,
            {"state": "installing", "progress": 0.0, "pid": os.getpid(), "last_outcome": previous},
        )
        with models.lock():
            models.clean()
        staging = models.staging(model)
        done = 0

        def progress(n: int) -> None:
            nonlocal done
            done += n
            fraction = done / total if total else 1.0
            report(fraction)

        try:
            records = []
            for artifact in wanted:
                cancel.check()
                target = staging / artifact.name
                shared = models.root / "files" / artifact.sha256
                if shared.is_file() and shared.stat().st_size == artifact.size:
                    os.link(shared, target)
                    progress(artifact.size)
                elif source is not None:
                    path = source / artifact.name
                    if not path.is_file():
                        raise installs.InstallError(f"{artifact.name} is not in {source}")
                    with path.open("rb") as f:
                        _copy_bounded(f, target, artifact.size, artifact.name, cancel, progress)
                else:
                    with transport.open(artifact.url) as response:
                        _copy_bounded(
                            response, target, artifact.size, artifact.name, cancel, progress
                        )
                got = installs.digest(target)
                if got != artifact.sha256:
                    raise installs.InstallError(
                        f"{artifact.name}: SHA-256 {got} is not artifacts.toml's "
                        f"{artifact.sha256}; nothing was installed"
                    )
                records.append((artifact.name, artifact.size, artifact.sha256))
                models.write_status(
                    model,
                    {
                        "state": "installing",
                        "progress": round(done / total, 4) if total else 1.0,
                        "pid": os.getpid(),
                        "last_outcome": previous,
                    },
                )
            cancel.check()
            manifest = installs.manifest_for(model, revision(entry), records)
            with models.lock():
                identity = models.publish(model, staging, manifest)
        except BaseException as e:
            shutil.rmtree(staging, ignore_errors=True)
            state = "cancelled" if isinstance(e, Cancelled | KeyboardInterrupt) else "failed"
            models.write_status(model, {"last_outcome": outcome("install", state, e)})
            if isinstance(e, KeyboardInterrupt):
                raise Cancelled("cancelled") from e
            raise
        models.write_status(model, {"last_outcome": outcome("install", "succeeded", None)})
        return identity


def outcome(action: str, state: str, error: BaseException | None) -> dict[str, Any]:
    return {
        "action": action,
        "state": state,
        "error": None if error is None else str(error) or type(error).__name__,
        "at": installs.now(),
    }


def remove(model: str, models: installs.Models, *, wait: bool = True) -> list[str]:
    """Removes `model`; returns the installations left because a run holds them."""
    with model_lock(models, model, wait):
        with models.lock(wait):
            kept = models.delete(model)
        models.write_status(model, {"last_outcome": outcome("uninstall", "succeeded", None)})
        return kept


def listing(models: installs.Models, entries: list[catalogue.Entry]) -> list[dict[str, Any]]:
    """Every catalogued model with its state, as `models --json` prints it."""
    out = []
    for entry in entries:
        status = models.status(entry.id)
        installing = status.get("state") == "installing" and installs.alive(
            int(status.get("pid", 0) or 0)
        )
        ident = models.current(entry.id)
        if not platform_ok(entry):
            state = "unavailable"
        elif installing:
            state = "installing"
        elif ident is not None:
            state = "installed"
        else:
            state = "not_installed"
        out.append(
            {
                "id": entry.id,
                "title": entry.title,
                "default": entry.default,
                "state": state,
                "progress": status.get("progress") if installing else None,
                "installation": ident,
                "licence": entry.licence,
                "download_mb": entry.download_mb,
                "installed_mb": entry.installed_mb,
                "platforms": list(entry.platforms),
                "lab": entry.lab,
                "last_outcome": status.get("last_outcome"),
            }
        )
    return out

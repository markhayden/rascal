"""Installations (SPEC.md "Choosable segmenters", "Installing a model"): how a model's files sit
in the models directory, published atomically, held by runs, verified before use.

    <models>/<id>/<identity>/       one installation: the model's files by name, install.json
    <models>/<id>/<identity>/.held/ one file per run holding it (its process id inside)
    <models>/<id>/current           the identity of the installation in use (one atomic rename)
    <models>/files/<sha256>         every installed file once, by digest (hard links)
    <models>/.lock                  the lock publish, delete and holds take
    <models>/.staging/              installs in progress, one directory each
    <models>/.status/<id>.json      the installer's state: installing, progress, last outcome

An installation's **identity** is the SHA-256 of its `install.json` (canonical JSON). `current`
names a complete, verified installation or does not exist. Nothing here downloads; the installer
(`rascal_tools.installer`) stages files and calls `publish`."""

from __future__ import annotations

import contextlib
import errno
import fcntl
import hashlib
import json
import os
import secrets
import shutil
import time
from collections.abc import Callable, Iterator
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Any

from rascal_tools import onnx_refs

INSTALL_JSON = "install.json"
HELD = ".held"


class Busy(RuntimeError):
    """The models directory is locked by someone else, or an installation is held."""


class InstallError(RuntimeError):
    """An installation is missing, incomplete or does not match its record."""


def digest(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def canonical(manifest: dict[str, Any]) -> bytes:
    return json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode("utf-8")


def identity_of(manifest: dict[str, Any]) -> str:
    return hashlib.sha256(canonical(manifest)).hexdigest()


def alive(pid: int) -> bool:
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


@dataclass(frozen=True)
class Installation:
    model: str
    identity: str
    dir: Path
    manifest: dict[str, Any]

    def path(self, name: str) -> Path:
        return self.dir / name

    @property
    def names(self) -> list[str]:
        return [f["name"] for f in self.manifest["files"]]


def manifest_for(model: str, revision: str, files: list[tuple[str, int, str]]) -> dict[str, Any]:
    """The `install.json` record: the model, the catalogue revision, each file's name, size and
    SHA-256, sorted by name."""
    return {
        "model": model,
        "catalogue": revision,
        "files": [{"name": n, "size": size, "sha256": sha} for n, size, sha in sorted(files)],
    }


def _check_name(name: str) -> None:
    p = PurePosixPath(name)
    if p.is_absolute() or len(p.parts) != 1 or name in ("", ".", "..") or name.startswith("."):
        raise InstallError(f"{name!r} is not a plain file name")


def verify(directory: Path, manifest: dict[str, Any]) -> None:
    """Every file the record lists is in `directory` with its size and digest, is a regular file
    and not a link elsewhere, and every graph's external data stays inside the installation."""
    names = set()
    for entry in manifest["files"]:
        name = entry["name"]
        _check_name(name)
        names.add(name)
        path = directory / name
        if path.is_symlink() or not path.is_file():
            raise InstallError(f"{path} is missing or not a regular file")
        if path.stat().st_size != entry["size"]:
            raise InstallError(
                f"{path}: {path.stat().st_size} bytes, the record says {entry['size']}"
            )
        if digest(path) != entry["sha256"]:
            raise InstallError(f"{path}: the SHA-256 does not match the record")
    for name in names:
        if not name.endswith(".onnx"):
            continue
        try:
            locations = onnx_refs.external_locations(directory / name)
        except onnx_refs.OnnxReadError as e:
            raise InstallError(f"{directory / name} is not a readable graph: {e}") from e
        for location in locations:
            p = PurePosixPath(location)
            if p.is_absolute() or ".." in p.parts or len(p.parts) != 1 or location not in names:
                raise InstallError(
                    f"{directory / name}: external data {location!r} is not a file of this "
                    "installation"
                )


class Models:
    """One models directory."""

    def __init__(self, root: Path) -> None:
        self.root = Path(root)
        self._verified: set[tuple[str, str]] = set()

    # The lock ------------------------------------------------------------------------------

    @contextlib.contextmanager
    def lock(self, wait: bool = True) -> Iterator[None]:
        self.root.mkdir(parents=True, exist_ok=True)
        with (self.root / ".lock").open("a+") as handle:
            try:
                fcntl.flock(handle, fcntl.LOCK_EX | (0 if wait else fcntl.LOCK_NB))
            except OSError as e:
                if e.errno in (errno.EAGAIN, errno.EACCES, errno.EWOULDBLOCK):
                    raise Busy(f"{self.root} is locked by another install or remove") from e
                raise
            try:
                yield
            finally:
                fcntl.flock(handle, fcntl.LOCK_UN)

    # Where things are ----------------------------------------------------------------------

    def model_dir(self, model: str) -> Path:
        _check_name(model)
        return self.root / model

    def current(self, model: str) -> str | None:
        try:
            ident = (self.model_dir(model) / "current").read_text().strip()
        except FileNotFoundError:
            return None
        return ident or None

    def installation_dir(self, model: str, identity: str) -> Path:
        _check_name(identity)
        return self.model_dir(model) / identity

    def installations(self, model: str) -> list[str]:
        directory = self.model_dir(model)
        if not directory.is_dir():
            return []
        return sorted(p.name for p in directory.iterdir() if p.is_dir())

    def staging(self, model: str) -> Path:
        path = self.root / ".staging" / f"{model}-{os.getpid()}-{secrets.token_hex(4)}"
        path.mkdir(parents=True)
        return path

    # Holds ---------------------------------------------------------------------------------

    def held(self, directory: Path) -> bool:
        holds = directory / HELD
        if not holds.is_dir():
            return False
        for hold in holds.iterdir():
            try:
                pid = int(hold.read_text().strip() or "0")
            except (OSError, ValueError):
                continue
            if pid > 0 and alive(pid):
                return True
        return False

    def hold(
        self,
        model: str,
        run: str,
        pid: int | None = None,
        wait: bool = True,
        between: Callable[[], None] | None = None,
    ) -> str:
        """Pins the model's current installation for a run: under the lock, reads `current`,
        checks the installation is there, and writes the hold. Returns the identity. `between`
        runs after `current` is read and before the hold is written (a test's barrier)."""
        _check_name(run)
        with self.lock(wait):
            ident = self.current(model)
            if between is not None:
                between()
            if ident is None:
                raise InstallError(f"{model} is not installed in {self.root}")
            directory = self.installation_dir(model, ident)
            if not directory.is_dir():
                raise InstallError(f"{model}'s installation {ident} is gone from {self.root}")
            (directory / HELD).mkdir(exist_ok=True)
            (directory / HELD / run).write_text(str(pid if pid is not None else os.getpid()))
            return ident

    def release(self, model: str, identity: str, run: str) -> None:
        with contextlib.suppress(FileNotFoundError):
            (self.installation_dir(model, identity) / HELD / run).unlink()

    # Publishing and deleting (the caller holds the lock) -----------------------------------

    def publish(self, model: str, staged: Path, manifest: dict[str, Any]) -> str:
        """Makes the staged files the model's current installation and returns its identity:
        verified, renamed into place (or an identical installation kept), `current` replaced
        in one rename, the previous installation deleted unless a run holds it. Call with the
        lock held."""
        verify(staged, manifest)
        identity = identity_of(manifest)
        target = self.installation_dir(model, identity)
        (staged / INSTALL_JSON).write_bytes(canonical(manifest))
        if target.exists():
            try:
                verify(target, manifest)
            except InstallError:
                if self.held(target):
                    raise Busy(f"{target} is damaged and a run holds it") from None
                shutil.rmtree(target)
            else:
                shutil.rmtree(staged)
                staged = target
        if staged != target:
            target.parent.mkdir(parents=True, exist_ok=True)
            staged.rename(target)
        self._share(target, manifest)
        previous = self.current(model)
        self._set_current(model, identity)
        if previous and previous != identity:
            old = self.installation_dir(model, previous)
            if old.is_dir() and not self.held(old):
                shutil.rmtree(old)
        self._collect()
        return identity

    def delete(self, model: str) -> list[str]:
        """Removes the model: `current` first, then every installation no live run holds.
        Returns the identities left behind because they are held. Call with the lock held."""
        with contextlib.suppress(FileNotFoundError):
            (self.model_dir(model) / "current").unlink()
        kept = []
        for ident in self.installations(model):
            directory = self.installation_dir(model, ident)
            if self.held(directory):
                kept.append(ident)
            else:
                shutil.rmtree(directory)
        with contextlib.suppress(OSError):
            self.model_dir(model).rmdir()
        self._collect()
        return kept

    def clean(self) -> None:
        """After a crash: deletes staging directories whose process is gone and installations
        neither `current` nor held. Call with the lock held."""
        staging = self.root / ".staging"
        if staging.is_dir():
            for path in staging.iterdir():
                parts = path.name.rsplit("-", 2)
                pid = int(parts[1]) if len(parts) == 3 and parts[1].isdigit() else 0
                if pid == os.getpid() or not alive(pid):
                    shutil.rmtree(path, ignore_errors=True)
        for model_dir in self.root.iterdir() if self.root.is_dir() else []:
            if (
                model_dir.name.startswith(".")
                or model_dir.name == "files"
                or not model_dir.is_dir()
            ):
                continue
            current = self.current(model_dir.name)
            for ident in self.installations(model_dir.name):
                directory = model_dir / ident
                if ident != current and not self.held(directory):
                    shutil.rmtree(directory)
        self._collect()

    def _set_current(self, model: str, identity: str) -> None:
        directory = self.model_dir(model)
        tmp = directory / f".current-{os.getpid()}-{secrets.token_hex(4)}"
        tmp.write_text(identity + "\n")
        tmp.replace(directory / "current")

    def _share(self, directory: Path, manifest: dict[str, Any]) -> None:
        """Each file once in `files/` by digest: an installation's copy becomes a hard link to
        the shared one, or becomes the shared one."""
        shared = self.root / "files"
        shared.mkdir(exist_ok=True)
        for entry in manifest["files"]:
            mine, theirs = directory / entry["name"], shared / entry["sha256"]
            if theirs.exists():
                if os.path.samefile(mine, theirs):
                    continue
                tmp = directory / f".link-{secrets.token_hex(4)}"
                os.link(theirs, tmp)
                tmp.replace(mine)
            else:
                os.link(mine, theirs)

    def _collect(self) -> None:
        """Deletes shared files no installation lists."""
        shared = self.root / "files"
        if not shared.is_dir():
            return
        listed: set[str] = set()
        for model_dir in self.root.iterdir():
            if (
                model_dir.name.startswith(".")
                or model_dir.name == "files"
                or not model_dir.is_dir()
            ):
                continue
            for ident in self.installations(model_dir.name):
                record = model_dir / ident / INSTALL_JSON
                with contextlib.suppress(OSError, ValueError, KeyError):
                    listed |= {f["sha256"] for f in json.loads(record.read_text())["files"]}
        for path in shared.iterdir():
            if path.name not in listed:
                path.unlink()

    # Using an installation -----------------------------------------------------------------

    def resolve(self, model: str, identity: str | None = None) -> Installation:
        """The installation `identity` names, else the current one, verified once per process
        per identity against its `install.json`."""
        ident = identity or self.current(model)
        if ident is None:
            raise InstallError(f"{model} is not installed in {self.root}")
        directory = self.installation_dir(model, ident)
        record = directory / INSTALL_JSON
        try:
            manifest = json.loads(record.read_text())
        except (OSError, ValueError) as e:
            raise InstallError(f"{model}'s installation {ident} is gone from {self.root}") from e
        if identity_of(manifest) != ident:
            raise InstallError(f"{record} does not match its identity {ident}")
        if (model, ident) not in self._verified:
            verify(directory, manifest)
            self._verified.add((model, ident))
        return Installation(model, ident, directory, manifest)

    # The installer's state -----------------------------------------------------------------

    def status(self, model: str) -> dict[str, Any]:
        try:
            return json.loads((self.root / ".status" / f"{model}.json").read_text())
        except (OSError, ValueError):
            return {}

    def write_status(self, model: str, status: dict[str, Any]) -> None:
        directory = self.root / ".status"
        directory.mkdir(parents=True, exist_ok=True)
        tmp = directory / f".{model}-{os.getpid()}-{secrets.token_hex(4)}"
        tmp.write_text(json.dumps(status, sort_keys=True))
        tmp.replace(directory / f"{model}.json")


def now() -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


class Slot:
    """The one loaded model, keyed by installation: `get` returns the loaded adapter when the
    model and identity are the ones loaded, and otherwise drops it and builds anew; a model whose
    installation is gone is dropped before the error is raised."""

    def __init__(self) -> None:
        self.key: tuple[str, str] | None = None
        self.value: Any = None

    def get(
        self,
        models: Models,
        model: str,
        identity: str | None,
        build: Callable[[Installation], Any],
    ) -> tuple[Installation, Any]:
        try:
            installation = models.resolve(model, identity)
        except InstallError:
            self.key, self.value = None, None
            raise
        key = (model, installation.identity)
        if self.key != key:
            self.key, self.value = None, None
            self.value = build(installation)
            self.key = key
        return installation, self.value

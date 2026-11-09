"""Installations (`rascal_tools.installs`, SPEC.md "Installing a model"), on temporary models
directories: publishing through `current`, identical and damaged installations, holds under the
lock, deleting, crash clean-up, shared files, verification once per identity, external data kept
inside the installation, and the loaded model dropped when its installation goes."""

from __future__ import annotations

import hashlib
import os
import subprocess
import sys
import threading
from pathlib import Path

import pytest

from rascal_tools import installs, onnx_refs


def _len(number: int, payload: bytes) -> bytes:
    """One length-delimited protobuf field."""

    def varint(v: int) -> bytes:
        out = bytearray()
        while True:
            b = v & 0x7F
            v >>= 7
            out.append(b | (0x80 if v else 0))
            if not v:
                return bytes(out)

    return varint((number << 3) | 2) + varint(len(payload)) + payload


def graph_with_external(*locations: str, nested: str | None = None) -> bytes:
    """A ModelProto whose graph's initializers name `locations` as external data, and whose one
    node carries a subgraph naming `nested`."""
    tensors = b"".join(
        _len(5, _len(13, _len(1, b"location") + _len(2, loc.encode()))) for loc in locations
    )
    node = b""
    if nested is not None:
        inner = _len(5, _len(13, _len(1, b"location") + _len(2, nested.encode())))
        node = _len(1, _len(5, _len(6, inner)))
    return _len(7, tensors + node)


def files(directory: Path, **contents: bytes) -> list[tuple[str, int, str]]:
    directory.mkdir(parents=True, exist_ok=True)
    out = []
    for name, data in contents.items():
        name = name.replace("_", ".")
        (directory / name).write_bytes(data)
        out.append((name, len(data), hashlib.sha256(data).hexdigest()))
    return out


@pytest.fixture
def models(tmp_path: Path) -> installs.Models:
    return installs.Models(tmp_path / "models")


def install(models: installs.Models, model: str = "tiny", **contents: bytes) -> str:
    staged = models.staging(model)
    record = installs.manifest_for(model, "test", files(staged, **contents))
    with models.lock():
        return models.publish(model, staged, record)


def test_external_locations_are_read_from_the_graph(tmp_path: Path) -> None:
    graph = tmp_path / "g.onnx"
    graph.write_bytes(graph_with_external("g.onnx.data", "g.onnx.data", nested="inner.data"))
    assert onnx_refs.external_locations(graph) == ["g.onnx.data", "inner.data"]
    (tmp_path / "none.onnx").write_bytes(_len(7, _len(5, _len(8, b"raw"))))
    assert onnx_refs.external_locations(tmp_path / "none.onnx") == []
    (tmp_path / "bad.onnx").write_bytes(b"\x3a\xff")
    with pytest.raises(onnx_refs.OnnxReadError):
        onnx_refs.external_locations(tmp_path / "bad.onnx")


def test_a_published_installation_is_read_through_current(models) -> None:
    ident = install(models, a_bin=b"weights", a_txt=b"licence")
    assert models.current("tiny") == ident
    found = models.resolve("tiny")
    assert found.identity == ident and found.path("a.bin").read_bytes() == b"weights"
    assert sorted(found.names) == ["a.bin", "a.txt"]
    assert installs.identity_of(found.manifest) == ident


def test_a_named_installation_is_read_even_when_not_current(models) -> None:
    first = install(models, a_bin=b"one")
    models.hold("tiny", "run-1")
    second = install(models, a_bin=b"two")
    assert models.current("tiny") == second
    assert models.resolve("tiny", first).path("a.bin").read_bytes() == b"one"


def test_a_held_installation_survives_publish_and_delete_until_released(models) -> None:
    first = install(models, a_bin=b"one")
    assert models.hold("tiny", "run-1") == first
    install(models, a_bin=b"two")
    assert models.installation_dir("tiny", first).is_dir()
    with models.lock():
        assert models.delete("tiny") == [first]
    assert models.current("tiny") is None
    assert models.installation_dir("tiny", first).is_dir()
    models.release("tiny", first, "run-1")
    with models.lock():
        models.clean()
    assert not models.installation_dir("tiny", first).exists()


def test_a_stale_hold_does_not_keep_an_installation(models) -> None:
    first = install(models, a_bin=b"one")
    dead = subprocess.run(
        [sys.executable, "-c", "import os; print(os.getpid())"],
        capture_output=True,
        text=True,
        check=True,
    )
    models.hold("tiny", "run-dead", pid=int(dead.stdout))
    install(models, a_bin=b"two")
    assert not models.installation_dir("tiny", first).exists()


def test_a_hold_and_a_delete_never_interleave(models) -> None:
    install(models, a_bin=b"one")
    read, go = threading.Event(), threading.Event()
    deleted: list[list[str]] = []

    def delete() -> None:
        read.wait()
        with models.lock():
            deleted.append(models.delete("tiny"))

    def between() -> None:
        read.set()
        # The delete is waiting for the lock: give it time to try.
        go.wait(0.3)

    worker = threading.Thread(target=delete)
    worker.start()
    ident = models.hold("tiny", "run-1", between=between)
    worker.join()
    # The hold was taken first, so the delete kept the held installation.
    assert deleted == [[ident]]
    assert models.installation_dir("tiny", ident).is_dir()


def test_the_same_installation_twice_is_kept_and_nothing_deleted(models) -> None:
    first = install(models, a_bin=b"same")
    marker = models.installation_dir("tiny", first) / "a.bin"
    inode = marker.stat().st_ino
    second = install(models, a_bin=b"same")
    assert first == second and marker.stat().st_ino == inode
    assert models.installations("tiny") == [first]
    assert not any((models.root / ".staging").iterdir())


def test_a_damaged_installation_is_replaced_and_refused_while_held(models) -> None:
    ident = install(models, a_bin=b"good")
    (models.installation_dir("tiny", ident) / "a.bin").unlink()
    models.hold("tiny", "run-1")
    with pytest.raises(installs.Busy):
        install(models, a_bin=b"good")
    models.release("tiny", ident, "run-1")
    assert install(models, a_bin=b"good") == ident
    models.resolve("tiny")


def test_a_tampered_file_is_refused(models) -> None:
    ident = install(models, a_bin=b"good")
    (models.installation_dir("tiny", ident) / "a.bin").write_bytes(b"evil")
    with pytest.raises(installs.InstallError, match="does not match|bytes"):
        installs.Models(models.root).resolve("tiny")


@pytest.mark.parametrize("location", ["/etc/passwd", "../other.data", "sub/g.data", "x.data"])
def test_external_data_outside_the_installation_is_refused(models, location) -> None:
    staged = models.staging("tiny")
    record = installs.manifest_for(
        "tiny", "test", files(staged, g_onnx=graph_with_external(location), g_data=b"w")
    )
    with models.lock(), pytest.raises(installs.InstallError, match="external data"):
        models.publish("tiny", staged, record)


def test_external_data_listed_in_the_installation_is_accepted(models) -> None:
    install(models, g_onnx=graph_with_external("g.data"), g_data=b"weights")
    assert models.resolve("tiny").names == ["g.data", "g.onnx"]


def test_a_symlinked_file_is_refused(models, tmp_path) -> None:
    staged = models.staging("tiny")
    record = installs.manifest_for("tiny", "test", files(staged, a_bin=b"x"))
    (tmp_path / "elsewhere").write_bytes(b"x")
    (staged / "a.bin").unlink()
    (staged / "a.bin").symlink_to(tmp_path / "elsewhere")
    with models.lock(), pytest.raises(installs.InstallError, match="regular file"):
        models.publish("tiny", staged, record)


def test_shared_files_are_kept_once_and_collected(models) -> None:
    install(models, "tiny", owl_bin=b"shared", sam_bin=b"tiny's")
    install(models, "other", owl_bin=b"shared", bi_bin=b"other's")
    shared = models.root / "files"
    assert len(list(shared.iterdir())) == 3
    owl = hashlib.sha256(b"shared").hexdigest()
    assert (shared / owl).stat().st_nlink == 3
    with models.lock():
        models.delete("tiny")
    assert (shared / owl).exists() and len(list(shared.iterdir())) == 2
    with models.lock():
        models.delete("other")
    assert not any(shared.iterdir())


def test_a_crash_leaves_nothing_the_next_clean_keeps(models) -> None:
    ident = install(models, a_bin=b"one")
    # A staging directory from a process that is gone, and an unreferenced installation.
    gone = models.root / ".staging" / "tiny-999999-dead"
    gone.mkdir(parents=True)
    orphan = models.model_dir("tiny") / ("f" * 64)
    orphan.mkdir()
    with models.lock():
        models.clean()
    assert not gone.exists() and not orphan.exists()
    assert models.current("tiny") == ident and models.resolve("tiny")


def test_the_lock_refuses_busy_when_asked_not_to_wait(models) -> None:
    with models.lock():
        code = (
            "import sys; from rascal_tools import installs; "
            f"m = installs.Models({str(models.root)!r})\n"
            "try:\n    m.lock(wait=False).__enter__()\nexcept installs.Busy:\n    sys.exit(7)\n"
        )
        result = subprocess.run([sys.executable, "-c", code], env=os.environ.copy())
    assert result.returncode == 7


def test_a_removed_model_is_unloaded_before_the_next_call(models) -> None:
    install(models, a_bin=b"one")
    slot, built = installs.Slot(), []

    def build(found: installs.Installation) -> str:
        built.append(found.identity)
        return f"adapter {found.identity[:6]}"

    _, first = slot.get(models, "tiny", None, build)
    assert slot.get(models, "tiny", None, build)[1] is first and len(built) == 1
    with models.lock():
        models.delete("tiny")
    with pytest.raises(installs.InstallError):
        slot.get(models, "tiny", None, build)
    assert slot.key is None and slot.value is None
    install(models, a_bin=b"two")
    slot.get(models, "tiny", None, build)
    assert len(built) == 2


def test_the_status_file_round_trips(models) -> None:
    assert models.status("tiny") == {}
    models.write_status("tiny", {"state": "installing", "progress": 0.5})
    assert models.status("tiny") == {"state": "installing", "progress": 0.5}

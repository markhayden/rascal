"""The installer (`rascal_tools.installer`, SPEC.md "Installing a model") over an in-process
fake transport: downloads checked by size and digest, cancel at each stage, crash recovery,
repeated and concurrent installs, a remove racing an install, the last outcome, the platform and
free-disk checks before any download, `--from` a local directory, and the listing."""

from __future__ import annotations

import contextlib
import hashlib
import io
import json
import threading
from pathlib import Path

import pytest

from rascal_tools import catalogue, installer, installs

GRAPH = b"graph bytes " * 300_000  # several chunks
TOKENS = b"tokenizer"
LICENCE = b"Apache License, Version 2.0"


class FakeTransport:
    """Serves bytes by URL; `gate` (an Event) makes the first read wait; `after_first_chunk`
    runs once a first chunk is read; `calls` lists every URL opened."""

    def __init__(self, served: dict[str, bytes]) -> None:
        self.served = served
        self.calls: list[str] = []
        self.gate: threading.Event | None = None
        self.started = threading.Event()
        self.after_first_chunk = None

    @contextlib.contextmanager
    def open(self, url: str):
        self.calls.append(url)
        data = self.served[url]
        transport = self

        class Stream(io.BytesIO):
            first = True

            def read(self, n=-1):
                transport.started.set()
                if transport.gate is not None:
                    transport.gate.wait(10)
                chunk = super().read(n)
                if Stream.first and transport.after_first_chunk is not None:
                    Stream.first = False
                    transport.after_first_chunk()
                return chunk

        yield Stream(data)


def artifact(name: str, data: bytes, size: int | None = None) -> str:
    return (
        f'[[artifact]]\nname = "{name}"\nurl = "https://example.invalid/{name}"\n'
        f'sha256 = "{hashlib.sha256(data).hexdigest()}"\n'
        f"size = {len(data) if size is None else size}\n"
        'licence = "Apache-2.0"\n\n'
    )


@pytest.fixture
def setup(tmp_path: Path):
    artifacts = tmp_path / "artifacts.toml"
    artifacts.write_text(
        artifact("g.bin", GRAPH)
        + artifact("t.json", TOKENS)
        + artifact("LICENSE.g", LICENCE)
        + artifact("other.bin", b"other")
    )
    cat = tmp_path / "segmenters.toml"
    cat.write_text(
        '[[segmenter]]\nid = "tiny"\ntitle = "T"\nadapter = "owlv2+sam2"\nparams = []\n'
        'default = true\nlicence = "Apache-2.0"\nlicence_files = ["LICENSE.g"]\n'
        'files = ["g.bin", "t.json"]\ndownload_mb = 1\ninstalled_mb = 1\n'
        'platforms = ["macos-arm64", "linux-x86_64"]\n\n'
        '[[segmenter]]\nid = "other"\ntitle = "O"\nadapter = "owlv2+sam2"\nparams = []\n'
        'licence = "MIT"\nfiles = ["g.bin", "other.bin"]\ndownload_mb = 1\ninstalled_mb = 1\n'
        'platforms = ["linux-x86_64"]\n'
    )
    entries = catalogue.read(cat, artifacts)
    transport = FakeTransport(
        {
            f"https://example.invalid/{n}": d
            for n, d in (
                ("g.bin", GRAPH),
                ("t.json", TOKENS),
                ("LICENSE.g", LICENCE),
                ("other.bin", b"other"),
            )
        }
    )
    models = installs.Models(tmp_path / "models")

    def get(model: str = "tiny", **kw):
        kw.setdefault("transport", transport)
        kw.setdefault("platform", lambda e: True)
        return installer.get(model, models, entries=entries, artifacts=artifacts, **kw)

    return models, transport, get, entries, tmp_path


def no_staging(models: installs.Models) -> bool:
    staging = models.root / ".staging"
    return not staging.exists() or not any(staging.iterdir())


def test_a_model_installs_from_the_release_with_its_licence(setup) -> None:
    models, transport, get, _, _ = setup
    ident = get()
    found = models.resolve("tiny")
    assert found.identity == ident
    assert sorted(found.names) == ["LICENSE.g", "g.bin", "t.json"]
    assert models.status("tiny")["last_outcome"]["state"] == "succeeded"
    assert no_staging(models) and len(transport.calls) == 3


@pytest.mark.parametrize(
    ("served", "message"),
    [(b"x" * len(TOKENS), "SHA-256"), (TOKENS + b"more", "more than"), (TOKENS[:3], "truncated")],
)
def test_a_bad_download_is_refused_and_nothing_is_installed(setup, served, message) -> None:
    models, transport, get, _, _ = setup
    transport.served["https://example.invalid/t.json"] = served
    with pytest.raises(installs.InstallError, match=message):
        get()
    assert models.current("tiny") is None and no_staging(models)
    assert models.status("tiny")["last_outcome"]["state"] == "failed"


@pytest.mark.parametrize("stage", ["before", "mid-file", "before-publish"])
def test_cancel_at_each_stage_leaves_the_previous_installation(setup, stage, monkeypatch) -> None:
    models, transport, get, _, _ = setup
    first = get()
    # A changed graph: the next get stages a new installation, which the cancel abandons.
    new = GRAPH + b"!"
    transport.served["https://example.invalid/g.bin"] = new
    text = Path(setup[4] / "artifacts.toml").read_text()
    text = text.replace(hashlib.sha256(GRAPH).hexdigest(), hashlib.sha256(new).hexdigest())
    text = text.replace(f"size = {len(GRAPH)}", f"size = {len(new)}")
    (setup[4] / "artifacts.toml").write_text(text)
    cancel = installer.Cancel()
    if stage == "before":
        cancel.set()
    elif stage == "mid-file":
        transport.after_first_chunk = cancel.set
    else:
        real = installs.Models.publish

        def publish(self, *a, **k):
            cancel.set()
            cancel.check()
            return real(self, *a, **k)

        monkeypatch.setattr(installs.Models, "publish", publish)
    with pytest.raises(installer.Cancelled):
        get(cancel=cancel)
    assert models.current("tiny") == first and models.resolve("tiny")
    assert no_staging(models)
    assert models.status("tiny")["last_outcome"]["state"] == "cancelled"


def test_a_crash_is_cleaned_by_the_next_get(setup) -> None:
    models, _, get, _, _ = setup
    crashed = models.root / ".staging" / "tiny-999999-dead"
    crashed.mkdir(parents=True)
    (crashed / "g.bin").write_bytes(b"half")
    get()
    assert not crashed.exists() and models.resolve("tiny")


def test_getting_an_unchanged_model_twice_keeps_it_and_downloads_nothing(setup) -> None:
    models, transport, get, _, _ = setup
    first = get()
    calls = len(transport.calls)
    assert get() == first and len(transport.calls) == calls
    assert models.installations("tiny") == [first]


def test_a_shared_file_is_not_downloaded_twice(setup) -> None:
    models, transport, get, _, _ = setup
    get()
    get("other", platform=lambda e: True)
    assert transport.calls.count("https://example.invalid/g.bin") == 1


def test_a_second_install_of_the_same_model_is_busy_or_waits(setup) -> None:
    models, transport, get, _, _ = setup
    transport.gate = threading.Event()
    worker = threading.Thread(target=get)
    worker.start()
    assert transport.started.wait(5)
    with pytest.raises(installs.Busy):
        get(wait=False)
    with pytest.raises(installs.Busy):
        installer.remove("tiny", models, wait=False)
    transport.gate.set()
    worker.join()
    assert models.resolve("tiny")
    assert installer.remove("tiny", models) == []
    assert models.current("tiny") is None
    assert models.status("tiny")["last_outcome"]["action"] == "uninstall"


def test_an_unsupported_platform_and_too_little_disk_are_refused_before_downloading(
    setup,
) -> None:
    models, transport, get, _, _ = setup
    with pytest.raises(installer.Unavailable):
        get(platform=lambda e: False)
    with pytest.raises(installer.NotReady, match="free"):
        get(free_bytes=lambda p: 10)
    assert transport.calls == []


def test_from_a_local_directory_with_no_network(setup, tmp_path) -> None:
    models, transport, get, _, _ = setup
    assets = tmp_path / "assets"
    assets.mkdir()
    (assets / "g.bin").write_bytes(GRAPH)
    (assets / "LICENSE.g").write_bytes(LICENCE)
    with pytest.raises(installs.InstallError, match="t.json is not in"):
        get(source=assets)
    (assets / "t.json").write_bytes(b"tampered!")
    with pytest.raises(installs.InstallError, match="SHA-256"):
        get(source=assets)
    (assets / "t.json").write_bytes(TOKENS)
    get(source=assets)
    assert transport.calls == [] and models.resolve("tiny")


def test_the_listing_shows_each_model_s_state(setup, monkeypatch) -> None:
    models, _, get, entries, _ = setup
    monkeypatch.setattr(installer, "platform_ok", lambda e: "macos-arm64" in e.platforms)
    get()
    listed = {m["id"]: m for m in installer.listing(models, entries)}
    assert listed["tiny"]["state"] == "installed" and listed["tiny"]["default"] is True
    assert listed["tiny"]["installation"] == models.current("tiny")
    assert listed["other"]["state"] == "unavailable"
    json.dumps(list(listed.values()))


def test_the_command_line_lists_and_refuses_cleanly(tmp_path, capsys) -> None:
    from rascal_tools import cli

    models = str(tmp_path / "models")
    assert cli.main(["models", "--json", "--models", models]) == 0
    listed = json.loads(capsys.readouterr().out)
    assert listed["models_dir"] == models
    [tiny] = [m for m in listed["models"] if m["id"] == "tiny"]
    assert tiny["default"] is True and tiny["installation"] is None
    assert tiny["state"] in ("not_installed", "unavailable")
    assert cli.main(["get", "nope", "--models", models]) == 3
    assert "no model 'nope'" in capsys.readouterr().err
    assert cli.main(["hold", "tiny", "--run", "r1", "--models", models]) == 3
    assert "not installed" in capsys.readouterr().err


def test_the_models_directory_holds_only_the_layout(setup) -> None:
    # SPEC.md "Installing a model": the per-model lock is `<models>/<id>/.lock`, so the top level
    # holds model ids, `files`, `.lock`, `.staging` and `.status` and nothing else (a re-run of
    # `rascal tools setup` refuses anything else as a stray; crates/rascal-mcp provision.rs).
    models, _, get, _, _ = setup
    get()
    installer.remove("tiny", models)
    get()
    top = {p.name for p in models.root.iterdir()}
    assert top <= {"tiny", "files", ".lock", ".staging", ".status"}, top
    assert (models.root / "tiny" / ".lock").is_file()
    assert models.installations("tiny") == [models.current("tiny")]

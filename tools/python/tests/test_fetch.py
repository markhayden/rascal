from __future__ import annotations

import hashlib
import http.server
import threading
from pathlib import Path

import pytest

from rascal_tools import fetch


@pytest.fixture
def served(tmp_path: Path):
    """A local HTTP server over a directory with one artifact in it."""
    root = tmp_path / "served"
    root.mkdir()
    payload = b"model bytes " * 1000
    (root / "model.bin").write_bytes(payload)
    handler = type("Handler", (http.server.SimpleHTTPRequestHandler,), {"directory": str(root)})
    handler.log_message = lambda *a, **k: None  # type: ignore[assignment]
    server = http.server.ThreadingHTTPServer(
        ("127.0.0.1", 0), lambda *a, **k: handler(*a, directory=str(root), **k)
    )
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    yield f"http://127.0.0.1:{server.server_address[1]}", hashlib.sha256(payload).hexdigest()
    server.shutdown()


def test_fetch_verifies_the_digest_and_refuses_a_tampered_one(tmp_path: Path, served) -> None:
    url, sha = served
    cache = tmp_path / "cache"
    good = fetch.Artifact("model.bin", f"{url}/model.bin", sha, "Apache-2.0")
    path = fetch.fetch_one(good, cache)
    assert path == cache / "model.bin" and fetch.fetched(good, cache)
    # A second fetch reuses the cached file (the server could be gone).
    assert fetch.fetch_one(good, cache) == path
    bad = fetch.Artifact("other.bin", f"{url}/model.bin", "0" * 64, "Apache-2.0")
    with pytest.raises(fetch.FetchError, match="does not match"):
        fetch.fetch_one(bad, cache)
    assert not (cache / "other.bin").exists() and not (cache / "other.bin.partial").exists()
    assert not fetch.fetched(bad, cache)


def test_artifacts_toml_is_read_and_checked(tmp_path: Path) -> None:
    toml = tmp_path / "artifacts.toml"
    toml.write_text(
        '[[artifact]]\nname = "a.onnx"\nurl = "https://example.invalid/a"\n'
        + 'sha256 = "'
        + "AB" * 32
        + '"\nlicence = "Apache-2.0"\n'
    )
    [a] = fetch.read_artifacts(toml)
    assert a.name == "a.onnx" and a.sha256 == "ab" * 32
    toml.write_text('[[artifact]]\nname = "../a.onnx"\nurl = "u"\nsha256 = "0"\nlicence = "x"\n')
    with pytest.raises(fetch.FetchError, match="plain file name"):
        fetch.read_artifacts(toml)
    toml.write_text('[[artifact]]\nname = "a.onnx"\nurl = "u"\n')
    with pytest.raises(fetch.FetchError, match="lacks sha256"):
        fetch.read_artifacts(toml)
    # The pack's own file is empty until the segmenter is decided, and reads as such.
    shipped = fetch.read_artifacts(fetch.ARTIFACTS)
    assert [a.name for a in shipped] == [
        "owlv2-base-patch16.onnx",
        "owlv2-base-patch16.tokenizer.json",
        "sam2.1-hiera-tiny.encoder.onnx",
        "sam2.1-hiera-tiny.decoder.onnx",
    ]
    assert all(a.licence == "Apache-2.0" and len(a.sha256) == 64 for a in shipped)


def test_a_missing_artifact_skips_a_segmenter_test() -> None:
    from tests.parity import requires_artifact

    with pytest.raises(pytest.skip.Exception):
        requires_artifact("not-fetched.onnx")

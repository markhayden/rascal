"""The segmenter catalogue (`segmenters.toml`) and `segment`'s use of it: the loader's checks,
the order and pairing every adapter's instances come back in, the parameters a model does not
use refused, and one model loaded at a time."""

from __future__ import annotations

from pathlib import Path

import numpy as np
import pytest

from rascal_tools import catalogue, fetch
from rascal_tools.models import adapters
from rascal_tools.tools import segment

ARTIFACTS = """
[[artifact]]
name = "a.onnx"
url = "https://example.invalid/a.onnx"
sha256 = "{z}"
size = 1
licence = "Apache-2.0"

[[artifact]]
name = "LICENSE.a"
url = "https://example.invalid/LICENSE.a"
sha256 = "{z}"
size = 1
licence = "Apache-2.0"
""".format(z="0" * 64)

ENTRY = """
[[segmenter]]
id = "{id}"
title = "A"
adapter = "owlv2+sam2"
params = {params}
default = {default}
licence = "Apache-2.0"
licence_files = ["LICENSE.a"]
files = {files}
download_mb = 1
installed_mb = 1
platforms = {platforms}
"""


def entry(
    id: str = "a",
    params: str = '["threshold"]',
    default: str = "true",
    files: str = '["a.onnx"]',
    platforms: str = '["macos-arm64"]',
) -> str:
    return ENTRY.format(id=id, params=params, default=default, files=files, platforms=platforms)


@pytest.fixture
def write(tmp_path: Path):
    artifacts = tmp_path / "artifacts.toml"
    artifacts.write_text(ARTIFACTS)
    path = tmp_path / "segmenters.toml"

    def _write(text: str) -> list[catalogue.Entry]:
        path.write_text(text)
        return catalogue.read(path, artifacts)

    return _write


def test_the_shipped_catalogue_reads_and_tiny_is_the_default() -> None:
    entries = catalogue.read()
    tiny = catalogue.default(entries)
    assert tiny.id == "tiny" and tiny.adapter == "owlv2+sam2"
    assert set(tiny.params) == set(catalogue.SEGMENT_PARAMS)
    names = {a.name for a in fetch.read_artifacts(fetch.ARTIFACTS)}
    assert set(tiny.files) <= names


def test_a_good_catalogue_reads(write) -> None:
    [a] = write(entry())
    assert a.id == "a" and a.files == ("a.onnx",) and a.licence_files == ("LICENSE.a",)


@pytest.mark.parametrize(
    ("text", "message"),
    [
        (entry() + entry(), "appears twice"),
        (entry(default="false"), "exactly one entry must be the default"),
        (entry() + entry(id="b"), "exactly one entry must be the default"),
        (entry(id="Big"), "lower-case"),
        (entry(params='["prompt"]'), "not segment parameters"),
        (entry(files='["b.onnx"]'), "not in artifacts.toml"),
        (entry(files="[]"), "no files"),
        (entry(platforms='["windows"]'), "platforms"),
        (entry().replace('"owlv2+sam2"', '"nope"'), "no adapter"),
        (entry().replace('title = "A"\n', ""), "lacks title"),
    ],
)
def test_a_broken_catalogue_is_refused(write, text: str, message: str) -> None:
    with pytest.raises(catalogue.CatalogueError, match=message):
        write(text)


def _instance(query: int, score: float, box, fill: bool = True) -> adapters.Instance:
    plane = np.zeros((48, 64), dtype=bool)
    if fill:
        x0, y0, x1, y1 = (int(v) for v in box)
        plane[y0:y1, x0:x1] = True
    return adapters.Instance(query, score, box, plane)


def test_instances_come_back_best_first_ties_by_query_then_top_left() -> None:
    a = _instance(1, 0.5, (0.0, 10.0, 5.0, 15.0))
    b = _instance(0, 0.5, (20.0, 20.0, 25.0, 25.0))
    c = _instance(0, 0.5, (30.0, 5.0, 35.0, 10.0))
    d = _instance(0, 0.9, (0.0, 0.0, 5.0, 5.0))
    assert adapters.order([a, b, c, d], 8) == [d, c, b, a]
    assert adapters.order([a, b, c, d], 2) == [d, c]


class _FakeAdapter:
    def __init__(self, instances) -> None:
        self.instances = instances
        self.settings: list[adapters.Settings] = []

    def find(self, pixels, queries, settings):
        self.settings.append(settings)
        return list(self.instances)


def _use(monkeypatch, adapter, params=catalogue.SEGMENT_PARAMS, ident="fake") -> None:
    fake = catalogue.Entry(
        ident, "Fake", "fake", tuple(params), True, "MIT", (), ("f.onnx",), 1, 1, ("macos-arm64",)
    )
    monkeypatch.setattr(segment, "ENTRIES", [fake])
    monkeypatch.setattr(segment, "DEFAULT", ident)
    monkeypatch.setattr(segment, "_loaded", (ident, adapter))


def test_each_region_is_paired_with_its_own_plane_in_order(harness, monkeypatch) -> None:
    low = _instance(0, 0.3, (40.0, 30.0, 50.0, 40.0))
    high = _instance(0, 0.8, (2.0, 2.0, 12.0, 6.0))
    _use(monkeypatch, _FakeAdapter([low, high]))
    result = segment.run({"prompt": "thing"}, harness)
    envelope = result.structuredContent
    assert envelope["status"] == "ok", envelope
    scores = [r["score"] for r in envelope["regions"]]
    assert scores == [0.8, 0.3]
    pixels = {m["name"]: m["value"] for m in envelope["measurements"]}
    assert pixels["plane_px_0"] == 40 and pixels["plane_px_1"] == 100
    # The diagnostics Tiny reports are only Tiny's: a model that reports none emits none.
    assert not any(n.startswith(("outline_", "retried_")) for n in pixels)
    assert "mask_iou_predicted" not in pixels


def test_a_parameter_the_model_does_not_use_is_refused_even_at_its_default(
    harness, monkeypatch
) -> None:
    adapter = _FakeAdapter([_instance(0, 0.9, (2.0, 2.0, 12.0, 6.0))])
    _use(monkeypatch, adapter, params=("threshold", "max_instances"))
    for unused in ({"pad": 0.05}, {"outline": "whole"}, {"min_cover": 0.2}):
        result = segment.run({"prompt": "thing", **unused}, harness)
        assert result.structuredContent["status"] == "invalid_args", unused
        assert "does not use" in result.structuredContent["summary"]
    assert segment.run({"prompt": "thing", "threshold": 0.3}, harness).isError is False
    assert adapter.settings[-1].threshold == 0.3


def test_an_unknown_model_is_refused(harness, monkeypatch) -> None:
    _use(monkeypatch, _FakeAdapter([]))
    result = segment.run({"prompt": "thing", "model": "nope"}, harness)
    assert result.structuredContent["status"] == "invalid_args"
    assert "fake" in result.structuredContent["summary"]


def test_one_model_is_loaded_at_a_time(monkeypatch) -> None:
    built: list[str] = []

    def builder(files):
        built.append(sorted(files)[0])
        return _FakeAdapter([])

    mac = ("macos-arm64",)
    one = catalogue.Entry("one", "1", "fake", (), True, "MIT", (), ("1.onnx",), 1, 1, mac)
    two = catalogue.Entry("two", "2", "fake", (), False, "MIT", (), ("2.onnx",), 1, 1, mac)
    monkeypatch.setitem(adapters.BUILDERS, "fake", builder)
    monkeypatch.setattr(segment.cache, "artifact", lambda name: Path("/models") / name)
    monkeypatch.setattr(segment, "_loaded", None)
    first = segment.load(one)
    assert segment.load(one) is first and built == ["1.onnx"]
    second = segment.load(two)
    assert second is not first and segment._loaded[0] == "two"
    assert segment.load(one) is not first and built == ["1.onnx", "2.onnx", "1.onnx"]


def test_the_manifest_offers_the_catalogue_s_models() -> None:
    model = segment.SPEC.manifest()["model_schema"]["properties"]["model"]
    assert model["enum"] == [e.id for e in catalogue.read()]
    assert model["default"] == "tiny"

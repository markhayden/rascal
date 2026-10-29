from __future__ import annotations

import pytest

from rascal_tools.manifest import Bounds, Param, Port, ToolSpec
from rascal_tools.tools import catalogue, echo_region


def test_echo_region_lists_both_documents_with_what_rascal_reads() -> None:
    manifest = echo_region.SPEC.manifest()
    for key in (
        "manifest_version",
        "name",
        "version",
        "purpose",
        "when_to_use",
        "when_not_to_use",
        "output_guidance",
        "cost",
        "returns",
        "model_schema",
        "input_schema",
        "output_schema",
    ):
        assert key in manifest, key
    assert manifest["name"] == "echo_region" and manifest["concurrent"] is False
    assert "_harness" in manifest["input_schema"]["required"]
    annotation = echo_region.SPEC.annotation()
    assert annotation["annotation_version"] == 1
    assert [p["name"] for p in annotation["inputs"]] == ["image"]
    assert [(p["name"], p["many"]) for p in annotation["outputs"]] == [
        ("region", True),
        ("plane", True),
    ]
    assert annotation["params"][0] == {
        "name": "bbox",
        "purpose": "The box as fractions of the frame: x0, y0, x1, y1.",
        "kind": "number_array",
        "required": True,
        "length": 4,
    }
    schema = echo_region.SPEC.input_schema()
    assert schema["properties"]["image"]["type"] == "string"
    assert schema["properties"]["bbox"]["minItems"] == 4
    assert schema["required"] == ["image", "bbox"]


def test_bounds_over_rascals_ceilings_are_refused_at_listing() -> None:
    with pytest.raises(ValueError, match="max_images 1000"):
        Bounds(max_images=1000, max_long_edge=1, max_image_bytes=1).check()
    with pytest.raises(ValueError, match="needs max_long_edge"):
        Bounds(max_images=1).check()
    with pytest.raises(ValueError, match="256 MiB"):
        Bounds(max_images=64, max_long_edge=1, max_image_bytes=64 << 20).check()
    spec = ToolSpec(
        name="t",
        version="0.1.0",
        purpose="p",
        when_to_use="w",
        when_not_to_use="n",
        output_guidance="o",
        cost_tier="cheap",
        cost_seconds=0.0,
        inputs=(Port("image", "Image", "i"),),
        outputs=(Port("mask", "Mask", "m"),),
        params=(Param("k", "k", "integer_list"),),
        returns=Bounds(max_images=5),
    )
    with pytest.raises(ValueError):
        spec.manifest()


def test_the_catalogue_hides_the_debug_tool_unless_asked() -> None:
    assert list(catalogue()) == ["echo_region"]
    assert list(catalogue(debug=True)) == ["echo_region", "echo_env"]

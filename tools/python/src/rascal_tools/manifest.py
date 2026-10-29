"""The two documents a tool lists in its `_meta`: the copied-shape manifest
(`schemas/copied/manifest-v1.json`) under `manifest`, and rascal's annotation block
(`schemas/rascal/pack-annotation-v1.json`) under `rascal`. rascal builds its native manifest
from the pair and checks it; the bounds must stay under rascal's ceilings."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

META_MANIFEST = "manifest"
META_RASCAL = "rascal"

# rascal's ceilings (rascal_core::pack): a tool may not declare more.
MAX_IMAGES = 64
MAX_IMAGE_BYTES = 64 << 20
MAX_ENVELOPE_BYTES = 16 << 20
MAX_LONG_EDGE = 32_768


@dataclass(frozen=True)
class Port:
    name: str
    artifact_type: str  # Image | Location | Mask | BandProfile | Series | Plan
    purpose: str
    required: bool = True
    many: bool = False

    def block(self) -> dict[str, Any]:
        out: dict[str, Any] = {
            "name": self.name,
            "artifact_type": self.artifact_type,
            "required": self.required,
            "purpose": self.purpose,
        }
        if self.many:
            out["many"] = True
        return out


@dataclass(frozen=True)
class Param:
    name: str
    purpose: str
    kind: str  # number | integer | boolean | string | number_array | choice | point_list | …
    required: bool = False
    default: Any = None
    min: float | None = None
    max: float | None = None
    length: int | None = None
    choices: tuple[str, ...] = ()

    def block(self) -> dict[str, Any]:
        out: dict[str, Any] = {
            "name": self.name,
            "purpose": self.purpose,
            "kind": self.kind,
            "required": self.required,
        }
        if not self.required:
            out["default"] = self.default
        if self.min is not None:
            out["min"] = self.min
        if self.max is not None:
            out["max"] = self.max
        if self.length is not None:
            out["length"] = self.length
        if self.choices:
            out["choices"] = list(self.choices)
        return out

    def json_schema(self) -> dict[str, Any]:
        """The property schema of one parameter, as rascal derives it."""
        base: dict[str, Any] = {"description": self.purpose}
        if self.kind in ("number", "integer"):
            base["type"] = self.kind
            if self.min is not None:
                base["minimum"] = self.min
            if self.max is not None:
                base["maximum"] = self.max
        elif self.kind == "boolean":
            base["type"] = "boolean"
        elif self.kind == "string":
            base["type"] = "string"
        elif self.kind == "choice":
            base["type"] = "string"
            base["enum"] = list(self.choices)
        elif self.kind == "number_array":
            base["type"] = "array"
            base["items"] = {"type": "number"}
            if self.length is not None:
                base["minItems"] = self.length
                base["maxItems"] = self.length
        elif self.kind == "integer_list":
            base["type"] = "array"
            base["items"] = {"type": "integer", "minimum": 0}
        elif self.kind == "box_list":
            base["type"] = "array"
            base["items"] = {
                "type": "array",
                "items": {"type": "number"},
                "minItems": 4,
                "maxItems": 4,
            }
        elif self.kind == "point_list":
            base["type"] = "array"
            base["minItems"] = 2
            base["items"] = {
                "type": "array",
                "items": {"type": "number"},
                "minItems": 2,
                "maxItems": 2,
            }
        if not self.required:
            base["default"] = self.default
        return base


@dataclass(frozen=True)
class Bounds:
    max_images: int = 0
    max_long_edge: int = 0
    max_image_bytes: int = 0
    max_summary_bytes: int = 1024
    max_regions: int = 64
    max_region_label_bytes: int = 256
    max_measurements: int = 64
    max_transforms: int = 0
    max_envelope_bytes: int = 1 << 20

    def block(self) -> dict[str, Any]:
        return dict(self.__dict__)

    def check(self) -> None:
        """rascal's ceilings, checked here so a listing never fails at the client."""
        if self.max_images > MAX_IMAGES:
            raise ValueError(f"max_images {self.max_images} is over rascal's ceiling {MAX_IMAGES}")
        if self.max_long_edge > MAX_LONG_EDGE:
            raise ValueError(f"max_long_edge {self.max_long_edge} is over {MAX_LONG_EDGE}")
        if self.max_image_bytes > MAX_IMAGE_BYTES:
            raise ValueError(f"max_image_bytes {self.max_image_bytes} is over {MAX_IMAGE_BYTES}")
        if self.max_envelope_bytes > MAX_ENVELOPE_BYTES:
            raise ValueError(f"max_envelope_bytes over {MAX_ENVELOPE_BYTES}")
        if self.max_images and not (self.max_long_edge and self.max_image_bytes):
            raise ValueError("a tool that returns images needs max_long_edge and max_image_bytes")
        answer = (self.max_images * self.max_image_bytes * 4 + 2) // 3 + self.max_envelope_bytes
        if answer > 256 << 20:
            raise ValueError("the largest answer would exceed the 256 MiB one line carries")


@dataclass(frozen=True)
class ToolSpec:
    """Everything a tool says about itself; `manifest()` and `annotation()` are the two
    documents, `input_schema()` the MCP tool schema (the ports as string ids plus the
    parameters, as rascal's own server lists them)."""

    name: str
    version: str
    purpose: str
    when_to_use: str
    when_not_to_use: str
    output_guidance: str
    cost_tier: str
    cost_seconds: float
    inputs: tuple[Port, ...]
    outputs: tuple[Port, ...]
    params: tuple[Param, ...] = ()
    constants: tuple[str, ...] = ()
    returns: Bounds = field(default_factory=Bounds)
    concurrent: bool = False

    def input_schema(self) -> dict[str, Any]:
        properties: dict[str, Any] = {}
        required: list[str] = []
        for port in self.inputs:
            properties[port.name] = (
                {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": f"{port.purpose} ({port.artifact_type} artifact ids, in order)",
                }
                if port.many
                else {
                    "type": "string",
                    "description": f"{port.purpose} ({port.artifact_type} artifact id)",
                }
            )
            if port.required:
                required.append(port.name)
        for param in self.params:
            properties[param.name] = param.json_schema()
            if param.required:
                required.append(param.name)
        if self.constants:
            properties["constants"] = {
                "type": "string",
                "enum": list(self.constants),
                "description": "The constant set; the first is the default.",
            }
        return {
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": f"{self.name} arguments",
            "type": "object",
            "additionalProperties": False,
            "properties": properties,
            "required": required,
        }

    def manifest(self) -> dict[str, Any]:
        """The copied-shape manifest. `model_schema` is the parameters alone, `input_schema`
        the parameters plus the `_harness` context, `output_schema` names the measurements."""
        self.returns.check()
        params_schema = {
            "type": "object",
            "additionalProperties": False,
            "properties": {p.name: p.json_schema() for p in self.params},
            "required": [p.name for p in self.params if p.required],
        }
        return {
            "manifest_version": 1,
            "name": self.name,
            "version": self.version,
            "purpose": self.purpose,
            "when_to_use": self.when_to_use,
            "when_not_to_use": self.when_not_to_use,
            "output_guidance": self.output_guidance,
            "cost": {"tier": self.cost_tier, "seconds": self.cost_seconds},
            "returns": self.returns.block(),
            "model_schema": params_schema,
            "input_schema": {
                **params_schema,
                "properties": {
                    **params_schema["properties"],
                    "_harness": {"$ref": "urn:rascal:schemas:rascal:pack-context-v1"},
                },
                "required": [*params_schema["required"], "_harness"],
            },
            "output_schema": {"type": "object"},
            "concurrent": self.concurrent,
            "image_args": [p.name for p in self.inputs if p.artifact_type == "Image"],
        }

    def annotation(self) -> dict[str, Any]:
        out: dict[str, Any] = {
            "annotation_version": 1,
            "inputs": [p.block() for p in self.inputs],
            "outputs": [p.block() for p in self.outputs],
            "params": [p.block() for p in self.params],
        }
        if self.constants:
            out["constants"] = list(self.constants)
        return out

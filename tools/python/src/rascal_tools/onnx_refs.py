"""The external-data locations an ONNX graph names, read from its protobuf without the `onnx`
package (the pack depends on onnxruntime only): every tensor's `external_data` entry whose key
is `location`, in the graph's initializers, sparse initializers, and node attributes, nested
graphs included. Used to keep a graph's external data inside its installation."""

from __future__ import annotations

import mmap
from collections.abc import Iterator
from pathlib import Path

# Field numbers (onnx.proto3).
MODEL_GRAPH = 7
GRAPH_NODE, GRAPH_INITIALIZER, GRAPH_SPARSE_INITIALIZER = 1, 5, 15
NODE_ATTRIBUTE = 5
ATTR_T, ATTR_G, ATTR_TENSORS, ATTR_GRAPHS, ATTR_SPARSE, ATTR_SPARSES = 5, 6, 10, 11, 22, 23
SPARSE_VALUES, SPARSE_INDICES = 1, 2
TENSOR_EXTERNAL_DATA = 13
ENTRY_KEY, ENTRY_VALUE = 1, 2


class OnnxReadError(ValueError):
    """The bytes are not a readable protobuf."""


Span = tuple[int, int]  # [start, end) offsets into the buffer


def _varint(data, i: int, end: int) -> tuple[int, int]:
    shift = value = 0
    while True:
        if i >= end:
            raise OnnxReadError("truncated varint")
        b = data[i]
        i += 1
        value |= (b & 0x7F) << shift
        if not b & 0x80:
            return value, i
        shift += 7
        if shift > 63:
            raise OnnxReadError("varint too long")


def fields(data, span: Span) -> Iterator[tuple[int, int, Span | None]]:
    """Each field in `span` as (number, wire type, the value's span for a length-delimited
    field, else None). Offsets only: the buffer is never sliced, so a map can close after."""
    i, end = span
    while i < end:
        key, i = _varint(data, i, end)
        number, wire = key >> 3, key & 7
        if wire == 0:
            _, i = _varint(data, i, end)
            yield number, wire, None
        elif wire == 1:
            i += 8
            yield number, wire, None
        elif wire == 2:
            length, i = _varint(data, i, end)
            if i + length > end:
                raise OnnxReadError("truncated field")
            yield number, wire, (i, i + length)
            i += length
        elif wire == 5:
            i += 4
            yield number, wire, None
        else:
            raise OnnxReadError(f"wire type {wire}")
    if i != end:
        raise OnnxReadError("truncated field")


def _sub(data, span: Span, number: int) -> Iterator[Span]:
    for n, wire, value in fields(data, span):
        if n == number and wire == 2 and value is not None:
            yield value


def _text(data, span: Span) -> str:
    return bytes(data[span[0] : span[1]]).decode("utf-8")


def _tensor(data, span: Span) -> Iterator[str]:
    for entry in _sub(data, span, TENSOR_EXTERNAL_DATA):
        key = value = None
        for n, wire, v in fields(data, entry):
            if wire == 2 and v is not None and n == ENTRY_KEY:
                key = _text(data, v)
            elif wire == 2 and v is not None and n == ENTRY_VALUE:
                value = _text(data, v)
        if key == "location" and value is not None:
            yield value


def _sparse(data, span: Span) -> Iterator[str]:
    for part in (SPARSE_VALUES, SPARSE_INDICES):
        for tensor in _sub(data, span, part):
            yield from _tensor(data, tensor)


def _graph(data, span: Span) -> Iterator[str]:
    for tensor in _sub(data, span, GRAPH_INITIALIZER):
        yield from _tensor(data, tensor)
    for sparse in _sub(data, span, GRAPH_SPARSE_INITIALIZER):
        yield from _sparse(data, sparse)
    for node in _sub(data, span, GRAPH_NODE):
        for attribute in _sub(data, node, NODE_ATTRIBUTE):
            for n, wire, v in fields(data, attribute):
                if wire != 2 or v is None:
                    continue
                if n in (ATTR_T, ATTR_TENSORS):
                    yield from _tensor(data, v)
                elif n in (ATTR_G, ATTR_GRAPHS):
                    yield from _graph(data, v)
                elif n in (ATTR_SPARSE, ATTR_SPARSES):
                    yield from _sparse(data, v)


def external_locations(path: Path) -> list[str]:
    """Every distinct external-data location the graph at `path` names, in order. The file is
    mapped, not read: a graph with its weights inside can be hundreds of megabytes."""
    seen: list[str] = []
    size = path.stat().st_size
    if size == 0:
        return seen
    with path.open("rb") as f, mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ) as data:
        for graph in _sub(data, (0, size), MODEL_GRAPH):
            for location in _graph(data, graph):
                if location not in seen:
                    seen.append(location)
    return seen

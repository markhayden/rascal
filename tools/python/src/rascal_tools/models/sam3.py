"""SAM 3 (`facebook/sam3`) on onnxruntime, end to end (MODEL.md "SAM 3"; SPEC.md "The adapters"):
the text query finds the instances and SAM 3 draws each one's mask itself, as the lab's
`sam3-own` cell runs it. No OWLv2, no outliner, no pick and no retry.

Three graphs: `sam3.vision.onnx` once per image, `sam3.text.onnx` once per query, and
`sam3.head.onnx` once per (image, query). The pre- and post-processing mirror Transformers'
`Sam3Processor` (5.22.1):

- the image resized to 1008x1008 as torchvision resizes a uint8 image (bilinear, antialiased:
  `resize_uint8`, bit for bit; Pillow's own resize differs by one level in about 0.5% of values,
  which moved one corpus image's core to IoU 0.990), then `(x - 127.5) / 127.5` (the processor's
  rescale by 1/255 and normalise with mean and deviation 0.5, fused);
- the query tokenised with SAM 3's CLIP tokenizer, padded to 32 tokens with the attention mask
  (and cut at 32, where the processor would fail on a longer query);
- an instance's score is sigmoid(logit) * sigmoid(presence logit); kept at `threshold` or above;
  its mask is the sigmoid of the head's 288x288 mask logits, resized to the image (bilinear,
  `align_corners=False`, as `torch.nn.functional.interpolate`) and kept above 0.5; an empty mask
  gives no instance; its region is the predicted box in image pixels, clipped to the image.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

import numpy as np

from rascal_tools.models.adapters import Instance, Settings

SIZE = 1008
MAX_TOKENS = 32
VISION_OUT = ("fpn0", "fpn1", "fpn2", "pos0", "pos1", "pos2")


def _aa_weights(n_in: int, n_out: int) -> tuple[np.ndarray, np.ndarray, int]:
    """One axis of torchvision's antialiased bilinear resize of a uint8 image (PyTorch's
    `_upsample_bilinear2d_aa` with int16 weights): per output index the first input index, the
    fixed-point weights and their precision in bits."""
    scale = n_in / n_out
    support = scale if scale >= 1.0 else 1.0
    invscale = 1.0 / scale if scale >= 1.0 else 1.0
    ksize = int(np.ceil(support)) * 2 + 1
    first = np.zeros(n_out, dtype=np.int64)
    weights = np.zeros((n_out, ksize), dtype=np.float64)
    for i in range(n_out):
        center = scale * (i + 0.5)
        xmin = max(int(center - support + 0.5), 0)
        xsize = min(int(center + support + 0.5), n_in) - xmin
        x = np.abs((np.arange(xsize) + xmin - center + 0.5) * invscale)
        w = np.where(x < 1.0, 1.0 - x, 0.0)
        total = w.sum()
        first[i] = xmin
        weights[i, :xsize] = w / total if total != 0 else w
    peak = weights.max()
    precision = 0
    while precision < 22 and int(0.5 + peak * (1 << (precision + 1))) < (1 << 15):
        precision += 1
    v = weights * (1 << precision)
    fixed = np.where(v < 0, np.ceil(v - 0.5), np.floor(v + 0.5)).astype(np.int64)
    return first, fixed, precision


def _resample(a: np.ndarray, n_out: int, axis: int) -> np.ndarray:
    n_in = a.shape[axis]
    if n_in == n_out:
        return a
    first, fixed, precision = _aa_weights(n_in, n_out)
    src = np.moveaxis(a, axis, 0).astype(np.int64)
    src = np.concatenate([src, np.zeros((fixed.shape[1],) + src.shape[1:], dtype=np.int64)])
    acc = np.full((n_out,) + src.shape[1:], 1 << (precision - 1), dtype=np.int64)
    shape = (-1,) + (1,) * (src.ndim - 1)
    for j in range(fixed.shape[1]):
        acc += fixed[:, j].reshape(shape) * src[first + j]
    return np.moveaxis(np.clip(acc >> precision, 0, 255).astype(np.uint8), 0, axis)


def resize_uint8(pixels: np.ndarray, height: int, width: int) -> np.ndarray:
    """An [h, w, c] uint8 image resized as torchvision's `resize(..., antialias=True)` (bilinear)
    resizes a uint8 tensor: horizontally, then vertically, each pass rounded to uint8."""
    return _resample(_resample(pixels, width, 1), height, 0)


def preprocess(pixels: np.ndarray) -> np.ndarray:
    """The image as `pixel_values` [1, 3, 1008, 1008], float32."""
    resized = resize_uint8(np.ascontiguousarray(pixels[..., :3], dtype=np.uint8), SIZE, SIZE)
    x = (resized.astype(np.float32) - np.float32(127.5)) / np.float32(127.5)
    return np.ascontiguousarray(x.transpose(2, 0, 1)[None])


def _weights(n_out: int, n_in: int) -> np.ndarray:
    """The [n_out, n_in] matrix of 1-D bilinear interpolation with `align_corners=False`, as
    PyTorch computes it (a negative source coordinate clamped to 0)."""
    scale = n_in / n_out
    src = np.maximum((np.arange(n_out, dtype=np.float64) + 0.5) * scale - 0.5, 0.0)
    i0 = np.minimum(np.floor(src).astype(np.int64), n_in - 1)
    i1 = np.minimum(i0 + 1, n_in - 1)
    frac = src - i0
    w = np.zeros((n_out, n_in), dtype=np.float64)
    rows = np.arange(n_out)
    np.add.at(w, (rows, i0), 1.0 - frac)
    np.add.at(w, (rows, i1), frac)
    return w


def resize_probabilities(prob: np.ndarray, height: int, width: int) -> np.ndarray:
    """A [h, w] map resized bilinearly to [height, width] (`align_corners=False`)."""
    h, w = prob.shape
    return _weights(height, h) @ prob.astype(np.float64) @ _weights(width, w).T


def sigmoid(x: np.ndarray) -> np.ndarray:
    return 1.0 / (1.0 + np.exp(-x.astype(np.float64)))


def load_tokenizer(path: Path) -> Any:
    """SAM 3's CLIP tokenizer, padded with its end-of-text token to 32 tokens and cut at 32."""
    from tokenizers import Tokenizer

    tokenizer = Tokenizer.from_file(str(path))
    end = tokenizer.token_to_id("<|endoftext|>")
    tokenizer.enable_padding(length=MAX_TOKENS, pad_id=end, pad_token="<|endoftext|>")
    tokenizer.enable_truncation(MAX_TOKENS)
    return tokenizer


class Sam3:
    """The three sessions and the tokenizer, loaded once."""

    def __init__(self, vision: Path, text: Path, head: Path, tokenizer: Path) -> None:
        import onnxruntime as ort

        cpu = ["CPUExecutionProvider"]
        self.vision = ort.InferenceSession(str(vision), providers=cpu)
        self.text = ort.InferenceSession(str(text), providers=cpu)
        self.head = ort.InferenceSession(str(head), providers=cpu)
        self.tokenizer = load_tokenizer(tokenizer)

    def tokenize(self, query: str) -> tuple[np.ndarray, np.ndarray]:
        encoding = self.tokenizer.encode(query)
        ids = np.asarray([encoding.ids], dtype=np.int64)
        mask = np.asarray([encoding.attention_mask], dtype=np.int64)
        return ids, mask

    def find(self, pixels: np.ndarray, queries: list[str], settings: Settings) -> list[Instance]:
        height, width = pixels.shape[:2]
        features = dict(
            zip(
                VISION_OUT, self.vision.run(None, {"pixel_values": preprocess(pixels)}), strict=True
            )
        )
        out: list[Instance] = []
        for q, query in enumerate(queries):
            ids, mask = self.tokenize(query)
            (text,) = self.text.run(None, {"input_ids": ids, "attention_mask": mask})
            logits, boxes, masks, presence = self.head.run(
                None, {**features, "text_features": text, "attention_mask": mask}
            )
            out += instances(q, logits[0], boxes[0], masks[0], presence[0], settings, width, height)
        return out


def instances(
    query: int,
    logits: np.ndarray,
    boxes: np.ndarray,
    masks: np.ndarray,
    presence: np.ndarray,
    settings: Settings,
    width: int,
    height: int,
) -> list[Instance]:
    """One query's head outputs (logits [N], boxes [N, 4] as xyxy fractions, mask logits
    [N, 288, 288], presence [1]) to its instances."""
    scores = sigmoid(logits) * sigmoid(presence.reshape(-1)[:1])[0]
    found = []
    for n in np.flatnonzero(scores >= settings.threshold):
        plane = resize_probabilities(sigmoid(masks[n]), height, width) > 0.5
        if not plane.any():
            continue
        x0, y0, x1, y1 = (float(v) for v in boxes[n] * np.array([width, height, width, height]))
        box = (
            min(max(x0, 0.0), width),
            min(max(y0, 0.0), height),
            min(max(x1, 0.0), width),
            min(max(y1, 0.0), height),
        )
        found.append(Instance(query=query, score=float(scores[n]), box=box, plane=plane))
    return found


def build(files: dict[str, Any]) -> Sam3:
    return Sam3(
        files["sam3.vision.onnx"],
        files["sam3.text.onnx"],
        files["sam3.head.onnx"],
        files["sam3.tokenizer.json"],
    )

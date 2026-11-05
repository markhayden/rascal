"""SAM 2.1 Hiera-Tiny (`facebook/sam2.1-hiera-tiny`): a box, and optional positive points, to
three candidate outlines, on the two ONNX graphs `MODEL.md` describes. Preprocessing mirrors
`Sam2ImageProcessor`: the image is resized to 1024 by 1024 (bilinear, Pillow), scaled to [0, 1]
and normalised with the ImageNet mean and deviation; the box (as its two corners, labels 2 and
3, the way SAM 2's own predictor passes a box) and the points (label 1) are scaled into those
1024 coordinates. Each outline is 256 by 256 logits, resized to the image (bilinear) and
thresholded at zero, as the upstream post-processing does."""

from __future__ import annotations

from pathlib import Path

import numpy as np
from PIL import Image

SIZE = 1024
LOW_RES = 256
MEAN = np.array([0.485, 0.456, 0.406], dtype=np.float32)
STD = np.array([0.229, 0.224, 0.225], dtype=np.float32)
MAX_POINTS = 8


def preprocess(pixels: np.ndarray) -> np.ndarray:
    """`pixel_values` [1, 3, 1024, 1024]."""
    resized = Image.fromarray(pixels).resize((SIZE, SIZE), Image.Resampling.BILINEAR)
    array = np.asarray(resized, dtype=np.float32) / 255.0
    array = (array - MEAN) / STD
    return np.ascontiguousarray(array.transpose(2, 0, 1)[None])


def prompt(
    box: tuple[float, float, float, float],
    points: list[tuple[float, float]],
    width: int,
    height: int,
) -> tuple[np.ndarray, np.ndarray]:
    """`points` [1, P, 2] float32 and `point_labels` [1, P] int64 in the 1024-pixel frame: the
    box's corners (labels 2, 3), then each positive point (label 1); at most `MAX_POINTS`."""
    if len(points) > MAX_POINTS:
        raise ValueError(f"{len(points)} points; the decoder takes at most {MAX_POINTS}")
    sx, sy = SIZE / width, SIZE / height
    x0, y0, x1, y1 = box
    coords = [(x0 * sx, y0 * sy), (x1 * sx, y1 * sy)] + [(x * sx, y * sy) for x, y in points]
    labels = [2, 3] + [1] * len(points)
    return (
        np.array([coords], dtype=np.float32),
        np.array([labels], dtype=np.int64),
    )


def plane_of(logits: np.ndarray, width: int, height: int) -> np.ndarray:
    """The low-resolution mask logits resized to the image and thresholded: a bool plane."""
    full = Image.fromarray(np.ascontiguousarray(logits, dtype=np.float32), mode="F").resize(
        (width, height), Image.Resampling.BILINEAR
    )
    return np.asarray(full) > 0.0


class Segmenter:
    """The encoder and decoder graphs, loaded once."""

    def __init__(self, encoder: Path, decoder: Path) -> None:
        import onnxruntime as ort

        options = ort.SessionOptions()
        options.log_severity_level = 3
        providers = ["CPUExecutionProvider"]
        self.encoder = ort.InferenceSession(str(encoder), options, providers=providers)
        self.decoder = ort.InferenceSession(str(decoder), options, providers=providers)

    def embed(self, pixels: np.ndarray) -> list[np.ndarray]:
        return self.encoder.run(None, {"pixel_values": preprocess(pixels)})

    def outlines(
        self,
        features: list[np.ndarray],
        box: tuple[float, float, float, float],
        points: list[tuple[float, float]],
        width: int,
        height: int,
    ) -> tuple[list[np.ndarray], list[float]]:
        """The decoder's three candidate outlines for one box and its positive points, as bool
        planes in image pixels, and the predicted IoU of each, in the decoder's order."""
        coords, labels = prompt(box, points, width, height)
        masks, ious = self.decoder.run(
            None,
            {
                "feat0": features[0],
                "feat1": features[1],
                "feat2": features[2],
                "points": coords,
                "point_labels": labels,
            },
        )
        planes = [plane_of(masks[0, k], width, height) for k in range(masks.shape[1])]
        return planes, [float(v) for v in ious[0]]

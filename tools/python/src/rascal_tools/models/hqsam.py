"""HQ-SAM ViT-L (`lkeab/hq-sam`, `sam_hq_vit_l.pth`): a box, and optional positive points, to one
outline from the high-quality output alone (`hq_token_only=True`, `multimask_output=False`), on
the two ONNX graphs `MODEL.md` describes. Preprocessing mirrors `segment_anything_hq`'s
`SamPredictor`: the image's longest side is resized to 1024 (Pillow bilinear, the rounding of
`ResizeLongestSide`), normalised with SAM's pixel mean and deviation (0-255 scale) and padded with
zeros at the bottom and right to 1024 by 1024; the box (its two corners, labels 2 and 3) and the
points (label 1) are scaled into the resized frame. The decoder graph carries SAM's own mask
post-processing (upscale to 1024, crop, resize to the image), so its output is already in image
pixels and is thresholded at zero."""

from __future__ import annotations

from pathlib import Path

import numpy as np
from PIL import Image

SIZE = 1024
MEAN = np.array([123.675, 116.28, 103.53], dtype=np.float32)
STD = np.array([58.395, 57.12, 57.375], dtype=np.float32)
MAX_POINTS = 8


def resized_shape(height: int, width: int) -> tuple[int, int]:
    """`ResizeLongestSide.get_preprocess_shape`: (new height, new width), longest side 1024."""
    scale = SIZE * 1.0 / max(height, width)
    return int(height * scale + 0.5), int(width * scale + 0.5)


def preprocess(pixels: np.ndarray) -> np.ndarray:
    """`pixel_values` [1, 3, 1024, 1024]: resized, normalised, padded at the bottom and right."""
    height, width = pixels.shape[:2]
    new_h, new_w = resized_shape(height, width)
    resized = Image.fromarray(pixels).resize((new_w, new_h), Image.Resampling.BILINEAR)
    array = (np.asarray(resized, dtype=np.float32) - MEAN) / STD
    out = np.zeros((1, 3, SIZE, SIZE), dtype=np.float32)
    out[0, :, :new_h, :new_w] = array.transpose(2, 0, 1)
    return out


def prompt(
    box: tuple[float, float, float, float],
    points: list[tuple[float, float]],
    width: int,
    height: int,
) -> tuple[np.ndarray, np.ndarray]:
    """`point_coords` [1, P, 2] and `point_labels` [1, P], float32, in the resized frame: each
    positive point (label 1), then the box's corners (labels 2, 3), the order SAM's prompt
    encoder embeds them in; at most `MAX_POINTS` points."""
    if len(points) > MAX_POINTS:
        raise ValueError(f"{len(points)} points; the decoder takes at most {MAX_POINTS}")
    new_h, new_w = resized_shape(height, width)
    sx, sy = new_w / width, new_h / height
    x0, y0, x1, y1 = box
    coords = [(x * sx, y * sy) for x, y in points] + [(x0 * sx, y0 * sy), (x1 * sx, y1 * sy)]
    labels = [1.0] * len(points) + [2.0, 3.0]
    return np.array([coords], dtype=np.float32), np.array([labels], dtype=np.float32)


class Segmenter:
    """The encoder and decoder graphs, loaded once. One outline per box: `outlines` returns one
    plane and its predicted IoU, so rascal's pick has nothing to choose and its retry runs."""

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
        coords, labels = prompt(box, points, width, height)
        masks, scores = self.decoder.run(
            None,
            {
                "image_embeddings": features[0],
                "interm_embeddings": features[1],
                "point_coords": coords,
                "point_labels": labels,
                "orig_im_size": np.array([height, width], dtype=np.float32),
            },
        )
        return [masks[0, 0] > 0.0], [float(scores[0, 0])]

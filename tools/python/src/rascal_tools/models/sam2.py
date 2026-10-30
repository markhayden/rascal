"""SAM 2.1 Hiera-Tiny (`facebook/sam2.1-hiera-tiny`): a box to a plane, on the two ONNX graphs
`MODEL.md` describes. Preprocessing mirrors `Sam2ImageProcessor`: the image is resized to
1024 by 1024 (bilinear, Pillow), scaled to [0, 1] and normalised with the ImageNet mean and
deviation; a box is scaled into those 1024 coordinates. The decoder's mask is 256 by 256
logits, resized to the image (bilinear) and thresholded at zero, as the upstream
post-processing does."""

from __future__ import annotations

from pathlib import Path

import numpy as np
from PIL import Image

SIZE = 1024
LOW_RES = 256
MEAN = np.array([0.485, 0.456, 0.406], dtype=np.float32)
STD = np.array([0.229, 0.224, 0.225], dtype=np.float32)


def preprocess(pixels: np.ndarray) -> np.ndarray:
    """`pixel_values` [1, 3, 1024, 1024]."""
    resized = Image.fromarray(pixels).resize((SIZE, SIZE), Image.Resampling.BILINEAR)
    array = np.asarray(resized, dtype=np.float32) / 255.0
    array = (array - MEAN) / STD
    return np.ascontiguousarray(array.transpose(2, 0, 1)[None])


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

    def planes(
        self,
        features: list[np.ndarray],
        boxes: list[tuple[float, float, float, float]],
        width: int,
        height: int,
    ) -> tuple[list[np.ndarray], list[float]]:
        """One bool plane per box, in image pixels, and the decoder's predicted IoU of each."""
        if not boxes:
            return [], []
        scale = np.array([SIZE / width, SIZE / height, SIZE / width, SIZE / height])
        planes: list[np.ndarray] = []
        ious: list[float] = []
        # The exported decoder takes one box per call (MODEL.md); the features are reused.
        for box in boxes:
            scaled = (np.array(box, dtype=np.float64) * scale).astype(np.float32)[None, None]
            masks, iou = self.decoder.run(
                None,
                {
                    "feat0": features[0],
                    "feat1": features[1],
                    "feat2": features[2],
                    "boxes": scaled,
                },
            )
            planes.append(plane_of(masks[0, 0], width, height))
            ious.append(float(iou[0, 0]))
        return planes, ious

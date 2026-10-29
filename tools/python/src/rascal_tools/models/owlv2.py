"""OWLv2 base (`google/owlv2-base-patch16`): text queries to boxes with scores, on the ONNX
graph `MODEL.md` describes. Preprocessing mirrors `Owlv2ImageProcessor`: the image is padded
to a square at the bottom and right with mid-grey, resized to 960 by 960 (bilinear, Pillow),
scaled to [0, 1] and normalised with the CLIP mean and deviation. The graph returns one logit
per proposal and query and one box per proposal as fractions of the padded square; a box's
score is the sigmoid of its logit for the query."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

import numpy as np
from PIL import Image

SIZE = 960
MAX_TOKENS = 16
PAD_VALUE = 127
MEAN = np.array([0.48145466, 0.4578275, 0.40821073], dtype=np.float32)
STD = np.array([0.26862954, 0.26130258, 0.27577711], dtype=np.float32)


@dataclass(frozen=True)
class Detection:
    """A box in image pixels (x0, y0, x1, y1), its score, and the query it answered."""

    box: tuple[float, float, float, float]
    score: float
    query: int


def preprocess(pixels: np.ndarray) -> tuple[np.ndarray, int]:
    """`pixel_values` [1, 3, 960, 960] and the padded square's side in image pixels."""
    height, width = pixels.shape[:2]
    side = max(height, width)
    padded = np.full((side, side, 3), PAD_VALUE, dtype=np.uint8)
    padded[:height, :width] = pixels
    resized = Image.fromarray(padded).resize((SIZE, SIZE), Image.Resampling.BILINEAR)
    array = np.asarray(resized, dtype=np.float32) / 255.0
    array = (array - MEAN) / STD
    return np.ascontiguousarray(array.transpose(2, 0, 1)[None]), side


def sigmoid(x: np.ndarray) -> np.ndarray:
    return 1.0 / (1.0 + np.exp(-x))


def iou(box: np.ndarray, others: np.ndarray) -> np.ndarray:
    """Intersection over union of one box against many, all (x0, y0, x1, y1)."""
    x0 = np.maximum(box[0], others[:, 0])
    y0 = np.maximum(box[1], others[:, 1])
    x1 = np.minimum(box[2], others[:, 2])
    y1 = np.minimum(box[3], others[:, 3])
    inter = np.clip(x1 - x0, 0, None) * np.clip(y1 - y0, 0, None)
    area = (box[2] - box[0]) * (box[3] - box[1])
    areas = (others[:, 2] - others[:, 0]) * (others[:, 3] - others[:, 1])
    return inter / np.maximum(area + areas - inter, 1e-9)


def nms(boxes: np.ndarray, scores: np.ndarray, threshold: float) -> list[int]:
    """The indices kept by greedy non-maximum suppression, best score first."""
    order = np.argsort(-scores, kind="stable")
    kept: list[int] = []
    while order.size:
        best = int(order[0])
        kept.append(best)
        rest = order[1:]
        if rest.size == 0:
            break
        overlap = iou(boxes[best], boxes[rest])
        order = rest[overlap < threshold]
    return kept


class Detector:
    """The graph and its tokenizer, loaded once."""

    def __init__(self, graph: Path, tokenizer: Path) -> None:
        import onnxruntime as ort
        from tokenizers import Tokenizer

        options = ort.SessionOptions()
        options.log_severity_level = 3
        self.session = ort.InferenceSession(str(graph), options, providers=["CPUExecutionProvider"])
        self.tokenizer = Tokenizer.from_file(str(tokenizer))
        self.tokenizer.enable_padding(length=MAX_TOKENS, pad_id=0, pad_token="!")
        self.tokenizer.enable_truncation(MAX_TOKENS)

    def tokenize(self, queries: list[str]) -> tuple[np.ndarray, np.ndarray]:
        """`input_ids` and `attention_mask`, [Q, 16] int64, one row per query."""
        encodings = self.tokenizer.encode_batch([q.lower() for q in queries])
        ids = np.array([e.ids for e in encodings], dtype=np.int64)
        mask = np.array([e.attention_mask for e in encodings], dtype=np.int64)
        return ids, mask

    def detect(
        self,
        pixels: np.ndarray,
        queries: list[str],
        threshold: float,
        max_instances: int,
        nms_threshold: float = 0.5,
    ) -> list[Detection]:
        """The boxes over `threshold`, per query after suppression, best first, at most
        `max_instances` across the queries."""
        height, width = pixels.shape[:2]
        pixel_values, side = preprocess(pixels)
        ids, mask = self.tokenize(queries)
        logits, boxes, _ = self.session.run(
            None, {"pixel_values": pixel_values, "input_ids": ids, "attention_mask": mask}
        )
        scores = sigmoid(logits[0].astype(np.float64))  # [proposals, queries]
        cxcywh = boxes[0].astype(np.float64) * side
        xyxy = np.stack(
            [
                cxcywh[:, 0] - cxcywh[:, 2] / 2,
                cxcywh[:, 1] - cxcywh[:, 3] / 2,
                cxcywh[:, 0] + cxcywh[:, 2] / 2,
                cxcywh[:, 1] + cxcywh[:, 3] / 2,
            ],
            axis=1,
        )
        xyxy[:, [0, 2]] = np.clip(xyxy[:, [0, 2]], 0, width)
        xyxy[:, [1, 3]] = np.clip(xyxy[:, [1, 3]], 0, height)
        found: list[Detection] = []
        for q in range(len(queries)):
            over = np.flatnonzero(scores[:, q] > threshold)
            if over.size == 0:
                continue
            candidates = xyxy[over]
            candidate_scores = scores[over, q]
            for k in nms(candidates, candidate_scores, nms_threshold):
                box = candidates[k]
                if box[2] - box[0] < 1 or box[3] - box[1] < 1:
                    continue
                found.append(
                    Detection(
                        box=(float(box[0]), float(box[1]), float(box[2]), float(box[3])),
                        score=float(candidate_scores[k]),
                        query=q,
                    )
                )
        found.sort(key=lambda d: (-d.score, d.query, d.box))
        return found[:max_instances]

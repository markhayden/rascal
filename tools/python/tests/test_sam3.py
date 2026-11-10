"""The `sam3` adapter's pre- and post-processing and applicability with fake graphs (SPEC.md "The
adapters"; MODEL.md "SAM 3"). The real model's parity is test_model_parity.py."""

from __future__ import annotations

import numpy as np
import pytest

from rascal_tools import catalogue
from rascal_tools.models import adapters, sam3


def test_the_image_is_resized_to_1008_and_normalised_to_minus_one_to_one() -> None:
    pixels = np.zeros((40, 60, 3), dtype=np.uint8)
    pixels[..., 0] = 255
    x = sam3.preprocess(pixels)
    assert x.shape == (1, 3, 1008, 1008) and x.dtype == np.float32
    assert np.allclose(x[0, 0], 1.0) and np.allclose(x[0, 1:], -1.0)


# torchvision 0.29.2's resize(antialias=True, bilinear) of this uint8 image (torch 2.15.0, CPU).
SRC = [
    [139, 74, 229, 241, 169, 65, 6, 160, 149],
    [106, 38, 175, 188, 205, 175, 229, 98, 249],
    [10, 148, 95, 86, 147, 198, 66, 39, 106],
    [213, 171, 45, 167, 57, 69, 81, 55, 14],
    [153, 178, 215, 76, 52, 39, 250, 72, 215],
    [50, 161, 223, 112, 172, 161, 233, 38, 17],
    [89, 1, 221, 6, 242, 127, 143, 6, 60],
]
TORCHVISION = {
    (3, 4): [[104, 172, 141, 137], [141, 117, 107, 87], [122, 135, 148, 83]],
    (5, 9): [
        [128, 62, 211, 223, 181, 102, 80, 139, 182],
        [53, 99, 131, 131, 173, 188, 138, 65, 170],
        [165, 168, 85, 136, 72, 87, 109, 55, 67],
        [107, 170, 219, 92, 105, 93, 242, 57, 127],
        [76, 54, 222, 41, 219, 138, 173, 17, 46],
    ],
    (11, 13): [
        [139, 104, 110, 217, 236, 219, 169, 97, 42, 18, 124, 155, 149],
        [124, 88, 92, 193, 212, 207, 185, 137, 112, 109, 126, 161, 194],
        [97, 70, 76, 158, 174, 185, 200, 184, 192, 205, 120, 159, 236],
        [36, 80, 118, 117, 115, 129, 163, 183, 160, 106, 68, 97, 145],
        [84, 123, 138, 83, 100, 115, 114, 139, 120, 69, 51, 58, 73],
        [213, 190, 142, 55, 120, 133, 57, 65, 74, 79, 61, 36, 14],
        [175, 175, 171, 155, 126, 92, 54, 51, 103, 179, 94, 101, 142],
        [125, 151, 184, 214, 136, 86, 85, 76, 139, 231, 105, 108, 161],
        [59, 115, 176, 217, 153, 124, 161, 153, 183, 220, 86, 38, 35],
        [71, 73, 108, 210, 119, 102, 210, 163, 158, 171, 58, 30, 40],
        [89, 42, 52, 204, 89, 79, 242, 162, 133, 132, 38, 31, 60],
    ],
}


@pytest.mark.parametrize("size", sorted(TORCHVISION))
def test_the_resize_is_torchvisions_uint8_antialiased_bilinear(size) -> None:
    src = np.asarray(SRC, dtype=np.uint8)[..., None]
    got = sam3.resize_uint8(src, *size)
    assert got.dtype == np.uint8
    assert got[..., 0].tolist() == TORCHVISION[size]


def test_the_mask_resize_matches_bilinear_without_aligned_corners() -> None:
    # A 2x2 map to 4x4: PyTorch's interpolate(mode="bilinear", align_corners=False) gives these.
    prob = np.array([[0.0, 1.0], [1.0, 0.0]])
    got = sam3.resize_probabilities(prob, 4, 4)
    row0 = [0.0, 0.25, 0.75, 1.0]
    assert np.allclose(got[0], row0)
    assert np.allclose(got[:, 0], row0)
    assert np.allclose(got[1, 1], 0.375)


def test_identity_resize_is_exact() -> None:
    prob = np.random.default_rng(0).random((5, 7))
    assert np.allclose(sam3.resize_probabilities(prob, 5, 7), prob)


def head(n: int = 3) -> tuple[np.ndarray, ...]:
    """Fake head outputs for one query: logits, boxes (xyxy fractions), 288x288 mask logits,
    presence logits."""
    logits = np.array([3.0, 0.0, -3.0])[:n]
    boxes = np.array([[0.1, 0.2, 0.5, 0.6], [-0.1, 0.0, 1.2, 0.5], [0.0, 0.0, 1.0, 1.0]])[:n]
    masks = np.full((n, 288, 288), -10.0)
    masks[0, 50:100, 30:150] = 10.0
    masks[1, 0:10, 0:10] = 10.0
    masks[2, 100:200, 100:200] = 10.0
    return logits, boxes, masks, np.array([2.0])


def test_scores_are_the_logit_times_presence_and_threshold_keeps_at_or_above() -> None:
    logits, boxes, masks, presence = head()
    presence_p = 1 / (1 + np.exp(-2.0))
    found = sam3.instances(
        0, logits, boxes, masks, presence, adapters.Settings(threshold=0.3), 576, 288
    )
    assert [round(i.score, 6) for i in found] == [
        round(presence_p / (1 + np.exp(-3.0)), 6),
        round(presence_p * 0.5, 6),
    ]
    # the threshold at exactly an instance's score keeps it
    at = sam3.instances(
        0, logits, boxes, masks, presence, adapters.Settings(threshold=found[1].score), 576, 288
    )
    assert len(at) == 2


def test_planes_are_in_the_images_frame_and_boxes_in_pixels_clipped() -> None:
    logits, boxes, masks, presence = head()
    found = sam3.instances(
        2, logits, boxes, masks, presence, adapters.Settings(threshold=0.3), 576, 288
    )
    first = found[0]
    assert first.query == 2
    assert first.plane.shape == (288, 576) and first.plane.dtype == bool
    ys, xs = np.nonzero(first.plane)
    assert (ys.min(), ys.max()) == (50, 99)
    assert (xs.min(), xs.max()) == (60, 299)  # columns doubled: 288 wide to 576
    assert first.box == pytest.approx((57.6, 57.6, 288.0, 172.8))
    assert found[1].box == pytest.approx((0.0, 0.0, 576.0, 144.0))
    assert first.diagnostics == {} and first.iou is None


def test_an_empty_mask_gives_no_instance() -> None:
    logits, boxes, masks, presence = head()
    masks[0] = -10.0
    found = sam3.instances(
        0, logits, boxes, masks, presence, adapters.Settings(threshold=0.3), 576, 288
    )
    assert [round(i.box[2]) for i in found] == [576]


class FakeSession:
    def __init__(self, outputs) -> None:
        self.outputs = outputs
        self.feeds: list[dict] = []

    def run(self, _names, feeds):
        self.feeds.append(feeds)
        return self.outputs(feeds) if callable(self.outputs) else self.outputs


class FakeTokenizer:
    def tokenize(self, query: str):
        ids = np.zeros((1, 32), dtype=np.int64)
        ids[0, 0] = len(query)
        return ids, np.ones((1, 32), dtype=np.int64)


def fake_adapter() -> sam3.Sam3:
    adapter = sam3.Sam3.__new__(sam3.Sam3)
    adapter.vision = FakeSession([np.zeros(1, dtype=np.float32)] * 6)
    adapter.text = FakeSession(lambda f: [f["input_ids"].astype(np.float32)])
    logits, boxes, masks, presence = head()

    def answer(feeds):
        # the second query finds nothing
        found = logits if feeds["text_features"][0, 0] == len("chair") else logits - 100
        return [found[None], boxes[None], masks[None], presence[None]]

    adapter.head = FakeSession(answer)
    adapter.tokenize = FakeTokenizer().tokenize  # type: ignore[method-assign]
    return adapter


def test_one_vision_pass_per_image_and_one_text_and_head_pass_per_query() -> None:
    adapter = fake_adapter()
    pixels = np.zeros((288, 576, 3), dtype=np.uint8)
    found = adapter.find(pixels, ["chair", "giraffe"], adapters.Settings(threshold=0.3))
    assert len(adapter.vision.feeds) == 1
    assert adapter.vision.feeds[0]["pixel_values"].shape == (1, 3, 1008, 1008)
    assert len(adapter.text.feeds) == 2 and len(adapter.head.feeds) == 2
    assert set(adapter.head.feeds[0]) == {*sam3.VISION_OUT, "text_features", "attention_mask"}
    assert [i.query for i in found] == [0, 0]


def test_sam3_takes_threshold_and_max_instances_only() -> None:
    entries = catalogue.read()
    entry = catalogue.by_id(entries, "sam3")
    assert entry is not None
    assert entry.adapter == "sam3"
    assert entry.params == ("threshold", "max_instances")
    assert "LICENSE.sam3.txt" in entry.licence_files
    assert adapters.BUILDERS["sam3"]


def test_the_tokenizer_pads_with_end_of_text_to_32(tmp_path) -> None:
    from tokenizers import Tokenizer, models, pre_tokenizers

    vocab = {"<|startoftext|>": 0, "<|endoftext|>": 1, "chair": 2}
    tok = Tokenizer(models.WordLevel(vocab, unk_token="<|endoftext|>"))
    tok.pre_tokenizer = pre_tokenizers.Whitespace()
    path = tmp_path / "tokenizer.json"
    tok.save(str(path))
    adapter = sam3.Sam3.__new__(sam3.Sam3)
    adapter.tokenizer = sam3.load_tokenizer(path)
    ids, mask = adapter.tokenize("chair")
    assert adapter.tokenize("chair " * 40)[0].shape == (1, 32)
    assert ids.shape == mask.shape == (1, 32)
    assert ids[0, 0] == 2 and (ids[0, 1:] == 1).all()
    assert mask[0, 0] == 1 and (mask[0, 1:] == 0).all()

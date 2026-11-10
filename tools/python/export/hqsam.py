"""HQ-SAM ViT-L to ONNX, and its instance-parity references (MODEL.md "HQ-SAM ViT-L"; SPEC.md
"The export gate"). Called by `export.py --only hqsam [--references]`.

The checkpoint is `sam_hq_vit_l.pth` from `lkeab/hq-sam` at a pinned revision (Apache-2.0 on its
card). It is a pickle, so it is read only here, on the exporting machine, with `weights_only`.

The graphs:

- `sam-hq-vit-l.encoder.onnx`: pixel_values [1,3,1024,1024] (resized, normalised and padded as
  `rascal_tools.models.hqsam.preprocess`) -> image_embeddings [1,256,64,64], interm_embeddings
  [1,64,64,1024] (the ViT's output after its first global-attention block, the one feature the HQ
  decoder reads).
- `sam-hq-vit-l.decoder.onnx`: image_embeddings, interm_embeddings, point_coords [1,P,2] and
  point_labels [1,P] (float32, in the resized frame: positive points, label 1, then the box's
  corners, labels 2 and 3), orig_im_size [2] (height, width) -> masks [1,1,H,W] (the HQ output's
  logits in image pixels, `hq_token_only=True`, `multimask_output=False`, SAM's own upscaling,
  crop and resize) and scores [1,1] (the predicted IoU). P is dynamic.
"""

from __future__ import annotations

import json
import shutil
import sys
import time
from pathlib import Path

import numpy as np
import torch
from huggingface_hub import hf_hub_download
from PIL import Image

REPO_ID = "lkeab/hq-sam"
REVISION = "09b02a333b37772133eff3997bdba997867374b7"
CHECKPOINT = "sam_hq_vit_l.pth"
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(ROOT / "tools/python/src"))

from rascal_tools.models import adapters, hqsam  # noqa: E402

NAMES = {
    "encoder": "sam-hq-vit-l.encoder.onnx",
    "decoder": "sam-hq-vit-l.decoder.onnx",
    "licence": "LICENSE.sam-hq-vit-l.txt",
}
CHAIR = ROOT / "fixtures/images/chair-blob.png"
TALL = ROOT / "fixtures/images/chair-blob-tall.png"
REFERENCES = ROOT / "fixtures/models/hqsam"
THRESHOLDS = (0.2, 0.1, 0.05, 0.01)


def load_model():
    from segment_anything_hq import sam_model_registry

    path = hf_hub_download(REPO_ID, CHECKPOINT, revision=REVISION)
    model = sam_model_registry["vit_l"](checkpoint=None)
    model.load_state_dict(torch.load(path, map_location="cpu", weights_only=True))
    return model.eval()


class EncoderGraph(torch.nn.Module):
    def __init__(self, model) -> None:
        super().__init__()
        self.encoder = model.image_encoder

    def forward(self, pixel_values):
        x, interm = self.encoder(pixel_values)
        return x, interm[0]


class DecoderGraph(torch.nn.Module):
    def __init__(self, model) -> None:
        super().__init__()
        from segment_anything_hq.utils.onnx import SamOnnxModel

        self.inner = SamOnnxModel(model, hq_token_only=True, multimask_output=False)

    def forward(
        self, image_embeddings, interm_embeddings, point_coords, point_labels, orig_im_size
    ):
        mask_input = torch.zeros(1, 1, 256, 256)
        has_mask_input = torch.zeros(1)
        masks, scores, _ = self.inner(
            image_embeddings,
            interm_embeddings.unsqueeze(0),
            point_coords,
            point_labels,
            mask_input,
            has_mask_input,
            orig_im_size,
        )
        return masks, scores


class TorchSegmenter:
    """The maker's predictor behind the adapter's `embed` and `outlines`, as the lab's
    `hqsam-vitl-hqonly` cell calls it: the reference the ONNX graphs are checked against."""

    def __init__(self, model) -> None:
        from segment_anything_hq import SamPredictor

        self.predictor = SamPredictor(model)

    def embed(self, pixels):
        self.predictor.set_image(pixels)
        return self.predictor

    def outlines(self, features, box, points, width, height):
        coords = np.array(points, dtype=np.float32) if points else None
        labels = np.ones(len(points), dtype=np.int64) if points else None
        masks, scores, _ = features.predict(
            point_coords=coords,
            point_labels=labels,
            box=np.array(box, dtype=np.float32),
            multimask_output=False,
            hq_token_only=True,
        )
        return [np.asarray(m, dtype=bool) for m in masks], [float(s) for s in scores]


def tall_fixture() -> None:
    """`chair-blob-tall.png`: the chair fixture on a portrait canvas (480 by 640, mid-grey), the
    non-square image the instance-parity references use beside the chair fixture."""
    if TALL.exists():
        return
    chair = Image.open(CHAIR).convert("RGB")
    canvas = Image.new("RGB", (480, 640), (127, 127, 127))
    canvas.paste(chair, (0, 160))
    canvas.save(TALL)


def export(out: Path, session, sha256) -> dict:
    model = load_model()
    out.mkdir(parents=True, exist_ok=True)
    chair = np.asarray(Image.open(CHAIR).convert("RGB"))
    height, width = chair.shape[:2]
    pixel_values = torch.from_numpy(hqsam.preprocess(chair))
    t0 = time.monotonic()
    with torch.no_grad():
        torch.onnx.export(
            EncoderGraph(model),
            (pixel_values,),
            str(out / NAMES["encoder"]),
            input_names=["pixel_values"],
            output_names=["image_embeddings", "interm_embeddings"],
            opset_version=17,
            dynamo=False,
        )
        print(f"  encoder in {time.monotonic() - t0:.0f} s")
        image_embeddings, interm = EncoderGraph(model)(pixel_values)
        coords, labels = hqsam.prompt((130.0, 70.0, 250.0, 250.0), [(190.0, 160.0)], width, height)
        t0 = time.monotonic()
        torch.onnx.export(
            DecoderGraph(model),
            (
                image_embeddings,
                interm,
                torch.from_numpy(coords),
                torch.from_numpy(labels),
                torch.tensor([height, width], dtype=torch.float32),
            ),
            str(out / NAMES["decoder"]),
            input_names=[
                "image_embeddings",
                "interm_embeddings",
                "point_coords",
                "point_labels",
                "orig_im_size",
            ],
            output_names=["masks", "scores"],
            dynamic_axes={
                "point_coords": {1: "points"},
                "point_labels": {1: "points"},
                "masks": {2: "height", 3: "width"},
            },
            opset_version=17,
            dynamo=False,
        )
        print(f"  decoder in {time.monotonic() - t0:.0f} s")
    shutil.copyfile(HERE / "licences" / NAMES["licence"], out / NAMES["licence"])

    # Parity on the chair fixture: the graphs against the maker's predictor, for a box alone and
    # with one and three points.
    from rascal_tools.models.hqsam import Segmenter

    onnx_seg = Segmenter(out / NAMES["encoder"], out / NAMES["decoder"])
    ref = TorchSegmenter(model)
    features = onnx_seg.embed(chair)
    predictor = ref.embed(chair)
    checks = {}
    box = (130.0, 70.0, 250.0, 250.0)
    for case, points in {
        "box": [],
        "box+1": [(190.0, 160.0)],
        "box+3": [(190.0, 160.0), (160.0, 120.0), (220.0, 220.0)],
    }.items():
        got, got_iou = onnx_seg.outlines(features, box, points, width, height)
        want, want_iou = ref.outlines(predictor, box, points, width, height)
        inter = float((got[0] & want[0]).sum())
        union = float(max(1, (got[0] | want[0]).sum()))
        checks[case] = {"plane_iou": inter / union, "iou_diff": abs(got_iou[0] - want_iou[0])}
        print(f"  parity {case}: {checks[case]}")
        assert checks[case]["plane_iou"] >= 0.999 and checks[case]["iou_diff"] <= 1e-3, checks
    files = {}
    for name in NAMES.values():
        path = out / name
        files[name] = {"sha256": sha256(path), "bytes": path.stat().st_size}
    return {
        "source": {"repo": REPO_ID, "revision": REVISION, "file": CHECKPOINT},
        "versions": {
            "torch": torch.__version__,
            "segment-anything-hq": __import__("importlib.metadata").metadata.version(
                "segment-anything-hq"
            ),
            "opset": 17,
        },
        "files": files,
        "checks": checks,
    }


def _run(adapter, pixels, prompt_text: str, threshold: float) -> list[adapters.Instance]:
    parts = prompt_text.replace(";", ".").replace(",", ".").split(".")
    queries = [q.strip().lower() for q in parts]
    queries = [q for q in queries if q]
    settings = adapters.Settings(threshold=threshold, max_instances=8)
    return adapters.order(adapter.find(pixels, queries, settings), 8)


def references() -> dict:
    """The instance-parity references (SPEC.md "The export gate", check 1) from the maker's
    PyTorch model behind the shipped OWLv2, into `fixtures/models/hqsam/`."""
    from rascal_tools.models import cache
    from rascal_tools.models.owlv2 import Detector

    tall_fixture()
    tiny = cache.installation("tiny")
    detector = Detector(
        tiny.path("owlv2-base-patch16.onnx"), tiny.path("owlv2-base-patch16.tokenizer.json")
    )
    adapter = adapters.Owlv2Sam2(detector, TorchSegmenter(load_model()), three_outlines=False)
    images = {p.name: np.asarray(Image.open(p).convert("RGB")) for p in (CHAIR, TALL)}
    threshold = None
    for t in THRESHOLDS:
        found = {n: _run(adapter, px, "chair. table", t) for n, px in images.items()}
        if all(any(i.plane.any() for i in inst) for inst in found.values()):
            threshold = t
            break
    if threshold is None:
        raise SystemExit("no positive-case threshold gives an instance on every image")
    if REFERENCES.exists():
        shutil.rmtree(REFERENCES)
    REFERENCES.mkdir(parents=True)
    record: dict = {"model": "hqsam", "positive_threshold": threshold, "cases": []}
    for case, prompt_text, t in (
        ("positive", "chair. table", threshold),
        ("negative", "giraffe", 0.2),
    ):
        for name, px in images.items():
            instances = _run(adapter, px, prompt_text, t)
            entries = []
            for k, inst in enumerate(instances):
                plane = f"{case}-{Path(name).stem}-{k}.png"
                Image.fromarray(inst.plane).convert("1").save(REFERENCES / plane)
                entries.append(
                    {
                        "query": inst.query,
                        "score": inst.score,
                        "box": list(inst.box),
                        "plane": plane,
                    }
                )
            record["cases"].append(
                {
                    "case": case,
                    "image": name,
                    "prompt": prompt_text,
                    "threshold": t,
                    "instances": entries,
                }
            )
    (REFERENCES / "references.json").write_text(json.dumps(record, indent=2) + "\n")
    print(json.dumps({k: v for k, v in record.items() if k != "cases"}))
    return record

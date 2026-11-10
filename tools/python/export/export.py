"""Exports the segmenter's two checkpoints to ONNX (MODEL.md, ADR 0018).

    uv run --project tools/python/export python tools/python/export/export.py [--out DIR]

Downloads `google/owlv2-base-patch16` and `facebook/sam2.1-hiera-tiny` from the Hub, exports
three graphs and the text tokenizer into DIR (default: the pack's model cache,
`RASCAL_TOOLS_MODELS` or `~/.cache/rascal-tools`), checks each graph against PyTorch on the
chair fixture, and writes `export.json` with every file's SHA-256 and size, the Hub revisions
and the library versions: what `artifacts.toml` and MODEL.md record. The pack never imports
anything from here; it runs the graphs on onnxruntime with numpy preprocessing that mirrors
the processors (`rascal_tools.models`).

The graphs:

- `owlv2-base-patch16.onnx`: pixel_values [1,3,960,960], input_ids [Q,16], attention_mask
  [Q,16] -> logits [1,3600,Q], pred_boxes [1,3600,4] (cx, cy, w, h as fractions of the padded
  square), objectness [1,3600]. Q is dynamic.
- `owlv2-base-patch16.tokenizer.json`: the CLIP byte-pair tokenizer, for `tokenizers`.
- `sam2.1-hiera-tiny.encoder.onnx`: pixel_values [1,3,1024,1024] -> feat0 [1,256,256,256],
  feat1 [1,256,128,128], feat2 [1,256,64,64].
- `sam2.1-hiera-tiny.decoder3.onnx`: feat0, feat1, feat2, points [1,P,2] (x, y in 1024-pixel
  coordinates), point_labels [1,P] int64 -> masks [1,3,256,256] (three candidate outlines as
  logits; > 0 is inside), iou [1,3] (the decoder's predicted IoU of each). P is dynamic, 2 to
  10: the box as its two corners (labels 2 and 3, the way SAM 2's own predictor passes a box)
  followed by up to 8 positive points (label 1). One box per call; the features are reused.

`--only sam_decoder` exports and checks the decoder alone, against features from PyTorch's
encoder, leaving the other files as they are (the 2026-11 re-export, PLAN.md 13.1).

`--only tiny --references` freezes Tiny's instance-parity references under `fixtures/models/tiny/`
(`export/tiny.py`; SPEC.md "The export gate").

`--only sam3 [--references]` exports SAM 3's three graphs, its tokenizer and the SAM License into
`--out`, then (with `--references`) freezes its references into `fixtures/models/sam3/`: it runs
`sam3/export_sam3.py` in its own environment (`sam3/pyproject.toml`, Transformers 5).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import sys
import time
from pathlib import Path

import numpy as np
import onnx
import onnxruntime as ort
import torch
from huggingface_hub import HfApi
from PIL import Image
from transformers import Owlv2ForObjectDetection, Owlv2Processor, Sam2Model, Sam2Processor

OWL = "google/owlv2-base-patch16"
SAM = "facebook/sam2.1-hiera-tiny"
OPSET = 18
REPO = Path(__file__).resolve().parents[3]
FIXTURE = REPO / "fixtures/images/chair-blob.png"

NAMES = {
    "owl": "owlv2-base-patch16.onnx",
    "tokenizer": "owlv2-base-patch16.tokenizer.json",
    "sam_encoder": "sam2.1-hiera-tiny.encoder.onnx",
    "sam_decoder": "sam2.1-hiera-tiny.decoder3.onnx",
}


class OwlGraph(torch.nn.Module):
    def __init__(self, model: Owlv2ForObjectDetection) -> None:
        super().__init__()
        self.model = model

    def forward(self, pixel_values, input_ids, attention_mask):
        out = self.model(
            input_ids=input_ids,
            pixel_values=pixel_values,
            attention_mask=attention_mask,
            return_dict=True,
        )
        return out.logits, out.pred_boxes, out.objectness_logits


class SamEncoderGraph(torch.nn.Module):
    def __init__(self, model: Sam2Model) -> None:
        super().__init__()
        self.model = model

    def forward(self, pixel_values):
        feats = self.model.get_image_embeddings(pixel_values)
        return feats[0], feats[1], feats[2]


class SamDecoderGraph(torch.nn.Module):
    def __init__(self, model: Sam2Model) -> None:
        super().__init__()
        self.model = model

    def forward(self, feat0, feat1, feat2, points, point_labels):
        out = self.model(
            image_embeddings=[feat0, feat1, feat2],
            input_points=points[:, None],
            input_labels=point_labels[:, None],
            multimask_output=True,
        )
        return out.pred_masks[:, 0], out.iou_scores[:, 0]


def export(module, args, path: Path, input_names, output_names, dynamic_shapes) -> None:
    """torch.onnx.export through the dynamo exporter; the graph is checked and saved as one
    file (no external data), so a single digest covers it."""
    module.eval()
    t0 = time.monotonic()
    with torch.no_grad():
        program = torch.onnx.export(
            module,
            args,
            input_names=input_names,
            output_names=output_names,
            dynamic_shapes=dynamic_shapes,
            opset_version=OPSET,
            dynamo=True,
            optimize=True,
        )
    program.save(str(path), external_data=False)
    onnx.checker.check_model(str(path))
    print(f"  {path.name}: {path.stat().st_size / 1e6:.1f} MB in {time.monotonic() - t0:.0f} s")


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def session(path: Path) -> ort.InferenceSession:
    return ort.InferenceSession(str(path), providers=["CPUExecutionProvider"])


def export_owl(out: Path, image: Image.Image) -> dict:
    print(f"OWLv2: {OWL}")
    processor = Owlv2Processor.from_pretrained(OWL)
    model = Owlv2ForObjectDetection.from_pretrained(OWL).eval()
    inputs = processor(text=[["a chair", "a table"]], images=image, return_tensors="pt")
    pixel_values = inputs["pixel_values"]
    input_ids = inputs["input_ids"]
    attention_mask = inputs["attention_mask"]
    assert tuple(pixel_values.shape) == (1, 3, 960, 960), pixel_values.shape
    assert tuple(input_ids.shape) == (2, 16), input_ids.shape
    queries = torch.export.Dim("queries", min=1, max=64)
    export(
        OwlGraph(model),
        (pixel_values, input_ids, attention_mask),
        out / NAMES["owl"],
        ["pixel_values", "input_ids", "attention_mask"],
        ["logits", "pred_boxes", "objectness"],
        {"pixel_values": None, "input_ids": {0: queries}, "attention_mask": {0: queries}},
    )
    # The tokenizer, as `tokenizers` loads it; the pack pads to 16 with id 0 and truncates.
    tokenizer_path = out / NAMES["tokenizer"]
    processor.tokenizer.backend_tokenizer.save(str(tokenizer_path))
    from tokenizers import Tokenizer

    tok = Tokenizer.from_file(str(tokenizer_path))
    tok.enable_padding(length=16, pad_id=0, pad_token="!")
    tok.enable_truncation(16)
    long = "the big red armchair by the window in the corner of the room"
    for text in ("a chair", "a table", long):
        ours = tok.encode(text.lower()).ids
        theirs = processor.tokenizer(text, padding="max_length", max_length=16, truncation=True)[
            "input_ids"
        ]
        assert ours == theirs, (text, ours, theirs)
    # Parity against PyTorch on the fixture.
    with torch.no_grad():
        ref = model(input_ids=input_ids, pixel_values=pixel_values, attention_mask=attention_mask)
    got = session(out / NAMES["owl"]).run(
        None,
        {
            "pixel_values": pixel_values.numpy(),
            "input_ids": input_ids.numpy().astype(np.int64),
            "attention_mask": attention_mask.numpy().astype(np.int64),
        },
    )
    logits_diff = float(np.abs(got[0] - ref.logits.numpy()).max())
    boxes_diff = float(np.abs(got[1] - ref.pred_boxes.numpy()).max())
    print(f"  parity: logits max |diff| {logits_diff:.2e}, boxes max |diff| {boxes_diff:.2e}")
    assert logits_diff < 1e-2 and boxes_diff < 1e-3, (logits_diff, boxes_diff)
    scores = torch.sigmoid(ref.logits[0]).max(dim=-1)
    best = int(scores.values.argmax())
    print(
        f"  best proposal on the fixture: score {float(scores.values[best]):.3f} for query "
        f"{int(scores.indices[best])}, box {ref.pred_boxes[0, best].tolist()}"
    )
    return {"logits_max_abs_diff": logits_diff, "boxes_max_abs_diff": boxes_diff}


POINT_CASES = {
    "box": [],
    "box+1": [[190.0, 160.0]],
    "box+3": [[190.0, 160.0], [160.0, 120.0], [220.0, 220.0]],
}


def prompt(processor, box_xyxy: list[float], positives: list[list[float]], size) -> tuple:
    """The box's corners (labels 2, 3) and the positive points (label 1), scaled into the
    1024-pixel frame as the processor scales a box: `points` [1,P,2], `point_labels` [1,P]."""
    width, height = size
    sx, sy = 1024.0 / width, 1024.0 / height
    x0, y0, x1, y1 = box_xyxy
    pts = [[x0 * sx, y0 * sy], [x1 * sx, y1 * sy]] + [[x * sx, y * sy] for x, y in positives]
    labels = [2, 3] + [1] * len(positives)
    return torch.tensor([pts], dtype=torch.float32), torch.tensor([labels], dtype=torch.int64)


def export_sam(out: Path, image: Image.Image, box_xyxy: list[float], only_decoder: bool) -> dict:
    print(f"SAM 2.1: {SAM}")
    processor = Sam2Processor.from_pretrained(SAM)
    model = Sam2Model.from_pretrained(SAM).eval()
    inputs = processor(images=image, input_boxes=[[box_xyxy]], return_tensors="pt")
    pixel_values = inputs["pixel_values"]
    assert tuple(pixel_values.shape) == (1, 3, 1024, 1024), pixel_values.shape
    # The processor's box scaling, which `prompt` mirrors for the corners.
    scaled = inputs["input_boxes"][0, 0].tolist()
    points, labels = prompt(processor, box_xyxy, [], image.size)
    assert np.allclose(points[0].flatten().tolist(), scaled, atol=1e-3), (points, scaled)
    with torch.no_grad():
        feats = model.get_image_embeddings(pixel_values)
    checks: dict = {}
    if not only_decoder:
        export(
            SamEncoderGraph(model),
            (pixel_values,),
            out / NAMES["sam_encoder"],
            ["pixel_values"],
            ["feat0", "feat1", "feat2"],
            {"pixel_values": None},
        )
        enc = session(out / NAMES["sam_encoder"]).run(None, {"pixel_values": pixel_values.numpy()})
        checks["features_max_abs_diff"] = max(
            float(np.abs(a - b.numpy()).max()) for a, b in zip(enc, feats, strict=True)
        )
        assert checks["features_max_abs_diff"] < 1e-2, checks
    n_points = torch.export.Dim("points", min=2, max=10)
    three, three_labels = prompt(processor, box_xyxy, POINT_CASES["box+3"], image.size)
    export(
        SamDecoderGraph(model),
        (feats[0], feats[1], feats[2], three, three_labels),
        out / NAMES["sam_decoder"],
        ["feat0", "feat1", "feat2", "points", "point_labels"],
        ["masks", "iou"],
        {
            "feat0": None,
            "feat1": None,
            "feat2": None,
            "points": {1: n_points},
            "point_labels": {1: n_points},
        },
    )
    decoder = session(out / NAMES["sam_decoder"])
    feed = {f"feat{i}": feats[i].numpy() for i in range(3)}
    # Parity (MODEL.md): every outline and its predicted IoU, for a box alone and with points,
    # the ONNX graph against PyTorch on the same inputs.
    for case, positives in POINT_CASES.items():
        pts, lbl = prompt(processor, box_xyxy, positives, image.size)
        with torch.no_grad():
            ref = SamDecoderGraph(model)(feats[0], feats[1], feats[2], pts, lbl)
        masks, ious = decoder.run(
            None, {**feed, "points": pts.numpy(), "point_labels": lbl.numpy()}
        )
        assert masks.shape == (1, 3, 256, 256) and ious.shape == (1, 3), (masks.shape, ious.shape)
        outlines = []
        for k in range(3):
            want = ref[0][0, k].numpy() > 0
            got = masks[0, k] > 0
            outlines.append(float((want & got).sum() / max(1, (want | got).sum())))
        iou_diff = float(np.abs(ious[0] - ref[1][0].numpy()).max())
        print(
            f"  parity {case}: outline IoUs {[round(v, 4) for v in outlines]}, "
            f"predicted IoU max |diff| {iou_diff:.2e}"
        )
        assert min(outlines) >= 0.99 and iou_diff <= 0.02, (case, outlines, iou_diff)
        checks[case] = {"outline_iou": outlines, "predicted_iou_max_abs_diff": iou_diff}
    return checks


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    default = os.environ.get("RASCAL_TOOLS_MODELS") or str(Path.home() / ".cache" / "rascal-tools")
    parser.add_argument("--out", type=Path, default=Path(default))
    parser.add_argument(
        "--only",
        choices=["all", "sam_decoder", "tiny", "sam3"],
        default="all",
        help="export every graph (default), the SAM decoder alone, SAM 3 (in its own "
        "environment), or (with --references) tiny",
    )
    parser.add_argument(
        "--references", action="store_true", help="freeze the model's parity references instead"
    )
    args = parser.parse_args()
    if args.only == "sam3":
        import subprocess

        here = Path(__file__).resolve().parent
        command = ["uv", "run", "--frozen", "--project", str(here / "sam3"), "python",
                   str(here / "sam3" / "export_sam3.py"), "--out", str(args.out)]  # fmt: skip
        return subprocess.run(command + (["--references"] if args.references else [])).returncode
    if args.only == "tiny":
        if not args.references:
            parser.error("--only tiny takes --references (tiny's graphs are the default export)")
        import tiny

        tiny.references()
        return 0
    out: Path = args.out
    out.mkdir(parents=True, exist_ok=True)
    torch.manual_seed(0)
    image = Image.open(FIXTURE).convert("RGB")
    api = HfApi()
    revisions = {repo: api.model_info(repo).sha for repo in (OWL, SAM)}
    licences = {repo: (api.model_info(repo).card_data or {}).get("license") for repo in (OWL, SAM)}
    print(f"revisions: {revisions}\nlicences: {licences}")
    only_decoder = args.only == "sam_decoder"
    checks = {} if only_decoder else {"owlv2": export_owl(out, image)}
    # The chair in the fixture, as a box in image pixels (x0, y0, x1, y1).
    checks["sam2"] = export_sam(out, image, [130.0, 70.0, 250.0, 250.0], only_decoder)
    files = {}
    for key, name in NAMES.items():
        if only_decoder and key != "sam_decoder":
            continue
        path = out / name
        files[name] = {"sha256": sha256(path), "bytes": path.stat().st_size}
    record = {
        "exported": time.strftime("%Y-%m-%d"),
        "sources": {
            OWL: {"revision": revisions[OWL], "licence": licences[OWL]},
            SAM: {"revision": revisions[SAM], "licence": licences[SAM]},
        },
        "versions": {
            "python": platform.python_version(),
            "torch": torch.__version__,
            "transformers": __import__("transformers").__version__,
            "onnx": onnx.__version__,
            "onnxruntime": ort.__version__,
            "opset": OPSET,
        },
        "files": files,
        "checks": checks,
    }
    (out / "export.json").write_text(json.dumps(record, indent=2) + "\n")
    print(json.dumps(record, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())

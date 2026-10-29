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
- `sam2.1-hiera-tiny.decoder.onnx`: feat0, feat1, feat2, boxes [1,B,4] (x0, y0, x1, y1 in
  1024-pixel coordinates) -> masks [1,B,256,256] (logits; > 0 is inside), iou [1,B]. B is
  dynamic.
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
    "sam_decoder": "sam2.1-hiera-tiny.decoder.onnx",
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

    def forward(self, feat0, feat1, feat2, boxes):
        out = self.model(
            image_embeddings=[feat0, feat1, feat2],
            input_boxes=boxes,
            multimask_output=False,
        )
        return out.pred_masks[:, :, 0], out.iou_scores[:, :, 0]


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


def export_sam(out: Path, image: Image.Image, box_xyxy: list[float]) -> dict:
    print(f"SAM 2.1: {SAM}")
    processor = Sam2Processor.from_pretrained(SAM)
    model = Sam2Model.from_pretrained(SAM).eval()
    inputs = processor(images=image, input_boxes=[[box_xyxy]], return_tensors="pt")
    pixel_values = inputs["pixel_values"]
    boxes = inputs["input_boxes"].float()
    assert tuple(pixel_values.shape) == (1, 3, 1024, 1024), pixel_values.shape
    assert tuple(boxes.shape) == (1, 1, 4), boxes.shape
    with torch.no_grad():
        feats = model.get_image_embeddings(pixel_values)
    export(
        SamEncoderGraph(model),
        (pixel_values,),
        out / NAMES["sam_encoder"],
        ["pixel_values"],
        ["feat0", "feat1", "feat2"],
        {"pixel_values": None},
    )
    n_boxes = torch.export.Dim("boxes", min=1, max=64)
    export(
        SamDecoderGraph(model),
        (feats[0], feats[1], feats[2], boxes),
        out / NAMES["sam_decoder"],
        ["feat0", "feat1", "feat2", "boxes"],
        ["masks", "iou"],
        {"feat0": None, "feat1": None, "feat2": None, "boxes": {1: n_boxes}},
    )
    with torch.no_grad():
        ref = model(image_embeddings=feats, input_boxes=boxes, multimask_output=False)
    enc = session(out / NAMES["sam_encoder"]).run(None, {"pixel_values": pixel_values.numpy()})
    feat_diff = max(float(np.abs(a - b.numpy()).max()) for a, b in zip(enc, feats, strict=True))
    dec = session(out / NAMES["sam_decoder"]).run(
        None, {"feat0": enc[0], "feat1": enc[1], "feat2": enc[2], "boxes": boxes.numpy()}
    )
    mask_ref = ref.pred_masks[0, 0, 0].numpy() > 0
    mask_got = dec[0][0, 0] > 0
    iou = float((mask_ref & mask_got).sum() / max(1, (mask_ref | mask_got).sum()))
    mask_diff = float(np.abs(dec[0][0, 0] - ref.pred_masks[0, 0, 0].numpy()).max())
    print(
        f"  parity: features max |diff| {feat_diff:.2e}, mask logits max |diff| {mask_diff:.2e}, "
        f"mask IoU {iou:.4f}, {int(mask_got.sum())} of 65536 low-res pixels inside"
    )
    assert feat_diff < 1e-2 and iou > 0.99, (feat_diff, iou)
    return {
        "features_max_abs_diff": feat_diff,
        "mask_logits_max_abs_diff": mask_diff,
        "mask_iou": iou,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    default = os.environ.get("RASCAL_TOOLS_MODELS") or str(Path.home() / ".cache" / "rascal-tools")
    parser.add_argument("--out", type=Path, default=Path(default))
    args = parser.parse_args()
    out: Path = args.out
    out.mkdir(parents=True, exist_ok=True)
    torch.manual_seed(0)
    image = Image.open(FIXTURE).convert("RGB")
    api = HfApi()
    revisions = {repo: api.model_info(repo).sha for repo in (OWL, SAM)}
    licences = {repo: (api.model_info(repo).card_data or {}).get("license") for repo in (OWL, SAM)}
    print(f"revisions: {revisions}\nlicences: {licences}")
    checks = {"owlv2": export_owl(out, image)}
    # The chair in the fixture, as a box in image pixels (x0, y0, x1, y1).
    checks["sam2"] = export_sam(out, image, [130.0, 70.0, 250.0, 250.0])
    files = {}
    for name in NAMES.values():
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

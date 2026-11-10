"""SAM 3 to ONNX, and its instance-parity references (MODEL.md "SAM 3"; SPEC.md "The export
gate"). Run by `export.py --only sam3 [--references]` in this directory's environment
(Transformers 5):

    uv run --project tools/python/export/sam3 python tools/python/export/sam3/export_sam3.py \
        --out DIR [--references]

The checkpoint is `facebook/sam3` at a pinned revision (safetensors; gated on the Hub behind
Meta's SAM License, so the exporting machine needs a login that accepted it). The graphs, as the
lab's `sam3-own` cell runs the model (segment-lab `onnxcheck/sam3`): text prompt only, with the
one box labelled -10 that makes Transformers run the geometry encoder as Meta's own code does.

- `sam3.vision.onnx` (+ `sam3.vision.onnx.data`, its external data: one file would pass
  protobuf's 2 GB): pixel_values [1,3,1008,1008] -> fpn0 [1,256,288,288], fpn1 [1,256,144,144],
  fpn2 [1,256,72,72] and their position encodings pos0, pos1, pos2.
- `sam3.text.onnx`: input_ids, attention_mask [1,32] -> text_features [1,32,256].
- `sam3.head.onnx`: the six vision tensors, text_features, attention_mask -> pred_logits [1,200],
  pred_boxes [1,200,4] (xyxy, fractions), pred_masks [1,200,288,288] (logits), presence_logits
  [1,1].

With them: `sam3.tokenizer.json` (the checkpoint's own CLIP tokenizer) and `LICENSE.sam3.txt`
(the SAM License, which goes with any redistribution)."""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import re
import shutil
import sys
import time
from pathlib import Path

import numpy as np
import onnx
import onnxruntime as ort
import torch
from huggingface_hub import hf_hub_download
from PIL import Image
from transformers import Sam3Model, Sam3Processor
from transformers.modeling_outputs import BaseModelOutputWithPooling
from transformers.models.sam3.modeling_sam3 import Sam3VisionEncoderOutput

MODEL = "facebook/sam3"
REVISION = "3c879f39826c281e95690f02c7821c4de09afae7"
OPSET = 18
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
sys.path.insert(0, str(ROOT / "tools/python/src"))

from rascal_tools.models import adapters  # noqa: E402
from rascal_tools.models import sam3 as pack  # noqa: E402

NAMES = {
    "vision": "sam3.vision.onnx",
    "vision_data": "sam3.vision.onnx.data",
    "text": "sam3.text.onnx",
    "head": "sam3.head.onnx",
    "tokenizer": "sam3.tokenizer.json",
    "licence": "LICENSE.sam3.txt",
}
VISION_OUT = list(pack.VISION_OUT)
HEAD_IN = [*VISION_OUT, "text_features", "attention_mask"]
HEAD_OUT = ["pred_logits", "pred_boxes", "pred_masks", "presence_logits"]
FIXTURES = ROOT / "fixtures"
IMAGES = ["chair-blob.png", "chair-wide.png"]
POSITIVE, NEGATIVE = "chair. table", "giraffe"
THRESHOLDS = (0.2, 0.1, 0.05, 0.01)
REFERENCES = FIXTURES / "models/sam3"


class VisionGraph(torch.nn.Module):
    def __init__(self, model: Sam3Model) -> None:
        super().__init__()
        self.model = model

    def forward(self, pixel_values):
        v = self.model.get_vision_features(pixel_values)
        h, p = v.fpn_hidden_states, v.fpn_position_encoding
        return h[0], h[1], h[2], p[0], p[1], p[2]


class TextGraph(torch.nn.Module):
    def __init__(self, model: Sam3Model) -> None:
        super().__init__()
        self.model = model

    def forward(self, input_ids, attention_mask):
        out = self.model.get_text_features(input_ids=input_ids, attention_mask=attention_mask)
        return out.pooler_output


class HeadGraph(torch.nn.Module):
    def __init__(self, model: Sam3Model) -> None:
        super().__init__()
        self.model = model

    def forward(self, fpn0, fpn1, fpn2, pos0, pos1, pos2, text_features, attention_mask):
        # forward() drops the last FPN level and the image model never reads it: a stand-in
        # fills the slot.
        vision = Sam3VisionEncoderOutput(
            fpn_hidden_states=(fpn0, fpn1, fpn2, fpn2),
            fpn_position_encoding=(pos0, pos1, pos2, pos2),
        )
        out = self.model(
            vision_embeds=vision,
            text_embeds=BaseModelOutputWithPooling(pooler_output=text_features),
            attention_mask=attention_mask,
            input_boxes=torch.zeros(1, 1, 4, dtype=fpn0.dtype),
            input_boxes_labels=torch.full((1, 1), -10, dtype=torch.long),
        )
        return out.pred_logits, out.pred_boxes, out.pred_masks, out.presence_logits


def load() -> Sam3Model:
    torch.set_grad_enabled(False)
    return Sam3Model.from_pretrained(MODEL, revision=REVISION).eval()


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def save(module, args, path: Path, inputs, outputs, external: bool) -> None:
    module.eval()
    t0 = time.monotonic()
    with torch.no_grad():
        program = torch.onnx.export(
            module, args, input_names=inputs, output_names=outputs,
            opset_version=OPSET, dynamo=True, optimize=True,
        )  # fmt: skip
    program.save(str(path), external_data=external)
    onnx.checker.check_model(str(path))
    print(f"  {path.name} in {time.monotonic() - t0:.0f} s", flush=True)


def diff(a, b) -> float:
    return float(np.abs(np.asarray(a, dtype=np.float64) - np.asarray(b, dtype=np.float64)).max())


def export(out: Path) -> dict:
    out.mkdir(parents=True, exist_ok=True)
    model = load()
    tokenizer = hf_hub_download(MODEL, "tokenizer.json", revision=REVISION)
    licence = hf_hub_download(MODEL, "LICENSE", revision=REVISION)
    shutil.copyfile(tokenizer, out / NAMES["tokenizer"])
    shutil.copyfile(licence, out / NAMES["licence"])
    shutil.copyfile(licence, HERE.parent / "licences" / NAMES["licence"])

    image = np.asarray(Image.open(FIXTURES / "images/chair-blob.png").convert("RGB"))
    pv = torch.from_numpy(pack.preprocess(image))
    ids, am = (torch.from_numpy(a) for a in tokenizer_of(out).tokenize("chair"))
    vision, text = VisionGraph(model)(pv), TextGraph(model)(ids, am)
    print("vision", flush=True)
    save(VisionGraph(model), (pv,), out / NAMES["vision"], ["pixel_values"], VISION_OUT, True)
    data = sorted(p.name for p in out.glob(NAMES["vision"] + "*") if p.name != NAMES["vision"])
    assert data == [NAMES["vision_data"]], f"the vision graph's external data is {data}"
    print("text", flush=True)
    save(TextGraph(model), (ids, am), out / NAMES["text"], ["input_ids", "attention_mask"],
         ["text_features"], False)  # fmt: skip
    print("head", flush=True)
    head_args = (*vision, text, am)
    save(HeadGraph(model), head_args, out / NAMES["head"], HEAD_IN, HEAD_OUT, False)

    # Parity on the chair fixture, graph by graph and through the pack's adapter.
    cpu = ["CPUExecutionProvider"]
    got_v = ort.InferenceSession(str(out / NAMES["vision"]), providers=cpu).run(
        None, {"pixel_values": pv.numpy()}
    )
    got_t = ort.InferenceSession(str(out / NAMES["text"]), providers=cpu).run(
        None, {"input_ids": ids.numpy(), "attention_mask": am.numpy()}
    )
    ref_h = HeadGraph(model)(*head_args)
    got_h = ort.InferenceSession(str(out / NAMES["head"]), providers=cpu).run(
        None, {n: t.numpy() for n, t in zip(HEAD_IN, head_args, strict=True)}
    )
    parity = {
        "vision": {n: diff(g, r) for n, g, r in zip(VISION_OUT, got_v, vision, strict=True)},
        "text": {"text_features": diff(got_t[0], text)},
        "head": {n: diff(g, r) for n, g, r in zip(HEAD_OUT, got_h, ref_h, strict=True)},
    }
    onnx_adapter = pack.build({n: out / n for n in NAMES.values()})
    torch_adapter = torch_sam3(model, out / NAMES["tokenizer"])
    settings = adapters.Settings(threshold=0.2, max_instances=8)
    a = adapters.order(onnx_adapter.find(image, ["chair", "table"], settings), 8)
    b = adapters.order(torch_adapter.find(image, ["chair", "table"], settings), 8)
    parity["adapter"] = {
        "instances": [len(a), len(b)],
        "score": max((abs(x.score - y.score) for x, y in zip(a, b, strict=True)), default=0.0),
        "plane_iou": min((iou(x.plane, y.plane) for x, y in zip(a, b, strict=True)), default=1.0),
    }
    print(json.dumps(parity, indent=2), flush=True)
    files = {}
    for name in NAMES.values():
        path = out / name
        files[name] = {"sha256": sha256(path), "size": path.stat().st_size}
    return {
        "exported": time.strftime("%Y-%m-%d"),
        "source": {"repo": MODEL, "revision": REVISION, "licence": "SAM License (Meta)"},
        "opset": OPSET,
        "exporter": "dynamo",
        "versions": {
            "torch": torch.__version__,
            "transformers": __import__("transformers").__version__,
            "onnx": onnx.__version__,
            "onnxruntime": ort.__version__,
            "python": platform.python_version(),
        },
        "parity": parity,
        "files": files,
    }


def iou(a: np.ndarray, b: np.ndarray) -> float:
    union = np.count_nonzero(a | b)
    return 1.0 if union == 0 else np.count_nonzero(a & b) / union


class TorchSession:
    """An onnxruntime session's `run`, answered by the PyTorch module the graph was exported
    from, with the graph's input names in order."""

    def __init__(self, module: torch.nn.Module, inputs: list[str]) -> None:
        self.module = module.eval()
        self.inputs = inputs

    def run(self, _outputs, feeds: dict[str, np.ndarray]) -> list[np.ndarray]:
        with torch.no_grad():
            out = self.module(
                *(torch.from_numpy(np.ascontiguousarray(feeds[n])) for n in self.inputs)
            )
        return [o.numpy() for o in (out if isinstance(out, tuple) else (out,))]


def tokenizer_of(directory: Path) -> pack.Sam3:
    """A `pack.Sam3` with the tokenizer alone (no sessions), for tokenising."""
    found = pack.Sam3.__new__(pack.Sam3)
    found.tokenizer = pack.load_tokenizer(directory / NAMES["tokenizer"])
    return found


def torch_sam3(model: Sam3Model, tokenizer: Path) -> pack.Sam3:
    """The pack's own adapter with its three sessions answered by PyTorch."""
    found = tokenizer_of(tokenizer.parent)
    found.vision = TorchSession(VisionGraph(model), ["pixel_values"])
    found.text = TorchSession(TextGraph(model), ["input_ids", "attention_mask"])
    found.head = TorchSession(HeadGraph(model), HEAD_IN)
    return found


def check_tokenizer(directory: Path) -> None:
    """The pack's tokenisation against the processor's, on the prompts the gates use."""
    processor = Sam3Processor.from_pretrained(MODEL, revision=REVISION)
    mine = tokenizer_of(directory)
    for query in ("chair", "table", "giraffe", "coffee mug", "a very long query " * 3):
        want = processor(text=query, return_tensors="np")
        ids, am = mine.tokenize(query)
        if want["input_ids"].shape[1] > pack.MAX_TOKENS:
            continue
        assert (ids == want["input_ids"]).all() and (am == want["attention_mask"]).all(), query


def queries_of(prompt: str) -> list[str]:
    """`segment.queries_of` (the pack's own split), without importing the MCP tool module."""
    seen: list[str] = []
    for part in re.split(r"[.,;]", prompt):
        query = " ".join(part.lower().split())
        if query and query not in seen:
            seen.append(query)
    return seen


def references(directory: Path) -> dict:
    """Freeze the maker's model's answers through the pack's adapter (the spec's check 1)."""
    check_tokenizer(directory)
    adapter = torch_sam3(load(), directory / NAMES["tokenizer"])
    images = {n: np.asarray(Image.open(FIXTURES / "images" / n).convert("RGB")) for n in IMAGES}

    def run(px, prompt, threshold):
        settings = adapters.Settings(threshold=threshold, max_instances=8)
        return adapters.order(adapter.find(px, queries_of(prompt), settings), 8)

    positive = next(
        (
            t
            for t in THRESHOLDS
            if all(any(i.plane.any() for i in run(px, POSITIVE, t)) for px in images.values())
        ),
        None,
    )
    if positive is None:
        raise SystemExit("no positive-case threshold gives an instance on every image")
    REFERENCES.mkdir(parents=True, exist_ok=True)
    for stale in REFERENCES.glob("*.png"):
        stale.unlink()
    cases = []
    for case, prompt, threshold in (("positive", POSITIVE, positive), ("negative", NEGATIVE, 0.2)):
        for name, px in images.items():
            found = []
            for n, inst in enumerate(run(px, prompt, threshold)):
                plane = f"{case}-{Path(name).stem}-{n}.png"
                Image.fromarray(inst.plane).convert("1").save(REFERENCES / plane)
                found.append({"query": inst.query, "score": inst.score, "box": list(inst.box),
                              "plane": plane, "sha256": sha256(REFERENCES / plane)})  # fmt: skip
            cases.append({"case": case, "image": name, "prompt": prompt, "threshold": threshold,
                          "instances": found})  # fmt: skip
    record = {"model": "sam3", "source": {"repos": [f"{MODEL}@{REVISION}"]}, "max_instances": 8,
              "positive_threshold": positive, "cases": cases}  # fmt: skip
    (REFERENCES / "references.json").write_text(json.dumps(record, indent=2) + "\n")
    for c in cases:
        print(f"  {c['case']} {c['image']}: {len(c['instances'])} instance(s)")
    return record


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument(
        "--references",
        action="store_true",
        help="after exporting (or alone, with --skip-export), freeze the references",
    )
    parser.add_argument("--skip-export", action="store_true",
                        help="use the export already in --out")  # fmt: skip
    args = parser.parse_args()
    if not args.skip_export:
        record = export(args.out)
        (args.out / "export.json").write_text(json.dumps(record, indent=2) + "\n")
    if args.references:
        references(args.out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

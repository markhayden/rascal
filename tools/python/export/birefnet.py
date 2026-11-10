"""BiRefNet's export and its instance-parity references (SPEC.md "The export gate"; PLAN.md 14.5).

    uv run --project tools/python/export python tools/python/export/export.py \
        --only birefnet [--out DIR | --references]

The export: `ZhengPeng7/BiRefNet` at a pinned revision (the maker's own code, loaded with
`trust_remote_code` on the exporting machine only), its last and finest output, through the dynamo
exporter at opset 19, which writes torchvision's `deform_conv2d` as ONNX's `DeformConv` (new in
opset 19; onnxruntime runs it on the CPU). One file, `birefnet.onnx`: `input_image` [1, 3, 1024,
1024] (a crop as `adapters.birefnet_input` prepares it) to `output_image` [1, 1, 1024, 1024]
logits. The maker's MIT licence comes from their GitHub repository at a pinned commit (the Hub
repository has none), as `LICENSE.birefnet.txt`. Parity: the graph against PyTorch on the chair
fixture's padded box, as probabilities and as the plane `adapters.birefnet_plane` keeps.

The references: the maker's PyTorch model run through the pack's own adapter
(`adapters.Owlv2BiRefNet`, with rascal's shipped OWLv2 from the installed `tiny`), frozen under
`fixtures/models/birefnet/` for `tests/test_model_parity.py`."""

from __future__ import annotations

import hashlib
import json
import sys
import time
import urllib.request
from pathlib import Path

import numpy as np
import onnx
import onnxruntime as ort
import torch
from PIL import Image

REPO_ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO_ROOT / "tools/python/src"))

from rascal_tools.models import adapters, cache  # noqa: E402
from rascal_tools.models.owlv2 import Detector  # noqa: E402

REPO = "ZhengPeng7/BiRefNet"
REVISION = "e2bf8e4460fc8fa32bba5ea4d94b3233d367b0e4"
LICENSE_URL = (
    "https://raw.githubusercontent.com/ZhengPeng7/BiRefNet/"
    "2713d1293ce8a923d5878f1f30177b78770bf24c/LICENSE"
)
LICENSE_SHA256 = "92a7089e0915fc32bc40067560b398f1e6a7a5958abd7d04eda393629a5acefb"
GRAPH = "birefnet.onnx"
LICENCE = "LICENSE.birefnet.txt"
OPSET = 19
FIXTURES = REPO_ROOT / "fixtures"
IMAGES = ["chair-blob.png", "chair-wide.png"]
POSITIVE = "chair. table"
NEGATIVE = "giraffe"
THRESHOLDS = [0.2, 0.1, 0.05, 0.01]


class Last(torch.nn.Module):
    """The model's last (finest) output, as the lab and the makers' tutorial read it."""

    def __init__(self, model: torch.nn.Module) -> None:
        super().__init__()
        self.model = model

    def forward(self, x):
        return self.model(x)[-1]


def model() -> torch.nn.Module:
    from transformers import AutoModelForImageSegmentation

    torch.set_grad_enabled(False)
    loaded = AutoModelForImageSegmentation.from_pretrained(
        REPO, revision=REVISION, trust_remote_code=True
    )
    # the weights are stored in half precision; the export and the references run in full
    return Last(loaded.float().eval())


def torch_soft(net: torch.nn.Module):
    def soft(x: np.ndarray) -> np.ndarray:
        return net(torch.from_numpy(x)).sigmoid()[0, 0].float().numpy()

    return soft


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def detector() -> Detector:
    tiny = cache.installation("tiny")
    return Detector(
        tiny.path("owlv2-base-patch16.onnx"), tiny.path("owlv2-base-patch16.tokenizer.json")
    )


def export(out: Path) -> dict:
    out.mkdir(parents=True, exist_ok=True)
    net = model()
    x = torch.zeros(1, 3, adapters.BIREFNET_SIDE, adapters.BIREFNET_SIDE)
    path = out / GRAPH
    t0 = time.monotonic()
    program = torch.onnx.export(
        net, (x,), input_names=["input_image"], output_names=["output_image"],
        opset_version=OPSET, dynamo=True, optimize=True,
    )  # fmt: skip
    program.save(str(path), external_data=False)
    onnx.checker.check_model(str(path))
    print(f"  {GRAPH}: {path.stat().st_size / 1e6:.1f} MB in {time.monotonic() - t0:.0f} s")
    with urllib.request.urlopen(LICENSE_URL) as response:  # noqa: S310 (a pinned https URL)
        text = response.read()
    assert hashlib.sha256(text).hexdigest() == LICENSE_SHA256, "the licence text changed"
    (out / LICENCE).write_bytes(text)
    # Parity on the chair fixture's box, padded as the adapter pads it.
    pixels = np.asarray(Image.open(FIXTURES / "images" / IMAGES[0]).convert("RGB"))
    height, width = pixels.shape[:2]
    box = adapters.outline.pad_box((130.0, 70.0, 250.0, 250.0), 0.05, width, height)
    x0, y0, x1, y1 = adapters.crop_of(box, width, height)
    crop_input = adapters.birefnet_input(pixels[y0:y1, x0:x1])
    want = torch_soft(net)(crop_input)
    got = adapters.onnx_soft(path)(crop_input)
    diff = float(np.abs(got - want).max())
    a = adapters.birefnet_plane(want, (x1 - x0, y1 - y0))
    b = adapters.birefnet_plane(got, (x1 - x0, y1 - y0))
    union = np.count_nonzero(a | b)
    plane_iou = 1.0 if union == 0 else np.count_nonzero(a & b) / union
    print(f"  parity: probabilities max |diff| {diff:.2e}, plane IoU {plane_iou:.6f}")
    assert diff < 1e-3 and plane_iou >= 0.999, (diff, plane_iou)
    return {
        "source": {
            "repo": REPO,
            "revision": REVISION,
            "licence": "MIT",
            "licence_url": LICENSE_URL,
        },
        "opset": OPSET,
        "versions": {
            "torch": torch.__version__,
            "onnx": onnx.__version__,
            "onnxruntime": ort.__version__,
        },
        "files": {
            n: {"sha256": sha256(out / n), "bytes": (out / n).stat().st_size}
            for n in (GRAPH, LICENCE)
        },
        "parity": {"probabilities_max_abs_diff": diff, "plane_iou": plane_iou},
    }


def run(adapter, pixels: np.ndarray, prompt: str, threshold: float) -> list[adapters.Instance]:
    queries = [q.strip().lower() for q in prompt.replace(",", ".").replace(";", ".").split(".")]
    queries = [q for q in queries if q]
    settings = adapters.Settings(threshold=threshold, max_instances=8)
    return adapters.order(adapter.find(pixels, queries, settings), 8)


def references() -> dict:
    """Freeze the maker's model's answers through the pack's adapter (the spec's check 1)."""
    adapter = adapters.Owlv2BiRefNet(detector(), torch_soft(model()))
    images = {n: np.asarray(Image.open(FIXTURES / "images" / n).convert("RGB")) for n in IMAGES}
    positive = None
    for threshold in THRESHOLDS:
        if all(
            any(i.plane.any() for i in run(adapter, px, POSITIVE, threshold))
            for px in images.values()
        ):
            positive = threshold
            break
    if positive is None:
        raise SystemExit("no positive-case threshold gives an instance on every image")
    target = FIXTURES / "models" / "birefnet"
    target.mkdir(parents=True, exist_ok=True)
    for stale in target.glob("*.png"):
        stale.unlink()
    cases = []
    for case, prompt, threshold in (("positive", POSITIVE, positive), ("negative", NEGATIVE, 0.2)):
        for name, px in images.items():
            found = []
            for n, inst in enumerate(run(adapter, px, prompt, threshold)):
                plane_name = f"{case}-{Path(name).stem}-{n}.png"
                Image.fromarray(inst.plane).convert("1").save(target / plane_name)
                found.append({
                    "query": inst.query, "score": inst.score, "box": list(inst.box),
                    "plane": plane_name, "sha256": sha256(target / plane_name),
                })  # fmt: skip
            cases.append({"case": case, "image": name, "prompt": prompt, "threshold": threshold,
                          "instances": found})  # fmt: skip
    record = {"model": "birefnet", "source": {"repo": REPO, "revision": REVISION},
              "max_instances": 8, "positive_threshold": positive, "cases": cases}  # fmt: skip
    (target / "references.json").write_text(json.dumps(record, indent=2) + "\n")
    print(json.dumps({k: v for k, v in record.items() if k != "cases"}, indent=2))
    for c in cases:
        print(f"  {c['case']} {c['image']}: {len(c['instances'])} instance(s)")
    return record

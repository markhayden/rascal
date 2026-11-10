"""EVF-SAM2's export and its instance-parity references (SPEC.md "The export gate"; PLAN.md 14.7).

    uv run --project tools/python/export/evfsam2 python tools/python/export/evfsam2.py \
        [--out DIR | --references]

(`export.py --only evfsam2` runs this in its own environment.)

The source: the maker's code, `hustvl/EVF-SAM` at a pinned commit (fetched as GitHub's archive
and checked by digest; it is not on PyPI), with the weights `YxZhang/evf-sam2-multitask` at a
pinned revision, as the lab's `evfsam2` cell runs them: BEIT-3 Large reads the image (224²,
bicubic) and the query together and its first token, projected, prompts SAM 2.1 Large's decoder
directly; of the three outlines the one with the highest predicted IoU is kept. Float32.

Two graphs:

- `evf-sam2.image.onnx`: `image` [1, 3, H, W] (RGB, 0 to 255, float32, any size) to SAM 2.1's
  three feature levels, `image_embed` [1, 256, 64, 64], `high_res_0` [1, 32, 256, 256] and
  `high_res_1` [1, 64, 128, 128] (the image stretched to 1024² bilinear, SAM 2's mean and
  deviation, `no_mem_embed` added, as `EvfSam2Model.inference`), and BEIT-3's input `beit_image`
  [1, 3, 224, 224] (the image over 255, bicubic without antialiasing, as torchvision's `Resize`
  with `antialias=None` does to a tensor, then normalised to [-1, 1]). Run once per image.
- `evf-sam2.prompted.onnx`: `beit_image`, `input_ids` [1, T] (the query, XLM-R's tokenizer) and
  the three feature levels to `low_res_mask` [1, 1, 256, 256] (the logits of the outline with the
  highest predicted IoU) and `iou` [1, 3]. Run once per query.

The pack resizes the low-resolution logits to the image (bilinear, half-pixel centres, as
`F.interpolate` with `align_corners=False`) and keeps logits above 0 (`adapters.evfsam2_plane`).
The tokenizer: the checkpoint's sentencepiece model converted to `tokenizers` JSON, checked
against the maker's slow tokenizer. The licence: the repository's Apache-2.0 `LICENSE`.

The references: the maker's PyTorch model run through the pack's own adapter
(`adapters.EvfSam2`, with a PyTorch backend), frozen under `fixtures/models/evfsam2/` for
`tests/test_model_parity.py`."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import sys
import tarfile
import time
import urllib.request
from pathlib import Path

import numpy as np
import torch
import torch.nn.functional as F
from PIL import Image

REPO_ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO_ROOT / "tools/python/src"))

from rascal_tools.models import adapters  # noqa: E402

CODE = "hustvl/EVF-SAM"
CODE_COMMIT = "9935dc52571678b806830821b3ddeacb30e7af07"
CODE_URL = f"https://github.com/{CODE}/archive/{CODE_COMMIT}.tar.gz"
CODE_SHA256 = "37d84f8c93c5b56145da538d150004e062bfb64a10f3728e4f7f94eb8bfaff13"
REPO = "YxZhang/evf-sam2-multitask"
REVISION = "df3644009016a4f62d13c02f3f8fb6c9bbf94310"
IMAGE_GRAPH = "evf-sam2.image.onnx"
PROMPTED_GRAPH = "evf-sam2.prompted.onnx"
TOKENIZER = "evf-sam2.tokenizer.json"
LICENCE = "LICENSE.evf-sam2.txt"
OPSET = 17
CACHE = Path.home() / ".cache/rascal-tools-export"
FIXTURES = REPO_ROOT / "fixtures"
IMAGES = ["chair-blob.png", "chair-wide.png"]
POSITIVE = "chair. table"
NEGATIVE = "giraffe"
THRESHOLDS = [0.2, 0.1, 0.05, 0.01]
SAM_MEAN = torch.tensor([123.675, 116.28, 103.53]).view(1, 3, 1, 1)
SAM_STD = torch.tensor([58.395, 57.12, 57.375]).view(1, 3, 1, 1)
FEAT_SIZES = [(256, 256), (128, 128), (64, 64)]


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def code_dir() -> Path:
    """The maker's code at the pinned commit, fetched once into the export cache."""
    target = CACHE / f"EVF-SAM-{CODE_COMMIT}"
    if not (target / "model" / "evf_sam2.py").is_file():
        CACHE.mkdir(parents=True, exist_ok=True)
        with urllib.request.urlopen(CODE_URL) as response:  # noqa: S310 (a pinned https URL)
            data = response.read()
        digest = hashlib.sha256(data).hexdigest()
        if digest != CODE_SHA256:
            raise SystemExit(f"{CODE_URL}: sha256 {digest}, expected {CODE_SHA256}")
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as tar:
            tar.extractall(CACHE, filter="data")
        (CACHE / "EVF-SAM.sha256").write_text(digest + "\n")
    return target


def model() -> torch.nn.Module:
    sys.path.insert(0, str(code_dir()))
    from model.evf_sam2 import EvfSam2Model

    torch.set_grad_enabled(False)
    loaded = EvfSam2Model.from_pretrained(
        REPO, revision=REVISION, low_cpu_mem_usage=True, torch_dtype=torch.float32
    )
    return loaded.eval()


def slow_tokenizer():
    from transformers import AutoTokenizer

    return AutoTokenizer.from_pretrained(
        REPO, revision=REVISION, padding_side="right", use_fast=False
    )


class ImageGraph(torch.nn.Module):
    """The image to SAM 2.1's feature levels and BEIT-3's input (the module docstring)."""

    def __init__(self, m: torch.nn.Module) -> None:
        super().__init__()
        self.v = m.visual_model

    def forward(self, image: torch.Tensor):
        sam = F.interpolate(image, (1024, 1024), mode="bilinear", align_corners=False)
        sam = (sam - SAM_MEAN) / SAM_STD
        backbone_out = self.v.forward_image(sam)
        _, embeddings, _, _ = self.v._prepare_backbone_features(backbone_out)
        if self.v.directly_add_no_mem_embed:
            embeddings[-1] = embeddings[-1] + self.v.no_mem_embed
        feats = [
            feat.permute(1, 2, 0).reshape(1, -1, *size)
            for feat, size in zip(embeddings[::-1], FEAT_SIZES[::-1], strict=True)
        ][::-1]
        beit = F.interpolate(
            image / 255.0, (224, 224), mode="bicubic", align_corners=False, antialias=False
        )
        beit = (beit - 0.5) / 0.5
        return feats[2], feats[0], feats[1], beit


class PromptedGraph(torch.nn.Module):
    """BEIT-3 on the image and the query, its first token projected, SAM 2.1's decoder."""

    def __init__(self, m: torch.nn.Module) -> None:
        super().__init__()
        self.m = m
        self.v = m.visual_model

    def forward(self, beit_image, input_ids, image_embed, high_res_0, high_res_1):
        output = self.m.mm_extractor.beit3(
            visual_tokens=beit_image,
            textual_tokens=input_ids,
            text_padding_position=torch.zeros_like(input_ids),
        )
        feat = self.m.text_hidden_fcs[0](output["encoder_out"][:, :1, ...])
        sparse, dense = self.v.sam_prompt_encoder(
            points=None, boxes=None, masks=None, text_embeds=feat
        )
        low, iou, _, _ = self.v.sam_mask_decoder(
            image_embeddings=image_embed,
            image_pe=self.v.sam_prompt_encoder.get_dense_pe(),
            sparse_prompt_embeddings=sparse.to(feat.dtype),
            dense_prompt_embeddings=dense,
            multimask_output=True,
            repeat_image=True,
            high_res_features=[high_res_0, high_res_1],
        )
        best = torch.argmax(iou, dim=-1)
        low = torch.index_select(low, 1, best[0])
        return low, iou


class TorchBackend:
    """The maker's PyTorch model behind the adapter's backend interface (for the references)."""

    def __init__(self, m: torch.nn.Module) -> None:
        self.image = ImageGraph(m)
        self.prompted = PromptedGraph(m)
        self.tokenizer = slow_tokenizer()

    def embed(self, image: np.ndarray) -> tuple[np.ndarray, ...]:
        return tuple(t.numpy() for t in self.image(torch.from_numpy(image)))

    def ids(self, query: str) -> np.ndarray:
        return self.tokenizer(query, return_tensors="np")["input_ids"].astype(np.int64)

    def mask(self, features: tuple[np.ndarray, ...], ids: np.ndarray) -> np.ndarray:
        image_embed, high_res_0, high_res_1, beit = (torch.from_numpy(f) for f in features)
        low, _ = self.prompted(beit, torch.from_numpy(ids), image_embed, high_res_0, high_res_1)
        return low.numpy()


def lab_mask(m: torch.nn.Module, tokenizer, pixels: np.ndarray, query: str) -> np.ndarray:
    """The lab cell's own path (`segment_lab/adapters/evfsam.py`), for the export's parity."""
    from torchvision import transforms
    from torchvision.transforms.functional import InterpolationMode

    x = torch.from_numpy(pixels).permute(2, 0, 1).contiguous().float()
    x = F.interpolate(x[None], (1024, 1024), mode="bilinear", align_corners=False)[0]
    sam = (x - SAM_MEAN[0]) / SAM_STD[0]
    beit = transforms.Compose([
        transforms.ToTensor(),
        transforms.Resize((224, 224), interpolation=InterpolationMode.BICUBIC, antialias=None),
        transforms.Normalize(mean=(0.5, 0.5, 0.5), std=(0.5, 0.5, 0.5)),
    ])(pixels)  # fmt: skip
    ids = tokenizer(query, return_tensors="pt")["input_ids"]
    logits = m.inference(
        sam[None], beit[None], ids, resize_list=[None], original_size_list=[pixels.shape[:2]]
    )
    return (logits[0] > 0).numpy()


def tokenizer_json(out: Path) -> Path:
    """The checkpoint's sentencepiece tokenizer as `tokenizers` JSON, checked against the slow
    tokenizer on the fixtures' and the corpus' queries and a few awkward ones."""
    from transformers import XLMRobertaTokenizerFast

    slow = slow_tokenizer()
    fast = XLMRobertaTokenizerFast.from_pretrained(REPO, revision=REVISION, from_slow=True)
    path = out / TOKENIZER
    fast.backend_tokenizer.save(str(path))
    from tokenizers import Tokenizer

    loaded = Tokenizer.from_file(str(path))
    for text in (
        "chair",
        "table",
        "giraffe",
        "leather bag",
        "the dog",
        "side table",
        "hose reel cart",
        "a bike's wheel",
        "Café-chair",
        "  spaced   out  ",
        "ünïcödé straße",
        "pot plant",
        "lamp",
        "x",
        "two words, here",
    ):
        want = slow(text)["input_ids"]
        got = loaded.encode(text).ids
        assert got == want, (text, got, want)
    return path


def export(out: Path) -> dict:
    import onnx
    import onnxruntime as ort

    out.mkdir(parents=True, exist_ok=True)
    m = model()
    image_graph, prompted_graph = ImageGraph(m).eval(), PromptedGraph(m).eval()
    pixels = np.asarray(Image.open(FIXTURES / "images" / IMAGES[0]).convert("RGB"))
    x = torch.from_numpy(adapters.evfsam2_input(pixels))
    t0 = time.monotonic()
    torch.onnx.export(
        image_graph, (x,), str(out / IMAGE_GRAPH),
        input_names=["image"],
        output_names=["image_embed", "high_res_0", "high_res_1", "beit_image"],
        dynamic_axes={"image": {2: "height", 3: "width"}},
        opset_version=OPSET, dynamo=False,
    )  # fmt: skip
    print(f"  {IMAGE_GRAPH}: {(out / IMAGE_GRAPH).stat().st_size / 1e6:.1f} MB "
          f"in {time.monotonic() - t0:.0f} s")  # fmt: skip
    features = image_graph(x)
    tokenizer = slow_tokenizer()
    ids = torch.from_numpy(tokenizer("chair", return_tensors="np")["input_ids"].astype(np.int64))
    t0 = time.monotonic()
    torch.onnx.export(
        prompted_graph, (features[3], ids, features[0], features[1], features[2]),
        str(out / PROMPTED_GRAPH),
        input_names=["beit_image", "input_ids", "image_embed", "high_res_0", "high_res_1"],
        output_names=["low_res_mask", "iou"],
        dynamic_axes={"input_ids": {1: "tokens"}},
        opset_version=OPSET, dynamo=False,
    )  # fmt: skip
    # over 2 GB: the weights go beside the graph as one external-data file
    graph = onnx.load(str(out / PROMPTED_GRAPH))
    for stale in out.glob("*"):
        if stale.name not in (IMAGE_GRAPH, PROMPTED_GRAPH) and stale.suffix != ".json":
            stale.unlink()
    onnx.save_model(
        graph, str(out / PROMPTED_GRAPH), save_as_external_data=True, all_tensors_to_one_file=True,
        location=PROMPTED_GRAPH + ".data", size_threshold=1024,
    )  # fmt: skip
    del graph
    print(f"  {PROMPTED_GRAPH}: {(out / (PROMPTED_GRAPH + '.data')).stat().st_size / 1e6:.1f} MB "
          f"of weights in {time.monotonic() - t0:.0f} s")  # fmt: skip
    for name in (IMAGE_GRAPH, PROMPTED_GRAPH):
        onnx.checker.check_model(str(out / name))
    tokenizer_json(out)
    licence = code_dir() / "LICENSE"
    (out / LICENCE).write_bytes(licence.read_bytes())
    # Parity: the ONNX backend against the lab's own path, on both fixtures and a non-square
    # photo-sized image, per query.
    backend = adapters.OnnxEvfSam2(out / IMAGE_GRAPH, out / PROMPTED_GRAPH, out / TOKENIZER)
    parity = []
    for name in IMAGES:
        px = np.asarray(Image.open(FIXTURES / "images" / name).convert("RGB"))
        features = backend.embed(adapters.evfsam2_input(px))
        for query in ("chair", "table", "giraffe"):
            want = lab_mask(m, tokenizer, px, query)
            low = backend.mask(features, backend.ids(query))
            got = adapters.evfsam2_plane(low, px.shape[:2])
            union = np.count_nonzero(want | got)
            iou = 1.0 if union == 0 else np.count_nonzero(want & got) / union
            parity.append({"image": name, "query": query, "plane_iou": iou,
                           "px": int(want.sum())})  # fmt: skip
            print(f"  parity {name} {query!r}: IoU {iou:.6f} ({int(want.sum())} px)")
    worst = min(p["plane_iou"] for p in parity)
    assert worst >= 0.999, parity
    files = (IMAGE_GRAPH, PROMPTED_GRAPH, PROMPTED_GRAPH + ".data", TOKENIZER, LICENCE)
    return {
        "source": {
            "code": CODE,
            "code_commit": CODE_COMMIT,
            "code_url": CODE_URL,
            "code_sha256": (CACHE / "EVF-SAM.sha256").read_text().strip()
            if (CACHE / "EVF-SAM.sha256").is_file()
            else None,
            "repo": REPO,
            "revision": REVISION,
            "licence": "Apache-2.0",
        },  # fmt: skip
        "opset": OPSET,
        "versions": {
            "torch": torch.__version__,
            "onnx": onnx.__version__,
            "onnxruntime": ort.__version__,
        },  # fmt: skip
        "files": {
            n: {"sha256": sha256(out / n), "bytes": (out / n).stat().st_size} for n in files
        },  # fmt: skip
        "parity": {"lowest_plane_iou": worst, "cases": parity},
    }


def run(adapter, pixels: np.ndarray, prompt: str, threshold: float) -> list[adapters.Instance]:
    queries = [q.strip().lower() for q in prompt.replace(",", ".").replace(";", ".").split(".")]
    queries = [q for q in queries if q]
    settings = adapters.Settings(threshold=threshold, max_instances=8)
    return adapters.order(adapter.find(pixels, queries, settings), 8)


def references() -> dict:
    """Freeze the maker's model's answers through the pack's adapter (the spec's check 1).
    EVF-SAM2 takes no threshold, so every threshold gives the same answer and the positive case
    freezes the first, 0.2."""
    adapter = adapters.EvfSam2(TorchBackend(model()))
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
    target = FIXTURES / "models" / "evfsam2"
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
    record = {"model": "evfsam2", "source": {"code": CODE, "code_commit": CODE_COMMIT,
              "repo": REPO, "revision": REVISION},
              "max_instances": 8, "positive_threshold": positive, "cases": cases}  # fmt: skip
    (target / "references.json").write_text(json.dumps(record, indent=2) + "\n")
    print(json.dumps({k: v for k, v in record.items() if k != "cases"}, indent=2))
    for c in cases:
        print(f"  {c['case']} {c['image']}: {len(c['instances'])} instance(s)")
    return record


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--out", type=Path, default=REPO_ROOT / "target/export/evfsam2")
    parser.add_argument("--references", action="store_true")
    args = parser.parse_args()
    if args.references:
        references()
        return 0
    record = {"exported": time.strftime("%Y-%m-%d"), **export(args.out)}
    (args.out / "export.json").write_text(json.dumps(record, indent=2) + "\n")
    print(json.dumps({k: v for k, v in record.items() if k != "parity"}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

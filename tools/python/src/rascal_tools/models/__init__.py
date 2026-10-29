"""The learned models the pack runs, on onnxruntime only: `owlv2` turns a text prompt into
boxes with scores, `sam2` turns a box into a plane; `cache` finds the exported graphs
(`artifacts.toml`, `MODEL.md`). The preprocessing is numpy and Pillow, mirroring the
upstream processors as `MODEL.md` describes; nothing here imports PyTorch."""

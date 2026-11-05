"""The rules that turn a segmenter's candidate outlines into the one `segment` keeps (SPEC.md
"Self-correcting remove", part 1). Pure functions over bool planes and boxes, no model, so the
lab runs the same rules over other segmenters' candidates.

Boxes are (x0, y0, x1, y1) in image pixels, end exclusive; a pixel is inside a box when its
centre is. Planes are bool arrays (height, width). Points are (x, y) pixel coordinates."""

from __future__ import annotations

import numpy as np

Box = tuple[float, float, float, float]

RULES = ("whole", "best")
INSIDE = 0.95  # the share of an outline that must lie inside its box
MIN_IOU = 0.5  # the predicted IoU an outline needs to be the whole-object pick
SPLIT = 0.8  # an outline whose largest piece holds less than this share is in pieces
MAX_POINTS = 8


def pad_box(box: Box, pad: float, width: int, height: int) -> Box:
    """Each side moved out by `pad` times the box's side on that axis, clamped to the image."""
    x0, y0, x1, y1 = box
    dx, dy = (x1 - x0) * pad, (y1 - y0) * pad
    return (
        max(0.0, x0 - dx),
        max(0.0, y0 - dy),
        min(float(width), x1 + dx),
        min(float(height), y1 + dy),
    )


def _box_slices(box: Box, shape: tuple[int, ...]) -> tuple[slice, slice]:
    """The rows and columns whose pixel centres fall inside the box."""
    x0, y0, x1, y1 = box
    height, width = shape[:2]
    c0 = max(0, int(np.ceil(x0 - 0.5)))
    c1 = min(width, int(np.ceil(x1 - 0.5)))
    r0 = max(0, int(np.ceil(y0 - 0.5)))
    r1 = min(height, int(np.ceil(y1 - 0.5)))
    return slice(r0, max(r0, r1)), slice(c0, max(c0, c1))


def inside_share(plane: np.ndarray, box: Box) -> float:
    """The share of the plane's pixels inside the box; 0 for an empty plane."""
    total = int(np.count_nonzero(plane))
    if total == 0:
        return 0.0
    rows, cols = _box_slices(box, plane.shape)
    return int(np.count_nonzero(plane[rows, cols])) / total


def pick(planes: list[np.ndarray], ious: list[float], box: Box, rule: str) -> int:
    """The index of the outline to keep.

    `whole`: of the non-empty candidates with at least 95% of their pixels inside the box and a
    predicted IoU of at least 0.5, the largest; ties by higher predicted IoU, then lower index.
    When none qualifies, `best`. `best`: the highest predicted IoU, ties by lower index."""
    if rule not in RULES:
        raise ValueError(f"outline must be one of {', '.join(RULES)}, not {rule!r}")
    best = max(range(len(planes)), key=lambda k: (ious[k], -k))
    if rule == "best":
        return best
    qualified = [
        k
        for k, p in enumerate(planes)
        if p.any() and ious[k] >= MIN_IOU and inside_share(p, box) >= INSIDE
    ]
    if not qualified:
        return best
    return max(qualified, key=lambda k: (int(np.count_nonzero(planes[k])), ious[k], -k))


def _labels(plane: np.ndarray) -> tuple[np.ndarray, int]:
    """8-connected component labels (1..n, 0 for unset) by runs per row joined with
    union-find, so a large plane costs its runs, not its pixels."""
    height, width = plane.shape
    parent: list[int] = [0]
    runs: list[tuple[int, int, int, int]] = []  # (row, start, end, run id)

    def find(a: int) -> int:
        while parent[a] != a:
            parent[a] = parent[parent[a]]
            a = parent[a]
        return a

    previous: list[tuple[int, int, int]] = []
    for row in range(height):
        line = plane[row]
        if not line.any():
            previous = []
            continue
        edges = np.flatnonzero(np.diff(np.concatenate(([0], line.astype(np.int8), [0]))))
        current: list[tuple[int, int, int]] = []
        j = 0
        for start, end in zip(edges[0::2], edges[1::2], strict=True):
            run = len(parent)
            parent.append(run)
            # A run touches a run above that overlaps [start - 1, end] (8-connected).
            while j < len(previous) and previous[j][1] < start:
                j += 1
            k = j
            while k < len(previous) and previous[k][0] <= end:
                a, b = find(run), find(previous[k][2])
                if a != b:
                    parent[max(a, b)] = min(a, b)
                k += 1
            if k > j:
                j = k - 1
            current.append((int(start), int(end), run))
            runs.append((row, int(start), int(end), run))
        previous = current
    labels = np.zeros((height, width), dtype=np.int32)
    roots: dict[int, int] = {}
    for row, start, end, run in runs:
        root = find(run)
        label = roots.setdefault(root, len(roots) + 1)
        labels[row, start:end] = label
    return labels, len(roots)


def pieces(plane: np.ndarray) -> list[np.ndarray]:
    """The plane's 8-connected pieces as bool planes, largest first, ties by the piece's first
    pixel in row-major order."""
    labels, count = _labels(plane)
    if count == 0:
        return []
    flat = labels.ravel()
    areas = np.bincount(flat, minlength=count + 1)
    first = np.full(count + 1, flat.size, dtype=np.int64)
    nonzero = np.flatnonzero(flat)
    np.minimum.at(first, flat[nonzero], nonzero)
    order = sorted(range(1, count + 1), key=lambda n: (-int(areas[n]), int(first[n])))
    return [labels == n for n in order]


def point_of(piece: np.ndarray) -> tuple[int, int]:
    """The piece's pixel nearest its centroid, ties by smaller y then smaller x, as (x, y): a
    point on the piece even when its centroid is not (a U, a ring)."""
    ys, xs = np.nonzero(piece)
    cy, cx = ys.mean(), xs.mean()
    distance = (ys - cy) ** 2 + (xs - cx) ** 2
    order = np.lexsort((xs, ys, distance))
    k = int(order[0])
    return int(xs[k]), int(ys[k])


def points(plane: np.ndarray) -> list[tuple[int, int]]:
    """One positive point per piece, in the pieces' order, at most 8."""
    return [point_of(p) for p in pieces(plane)[:MAX_POINTS]]


def needs_retry(plane: np.ndarray, box: Box, min_cover: float) -> bool:
    """True when the outline covers less than `min_cover` of its box, or its largest piece
    holds under 80% of its pixels."""
    rows, cols = _box_slices(box, plane.shape)
    area = (rows.stop - rows.start) * (cols.stop - cols.start)
    covered = int(np.count_nonzero(plane[rows, cols]))
    if area == 0 or covered / area < min_cover:
        return True
    found = pieces(plane)
    total = int(np.count_nonzero(plane))
    return bool(found) and int(np.count_nonzero(found[0])) < SPLIT * total


def accept_retry(first: np.ndarray, retry: np.ndarray, box: Box) -> bool:
    """The retry's outline replaces the first when it has more pixels and at least 95% of them
    inside its own box."""
    return (
        int(np.count_nonzero(retry)) > int(np.count_nonzero(first))
        and inside_share(retry, box) >= INSIDE
    )

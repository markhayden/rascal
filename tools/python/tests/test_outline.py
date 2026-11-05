"""The outline rules (`rascal_tools.outline`, SPEC.md "Self-correcting remove", part 1): pad a
box, pick the whole-object outline, split an outline into pieces, a point per piece, the
coverage check and the retry's acceptance. Synthetic planes only; no model."""

from __future__ import annotations

import numpy as np
import pytest

from rascal_tools import outline


def plane(height: int, width: int, *boxes: tuple[int, int, int, int]) -> np.ndarray:
    """A bool plane with each (x0, y0, x1, y1) rectangle set, end exclusive."""
    out = np.zeros((height, width), dtype=bool)
    for x0, y0, x1, y1 in boxes:
        out[y0:y1, x0:x1] = True
    return out


def test_pad_moves_each_side_by_a_share_of_the_box_and_clamps_to_the_image() -> None:
    assert outline.pad_box((10.0, 20.0, 30.0, 60.0), 0.1, 100, 100) == (8.0, 16.0, 32.0, 64.0)
    assert outline.pad_box((0.0, 0.0, 50.0, 100.0), 0.5, 60, 100) == (0.0, 0.0, 60.0, 100.0)
    assert outline.pad_box((10.0, 20.0, 30.0, 60.0), 0.0, 100, 100) == (10.0, 20.0, 30.0, 60.0)


def test_whole_takes_the_largest_outline_that_stays_inside_the_box() -> None:
    box = (0.0, 0.0, 20.0, 20.0)
    part = plane(30, 30, (2, 2, 6, 6))
    larger = plane(30, 30, (2, 2, 12, 12))
    whole = plane(30, 30, (1, 1, 19, 19))
    assert outline.pick([part, larger, whole], [0.95, 0.9, 0.7], box, "whole") == 2
    # best: the most confident, whatever its size.
    assert outline.pick([part, larger, whole], [0.95, 0.9, 0.7], box, "best") == 0


def test_whole_refuses_an_outline_that_spills_out_of_its_box() -> None:
    box = (0.0, 0.0, 10.0, 10.0)
    inside = plane(30, 30, (1, 1, 9, 9))
    spills = plane(30, 30, (0, 0, 25, 25))  # most of it is outside the box
    assert outline.pick([inside, spills, inside], [0.6, 0.9, 0.5], box, "whole") == 0


def test_whole_skips_an_outline_under_half_predicted_iou() -> None:
    box = (0.0, 0.0, 20.0, 20.0)
    small = plane(30, 30, (2, 2, 6, 6))
    big_but_unsure = plane(30, 30, (1, 1, 19, 19))
    assert outline.pick([small, big_but_unsure, small], [0.8, 0.49, 0.7], box, "whole") == 0


def test_whole_skips_empty_outlines_and_falls_back_to_best_when_none_qualifies() -> None:
    box = (0.0, 0.0, 10.0, 10.0)
    empty = np.zeros((30, 30), dtype=bool)
    outside = plane(30, 30, (15, 15, 30, 30))
    assert outline.pick([empty, outside, outside], [0.9, 0.4, 0.8], box, "whole") == 0
    assert outline.pick([outside, empty, outside], [0.3, 0.9, 0.3], box, "whole") == 1


def test_ties_break_by_higher_iou_then_lower_index() -> None:
    box = (0.0, 0.0, 20.0, 20.0)
    a = plane(30, 30, (0, 0, 10, 10))
    b = plane(30, 30, (10, 10, 20, 20))  # the same area as a
    assert outline.pick([a, b, a], [0.7, 0.8, 0.6], box, "whole") == 1
    assert outline.pick([a, b, a], [0.8, 0.8, 0.8], box, "whole") == 0
    assert outline.pick([a, b, a], [0.8, 0.8, 0.8], box, "best") == 0


def test_an_unknown_rule_is_refused() -> None:
    with pytest.raises(ValueError, match="outline"):
        outline.pick([plane(4, 4)], [0.5], (0.0, 0.0, 4.0, 4.0), "largest")


def test_pieces_are_eight_connected_and_ordered_largest_first_then_by_first_pixel() -> None:
    p = plane(20, 20, (0, 0, 2, 2), (10, 10, 15, 15), (15, 0, 17, 2))
    # A diagonal neighbour joins the first piece: (2, 2) touches (1, 1) at a corner.
    p[2, 2] = True
    found = outline.pieces(p)
    assert [int(q.sum()) for q in found] == [25, 5, 4]
    # Ties: the two smaller pieces' order is by area, so check the first-pixel rule apart.
    tie = outline.pieces(plane(10, 10, (6, 0, 8, 2), (0, 5, 2, 7)))
    assert [tuple(np.argwhere(q)[0]) for q in tie] == [(0, 6), (5, 0)]
    assert outline.pieces(np.zeros((5, 5), dtype=bool)) == []


def test_a_piece_point_lies_on_the_piece_even_when_its_centroid_does_not() -> None:
    # A U shape: its centroid sits in the gap between the arms.
    u = plane(20, 20, (2, 2, 4, 18), (16, 2, 18, 18), (2, 16, 18, 18))
    cy, cx = np.argwhere(u).mean(axis=0)
    assert not u[round(cy), round(cx)]
    x, y = outline.point_of(u)
    assert u[y, x]


def test_the_point_ties_break_by_smaller_y_then_smaller_x() -> None:
    # Two pixels equally near the centroid (2, 2): (1, 2) and (3, 2) by (y, x); smaller y wins.
    p = np.zeros((5, 5), dtype=bool)
    p[1, 2] = p[3, 2] = True
    assert outline.point_of(p) == (2, 1)


def test_points_are_one_per_piece_capped_at_eight() -> None:
    nine = plane(30, 30, *[(3 * k, 0, 3 * k + 1 + (k == 0), 1) for k in range(9)])
    assert len(outline.pieces(nine)) == 9
    assert len(outline.points(nine)) == 8


def test_a_thin_outline_or_one_in_pieces_needs_a_retry() -> None:
    box = (0.0, 0.0, 20.0, 20.0)
    solid = plane(30, 30, (0, 0, 20, 20))
    thin = plane(30, 30, (0, 0, 20, 2))  # 10% of the box
    split = plane(30, 30, (0, 0, 10, 20), (12, 0, 20, 20))  # largest piece 200 of 360
    assert not outline.needs_retry(solid, box, 0.2)
    assert outline.needs_retry(thin, box, 0.2)
    assert not outline.needs_retry(thin, box, 0.05)
    assert outline.needs_retry(split, box, 0.2)


def test_a_retry_is_kept_only_when_larger_and_inside_its_own_box() -> None:
    box = (0.0, 0.0, 20.0, 20.0)
    first = plane(30, 30, (0, 0, 10, 10))
    larger = plane(30, 30, (0, 0, 15, 15))
    smaller = plane(30, 30, (0, 0, 5, 5))
    spills = plane(30, 30, (0, 0, 30, 30))
    assert outline.accept_retry(first, larger, box)
    assert not outline.accept_retry(first, smaller, box)
    assert not outline.accept_retry(first, spills, box)


def _flood_areas(p: np.ndarray) -> list[int]:
    """8-connected piece areas by plain flood fill: the reference for the run labeller."""
    seen = np.zeros_like(p)
    areas = []
    for y, x in np.argwhere(p):
        if seen[y, x]:
            continue
        stack, area = [(y, x)], 0
        seen[y, x] = True
        while stack:
            cy, cx = stack.pop()
            area += 1
            for dy in (-1, 0, 1):
                for dx in (-1, 0, 1):
                    ny, nx = cy + dy, cx + dx
                    if (
                        0 <= ny < p.shape[0]
                        and 0 <= nx < p.shape[1]
                        and p[ny, nx]
                        and not seen[ny, nx]
                    ):
                        seen[ny, nx] = True
                        stack.append((ny, nx))
        areas.append(area)
    return sorted(areas, reverse=True)


def test_the_run_labeller_agrees_with_a_flood_fill_on_random_planes() -> None:
    rng = np.random.default_rng(7)
    for density in (0.2, 0.45, 0.6):
        for _ in range(20):
            p = rng.random((23, 31)) < density
            found = outline.pieces(p)
            assert sorted((int(q.sum()) for q in found), reverse=True) == _flood_areas(p)
            assert not np.any(np.sum(found, axis=0) > 1) if found else True

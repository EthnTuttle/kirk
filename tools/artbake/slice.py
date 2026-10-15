#!/usr/bin/env python3
"""Grid slicer + registration analyzer for animation sprite sheets (card 4.3).

Slices a sheet into a strict CxR grid of equal regions, strips bottom caption
bands (background-gap detection), saves one uniform square crop per cell
(per-row baseline anchor so as-drawn registration is preserved), and reports
silhouette registration: per-cell foreground bbox/centroid + pairwise IoU of
binary masks in the common square frame (drift quantification).

Stdlib + Pillow only.
Usage: slice.py <sheet.png> <outdir> [--grid 3x2] [--labels a,b,c]
       [--bg 0B0E1B] [--fg 25] [--gap 6]
"""
import json
import math
import os
import sys

from PIL import Image


def parse_args(argv):
    sheet, outdir = argv[1], argv[2]
    grid, labels, bg, fg, gap = "3x2", None, "0B0E1B", 25, 6
    i = 3
    while i < len(argv):
        a = argv[i]
        if a == "--grid":
            grid = argv[i + 1]; i += 2
        elif a == "--labels":
            labels = argv[i + 1].split(","); i += 2
        elif a == "--bg":
            bg = argv[i + 1]; i += 2
        elif a == "--fg":
            fg = int(argv[i + 1]); i += 2
        elif a == "--gap":
            gap = int(argv[i + 1]); i += 2
        else:
            sys.exit(f"unknown arg {a}")
    cols, rows = (int(x) for x in grid.lower().split("x"))
    return sheet, outdir, cols, rows, labels, bg, fg, gap


def fg_mask(px, w, h, x0, y0, bg, thr):
    """Binary mask (list of bytearrays) of pixels differing from bg over thr
    (max per-channel abs diff), region offset x0,y0 in image px."""
    m = []
    for y in range(y0, y0 + h):
        row = bytearray()
        for x in range(x0, x0 + w):
            p = px[x, y]
            d = max(abs(p[0] - bg[0]), abs(p[1] - bg[1]), abs(p[2] - bg[2]))
            row.append(1 if d > thr else 0)
        m.append(row)
    return m


def bbox_of(mask, w, h):
    x0, y0, x1, y1 = w, h, -1, -1
    n = 0
    sx = sy = 0
    for y in range(h):
        row = mask[y]
        for x in range(w):
            if row[x]:
                n += 1
                sx += x
                sy += y
                if x < x0: x0 = x
                if x > x1: x1 = x
                if y < y0: y0 = y
                if y > y1: y1 = y
    if n == 0:
        return None
    return {"x0": x0, "y0": y0, "x1": x1, "y1": y1,
            "w": x1 - x0 + 1, "h": y1 - y0 + 1, "n": n,
            "cx": round(sx / n, 1), "cy": round(sy / n, 1)}


def strip_caption(mask, w, h, gap):
    """Return cut_y: sprites live above the first >=gap-row all-background
    band starting past 55% of region height (caption band is below it)."""
    rowsum = [sum(mask[y]) for y in range(h)]
    start = int(h * 0.55)
    y = start
    while y + gap <= h:
        if all(rowsum[y + k] == 0 for k in range(gap)):
            return y
        y += 1
    return int(h * 0.86)  # fallback: fixed crop of bottom 14%


def main():
    sheet, outdir, cols, rows, labels, bghex, thr, gap = parse_args(sys.argv)
    bg = tuple(int(bghex[i:i + 2], 16) for i in (0, 2, 4))
    im = Image.open(sheet).convert("RGBA")
    W, H = im.size
    px = im.load()
    cells_dir = os.path.join(outdir, "cells")
    os.makedirs(cells_dir, exist_ok=True)

    xc = [round(i * W / cols) for i in range(cols + 1)]
    yc = [round(j * H / rows) for j in range(rows + 1)]
    side = min(xc[i + 1] - xc[i] for i in range(cols))  # uniform square side
    n_cells = cols * rows
    if labels is None:
        labels = [f"cell{i+1}" for i in range(n_cells)]
    assert len(labels) == n_cells, "label count != grid cells"

    cells = []  # per-cell record
    # pass 1: masks, caption strip, bboxes
    for j in range(rows):
        for i in range(cols):
            k = j * cols + i
            rw, rh = xc[i + 1] - xc[i], yc[j + 1] - yc[j]
            m = fg_mask(px, rw, rh, xc[i], yc[j], bg, thr)
            cut = strip_caption(m, rw, rh, gap)
            bb = bbox_of(m[:cut], rw, cut)
            cells.append({"i": i, "j": j, "label": labels[k], "rx": xc[i],
                          "ry": yc[j], "rw": rw, "rh": rh, "cut_y": cut,
                          "bbox": bb, "mask": m})
            assert bb, f"no foreground in cell {labels[k]}"

    # uniform square window per row: bottom anchored at row max sprite bottom
    for j in range(rows):
        row_cells = cells[j * cols:(j + 1) * cols]
        rh = row_cells[0]["rh"]
        bottom = max(c["bbox"]["y1"] for c in row_cells) + 3
        y1 = min(rh, bottom)
        y0 = max(0, y1 - side)
        for c in row_cells:
            x0 = c["rx"] + (c["rw"] - side) // 2
            crop = im.crop((x0, c["ry"] + y0, x0 + side, c["ry"] + y0 + side))
            out = os.path.join(cells_dir, f"{c['j']*cols+c['i']+1:02d}_{c['label']}.png")
            crop.save(out, "PNG")
            c["win"] = {"x0": x0, "y0": c["ry"] + y0, "side": side}
            c["file"] = out
            # mask of cell inside the common window coords
            c["wmask"] = [row[(c['rw'] - side) // 2:(c['rw'] - side) // 2 + side]
                          for row in c["mask"][y0:y0 + side]] if len(c["mask"]) >= y0 + side else None

    # registration analysis: pairwise IoU on common-frame masks + centroid drift
    def iou(a, b):
        inter = union = 0
        for y in range(len(a)):
            ra, rb = a[y], b[y]
            for x in range(len(ra)):
                va, vb = ra[x], rb[x]
                inter += va & vb
                union += va | vb
        return inter / union if union else 0.0

    for c in cells:
        m = c["wmask"]
        bb = bbox_of(m, side, len(m)) if m else None
        c["wframe_bbox"] = bb

    pairs = []
    for a in range(n_cells):
        for b in range(a + 1, n_cells):
            A, B = cells[a], cells[b]
            if A["wmask"] is None or B["wmask"] is None:
                continue
            v = iou(A["wmask"], B["wmask"])
            dx = B["wframe_bbox"]["cx"] - A["wframe_bbox"]["cx"]
            dy = B["wframe_bbox"]["cy"] - A["wframe_bbox"]["cy"]
            pairs.append({"a": A["label"], "b": B["label"], "iou": round(v, 3),
                          "dcx": round(dx, 1), "dcy": round(dy, 1)})

    ious = [p["iou"] for p in pairs]
    drifts_x = [abs(p["dcx"]) for p in pairs]
    drifts_y = [abs(p["dcy"]) for p in pairs]
    summary = {
        "sheet": sheet, "sheet_size": [W, H], "grid": [cols, rows],
        "square_side": side, "fg_threshold": thr,
        "iou_mean": round(sum(ious) / len(ious), 3),
        "iou_min": round(min(ious), 3), "iou_max": round(max(ious), 3),
        "drift_cx_max": round(max(drifts_x), 1),
        "drift_cy_max": round(max(drifts_y), 1),
        "drift_cx_mean": round(sum(drifts_x) / len(drifts_x), 1),
        "drift_cy_mean": round(sum(drifts_y) / len(drifts_y), 1),
        "cells": [{k: c[k] for k in ("label", "cut_y", "bbox", "win", "wframe_bbox")}
                  for c in cells],
        "pairs": pairs,
    }
    ap = os.path.join(outdir, "analysis.json")
    json.dump(summary, open(ap, "w"), indent=2)

    print(f"sheet {W}x{H} grid {cols}x{rows} square_side={side}px")
    for c in cells:
        b, w = c["bbox"], c["wframe_bbox"]
        print(f"  {c['label']:>7} cut@{c['cut_y']:>3} bbox {b['w']}x{b['h']} "
              f"centroid=({w['cx']},{w['cy']}) n={w['n']}")
    print(f"IoU mean={summary['iou_mean']} min={summary['iou_min']} "
          f"max={summary['iou_max']}")
    print(f"centroid drift px: cx max={summary['drift_cx_max']} "
          f"mean={summary['drift_cx_mean']} | cy max={summary['drift_cy_max']} "
          f"mean={summary['drift_cy_mean']} (of {side}px cell)")
    print(f"wrote {len(cells)} cells + {ap}")


if __name__ == "__main__":
    main()

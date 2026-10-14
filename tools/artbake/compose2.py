#!/usr/bin/env python3
"""Round-2 contact sheet, PIL edition.

Section A — hand-painted lane: rows = model, cols = the 4 portfolio pieces
(64x64 -> x4, 128x128 -> x2; every cell displays at DISPLAY x DISPLAY, NEAREST).
Section B — diffusion lane: rows = model, col1 = RAW (source res -> DISPLAY via
LANCZOS), col2 = quantized 64x64/16c (-> DISPLAY via NEAREST). Raw reveals what
the model imagined; quantized reveals what survives the game's canvas.
Writes r2/sheet.png + r2/INDEX2.txt.
"""
import os
from PIL import Image, ImageDraw

ROOT = os.path.dirname(os.path.abspath(__file__))
DISPLAY, GAP, LABELH, MARGIN = 256, 12, 26, 12
BG = (0x0B, 0x0E, 0x1B, 255)
INK = (0xD8, 0xD3, 0xC0, 255)
DIM = (0x8A, 0x85, 0x77, 255)

HAND_MODELS = ["deepseek-unc", "kimi-k3", "glm-53"]
HAND_PIECES = ["audric-battle", "prince-of-pride", "cleric-mabel", "audric-128"]
DIFF_DIR = os.path.join(ROOT, "ppq", "q64")
RAW_DIR = os.path.join(ROOT, "ppq", "truepng")


def fit(img, resample):
    return img.convert("RGBA").resize((DISPLAY, DISPLAY), resample)


def main():
    diff_models = sorted(
        f[:-4] for f in os.listdir(DIFF_DIR)
        if f.endswith(".png") and os.path.isfile(os.path.join(RAW_DIR, f))
    )

    hand_col_w = DISPLAY * 4 + GAP * 3
    diff_col_w = DISPLAY * 2 + GAP
    cw = max(hand_col_w, diff_col_w) + MARGIN * 2

    y = MARGIN
    rows, index = [], []

    def band(n_cells, first_label, second_labels):
        nonlocal y
        y += LABELH + 4
        return y + DISPLAY + GAP * 2

    # pre-measure
    h_band = LABELH + 4 + DISPLAY + GAP * 2
    height = MARGIN + LABELH + 4  # section titles
    height += h_band * len(HAND_MODELS) + LABELH + 8
    height += h_band * len(diff_models) + MARGIN
    sheet = Image.new("RGBA", (cw, height), BG)
    draw = ImageDraw.Draw(sheet)

    def text(x, yy, s, color=DIM):
        t = Image.new("RGBA", (len(s) * 6 + 4, 8), (0, 0, 0, 0))
        d = ImageDraw.Draw(t)
        d.text((0, 0), s, fill=color)
        t = t.resize((t.width * 2, t.height * 2), Image.NEAREST)
        sheet.paste(t, (x, yy), t)

    # ---------- Section A: hand-painted ----------
    text(MARGIN, y, "SECTION A — hand-painted (palette-authored grids)", INK)
    y += LABELH + 4
    for model in HAND_MODELS:
        d = os.path.join(ROOT, "r2", model)
        have = [p for p in HAND_PIECES if os.path.isfile(os.path.join(d, p + ".png"))]
        if not have:
            continue
        text(MARGIN, y, model + ("  [ELIMINATED-PROVIDER, kept: delivered]" if model == "glm-53" else ""))
        x = MARGIN
        for piece in HAND_PIECES:
            p = os.path.join(d, piece + ".png")
            if os.path.isfile(p):
                sheet.paste(fit(Image.open(p), Image.NEAREST), (x, y + LABELH), fit(Image.open(p), Image.NEAREST).split()[3])
            x += DISPLAY + GAP
        index.append(f"A/{model}: {', '.join(have)}")
        y += LABELH + DISPLAY + GAP * 2

    # ---------- Section B: diffusion ----------
    text(MARGIN, y, "SECTION B — diffusion (left: RAW model output, right: quantized to production canvas 64x64/16c)", INK)
    y += LABELH + 4
    for model in diff_models:
        text(MARGIN, y, model)
        raw = fit(Image.open(os.path.join(RAW_DIR, model + ".png")), Image.LANCZOS)
        quant = fit(Image.open(os.path.join(DIFF_DIR, model + ".png")), Image.NEAREST)
        sheet.paste(raw, (MARGIN, y + LABELH), raw.split()[3])
        sheet.paste(quant, (MARGIN + DISPLAY + GAP, y + LABELH), quant.split()[3])
        index.append(f"B/{model}: raw+quant")
        y += LABELH + DISPLAY + GAP * 2

    out = os.path.join(ROOT, "r2", "sheet.png")
    sheet.save(out, "PNG")
    with open(os.path.join(ROOT, "r2", "INDEX2.txt"), "w") as fh:
        fh.write("\n".join(index) + "\n")
    print(f"sheet: {cw}x{sheet.height} -> {out}")
    print("\n".join(index))


if __name__ == "__main__":
    main()

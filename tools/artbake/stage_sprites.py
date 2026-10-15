#!/usr/bin/env python3
"""Stage production sprites from the art bake-off staging area into
crusade-app/assets/ in the species_id-keyed layout the Bevy shell loads
(docs/ART-PIPELINE.md section 2):

    assets/sprites/champions/{id:03}-{slug}.png   128x128 battle sprite
    assets/sprites/icons/{id:03}.png              32x32 roster icon
    assets/sprites/scenes/<name>.png              128x128 scene/hero art
    assets/sprites/anim/{id:03}-{slug}/NN_<pose>.png  128x128 idle+arc frames

This script is the single source of truth for the art -> assets step
(cards D1 battle sprites + D2 roster icons). Stdlib + Pillow only.

The subject map below is hand-copied from crusade-core/src/data/species.rs
(names are the static SPECIES table; slugs are their lowercase hyphenated
form). Id 1 Squire Audric stages the `audric-battle` cell for the battle
slot per the user lock; `audric-prayer` is never a species sprite -- it
stages only via SCENES as the menu hero (card D3). The lane is USER-LOCKED
to nano-banana-2-1; the gpt-image-1.5 staging lane stays on disk untouched
as an alternate set.

Rerunnable/idempotent: outputs are pure functions of the staging cells and
are only rewritten when their bytes would change, so repeat runs do not
churn mtimes for bevy's (or trunk's) change detection.

Options:
  --lane NAME     staging lane under art/roster-staging/ (default:
                  nano-banana-2-1, the locked production lane).
  --no-key-bg     keep the flat ink-navy background tile. DEFAULT is keyed:
                  edge-connected flood fill over pixels EXACTLY matching the
                  palette ink #0B0E1B becomes transparent. Interior ink
                  pixels (armour shadow, coif shading) are NOT connected to
                  the border and stay opaque; the sprites are quantized to
                  hard palette edges, so no anti-aliased fringe exists and
                  keying is halo-free by construction (verified 2026-10-14:
                  0 partially-transparent pixels in all 19 outputs, and the
                  keyed tiles sit invisible on the shell's panels in
                  ui-shots). Applies to battle sprites and icons.
"""

import argparse
import os
import sys
from collections import deque

from PIL import Image

# (species_id, slug, staging cell) — species.rs static table order.
# id 1 -> audric-battle (NOT audric-prayer), per the user-locked decision.
SUBJECTS = [
    (1, "squire-audric", "audric-battle"),
    (2, "cleric-mabel", "cleric-mabel"),
    (3, "serjeant-bastien", "serjeant-bastien"),
    (4, "hospitaller-ysolt", "hospitaller-ysolt"),
    (5, "wayfarer-aelis", "wayfarer-aelis"),
    (6, "canoness-hrothwyn", "canoness-hrothwyn"),
    (7, "vexillor-anchetil", "vexillor-anchetil"),
    (8, "brother-aldwyn", "brother-aldwyn"),
    (9, "cantor-sabina", "cantor-sabina"),
    (10, "chevalier-raimbaud", "chevalier-raimbaud"),
    (11, "vigil-knight-wymar", "vigil-knight-wymar"),
    (12, "friar-osbert", "friar-osbert"),
    (101, "prince-of-pride", "prince-of-pride"),
    (102, "wretch-of-envy", "wretch-of-envy"),
    (103, "fiend-of-lust", "fiend-of-lust"),
    (104, "sleeper-of-sloth", "sleeper-of-sloth"),
    (105, "brute-of-wrath", "brute-of-wrath"),
    (106, "glutton-maw", "glutton-maw"),
    (107, "grasper-of-greed", "grasper-of-greed"),
]

CANVAS = 128  # production canvas (ART-STATUS section 1)
ICON = 32  # roster icon side (ART-PIPELINE section 6)
INK = (0x0B, 0x0E, 0x1B)  # palette background: shell NAVY, quarter-dark shell

# Decorative scene/hero slots (card D3): staging cell -> sprites/scenes/
# <cell>.png. These are ambience, never species sprites: `audric-prayer`
# here is the menu hero, while species id 1 keeps `audric-battle` for the
# battle slot (user lock, unchanged).
SCENES = [
    ("audric-prayer",),
]

# Battle animation frames (cards D5 + D6): copied from the staging area
# art/anim-staging/<cell>/ to sprites/anim/{id:03}-{slug}/NN_<pose>.png
# (stem minus the `_q128` tag), keyed by the same edge-flood pass as the
# battle sprites. Only the poses the shell actually animates ship: `01_idle`
# for every champion plus `04_idle2` where the sheet is the 4.5 batch grid
# (card D5 idle loop), and the card D6 combat arc `07_strike`/`08_hit`
# per-frame generations (4.3 hybrid decision: arcs are per-frame, never
# sheet slices). `audric-battle` is the 4.3 PILOT grid (idle/step/strike/
# hit/defend/pray — no idle2), so its loop is the single idle frame; its D6
# arc frames are the same per-frame generations as every other champion
# (07/08), NOT the pilot's 03/04 cells. Bosses (ids 101+) have no anim
# sheets at all and stay on the static battle sprite; their hurt feedback
# is a shell-side tint flash (battle.rs). step/pray/look/bow and the
# pilot's combat family stay in art/anim-staging only; pray never
# auto-plays.
ANIM_POSES = ["01_idle", "04_idle2", "07_strike", "08_hit"]
ANIM_POSES_BY_CELL = {
    "audric-battle": ["01_idle", "07_strike", "08_hit"],
}

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def parse_args(argv):
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--lane", default="nano-banana-2-1",
                   help="staging lane under art/roster-staging/ (default: the locked production lane)")
    p.add_argument("--staging", default=None,
                   help="override the staging root (default: <repo>/art/roster-staging/<lane>)")
    p.add_argument("--assets", default=None,
                   help="override the assets root (default: <repo>/crusade-app/assets)")
    p.add_argument("--anim-staging", default=None,
                   help="override the anim staging root (default: <repo>/art/anim-staging)")
    p.add_argument("--key-bg", dest="key_bg", action="store_true", default=True,
                   help="key the edge-connected ink-navy background to transparency (default)")
    p.add_argument("--no-key-bg", dest="key_bg", action="store_false",
                   help="keep the opaque ink-navy background tile (A/B inspection)")
    args = p.parse_args(argv)
    if args.staging is None:
        args.staging = os.path.join(REPO_ROOT, "art", "roster-staging", args.lane)
    if args.assets is None:
        args.assets = os.path.join(REPO_ROOT, "crusade-app", "assets")
    if args.anim_staging is None:
        args.anim_staging = os.path.join(REPO_ROOT, "art", "anim-staging")
    return args


def key_edge_connected_ink(img):
    """Zero the alpha of ink-navy pixels connected to the image border.

    A pixel is background only if it is exactly INK (the quantized palette
    has no near-ink fringe, so no tolerance/fuzz is needed) and reachable
    from the border through ink pixels (4-connectivity). Interior ink stays
    fully opaque. Returns (image, keyed_pixel_count).
    """
    w, h = img.size
    px = img.load()
    is_ink = [False] * (w * h)
    q = deque()
    for y in range(h):
        for x in range(w):
            r, g, b, _a = px[x, y]
            if (r, g, b) == INK:
                is_ink[y * w + x] = True
                if x in (0, w - 1) or y in (0, h - 1):
                    q.append(y * w + x)
    seen = set(q)
    while q:
        i = q.popleft()
        x, y = i % w, i // w
        for nx, ny in ((x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)):
            if 0 <= nx < w and 0 <= ny < h:
                ni = ny * w + nx
                if is_ink[ni] and ni not in seen:
                    seen.add(ni)
                    q.append(ni)
    for i in seen:
        x, y = i % w, i // w
        r, g, b, _ = px[x, y]
        px[x, y] = (r, g, b, 0)
    return img, len(seen)


def write_png_if_changed(img, path):
    """Idempotent write: encode in memory, skip the disk write when the
    existing file is already byte-identical."""
    import io

    buf = io.BytesIO()
    img.save(buf, format="PNG", optimize=True)
    data = buf.getvalue()
    if os.path.exists(path):
        with open(path, "rb") as fh:
            if fh.read() == data:
                return False
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as fh:
        fh.write(data)
    return True


def load_canvas(src):
    """Open a staging cell's q128.png and enforce the production canvas."""
    if not os.path.exists(src):
        print(f"stage_sprites: MISSING {src}", file=sys.stderr)
        sys.exit(1)
    img = Image.open(src).convert("RGBA")
    if img.size != (CANVAS, CANVAS):
        print(f"stage_sprites: {src} is {img.size}, expected {CANVAS}x{CANVAS}", file=sys.stderr)
        sys.exit(1)
    return img


def main(argv):
    args = parse_args(argv)
    if not os.path.isdir(args.staging):
        print(f"stage_sprites: staging lane not found: {args.staging}", file=sys.stderr)
        return 2

    written, kept = 0, 0
    print(f"stage_sprites: lane={os.path.basename(args.staging)} key_bg={args.key_bg}")
    print(f"  {'id':>4} {'slug':22} {'cell':22} {'keyed px':>8}  outputs")
    for species_id, slug, cell in SUBJECTS:
        src = os.path.join(args.staging, cell, "q128.png")
        img = load_canvas(src)

        keyed = 0
        if args.key_bg:
            img, keyed = key_edge_connected_ink(img)

        sprite = os.path.join(
            args.assets, "sprites", "champions", f"{species_id:03d}-{slug}.png"
        )
        icon = os.path.join(args.assets, "sprites", "icons", f"{species_id:03d}.png")
        icon_img = img.resize((ICON, ICON), Image.Resampling.NEAREST)

        for path, im in ((sprite, img), (icon, icon_img)):
            if write_png_if_changed(im, path):
                written += 1
            else:
                kept += 1
        print(f"  {species_id:>4} {slug:22} {cell:22} {keyed:>8}  "
              f"{os.path.relpath(sprite, args.assets)} + {os.path.relpath(icon, args.assets)}")

    for (cell,) in SCENES:
        src = os.path.join(args.staging, cell, "q128.png")
        img = load_canvas(src)
        keyed = 0
        if args.key_bg:
            img, keyed = key_edge_connected_ink(img)
        scene = os.path.join(args.assets, "sprites", "scenes", f"{cell}.png")
        if write_png_if_changed(img, scene):
            written += 1
        else:
            kept += 1
        print(f"  {'':>4} {'scene':22} {cell:22} {keyed:>8}  "
              f"{os.path.relpath(scene, args.assets)}")

    # Cards D5/D6: idle-loop + combat-arc frame sets under sprites/anim/.
    # Missing poses on a listed cell are an error like any missing cell
    # (exit 1 via load_canvas) EXCEPT the boss rows, which are skipped
    # outright below.
    anim_frames = 0
    if not os.path.isdir(args.anim_staging):
        print(f"stage_sprites: anim staging not found: {args.anim_staging} "
              f"(skipping anim-frame staging)", file=sys.stderr)
    for species_id, slug, cell in SUBJECTS:
        if species_id >= 100:
            continue  # bosses: no anim sheets (4.5), static sprite only
        poses = ANIM_POSES_BY_CELL.get(cell, ANIM_POSES)
        if not os.path.isdir(args.anim_staging):
            break
        outs = []
        for pose in poses:
            src = os.path.join(args.anim_staging, cell, f"{pose}_q128.png")
            img = load_canvas(src)
            if args.key_bg:
                img, _keyed = key_edge_connected_ink(img)
            out = os.path.join(
                args.assets, "sprites", "anim", f"{species_id:03d}-{slug}", f"{pose}.png"
            )
            if write_png_if_changed(img, out):
                written += 1
            else:
                kept += 1
            anim_frames += 1
            outs.append(f"{pose}.png")
        print(f"  {species_id:>4} {slug:22} {cell:22} {'':>8}  "
              f"anim/{species_id:03d}-{slug}/{{{', '.join(outs)}}}")

    n = len(SUBJECTS)
    total = 2 * n + len(SCENES) + anim_frames
    print(f"stage_sprites: {n} subjects + {len(SCENES)} scene(s) + {anim_frames} anim frames, "
          f"{written} files written, "
          f"{kept} unchanged, {total} files expected under "
          f"{os.path.relpath(args.assets, REPO_ROOT)}/sprites/")
    if os.path.basename(args.staging.rstrip(os.sep)) != "nano-banana-2-1":
        print("stage_sprites: NOTE — non-default lane staged; production lock is nano-banana-2-1")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

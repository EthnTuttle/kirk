#!/usr/bin/env python3
"""Build the static Lore site (site/dist/lore/) from tools/lore_content.json
plus the repo's art masters. Pixel-art masters are upscaled 4x nearest;
painterly raws are downscaled to 512px JPEG. Output is fully static and
self-contained (one html + img/), suitable for the Pages deploy pipeline.

Usage: python3 tools/build_lore.py
"""
from __future__ import annotations

import html
import json
import shutil
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
CONTENT = ROOT / "tools" / "lore_content.json"
OUT = ROOT / "site" / "dist" / "lore"
IMG_OUT = OUT / "img"

CSS = """
body{background:#0B0E1B;color:#E8E3D3;font-family:Georgia,serif;margin:0;line-height:1.6}
header{position:sticky;top:0;background:#0B0E1B;border-bottom:1px solid #2a3350;z-index:5;padding:.8rem 1.5rem}
header h1{color:#F4D35E;margin:0;font-size:1.3rem}
header nav{margin-top:.3rem}
header a{color:#c9c2ae;text-decoration:none;margin-right:1.1rem;font-size:.9rem}
header a:hover{color:#F4D35E}
main{max-width:960px;margin:0 auto;padding:1.5rem}
.dedication{color:#9a92a6;font-style:italic;margin-bottom:2rem}
h2{color:#F4D35E;border-bottom:1px solid #2a3350;padding-bottom:.3rem;margin-top:3rem}
.intro{color:#b9b2a0;font-style:italic;margin-bottom:1.5rem}
article{display:flex;gap:1.5rem;margin:2.5rem 0;border:1px solid #2a3350;border-radius:8px;padding:1.2rem;background:#101527}
article:nth-child(even){flex-direction:row-reverse}
.figwrap{display:flex;flex-direction:column;gap:6px}
.thumbs{display:flex;gap:5px;flex-wrap:wrap}
.thumb{width:56px;height:56px;object-fit:cover;border:1px solid #2a3350;border-radius:4px;cursor:pointer;opacity:.75}
.thumb:hover{opacity:1;border-color:#F4D35E}
article .figwrap > img{width:280px;max-width:38vw;align-self:flex-start;cursor:pointer;image-rendering:auto;border:1px solid #2a3350;border-radius:4px}
article .figwrap > img.pixel{image-rendering:pixelated}
article h3{color:#F4D35E;margin:.1rem 0 .1rem}
article .sub{color:#9a92a6;font-style:italic;margin-bottom:.7rem}
article p{margin:.4rem 0}
@media(max-width:640px){article,article:nth-child(even){flex-direction:column}article .figwrap > img{max-width:100%;width:100%}}
#lb{display:none;position:fixed;inset:0;background:rgba(0,0,0,.9);z-index:20;padding:4vh 4vw;cursor:pointer}
#lb img{display:block;margin:auto;max-height:88vh;max-width:92vw;image-rendering:auto}
footer{color:#5a6070;font-size:.8rem;padding:2rem 1.5rem;border-top:1px solid #2a3350;margin-top:3rem}
"""

JS = """
function zoom(src){let l=document.getElementById('lb');l.querySelector('img').src=src;l.style.display='block';}
document.getElementById('lb').onclick=function(){this.style.display='none';};
function swap(slug,src){document.getElementById('fig-'+slug).src=src;}
"""


def bake_image(src: Path, slug: str) -> str:
    """Copy/convert the art into img/ at a web-friendly 512px. Pixel-art
    masters (<= 256px) upscale nearest + keep PNG; larger art downsizes
    LANCZOS to JPEG."""
    im = Image.open(src).convert("RGB")
    # Pixel art (small masters, or any size with a flat locked palette)
    # keeps hard pixels as PNG; painterly masters downsize to JPEG.
    colors = im.getcolors(maxcolors=1 << 16)
    is_pixel = max(im.size) <= 256 or (colors is not None and len(colors) <= 64)
    if is_pixel:
        scale = max(1, 512 // max(im.size))
        im = im.resize((im.width * scale, im.height * scale), Image.NEAREST)
        name = f"{slug}.png"
        im.save(IMG_OUT / name)
        return name + "|pixel"
    im.thumbnail((512, 512), Image.LANCZOS)
    name = f"{slug}.jpg"
    im.save(IMG_OUT / name, quality=86)
    return name + "|paint"


def main() -> None:
    content_path = Path(sys.argv[1]) if len(sys.argv) > 1 else CONTENT
    content = json.loads(content_path.read_text())
    if OUT.exists():
        shutil.rmtree(OUT)
    IMG_OUT.mkdir(parents=True)

    nav = []
    body = []
    for sec in content["sections"]:
        sid = html.escape(sec["id"])
        nav.append(f"<a href='#{sid}'>{html.escape(sec['title'])}</a>")
        body.append(f"<h2 id='{sid}'>{html.escape(sec['title'])}</h2>")
        if sec.get("intro"):
            body.append(f"<p class='intro'>{html.escape(sec['intro'])}</p>")
        for e in sec["entries"]:
            imgs = e.get("imgs") or [e["img"]]
            baked = []
            for i, rp in enumerate(imgs):
                img_src = ROOT / rp
                if not img_src.exists():
                    raise SystemExit(f"missing art: {rp} (entry {e['slug']})")
                suff = "" if i == 0 else f"--{i}"
                baked.append(bake_image(img_src, f"{e['slug']}{suff}"))
            first_name, first_kind = baked[0].split("|")
            cls = " class='pixel'" if first_kind == "pixel" else ""
            thumbs = "".join(
                f"<img class='thumb' src='img/{b.split(chr(124))[0]}' onclick=\"swap('{e['slug']}',this.src)\">"
                for b in baked
            )
            paras = "".join(
                f"<p>{html.escape(p)}</p>" for p in e["body"].split("\n\n") if p.strip()
            )
            body.append(
                f"<article id='{html.escape(e['slug'])}'>"
                f"<div class='figwrap'><img{cls} id='fig-{html.escape(e['slug'])}' src='img/{first_name}'"
                f" onclick=\"zoom(document.getElementById('fig-{html.escape(e['slug'])}').src)\">"
                + (f"<div class='thumbs'>{thumbs}</div>" if len(baked) > 1 else "")
                + f"</div>"
                f"<div><h3>{html.escape(e['title'])}</h3>"
                f"<div class='sub'>{html.escape(e.get('subtitle', ''))}</div>{paras}</div>"
                f"</article>"
            )

    page = [
        "<!doctype html><html lang='en'><head><meta charset='utf-8'>",
        "<meta name='viewport' content='width=device-width,initial-scale=1'>",
        "<title>Deus Vult — A Chronicle of the Road East</title>",
        f"<style>{CSS}</style></head><body>",
        "<header><h1>Deus Vult — Lore: A Chronicle of the Road East</h1>",
        "<nav>" + "".join(nav) + "</nav></header>",
        "<main>",
        "<p class='dedication'>Set down by a clerk of the company, that those who come after may know the faces of the blessed and the worthy.</p>",
        *body,
        "</main>",
        "<div id='lb'><img></div>",
        "<footer>Deus Vult: Champions of the Cross — lore and art from the game. The chronicle register governs every entry.</footer>",
        f"<script>{JS}</script></body></html>",
    ]
    (OUT / "index.html").write_text("\n".join(page))
    n = sum(len(s["entries"]) for s in content["sections"])
    print(f"lore site -> {OUT}/index.html ({n} entries, {len(list(IMG_OUT.iterdir()))} images)")


if __name__ == "__main__":
    main()

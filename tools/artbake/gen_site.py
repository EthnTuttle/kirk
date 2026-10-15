#!/usr/bin/env python3
"""Regenerate index.html (served from the bakeoff root) for the winner portfolio.
Reads portfolio/<model>/<subject>/{raw,q64,q128}.png. Idempotent."""
import html, json, os

ROOT = os.path.dirname(os.path.abspath(__file__))
PORT = os.path.join(ROOT, "portfolio")

SECTIONS = [
    ("Champions of the Cross", [
        ("audric-prayer", "Squire Audric — Dawn Prayer (hero)"),
        ("audric-battle", "Squire Audric — Battle Stance"),
        ("cleric-mabel", "Cleric Mabel — Charity"),
        ("serjeant-bastien", "Serjeant Bastien — Fortitude"),
        ("hospitaller-ysolt", "Hospitaller Ysolt — Mercy"),
        ("wayfarer-aelis", "Wayfarer Aelis — the pilgrim scout"),
        ("canoness-hrothwyn", "Canoness Hrothwyn — the choir voice"),
        ("vexillor-anchetil", "Vexillor Anchetil — the standard-bearer"),
        ("brother-aldwyn", "Brother Aldwyn — Humility"),
        ("cantor-sabina", "Cantor Sabina — the measured psalm"),
        ("chevalier-raimbaud", "Chevalier Raimbaud — Patience in arms"),
        ("vigil-knight-wymar", "Vigil-Knight Wymar — the long vigil"),
        ("friar-osbert", "Friar Osbert — Purity of heart"),
    ]),
    ("Sin-Spawn (bosses)", [
        ("prince-of-pride", "Prince of Pride"),
        ("wretch-of-envy", "Wretch of Envy"),
        ("fiend-of-lust", "Fiend of Lust"),
        ("sleeper-of-sloth", "Sleeper of Sloth"),
        ("brute-of-wrath", "Brute of Wrath"),
        ("glutton-maw", "Glutton Maw"),
        ("grasper-of-greed", "Grasper of Greed"),
    ]),
]

def main():
    models = sorted(d for d in os.listdir(PORT) if os.path.isdir(os.path.join(PORT, d)))
    costs = {}
    for cf in ("costs.json", "costs_full.json"):
        p = os.path.join(PORT, cf)
        if os.path.isfile(p):
            costs.update(json.load(open(p)).get("results", {}))

    css = """
body{background:#0B0E1B;color:#D8D3C0;font-family:ui-monospace,Menlo,Consolas,monospace;margin:0;padding:24px}
h1{color:#F4D35E;font-size:22px;margin:0 0 4px}
h2{color:#C9A227;font-size:16px;margin:28px 0 10px;border-bottom:1px solid #232946;padding-bottom:4px}
.sub{color:#8A8577;font-size:12px;margin-bottom:18px}
.cards{display:flex;flex-wrap:wrap;gap:16px}
.card{background:#12152b;border:1px solid #232946;border-radius:8px;padding:12px;width:max-content}
.card h3{margin:0 0 8px;font-size:13px;color:#F5F1E6}
.row{display:flex;gap:10px;align-items:flex-end}
.cell{text-align:center;font-size:10px;color:#8A8577}
.cell img{display:block;margin-bottom:4px;background:#0B0E1B;border:1px solid #232946;border-radius:4px}
.pix{image-rendering:pixelated}
footer{margin-top:32px;font-size:11px;color:#5C6470;line-height:1.6}
.badge{font-size:10px;color:#0B0E1B;background:#C9A227;border-radius:3px;padding:1px 5px;margin-left:6px}
"""

    parts = [f"<!doctype html><meta charset=utf-8><title>Portrait Gallery — Deus Vult winners</title><style>{css}</style>"]
    parts.append("<h1>Portrait Gallery</h1>")
    parts.append('<div class=sub>Deus Vult: Champions of the Cross · winner models: nano-banana-2-1 + gpt-image-1.5 · '
                 'full canon roster: 12 champions + 7 sin-spawn · each subject per model as RAW / 64×64 / 128×128 (16-col production palette)</div>')

    for sec_title, sec_subjects in SECTIONS:
        present_any = any(
            os.path.isfile(os.path.join(PORT, m, s, "raw.png"))
            for s, _ in sec_subjects for m in models)
        if not present_any:
            continue
        parts.append(f'<h2 style="font-size:19px;color:#F4D35E">{html.escape(sec_title)}</h2>')
        for s, title in sec_subjects:
            present = [m for m in models if os.path.isfile(os.path.join(PORT, m, s, "raw.png"))]
            if not present:
                continue
            parts.append(f"<h2>{html.escape(title)}</h2><div class=cards>")
            for m in present:
                c = costs.get(f"{m}/{s}", {})
                cost = c.get("cost"); dims = c.get("src_dims", "?")
                badge = '<span class=badge>free</span>' if cost == 0 else (f'<span class=badge>${cost:.3f}</span>' if cost else "")
                parts.append(f"<div class=card><h3>{html.escape(m)}{badge}</h3><div class=row>")
                parts.append(f'<div class=cell><img src="portfolio/{m}/{s}/raw.png" width=200 alt="{m} {s} raw">raw<br>{dims}</div>')
                parts.append(f'<div class=cell><img class=pix src="portfolio/{m}/{s}/q64.png" width=128 alt="q64">64×64 / 16c</div>')
                parts.append(f'<div class=cell><img class=pix src="portfolio/{m}/{s}/q128.png" width=128 alt="q128">128×128 / 16c</div>')
                parts.append("</div></div>")
            parts.append("</div>")

    parts.append("""<footer>
RAW = model output as delivered (aspect-corrected, 24h-signed URLs archived to disk).<br>
64×64 / 128×128 = nearest-palette quantization onto the game's 16-color canvas
(ink·midnight·gold·palegold·bone·dimbone·red·brightred·leather·wood·steel·steeldk·white·green·bruise).<br>
Bake-off lineage: 3 hand-painted finalists (grid-authored) + 7 diffusion entries — these two won your pick.
Production use: quantized sprites match crusade-app's palette; raw files are concept art only.
</footer>""")

    out = os.path.join(ROOT, "index.html")
    open(out, "w").write("\n".join(parts))
    print(f"wrote {out} ({len(models)} models x {sum(len(s) for _, s in SECTIONS)} subjects in {len(SECTIONS)} sections)")


if __name__ == "__main__":
    main()

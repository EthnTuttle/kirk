#!/usr/bin/env python3
"""Winner portfolio: nano-banana-2-1 + gpt-image-1.5 x 4 canon subjects.
Per image: raw truepng + quant 64 + quant 128 under portfolio/<model>/<subject>/.
Key via PPQ_API_KEY env only. -> portfolio/costs.json"""
import json, os, subprocess, urllib.request, urllib.error

API = "https://api.ppq.ai/v1/images/generations"
KEY = os.environ.get("PPQ_API_KEY", "")
ROOT = os.path.dirname(os.path.abspath(__file__))
PORT = os.path.join(ROOT, "portfolio")

MODELS = [
    ("nano-banana-2-1", {}),
    ("gpt-image-1.5", {"quality": "low"}),
]

STYLE = (
    "Chunky 16-bit JRPG pixel-art style game sprite, limited retro color palette, "
    "crisp pixel clusters, readable silhouette, centered composition, plain solid "
    "dark navy background (#0B0E1B), absolutely no text, no letters, no watermark, "
    "no border, no card frame. Subject: "
)
SUBJECTS = {
    "audric-prayer": (
        "Squire Audric, a humble young Christian knight-in-training kneeling in dawn "
        "prayer, bowed head, hands clasped, plain bone-colored garb, small gold cross "
        "at his chest, wooden practice sword laid on the ground before him, soft "
        "pale-gold dawn rays falling from the upper left."
    ),
    "audric-battle": (
        "Squire Audric in battle stance, side profile, worn leather jerkin over a "
        "bone-colored tunic, wooden practice sword half-raised, weight forward, "
        "determined but humble expression."
    ),
    "prince-of-pride": (
        "the Prince of Pride, a towering crowned shadow-figure boss monster, cracked "
        "gold crown, body of ink and midnight blue, jagged mirror shards orbiting "
        "him, bruise-purple and deep red and gold accents, menacing allegory, "
        "not gory."
    ),
    "cleric-mabel": (
        "Cleric Mabel, a gentle feminine cleric in a white and bone-colored habit "
        "with small gold accents, warm kind face, holding an open alms purse mid-gift, "
        "a few gold coins tipping out."
    ),
}

def gen(model, extra, subject, prompt_id):
    payload = {"model": model, "prompt": STYLE + SUBJECTS[subject], "n": 1,
               "size": "1:1", "output_format": "png",
               "client_request_id": f"kirk-portfolio-{model}-{subject}-v1"}
    payload.update(extra)
    req = urllib.request.Request(API, data=json.dumps(payload).encode(),
        headers={"Authorization": f"Bearer {KEY}", "Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=290) as r:
        body = json.load(r)
    with urllib.request.urlopen(body["data"][0]["url"], timeout=120) as r:
        return r.read(), body.get("cost")

results, failures = {}, {}
for model, extra in MODELS:
    for subject in SUBJECTS:
        key = f"{model}/{subject}"
        d = os.path.join(PORT, model, subject)
        os.makedirs(d, exist_ok=True)
        try:
            png, cost = gen(model, extra, subject, "v1")
            raw = os.path.join(d, "raw.bin")          # format sniffed by PIL
            open(raw, "wb").write(png)
            from PIL import Image
            im = Image.open(raw)
            w, h = im.size
            if w != h:
                s = min(w, h); l, t = (w-s)//2, (h-s)//2
                im = im.crop((l, t, l+s, t+s))
            tp = os.path.join(d, "raw.png")
            im.convert("RGBA").save(tp, "PNG")
            os.remove(raw)
            for q in (64, 128):
                subprocess.run(["python3", os.path.join(ROOT, "quant.py"), tp,
                                os.path.join(d, f"q{q}.png"), "--size", str(q)],
                               check=True, capture_output=True)
            results[key] = {"bytes": len(png), "cost": cost, "src_dims": f"{w}x{h}"}
            print(f"OK   {key:<40} {w}x{h} cost={cost}", flush=True)
        except urllib.error.HTTPError as e:
            failures[key] = f"HTTP {e.code}: {e.read()[:100]}"
            print(f"FAIL {key:<40} HTTP {e.code}", flush=True)
        except Exception as e:
            failures[key] = f"{type(e).__name__}: {str(e)[:100]}"
            print(f"FAIL {key:<40} {type(e).__name__}", flush=True)

json.dump({"results": results, "failures": failures},
          open(os.path.join(PORT, "costs.json"), "w"), indent=2)
total = sum((v.get("cost") or 0) for v in results.values())
print(f"DONE ok={len(results)} fail={len(failures)} portfolio_cost=${total:.4f}")

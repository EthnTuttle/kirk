#!/usr/bin/env python3
"""Final ppq stragglers: fast-sdxl with aspect_ratio; gpt-image-1.5 replaces
twice-timed-out gpt-image-1. Key via PPQ_API_KEY env only. -> costs3.json"""
import json, os, urllib.request, urllib.error

API = "https://api.ppq.ai/v1/images/generations"
KEY = os.environ.get("PPQ_API_KEY", "")
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "ppq")

PROMPT = (
    "Pixel-art game sprite, chunky 16-bit JRPG boss-screen style: "
    "\"Squire Audric\", a humble young Christian knight-in-training kneeling "
    "in dawn prayer, bowed head, hands clasped, plain bone-colored garb, small "
    "gold cross at his chest, wooden practice sword laid on the ground before "
    "him, soft pale-gold dawn rays falling from the upper left. Plain solid "
    "dark navy background (#0B0E1B). Limited retro color palette, crisp pixel "
    "clusters, readable silhouette, centered composition, no text, no border."
)

ATTEMPTS = [
    ("fast-sdxl", {"model": "fast-sdxl", "prompt": PROMPT, "n": 1,
                   "aspect_ratio": "1:1", "output_format": "png",
                   "client_request_id": "kirk-bakeoff-r2-fast-sdxl-v2"}),
    ("gpt-image-1.5", {"model": "gpt-image-1.5", "prompt": PROMPT, "n": 1,
                       "size": "1:1", "output_format": "png", "quality": "low",
                       "client_request_id": "kirk-bakeoff-r2-gpt-image-15-v1"}),
]

results, failures = {}, {}
for name, payload in ATTEMPTS:
    try:
        req = urllib.request.Request(
            API, data=json.dumps(payload).encode(),
            headers={"Authorization": f"Bearer {KEY}", "Content-Type": "application/json"})
        with urllib.request.urlopen(req, timeout=290) as r:
            body = json.load(r)
        url = body["data"][0]["url"]
        cost = body.get("cost")
        with urllib.request.urlopen(url, timeout=120) as r:
            png = r.read()
        with open(os.path.join(OUT, f"{name}.png"), "wb") as f:
            f.write(png)
        results[name] = {"bytes": len(png), "cost": cost}
        print(f"OK   {name:<16} {len(png):>8} B  cost={cost}", flush=True)
    except urllib.error.HTTPError as e:
        print(f"FAIL {name:<16} HTTP {e.code} {e.read()[:110]}", flush=True)
        failures[name] = e.code
    except Exception as e:
        print(f"FAIL {name:<16} {type(e).__name__} {str(e)[:110]}", flush=True)
        failures[name] = str(e)[:80]

json.dump({"results": results, "failures": {k: str(v) for k, v in failures.items()}},
          open(os.path.join(OUT, "costs3.json"), "w"), indent=2)
print("DONE")

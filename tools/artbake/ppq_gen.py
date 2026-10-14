#!/usr/bin/env python3
"""PPQ image-generation bake-off lane. One 1:1 image per model, identical brief.
Key arrives ONLY via PPQ_API_KEY env; never written to disk.
Outputs: ppq/<model>.png + ppq/costs.json (+ failures logged)."""
import json, os, sys, time, urllib.request, urllib.error

API = "https://api.ppq.ai/v1/images/generations"
KEY = os.environ.get("PPQ_API_KEY", "")
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "ppq")
os.makedirs(OUT, exist_ok=True)

MODELS = [
    "flux-1-schnell",
    "flux-2-pro",
    "seedream-4.5",
    "qwen-image-3",
    "gpt-image-1",
    "recraft-v4.1",
    "nano-banana-2-lite",
]

PROMPT = (
    "Pixel-art game sprite, chunky 16-bit JRPG boss-screen style: "
    "\"Squire Audric\", a humble young Christian knight-in-training kneeling "
    "in dawn prayer, bowed head, hands clasped, plain bone-colored garb, small "
    "gold cross at his chest, wooden practice sword laid on the ground before "
    "him, soft pale-gold dawn rays falling from the upper left. Plain solid "
    "dark navy background (#0B0E1B). Limited retro color palette, crisp pixel "
    "clusters, readable silhouette, centered composition, no text, no border."
)

def gen(model):
    payload = {
        "model": model,
        "prompt": PROMPT,
        "n": 1,
        "size": "1:1",
        "output_format": "png",
        "client_request_id": f"kirk-bakeoff-r2-{model}-v1",
    }
    if model == "gpt-image-1":
        payload["quality"] = "low"
    req = urllib.request.Request(
        API,
        data=json.dumps(payload).encode(),
        headers={"Authorization": f"Bearer {KEY}", "Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=240) as r:
        body = json.load(r)
    url = body["data"][0]["url"]
    cost = body.get("cost")
    with urllib.request.urlopen(url, timeout=120) as r:
        png = r.read()
    path = os.path.join(OUT, f"{model}.png")
    with open(path, "wb") as f:
        f.write(png)
    return path, len(png), cost

results, failures = {}, {}
for m in MODELS:
    try:
        path, nbytes, cost = gen(m)
        results[m] = {"path": path, "bytes": nbytes, "cost": cost}
        print(f"OK   {m:<22} {nbytes:>8} B  cost={cost}", flush=True)
    except urllib.error.HTTPError as e:
        detail = e.read()[:200]
        failures[m] = f"HTTP {e.code}: {detail}"
        print(f"FAIL {m:<22} HTTP {e.code} {detail[:80]}", flush=True)
    except Exception as e:
        failures[m] = f"{type(e).__name__}: {e}"
        print(f"FAIL {m:<22} {type(e).__name__} {str(e)[:80]}", flush=True)

with open(os.path.join(OUT, "costs.json"), "w") as f:
    json.dump({"results": results, "failures": failures}, f, indent=2)
total = sum((v.get("cost") or 0) for v in results.values())
print(f"DONE ok={len(results)} fail={len(failures)} total_cost=${total:.4f}")

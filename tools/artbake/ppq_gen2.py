#!/usr/bin/env python3
"""Round 2 of ppq bake-off: substitutes + schema/timeout retries.
Key via PPQ_API_KEY env only. Appends into ppq/ + costs2.json."""
import json, os, urllib.request, urllib.error

API = "https://api.ppq.ai/v1/images/generations"
KEY = os.environ.get("PPQ_API_KEY", "")
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "ppq")
os.makedirs(OUT, exist_ok=True)

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
    # budget baseline substitute for disabled flux-1-schnell
    ("fast-sdxl", {"model": "fast-sdxl", "prompt": PROMPT, "n": 1,
                   "size": "1:1", "output_format": "png",
                   "client_request_id": "kirk-bakeoff-r2-fast-sdxl-v1"}),
    # docs' own example model replaces disabled nano-banana-2-lite
    ("nano-banana-2-1", {"model": "nano-banana-2-1", "prompt": PROMPT, "n": 1,
                         "size": "1:1", "output_format": "png",
                         "client_request_id": "kirk-bakeoff-r2-nano-banana-2-1-v1"}),
    # seedream rejected the size field -> try aspect_ratio spelling
    ("seedream-4.5", {"model": "seedream-4.5", "prompt": PROMPT, "n": 1,
                      "aspect_ratio": "1:1", "output_format": "png",
                      "client_request_id": "kirk-bakeoff-r2-seedream-4.5-v2"}),
    # gpt-image-1 upstream 502 -> idempotent retry, same request id
    ("gpt-image-1", {"model": "gpt-image-1", "prompt": PROMPT, "n": 1,
                     "size": "1:1", "output_format": "png", "quality": "low",
                     "client_request_id": "kirk-bakeoff-r2-gpt-image-1-v1"}),
]

results, failures = {}, {}
for name, payload in ATTEMPTS:
    try:
        req = urllib.request.Request(
            API, data=json.dumps(payload).encode(),
            headers={"Authorization": f"Bearer {KEY}", "Content-Type": "application/json"},
        )
        with urllib.request.urlopen(req, timeout=280) as r:
            body = json.load(r)
        url = body["data"][0]["url"]
        cost = body.get("cost")
        with urllib.request.urlopen(url, timeout=120) as r:
            png = r.read()
        path = os.path.join(OUT, f"{name}.png")
        with open(path, "wb") as f:
            f.write(png)
        results[name] = {"path": path, "bytes": len(png), "cost": cost}
        print(f"OK   {name:<20} {len(png):>8} B  cost={cost}", flush=True)
    except urllib.error.HTTPError as e:
        detail = e.read()[:200]
        failures[name] = f"HTTP {e.code}: {detail}"
        print(f"FAIL {name:<20} HTTP {e.code} {detail[:100]}", flush=True)
    except Exception as e:
        failures[name] = f"{type(e).__name__}: {e}"
        print(f"FAIL {name:<20} {type(e).__name__} {str(e)[:100]}", flush=True)

with open(os.path.join(OUT, "costs2.json"), "w") as f:
    json.dump({"results": results, "failures": failures}, f, indent=2)
total = sum((v.get("cost") or 0) for v in results.values())
print(f"DONE ok={len(results)} fail={len(failures)} round_cost=${total:.4f}")

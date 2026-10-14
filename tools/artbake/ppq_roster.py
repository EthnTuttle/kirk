#!/usr/bin/env python3
"""Full-roster portfolio expansion: nano-banana-2-1 + gpt-image-1.5 x 16 more
canon subjects (champions + sin-spawn). Prompts derived ONLY from crusade-core
species.rs lore lines. Skips subjects that already have raw.png (idempotent).
Per image: raw.png + q64.png + q128.png under portfolio/<model>/<subject>/.
Key via PPQ_API_KEY env only. -> portfolio/costs_full.json"""
import json, os, subprocess, urllib.request, urllib.error

API = "https://api.ppq.ai/v1/images/generations"
KEY = os.environ.get("PPQ_API_KEY", "")
ROOT = os.path.dirname(os.path.abspath(__file__))
PORT = os.path.join(ROOT, "portfolio")

MODELS = [("nano-banana-2-1", {}), ("gpt-image-1.5", {"quality": "low"})]

STYLE = (
    "Chunky 16-bit JRPG pixel-art style game sprite, limited retro color palette "
    "(ink navy, midnight blue, deep gold, pale gold, bone white, warm leather brown, "
    "dark wood, steel grey, deep red), crisp pixel clusters, full-body readable "
    "silhouette, centered, plain solid dark navy background (#0B0E1B), absolutely "
    "no text, no letters, no watermark, no border, no card frame. Subject: "
)

CHAMP_TAIL = " Humble, dignified medieval Christian-crusader allegory character."
BOSS_TAIL = (" Grotesque allegorical sin-spawn boss monster, menacing, "
             "stylized, not gory, no real people, no occult symbols.")

SUBJECTS = {
    # ---- champions (10 remaining) ----
    "serjeant-bastien": ("Serjeant Bastien, a stoic veteran spearman standing firm "
        "at ease, long spear planted butt-down beside him, plain steel kettle helm, "
        "leather jerkin over bone gambeson, calm watchful eyes." + CHAMP_TAIL),
    "hospitaller-ysolt": ("Hospitaller Ysolt, a battle-nun sister in a bone habit "
        "with white wimple and a gold-lined healer's satchel at her hip, calmly "
        "rolling a bandage, kind resolute face." + CHAMP_TAIL),
    "wayfarer-aelis": ("Wayfarer Aelis, a nimble pilgrim scout in a dusty brown "
        "travel cloak over bone tunic, wide pilgrim hat hanging on her back, "
        "walking staff in hand, keen road-wise eyes." + CHAMP_TAIL),
    "canoness-hrothwyn": ("Canoness Hrothwyn, an elder choir-nun in midnight-blue "
        "and white habit with gold trim, holding a raised open psalter mid-song, "
        "serene commanding presence." + CHAMP_TAIL),
    "vexillor-anchetil": ("Vexillor Anchetil, a steadfast standard-bearer gripping "
        "a tall banner pole, its banner showing a simple gold eight-pointed pilgrim "
        "star on midnight blue, chain coif, bone surcoat." + CHAMP_TAIL),
    "brother-aldwyn": ("Brother Aldwyn, a burly gentle friar in plain brown and "
        "bone robes with a coarse rope belt, an enormous pack on his back, humble "
        "downward gaze, slight weary smile." + CHAMP_TAIL),
    "cantor-sabina": ("Cantor Sabina, a young cantor nun in white habit with a "
        "pale-gold stole, ringing a small brass handbell at measured pace, composed "
        "rhythmic poise." + CHAMP_TAIL),
    "chevalier-romilda": ("Chevalier Romilda, a serene woman knight in battered "
        "polished steel plate with a plain bone tabard bearing a small gold cross, "
        "shield lowered at her side, merciful steady gaze." + CHAMP_TAIL),
    "vigil-knight-wymar": ("Vigil-Knight Wymar, a gaunt watchful knight kneeling "
        "upright in dark steel armor, both hands resting on the pommel of a "
        "longsword planted point-down before him, dim lantern glow at his feet." + CHAMP_TAIL),
    "friar-osbert": ("Friar Osbert, a wiry fiery-eyed friar in patched bone robes "
        "preaching with one arm raised, small wooden cross in the other hand, "
        "standing atop an upturned market crate." + CHAMP_TAIL),
    # ---- sin-spawn bosses (6 remaining) ----
    "wretch-of-envy": ("the Wretch of Envy, a gaunt hollow-eyed ghoul of ink and "
        "bruise-purple, elongated grasping fingers, a sickly green covetous glow "
        "in its eyes and chest cavity, crouched, glaring sideways at something unseen." + BOSS_TAIL),
    "fiend-of-lust": ("the Fiend of Lust, a veiled faceless apparition wreathed in "
        "curling smoldering ember-haze of deep red and dim gold, cowl of ash-grey "
        "smoke, no skin, no face, sweet-voiced herald of broken vows." + BOSS_TAIL),
    "sleeper-of-sloth": ("the Sleeper of Sloth, a colossal drowsy giant slumped "
        "in sleep, swaddled in heavy grey and midnight monk-like robes, eyes "
        "sealed shut, a spreading pool of dull grey mire around his base, a thin "
        "circlet of dust hovering above his head." + BOSS_TAIL),
    "brute-of-wrath": ("the Brute of Wrath, a hulking ogre of cracked crimson "
        "flesh veined with ink-black rage, frothing mouth mid-roar, crude iron "
        "shards riveted into its shoulders as armor, fists raised." + BOSS_TAIL),
    "glutton-maw": ("the Glutton Maw, a walking cavernous mouth on two stubby "
        "legs, its body mostly a gaping toothless black maw drooling gold coins "
        "and half-eaten sheaves of wheat, two sickly dim-gold eyes on its crown." + BOSS_TAIL),
    "grasper-of-greed": ("the Grasper of Greed, a gaunt miser-fiend of tarnished "
        "gold and ink, stooped under heavy chains hung with hoarded coins, both "
        "clawed hands clutching a fistful of glowing golden light, narrow "
        "avaricious eyes." + BOSS_TAIL),
}

def gen(model, extra, subject):
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
        d = os.path.join(PORT, model, subject)
        rawp = os.path.join(d, "raw.png")
        if os.path.isfile(rawp):
            print(f"SKIP {model}/{subject} (exists)", flush=True)
            continue
        os.makedirs(d, exist_ok=True)
        try:
            png, cost = gen(model, extra, subject)
            tmp = os.path.join(d, "raw.bin")
            open(tmp, "wb").write(png)
            from PIL import Image
            im = Image.open(tmp)
            w, h = im.size
            if w != h:
                s = min(w, h); l, t = (w-s)//2, (h-s)//2
                im = im.crop((l, t, l+s, t+s))
            im.convert("RGBA").save(rawp, "PNG")
            os.remove(tmp)
            for q in (64, 128):
                subprocess.run(["python3", os.path.join(ROOT, "quant.py"), rawp,
                                os.path.join(d, f"q{q}.png"), "--size", str(q)],
                               check=True, capture_output=True)
            results[f"{model}/{subject}"] = {"bytes": len(png), "cost": cost, "src_dims": f"{w}x{h}"}
            print(f"OK   {model}/{subject:<28} {w}x{h} cost={cost}", flush=True)
        except urllib.error.HTTPError as e:
            failures[f"{model}/{subject}"] = f"HTTP {e.code}: {e.read()[:100]}"
            print(f"FAIL {model}/{subject:<28} HTTP {e.code}", flush=True)
        except Exception as e:
            failures[f"{model}/{subject}"] = f"{type(e).__name__}: {str(e)[:100]}"
            print(f"FAIL {model}/{subject:<28} {type(e).__name__}: {str(e)[:60]}", flush=True)

json.dump({"results": results, "failures": failures},
          open(os.path.join(PORT, "costs_full.json"), "w"), indent=2)
total = sum((v.get("cost") or 0) for v in results.values())
print(f"DONE ok={len(results)} fail={len(failures)} expansion_cost=${total:.4f}")

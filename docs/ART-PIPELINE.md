# Art Pipeline — Pixel Art for Deus Vult

Status: researched plan, nothing built. Tone/IP constraints from `PLAN.md`
§1–§2 bind every sprite: reverent, no mockery, sin-bosses as Spenser/Dante
grotesque allegories, zero Nintendo-derivative designs, original names.

## 0. Verdict

No sprite file exists anywhere in the repo today. The whole game renders as
Bevy UI text nodes; placeholders already ship. Art is therefore **parallel
work that can never block code**: produce behind placeholders, swap in later.

**Primary path: hosted Retro Diffusion API (Replicate) → 64×64 indexed PNGs,
one master palette, per-species key `champions/{id:03}-{slug}.png`, human
cleanup pass.** Fallback ladder: (2) commission a pixel artist; (3) local
SDXL + pixel-art LoRA on a GPU box this dev must supply — **this machine
cannot run generation** (§4).

## 1. What the game actually needs (census from `crusade-core/src/data/`)

| Asset | Count | Visual brief source | Required for v1? |
|---|---|---|---|
| Champion battle sprites | 12 (ids 1–12) | role in name (squire, cleric, hospitaller, canoness, vexillor, cantor, chevalier, vigil-knight, friar, wayfarer, brother, serjeant) + `lore` strings ("plants the pilgrim star's banner", "kneels before every dawn") | **yes** |
| Sin-boss battle sprites | 7 (ids 101–107) | `lore` strings are ready-made design briefs ("A crowned shadow", "A walking maw", "grey mire of idleness", "golden tide") — grotesque allegory, never a real person/religion | **yes** |
| Roster icons | 19 | head-crop/downscale of the battle sprite | yes (auto-derivable) |
| Rite icons | 42 | virtue/sin glyph per rite | no — v1.2 nice-to-have |
| Virtue/sin type badges | 15 (7+7+typeless) | small glyphs, palette-tinted; already text-typed in UI | no — nice-to-have |
| App icon | 1 (+2 PNG) | existing `web/icon.svg` | **done — see §2** |
| Menu/battle backgrounds | — | flat `style.rs` colours read fine | no |

Total v1-must-exist: **19 battle sprites** (+19 derived icons = 38 files).

## 2. Render state today & integration seams

- `bevy_shell/screens.rs` + `battle.rs`: 100% `Text`/`Button`/`Node` UI.
  `grep -n "AssetServer|asset" bevy_shell/*.rs` → **zero hits**. No
  `crusade-app/assets/` dir exists.
- Exact seams art feeds:
  1. `battle.rs::champion_panel` (line ~58) — the left/right me/foe cards;
     an `ImageNode` when a sprite exists, text fallback when handle 404s.
  2. `screens.rs::champion_card` (line ~237) — roster grid, 32×32 icon slot.
  3. `web/index.html` — one `<link data-trunk rel="copy-dir" href="../assets" />`
     line so trunk ships sprites into `dist/assets/` under the `/kirk/`
     public-url (same pattern as the existing `copy-file` side files; trunk
     ≥0.21 per `web/Trunk.toml`).
  4. `bevy_shell/main.rs` — `ImagePlugin::default_nearest()` (crisp pixels).
  5. **Lookup key: `species_id` (u16)** — the only stable, layer-visible
     identity the data tables and `ViewSnapshot`/`RosterCard` share.
     Convention: `assets/sprites/champions/001-squire-audric.png`,
     `assets/sprites/icons/001.png`; boss ids `101-prince-of-pride.png` etc.
- Wasm mechanics: bevy's wasm `asset_server` is plain HTTP fetch of
  `assets/…` relative to the page; 19×64×64 RGBA8 decodes to ~300 KB VRAM
  loose — **no texture atlas needed at this count**; loose files + per-file
  fetch is correct and simplest. Don't prematurally atlas.
- App icon (`web/icon.svg` → 192/512 PNGs): flat navy `#0c1230` shield,
  gold `#c9a227` cross, full-bleed maskable-safe. **Honest verdict: shippable
  for v1.0.** Crisp, on-palette with the shell, theme-accurate, zero IP
  exposure. Weakness: generic vector, no linkage to pixel-art identity.
  Keep it; optional pixel-art re-render in card D4.

## 3. Environment check (verbatim, run on this box)

```
$ nvidia-smi 2>&1 | head -3
/bin/bash: line 1: nvidia-smi: command not found
$ python3 -c "import torch; print(torch.__version__, torch.cuda.is_available())"
2.11.0+cu130 False
$ python3 -c "import torch, diffusers" 2>&1
ModuleNotFoundError: No module named 'diffusers'
$ which comfy aseprite magick convert 2>&1
(no output — none installed)
$ df -h / | tail -1
/dev/mapper/vgmint-root  912G  848G   18G  98% /
```

**Conclusion: local generation on this box is not viable.** No GPU driver,
CUDA reports unavailable, no diffusers/PIL/comfy/aseprite/ImageMagick, and
only 18 GB free disk (98% full) — an SDXL stack + one checkpoint (~7–15 GB)
would fit on paper but runs CPU-only at minutes-per-image with the dev's
own QA time dominating. Generation must happen on: hosted API (chosen), the
dev's other GPU machine, or a rented GPU (~$0.5–1/hr) if fallback 3 triggers.
Nothing was installed to reach this verdict.

## 4. Tooling comparison (research, late 2026)

| Path | Quality for 19-sprite roster | Consistency lever | Cost (19 sprites) | Verdict |
|---|---|---|---|---|
| **Retro Diffusion (hosted, via Replicate/Scenario)** | Purpose-built pixel models; enforces true pixel grid + limited palettes natively | same model + palette + seed family; img2img variations | ~$0.05–0.20/gen → **$5–30** + human cleanup | **Primary** |
| Commissioned pixel artist | Best; human art direction | Trivially coherent — one hand | ~$30–80/sprite (2026 market) → **$600–1500** | Fallback 2; also the ip-cleanest path, keep as v1.1 polish upgrade |
| Local SDXL + pixel LoRAs (nerijs/pixel-art-xl, Pixel Art Diffusion XL) | Good concepts, "fake pixel" anti-aliasing needs downscale cleanup | train a 3-image style LoRA (~30 min on 24 GB); fixed seed | GPU box required — **not this machine** | Fallback 3 |
| Flux variants | Strong at 256–512 px illustration, weak at native low-res grids | same as SDXL | same | Not recommended over SDXL for sprites |
| PixelLab / Scenario hosted | Good (layers, animation tooling) | style refs | credit-based ~$20–50 | Viable alternative primary; no lock-in reason to prefer |

Honest calibrations from field reports: pure generative pixel art still
produces grid violations and anatomy drift at character scale; every path
through AI needs **a human cleanup pass per sprite** (pixel-peep, palette
snap, silhouette fix) — realistically 15–60 min each once the palette and
spec are fixed. The famous failure mode is 19 cousins instead of 19 siblings;
the only cure is one anchored style artifact (§5) plus ruthless rejection of
off-style generations.

Sources: [HF nerijs/pixel-art-xl](https://huggingface.co/nerijs/pixel-art-xl),
[Retro Diffusion](https://retrodiffusion.ai/) +
[on Replicate](https://replicate.com/blog/retro-diffusions-pixel-art-models-are-now-on-replicate),
[runware case study](https://runware.ai/blog/retro-diffusion-creating-authentic-pixel-art-with-ai-at-scale),
[mage.space 2026 roundup](https://blog.mage.space/article/best-ai-pixel-art-generators-2026/83330b2b-607d-4ef3-bca0-19e8ef307e2e),
[reddit r/aigamedev field report](https://www.reddit.com/r/aigamedev/comments/1jppezz/),
[sprite-ai.art commission pricing](https://www.sprite-ai.art/blog/ai-sprite-generator-vs-artist).

## 5. Consistency techniques (what actually works)

1. **One master palette, enforced at ingest.** Define ~24 colours
   (extend `style.rs` shell palette: navy/bone/gold family + one hue family
   per virtue). Every sprite is quantized to this palette in a CI checkable
   step (`scripts/check_palette.py`-flavoured; Pillow on a dev machine).
   A shared palette forgives 80% of inter-sprite style drift.
2. **Anchor characters first.** Nail 2 references by any means —
   Squire Audric (most generic knight) and Prince of Pride (boss showcase) —
   iterate until truly shippable, then use them as img2img/style refs for
   the other 17. This is cheaper and more reliable than LoRA training for
   a 19-unit roster; LoRA training (fallback 3's lever) needs a GPU anyway.
3. **Generate large → downscale → posterize** for any generic-model output:
   native 256–512 px render, k-centroid/region downscale to 64×64 (Retro
   Diffusion's own downscaler does this; Pillow `Image.resize(BOX)` +
   `.quantize()` reproduces it), then snap to master palette. Retro
   Diffusion generates at grid natively and skips half this.
4. **No sprite-sheet extraction** — we need 19 singles, sheets add seams
   for zero benefit.
5. **Human cleanup budget**: 30 min/sprite average × 19 ≈ **10 hrs** one
   person's focused weekend per full roster pass.

## 6. Specs (proposal)

| Spec | Value | Justification |
|---|---|---|
| Battle sprite | 64×64 px, 1 frame, front-facing | Battle layout is two symmetric cards (me/foe both face the player); no back-sprites needed — GB back-sprites served a 3/4 camera we don't have |
| Roster icon | 32×32 px | 2:1 downscale of battle sprite (auto) + optional head-crop touch |
| Palette | 24-colour shared master, ≤16 unique/sprite | 8-bit scale, cheap CI conformance check, matches the shell's existing flat navy/gold/bone art direction; GB-4 is too austere for 19 distinct units, SNES-16/unit keeps QA tractable |
| Animation (v1) | **none** | UI tween (translate 2-px bob, HP-hit flash) is a code feature, not an art asset; true sprite animation (idle/attack) is v1.3+ — pays 4× sprite cost now for thumbless return |
| Format | PNG (RGBA8 or indexed), nearest-neighbour sampling | bevy decodes both; indexed keeps repo size ~2–4 KB/sprite |

## 7. Legal / ethics (short, honest)

- Copyright: purely AI-generated art is broadly held non-copyrightable
  (USCO position stands through 2026); we can *use* such sprites but cannot
  exclusively own them. Human cleanup authorship strengthens the claim on
  the composite. Our actual IP defensibility lives where PLAN §2 puts it:
  original Latin-flavoured names, original lore, one coherent original
  style — none of which AI changes either way.
- Store disclosure: as of 2026, **Steam requires AI-content disclosure**
  (rules reworked mid-2026 to distinguish AI-generated in-game content from
  AI-assisted tooling); **itch.io requires a generative-AI disclosure tag**,
  possibly filterable by players. GitHub Pages self-host has no gate. If
  Retro Diffusion output ships, disclose; honest disclosure costs nothing
  here and matches the project's candour ethic.
  Sources: [Steam rework thread 2026](https://steamcommunity.com/discussions/forum/10/569290345532291867/),
  [agentbrush Steam/itch guide](https://agentbrush.dev/blog/ai-game-assets-steam-itch),
  [gamespot itch requirement](https://www.gamespot.com/articles/1100-6527920/).
- Provenance: Retro Diffusion advertises self-built models trained for
  pixel art on curated/licensed data — safer provenance than generic civitai
  merges, but "trained on" claims are marketing until audited. The
  commissioned-artist fallback eliminates the question entirely; that is
  part of what its price buys.

## 8. Roadmap slot — WS-D cards (same style as V1-SHIP-PLAN)

Art is **not release-gating**: placeholders ship in v1.0, sprites swap in
whenever they land. D1/D2 are pure content work (can start today, touching
no code files). **D3 touches `screens.rs`+`battle.rs` — the same files as
WS-B2; it must serialize after B2 merges** (card-disjointness rule).

### D1 — Style bible + palette + 2 anchor sprites [1 session + human art time]
- **Files:** `docs/ART-PIPELINE.md` (append style bible §), `crusade-app/assets/palette.aseprite.json` (or palette png), `assets/sprites/champions/001-…`, `101-…`.
- **Goal:** the 24-colour master palette + written silhouette/shading rules
  (light top-left, 2-px outline, bone highlights on mail) + Audric and
  Prince of Pride *approved* as anchors.
- **Acceptance:** both anchors pass palette CI check, render at 64×64 with
  ≤16 colours, and survive 3 em-dash-width squint test of style agreement.

### D2 — Remaining 17 species sprites [content, parallel with WS-B]
- **Files:** `crusade-app/assets/sprites/champions/{003..012,102..107}-*.png`,
  derived `assets/sprites/icons/*.png`.
- **Goal:** full 19 + 19 icon set against the D1 anchors/palette.
- **Acceptance:** palette CI green; contact sheet reviewed as one set
  (siblings, not cousins); every `species_id` 1–12, 101–107 resolves to a file.

### D3 — Sprite ingestion in the shell [worker; AFTER WS-B2 merges]
- **Files:** `bevy_shell/{screens.rs,battle.rs,main.rs,state.rs}`,
  `web/index.html` (+1 copy-dir line), `crusade-app/Cargo.toml` nothing new.
- **Goal:** `species_sprite(species_id) -> Option<Handle<Image>>` resolver;
  `ImageNode` in champion_panel/champion_card with the existing text as
  graceful fallback (an un-drawn species must never crash or blank);
  `ImagePlugin::default_nearest()`.
- **Acceptance:** `cargo test --workspace` green; headless smoke still
  exits 0; wasm check passes on CI; sprites visually present in practice duel.
- **Cost note:** ~120 lines of shell code; art is already ship-independent.

### D4 — Optional: rite/type icons + pixel app icon [v1.2+, defer]
- Rite icons keyed by virtue (15 glyph designs, palette-tinted, 42 instances
  are recombinations) + pixel re-render of the shield icon at 192/512.
  Only after the roster proves the style.

Sequence: **D1 → D2 ∥ (WS-B1/B3a/B2) → D3 → D4.** Art milestones never
appear in a code card's critical path.

## 9. The single cheapest first proof

One Replicate API call against the Retro Diffusion endpoint:

```
prompt: "squire kneeling at dawn, plain mail, lowered eyes, pilgrim star
pendant, game boy advance era jrpg monster sprite, front facing,
solid dark navy background"  →  native pixel output at target size
```

→ downscale/quantize to the 24-colour palette → save as
`crusade-app/assets/sprites/champions/001-squire-audric.png` (PNG, 64×64,
indexed). That file *is* bevy-consumable the moment it exists — D3's
`ImageNode` needs no other pipeline property. Total cost: one API call
(~$0.1) + ~30 minutes. If Audric can't be made shippable in three
regeneration rounds, that's the signal to climb the fallback ladder to a
commissioned artist before a single dollar more is spent on generation.

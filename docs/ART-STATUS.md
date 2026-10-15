# ART STATUS & HANDOFF — Deus Vult production art (2026-10-14)

Read this first on a new machine. Companion to `docs/ART-PIPELINE.md` (research +
WS-D cards); this file is the live state. Everything art-bake-off now lives in
`tools/artbake/` (scripts) and `art/roster-staging/` (quantized candidates).

## 1. Decisions (locked by user 2026-10-14)

- **Winners of Round 2:** `nano-banana-2-1` (Google, **$0.00/img** free tier) and
  `gpt-image-1.5` (OpenAI, **$0.023/img** at quality=low) — both via PayPerQ
  (`https://api.ppq.ai/v1/images/generations`).
- **Production canvas: 128×128** (nano-banana quantized to 128; the 64×64 vs
  128×128 question resolved in favor of 128). `q128.png` is the production
  candidate; `q64.png` kept for downscale checks; raw = concept art only.
- **Canon gender rule (APPLIED 2026-10-14):** combat characters are men;
  support/other characters may be women. Pass done: species 10 renamed
  `Chevalier Romilda` → `Chevalier Raimbaud` (id/stats/kit/lore untouched,
  lore was already gender-neutral); all other champions conform by role; slug
  + prompt in `ppq_roster.py` updated with id bumped to `-v2`; old staged
  banana cell retired (was free) pending regeneration in 4.1.
- Hand-painted lane finalists for reference: deepseek-v4.1-flash-uncensored-fp8,
  kimi-k3, glm-5.3 (portfolios in `/tmp/opencode/sprite-bakeoff/r2/` on the
  origin box, not in-repo).

## 2. Current artifact state

23 of 40 planned subject×model cells are complete (raw + q64 + q128 each);
was 24 before the §4.2 rename retired the banana romilda cell. Staged
quantized at `art/roster-staging/<model>/<subject>/{q64,q128}.png`
(79 KiB total). `tools/artbake/MANIFEST.json` = per-cell ledger: bytes,
client_request_id (idempotency), cost, dims, status.

- nano-banana-2-1: **18/20** done ($0 total). Missing: `sleeper-of-sloth`
  (upstream moderation 400 on v1 prompt — reworded already in
  `tools/artbake/ppq_roster.py`; rerun is free) and `chevalier-raimbaud`
  (romilda cell retired in the 4.2 rename; regenerates free at `-v2` id).
- gpt-image-1.5: **5/20** done ($0.115). Missing 15, incl. the renamed
  `chevalier-raimbaud` (job was paused with `pkill`; zero half-written files).
- Spend to date: bake-off $0.204 + portfolio $0.115 ≈ **$0.32** (all PayPerQ).

### Resume procedure (any machine)

```bash
export PPQ_API_KEY=sk-…(from user's password store — NEVER commit it)
mkdir -p /tmp/opencode/sprite-bakeoff && cp tools/artbake/* /tmp/opencode/sprite-bakeoff/
python3 ppq_roster.py          # skips cells whose raw.png exists; ~$0.35 remaining
python3 gen_site.py            # rebuilds /tmp/opencode/sprite-bakeoff/index.html
python3 -m http.server 8901 --directory /tmp/opencode/sprite-bakeoff &   # gallery
```

Scripts depend only on python3 stdlib + Pillow. Raw 1024² rasters are NOT in
git (regenerable via same `client_request_id` keys — retries attach to finished
generations and never double-bill).

## 3. Consumed context — pipelines that exist

| Tool | Purpose |
|---|---|
| `render.py` | palette-JSON grid → PNG (hand lane) |
| `quant.py` | any PNG → 16-color palette canvas at `--size` N (block-average + nearest-RGB) |
| `ppq_gen*.py`, `ppq_portfolio.py`, `ppq_roster.py` | PayPerQ generations (bake-off, 4-subject batch, full-roster batch) |
| `compose.py`, `compose2.py` | contact sheets (stdlib, PIL) |
| `gen_site.py` | gallery generator over `portfolio/<model>/<subject>/` |

Palette (16c, index order): transparent, ink `#0B0E1B`, midnight `#232946`,
gold `#C9A227`, palegold `#F4D35E`, bone `#D8D3C0`, dimbone `#8A8577`,
red `#A62639`, brightred `#D1495B`, leather `#8B5E34`, wood `#5D4020`,
steel `#9AA0A8`, steeldk `#5C6470`, white `#F5F1E6`, green `#4A7C59`,
bruise `#503A6B`.

## 4. Next steps (task cards)

### 4.1 Fill the roster (mechanical, ~30 min)
Resume §2 procedure until 40/40 complete: banana sleeper-of-sloth (v2 prompt,
free) + gpt 15 subjects (~$0.35). Regen site, eyeball fouls (text/border/
subject drift), re-prompt individual cells same as before (bump `-vN` id).

### 4.2 Canon gender rule pass (DONE 2026-10-14)
Rule: **combat characters are men; support/other characters may be women.**
- Audit `crusade-core/src/data/species.rs` + `callings.rs`/`rites.rs`:
  classify each of 12 champions combat vs support/other by rites/stats
  (damage-dealing = combat).
- Known violation: **Chevalier Romilda** (female-named melee tank; lore
  "bears every blow in patience and answers cruelty only with mercy") → rename
  male (suggest "Chevalier Raimbaud" or similar), keep stat/kit/virtue ids
  untouched, adjust lore wording.
- Adjudicate per ruling: Vexillor Anchetil (male already), Wayfarer Aelis
  (scout — non-combat by lore, may stay female), Hospitaller Ysolt (healer —
  support, stays), Canoness Hrothwyn / Cantor Sabina / Cleric Mabel / Brother
  Aldwyn all conform by role.
- Keep `roster_covers_every_virtue_twice_and_has_two_duals` and all data tests
  green; species ids stable; then update `tools/artbake/ppq_roster.py` prompt
  wording to match any rename, and bump that cell's client_request_id.
- Worker: tff/kimi-k3; reviewer: tff/kimi-k3#high.
- OUTCOME (2026-10-14): renamed to **Chevalier Raimbaud** (id 10; stats, kit,
  typing, lore all untouched — lore was already gender-neutral). Only
  combat-role violation found; all other champions conform. 53 core + 40 app
  tests green; prompt/slug/staging/ledger updated; **review: SHIP** (three
  doc nits fixed same-day). Raimbaud cell regenerates in 4.1 at `-v2` id.

### 4.3 Holistic animation generation (exploratory card)
Single-call sprite **sheets**, not per-frame images: prompt the winner model
for an N-cell turnaround/animation sheet on a strict uniform grid (e.g.
3×2 cells of 128px: idle / step / strike / hit / defend / pray), slice by grid
with a `slice.py`, quantize each cell `--size 128`, assemble a bevy texture
atlas. Rationale: models hold style internally consistent within one image far
better than across separate calls; grid slicing turns $0.02 into 6 frames.
Acceptance: for ONE subject (audric-battle), a 6-frame sheet where silhouette
registration survives slicing; q128 cells read in `cargo run -p crusade-app`
battle screen or a bevy ui sandbox; honest writeup of consistency limits
(diffusion sheet registration is the known weakness — pilot before scaling).

### 4.4 Integration (after 4.1+4.2 land where relevant)
Wire winning q128s into `crusade-app` per `docs/ART-PIPELINE.md` WS-D cards
(D1 hero/battle sprites, D2 boss art, D3 menu/campaign polish — D3 serialized
after any in-flight shell work touching `bevy_shell/`).

## 5. Non-art host state at pause time

- v1.0 PWA live at https://ethntuttle.github.io/kirk/ ; deploys serialize
  oldest-first on pushes touching `crusade-*/**`.
- Merged today: B1 lobby `02b8303` → wasm-panic fix round → B2 `401b11a`
  (Tournament of Virtue, review SHIP) → B4 Elo display `4b86dd4` →
  art handoff `41fad0f` → **B3b attestation + alms CTA `67e35b8`
  (tff/kimi-k3#high review: SHIP).** The paused B3b session completed after
  the first push and was sealed per this section's plan (a). **v1.1 surface
  is feature-complete; working tree clean.** B3b review minors on record:
  native desktop alms arms are compile-gated via the wasm check only;
  `nostr_duel` relay test is pre-existing flaky; lobby commands drop
  post-settle by design.
- The 9 `kirk::cashu` test panics remain pre-existing/out-of-scope (v1.2 mint).
- Known cosmetic debt: `screens.rs` menu subtitle tofu glyphs (embedded font
  lacks emoji); RECV_TIMEOUT liveness flake; mutex `.expect()` hardening —
  all listed in `docs/SHIPPING.md` limitations or §7bis notes.

## 6. Secrets & shelved threads

- PayPerQ key: user's personal store only; scripts read `PPQ_API_KEY` env.
  Nothing with the key may be committed (grep before pushes).
- Bedrock image lane: shelved (no AWS creds on box); quant.py pipeline is
  provider-agnostic if creds ever arrive.
- The serving gallery (box-local): `python3 -m http.server 8901 --directory
  /tmp/opencode/sprite-bakeoff` may still be running on the origin box;
  kill `pkill -f "http.server 8901"`.

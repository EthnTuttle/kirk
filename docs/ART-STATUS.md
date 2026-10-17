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

**40 of 40** planned subject×model cells are complete (raw + q64 + q128
each; filled 2026-10-14 per card 4.1). Staged quantized at
`art/roster-staging/<model>/<subject>/{q64,q128}.png` (148 KiB, 80 files).
`tools/artbake/MANIFEST.json` = per-cell ledger: bytes,
client_request_id (idempotency), cost, dims, status.

- nano-banana-2-1: **20/20** done ($0 total). 4.1 added `sleeper-of-sloth`
  (v2-worded prompt) + `chevalier-raimbaud` (at `-v2` id), both clean.
- gpt-image-1.5: **20/20** done ($0.46 = 20 × $0.023 low-quality). 4.1 added
  the 15 missing cells incl. `chevalier-raimbaud` (at `-v2` id).
- Spend to date: bake-off $0.204 + portfolio $0.115 + roster expansion
  **$0.345** = **$0.664** (all PayPerQ).
- 4.1 run notes: no fouls, no re-prompts — all 17 new cells passed eyeball
  (no text/border, on-prompt, raimbaud reads male in both models per the 4.2
  canon rule). One transient upstream 400 on gpt `brother-aldwyn` (spurious
  size-validation while 15 identical requests passed) cleared on plain retry.
  Scratch `/tmp` had been wiped since the pause: the 23 prior cells' raw.png
  were re-attached idempotently via their original client_request_ids (dims
  match the ledger exactly; staged q64/q128 copied back verbatim).

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

### 4.3 Holistic animation generation (PILOT DONE 2026-10-14, $0.00)
Audric 6-frame sheet (3×2, one banana call): **registration survived
slicing** — standing spread ~5px at q128 (~4% canvas, ≈ mild hand-authored
bob), 12–13 colors/cell all on-palette, identity consistent across poses.
Fouls measured: model drew caption labels (slice.py strips them
deterministically by background-gap detection; +`--labels`, add "no
captions" wording next time) and idle faced left vs the rest right.
**Recommendation: hybrid** — sheets for idle/pose families (one free call =
6 same-costume frames), per-frame generation for the combat arc. Slicer +
registration analyzer: `tools/artbake/slice.py` (reports bbox/centroid/IoU
per pair). Pilot artifacts: `/tmp/opencode/sprite-bakeoff/anim-audric/`.

### 4.4 Integration (after 4.1+4.2 land where relevant)
Wire winning q128s into `crusade-app` per `docs/ART-PIPELINE.md` WS-D cards
(D1 hero/battle sprites, D2 boss art, D3 menu/campaign polish — D3 serialized
after any in-flight shell work touching `bevy_shell/`).
- OUTCOME (2026-10-14, cards D1 battle sprites + D2 roster icons): the locked
  **nano-banana-2-1** lane staged into `crusade-app/assets/sprites/` —
  `champions/NNN-slug.png` (128², 19 files) + derived `icons/NNN.png` (32²,
  nearest downscale) — by `tools/artbake/stage_sprites.py` (new single source
  of truth for the art→assets step; 19-row id↔slug↔cell table from
  `species.rs`; rerunnable/idempotent; default edge-connected ink keying —
  halo-free: 0 semi-transparent px in all 38 outputs; `--no-key-bg` keeps the
  opaque tile). `audric-battle` wired for id 1; `audric-prayer` NOT wired;
  gpt-image-1.5 lane untouched on disk as alternates. Shell: `ImageNode` in
  `battle.rs::champion_panel` (128² portrait, name→table reverse lookup;
  unknown names keep text-only) and `screens.rs::champion_card` (32² icon via
  `species_id`, which `ChampionCard` already exposed; `BattleChampionView`
  surface untouched); name/resolve/type text rows remain the fallback (headless
  smoke has no AssetServer → text; wasm 404 → blank over intact text);
  `ImagePlugin::default_nearest()` in `main.rs`; `copy-dir ../assets` in
  `web/index.html`. `cargo test -p crusade-app` 40/40 green (±bevy-ui, incl. 3
  new slug/path bin tests), ui-shots: battle.png shows keyed Audric vs Bastien
  crisp + correct, roster.png 6/6 icon slots populated, `--smoke` exit 0. No
  atlas, no view/crate-surface changes; wasm32 check skipped (target not
  installed here; CI covers).
- OUTCOME (2026-10-14, card D3 menu/campaign polish, nano-banana lane only):
  `audric-prayer` staged as the first scene slot,
  `assets/sprites/scenes/audric-prayer.png` (keyed transparent by the same
  edge-flood pass; stage_sprites.py gained a SCENES table + shared
  load/validate; rerun: 38 unchanged + 1 scene written, idempotent re-run
  writes 0; 0 semi-transparent px). id 1's battle cell stays `audric-battle`
  per the lock — the prayer tile is decoration only. **Menu hero SHIPPED**:
  128x128 kneeling-at-dawn side piece left of the DEUS VULT/subtitle lockup
  (not above-title: the column was ~645px of 720 already); AssetServer-gated
  like every seam, buttons untouched. Reads as a manuscript initial; corner
  dawn-bands verified on-palette at x4 magnify. **Next-boss face SHIPPED** on
  the rest screen: panel with the staged 128x128 boss sprite + "Node N/7 -
  {name} waits beyond the rest." Verified `CampaignView` carries only
  node/defeated/shards_earned (no boss-id field) and
  `client.rs::CAMPAIGN_NODE_SPECIES` is private, so the shell mirrors that
  immutable 7-id table in `screens.rs` under the same shell-side
  id-reconstruction discipline as `battle.rs::champion_sprite_path`, pinned
  by 3 new bin tests (7 distinct static sin species; node>=7 → None; paths
  == staged slugs). Cleaner future seam if the mirror ever annoys:
  `pub(crate)` the client's const or add `CampaignView.next_boss_species`.
  `cargo test -p crusade-app` green ±bevy-ui; check --features bevy-ui clean;
  `--smoke` exit 0. ui-shots: menu.png + campaign.png (Glutton-Maw at node
  2) verified; the other 6 screens diff at exactly 0 changed pixels vs the
  pre-D3 baseline harness run.

### 4.5 Animation sheets (DONE 2026-10-14)
Scaled the 4.3 pilot recipe across **all 12 champions**, nano-banana-2-1 only
($0.00/img). **Grid (11 new sheets):** 3x2 row-major `idle / step / pray /
idle2 / look / bow`, all facing right; identity descriptors reused verbatim
from `ppq_roster.py`/`ppq_portfolio.py` SUBJECTS (pose-locks neutralized to
props where needed); prompt carries the two pilot fixes ("absolutely no
captions, no labels, no words, no letters" + "every pose faces right").
**Audit-battle exception:** Squire Audric reuses the 4.3 pilot sheet
(idle/step/strike/hit/defend/pray — combat-family grid; documented in its
poses.txt), not regenerated. Labels stay scoring-compatible (idle, step,
pray, idle2, look, bow). Staged at `art/anim-staging/<subject>/` (6 q128
cells + one-line `poses.txt` each) — **not wired into the game yet**.
**Bosses skipped this pass:** mostly single dominant poses; combat arcs are
per-frame per the 4.3 hybrid decision.
Driver: `/tmp/opencode/sprite-bakeoff/anim_batch.py` (scratch, AST-reads the
roster prompts; idempotent per `client_request_id kirk-anim-sheet-<s>-vN`);
slicing/analysis by existing `tools/artbake/slice.py` (no changes needed) +
`quant.py --size 128`. Gates per sheet: centroid drift ≤~8px @q128 across
non-kneel pose pairs, 0 off-palette px in q128s, full eyeball of every q128
strip. **16 API calls total** (11 v1 + 5 v2 re-prompts), **0 API errors**,
spend **$0.00** (cumulative project spend unchanged at $0.664).

| subject | final sheet id | tries | drift excl. kneel @q128 (dcx/dcy max) | eyeball | verdict |
|---|---|---|---|---|---|
| audric-battle | kirk-anim-pilot-audric-battle-v1 | 0 (reuse) | 8.5/4.7 (4.3 pilot) | OK | OK (pilot; left-facing idle foul FIXED by exact pixel-mirror at D5 wiring, 2026-10-14) |
| cleric-mabel | kirk-anim-sheet-cleric-mabel-v1 | 1 | 5.1/2.8 | OK | OK |
| serjeant-bastien | kirk-anim-sheet-serjeant-bastien-v1 | 1 | 4.4/6.2 | OK | OK |
| hospitaller-ysolt | kirk-anim-sheet-hospitaller-ysolt-v1 | 1 | 8.3/4.3 | OK | OK |
| wayfarer-aelis | kirk-anim-sheet-wayfarer-aelis-v2 | 2 | 1.4/9.5 | OK all cells | **RETRY, recorded residual:** dcy 9.5 from bow-vs-standing pairs + mild row scale wobble; staged anyway, hand-tune at integration |
| canoness-hrothwyn | kirk-anim-sheet-canoness-hrothwyn-v1 | 1 | 7.5/6.3 | OK; minor: raised psalter tip kisses top edge in idle/step | OK |
| vexillor-anchetil | kirk-anim-sheet-vexillor-anchetil-v2 | 2 | 2.8/8.3 | OK (banner pinned upright in all cells) | OK (v1 foul: banner shouldered in step, dcx 17.0) |
| brother-aldwyn | kirk-anim-sheet-brother-aldwyn-v2 | 2 | 4.8/3.2 | OK | OK (v1 foul: row-1 sprites spilled into bottom-row cells) |
| cantor-sabina | kirk-anim-sheet-cantor-sabina-v1 | 1 | 3.5/3.8 | OK | OK |
| chevalier-raimbaud | kirk-anim-sheet-chevalier-raimbaud-v2 | 2 | 3.9/4.4 | OK | OK (v1 foul: heads clipped at top in 5/6 cells — sprites too tall) |
| vigil-knight-wymar | kirk-anim-sheet-vigil-knight-wymar-v2 | 2 | 7.7/10.3 | OK (sword+lantern pinned in all cells) | **RETRY, recorded residual:** dcy 10.3 from bow-vs-standing pairs (bow bend lifts centroid; standing family ≤7.7); staged, hand-tune at integration |
| friar-osbert | kirk-anim-sheet-friar-osbert-v1 | 1 | 8.3/2.6 | OK (crate prop consistent, incl. pray-before-crate) | OK |

- Aggregate, 11 new sheets: drift excl. kneel pairs — dcx_max mean 5.2 /
  worst 8.3; dcy_max mean 5.6 / worst 10.3 (both >9 are the documented
  bow-pair residuals). Silhouette IoU mean 0.483. **0 off-palette pixels in
  all 66 new q128 cells** (also verified 0 in the 6 reused pilot cells).
- v2 rewording that worked (kept in the driver): small-figure margins
  ("about two thirds of cell height, clear background band on all sides") for
  overflow/clipping fouls, and per-cell prop pinning ("banner pole stays
  planted upright in every cell" / lantern at feet) for centroid walks.
- No caption text drawn in any of the 11 sheets (v1 wording fix held);
  slice.py caption-strip stayed a no-op backstop.
- Ledger: +12 entries in `tools/artbake/MANIFEST.json` (`*-sheet` subjects,
  src_dims, cost 0.0; retry notes on the 5 re-prompted cells).
- **D5 outcome (2026-10-14): battle idle-loop animation shipped.**
  `stage_sprites.py` gained an ANIM table staging `01_idle` (+ `04_idle2`
  where the batch grid has one — every champion except Audric, whose 4.3
  pilot grid has no idle2 and loops on the single idle frame) into
  `crusade-app/assets/sprites/anim/{id:03}-{slug}/NN_pose.png`, same
  edge-flood keying (23 files, reruns idempotent, 0 semi-transparent px);
  bosses 101–107 keep static sprites by design — no special-case beyond
  "no frames → static". Shell: `ChampionAnim` component + preloaded handles
  swapped by `battle::advance_champion_anim` on 90-frame plateaus
  (≈1.5 s/pose at vsync; idle→idle2→idle only — pray/step/look/bow never
  auto-play; strike/hit await the combat-arc card), driven by
  `state::AnimTick`, a frame counter advanced once per rendered frame from
  `input::process_inputs` — the one per-frame system all three binaries
  already register — and self-installed via deferred commands, so main.rs /
  screens.rs / ui_shots.rs / smoke.rs needed zero edits. Frame index is a
  pure function of tick (`(tick/90)%2`), never wall time: ui-shots FULL pass
  twice produced all 8 PNGs byte-identical (`cmp` clean), battle.png shows
  both panels' idle cells crisp on-panel. Fallback ladder intact: no
  AssetServer → text-only; unknown/boss names → static champions sprite.
  `cargo test -p crusade-app` green without and with bevy-ui (incl. 2 new
  battle.rs bin tests: anim resolver incl. boss/unknown None, phase-plateau
  boundaries); `--smoke` exit 0.

### 4.6 Pixel app icon — D4 (DONE 2026-10-15, $0.00)

`crusade-app/web/icon.svg` (generic vector) replaced by a native pixel-art
mark: nano-banana-2-1, 3 candidates (`kirk-icon-v1..v3`), winner **v1** —
flat ink-navy #0B0E1B heater-shield field, deep-gold #C9A227 pilgrim-
star-cross, bone upper-left rim highlight; quantized via quant.py @1024.
Rejects: v2 (tilt + rim speckle), v3 (muddy multi-tone rim). Shipped
`icon-192.png` + `icon-512.png` (NN exports, palette-true, 100% opaque,
emblem inside the maskable safe circle — 57.5px vs the 76.8px limit @192).
`icon.svg` deleted, ALL refs scrubbed (index.html / manifest.webmanifest /
sw.js; sw CACHE bumped to `deusvult-v2`), zero dangling refs (grep-verified).
ART-PIPELINE §2's icon paragraph is now historical.

### 4.7 Hagiography second book: Manuscript of Sins (DONE 2026-10-15)

ViewSnapshot gains `sins: Vec<SinCard>` (serde-default, populated only in
Phase::Hagiography): 7 rows in gauntlet order; sealed rows carry EMPTY
sin/lore so no view binding can leak allegories early. Reveal record: new
settings key `client.rs::SINS_DEFEATED_KEY` ("campaign.sins_defeated", u8
bit per node) written in on_battle_end beside the shard bounty. Finding of
note: campaign `defeated` was session-scoped (Hagiography unreachable
mid-run), so the persisted mask is the only reachable reveal source; it
survives reload + roster sync. DEDUP: `client.rs::CAMPAIGN_NODE_SPECIES`
went `pub` and screens.rs's D3 mirror const was deleted — one table drives
mechanics, mask bits, and rest-screen portraits. 42/42 app tests (new:
manuscript_of_sins_gates_by_phase_and_defeat_mask); harness two-pass
byte-identical incl. hagiography.png (1 revealed + 6 sealed).

### 4.8 Combat-arc frames + battle wiring (DONE 2026-10-15, $0.00)

Card D6. Per the 4.3 hybrid decision, strike/hit are PER-FRAME generations
(nano-banana-2-1, 1024, identities verbatim from ppq_roster/ppq_portfolio):
24 frames (`07_strike_q128.png`/`08_hit_q128.png` per champion under
art/anim-staging/, poses.txt extended, +24 MANIFEST entries, cost 0.0; all
0 off-palette px, right-facing). 25/26 passed at v1; vigil-knight-wymar
strike retried v2 (v1 foul: red sash off-identity) — hit kept v1 via
deterministic re-quant of its preserved raw. stage_sprites.py ANIM pass
ships 01_idle (+04_idle2) + 07_strike + 08_hit for ids 1..=12 (rerun wrote
24, kept 62, idempotent); Audric uses the same fresh frames. Shell
(battle.rs): arcs trigger off `ViewSnapshot.fresh_events` (RiteUsed ->
strike, Damage -> hit), stamped per rebuild as beat slices summing to
exactly 45 app frames (consecutive-run dedup so multi-hit is no strobe;
ailment chip deliberately never a beat), counted down in ChampionAnim, then
back to the idle loop. Champion hit = hit frame + white tint flash; bosses
get the flash alone via a degenerate always-idle ChampionAnim. Attribution
honesty: engine sides map exactly only for practice/campaign (me==0); net
duels self-suppress arcs on the roster_count==0 tell (documented in
battle_row). +4 arc tests; --smoke exit 0; ui-shots full pass twice all 9
PNGs byte-identical; battle.png deterministically catches turn 2 mid-arc.
Project spend unchanged at $0.664.

### 4.9 E2 QA round: battle screen fit + mirror tag (DONE 2026-10-15)

Real-browser QA findings fixed in battle.rs only. (A) Battle screen no
longer overflows 1280x720: rites render in a fixed 2x2 grid of compact
two-line cells (live button / dead slab split unchanged), the chronicle is
a battle-local panel capped at 8 freshest-last lines (client LOG_TAIL
already delivers <=8), switch targets + Concede are one compact chip row,
champ-card padding/gap tightened 14/6 -> 10/4. Measured content extent:
battle.png bottom y=685 of 719 (was 719, clipped — row-2 rites truncated,
command row invisible); battle-over 545, battle-over-net 657, campaign 643,
all unchanged. (B) Mirror matches (same species both sides) tag the foe
panel's name line "Foe {Name} - Lv {N}"; ours stays plain, no sprite tint.
+1 bin test -> 74 green under bevy-ui, 42 plain; --smoke exit 0; D5/D6 arc
machinery 0 diff lines; harness 3-byte-identical passes, no leg edits.

### 4.10 E3 tooltip scaffold (DONE 2026-10-15)

crusade-app/src/bevy_shell/tooltip.rs (379 lines): `TooltipText(&'static
str)` component + `with_tooltip(&mut EntityCommands, tip)` builder.
Entity-local On<Pointer<Over>>/Out observers (matches the shell's click
pattern; zero app-level registration — the whole wiring is one
`mod tooltip;` per binary). Popup: one max, dark-navy PANEL fill, gold 1px
border, bone 14px text, 480px wrap, Pickable::IGNORE, parents under UiRoot
so rebuild_ui despawn cleans it free; anchors top-right of cursor in
windowed runs, deterministic below-right fallback headless. Two headless
tests drive the production observers via World::trigger (bevy picking
events are pure ECS — no winit needed): hover swap/reap roundtrip + layout
pin contract. Call-site wave next (battle stats/rites/chips, roster stats)
once sibling E1 lands.

### 4.11 E5 alt-portrait variants (DONE 2026-10-15, $0.00)

12/12 battle champions x 2 wardrobe/palette variants under
art/roster-staging/nano-banana-2-1/<subject>/ as alt-{a,b}-master.png
(1024) + alt-{a,b}_q128.png (128 canon-quantized). Same identity/face per
subject; variant concepts span winter-cloak, penitent-robe, campaign-worn,
ceremonial. 23/24 v1 eyeball-pass; 1 identity foul (canoness-hrothwyn alt-b
pipe artifact) fixed via pinned v2 retry. Ledger:
art/roster-staging/.ledger-e5.jsonl (25 entries incl. the foul + fix);
MANIFEST.json fold deferred until the tools/artbake lane is quiescent.
Assets are NOT yet staged into the game: selection UI is a future card.

### 4.12 E1 Company page gallery redesign (DONE 2026-10-15, $0.00)

Roster screen rebuilt around a new GALLERY art class: un-quantized 512px
plates at crusade-app/assets/sprites/gallery/NNN-slug.png (LANCZOS from the
surviving nano-banana 1024 masters, no palette remap, no flood-key --
painted grounds kept). Identity proven by re-quantizing each master to q128
byte-identical with the shipped battle sprites. 19 species-keyed plates
(audric-prayer stays a menu scene). Spend $0.00 -- masters reused, zero
API calls. stage_sprites.py gains a `gallery` pass (MANIFEST keys
"nano-banana-2-1/<cell>/gallery", idempotent, skips clean master-less).
Cards run 600x194 (framed 176px plate + full stat block: devotion/grace
training rows via new serde-tail ChampionCard fields, rite list with
Pow/Acc/Zeal) in a 2x3 grid paging 6/page via a shell-local RosterPage
resource (self-installing observer, no main.rs change). ui-shots roster
leg settle widened to 14 (six plates must resolve; this rode the tooltip
commit 17fe210 prematurely -- noted for the record). +5 tests (gallery
path/slug pinning, pager windowing, pre-gallery save compat), 45 plain /
81 featured green, harness twice byte-identical, roster.png reads
reverent and fits 1260/710 of the frame exactly.

### 4.13 Extended combat-arc frames - E4 (DONE 2026-10-15, $0.00)

Four new per-frame pose families for all 12 battle champions: 09_windup
(recoil-back before the lunge), 10_followthrough (weight past the target),
11_victory (temperament-matched per species lore: kneel+weapon-raised or
prayer of thanks - Wymar returns to his canon vigil pose, Anchetil's
banner never touches ground even in 12_defeat), 12_defeat (collapse/
slumped). 48 frames under art/anim-staging/<subject>/, poses.txt extended
with one E4 segment per subject. 48/48 passed eyeball at v1 - ZERO v2
retries; Wymar's D6 no-red identity pin applied from v1 (0 red-palette px
verified). 0 API errors, 0 off-palette px across all 48 q128s (PIL sweep).
Driver /tmp/opencode/sprite-bakeoff/anim_frames2.py (scratch; sibling of
anim_frames.py). Ledger art/anim-staging/.ledger-e4.jsonl (48 lines).
Both deferred ledgers now folded into tools/artbake/MANIFEST.json in this
commit (168 keys total): 48 E4 pose cells keyed
"nano-banana-2-1/<subject>-<pose>", 24 E5 alt cells keyed
"nano-banana-2-1/<subject>-alt-<a|b>" (fouled hrothwyn v1 excluded, v2
noted). Spend unchanged at $0.664. Not yet wired into stage_sprites/
battle.rs - that wave-2 card lands next.

### 4.14 E6 tooltip call sites (DONE 2026-10-15)

Hover-help prose wired on the E3 scaffold (battle panels: resolve bar,
typing line, champion-only sprite lore with boss allegory kept
Manuscript-sealed; rite cells with per-rite Pow/Acc/Zeal + effect clauses;
switch chips; phase-honest Concede arms via the roster_count==0 duel
tell; roster gallery: devotion/grace axes, plate origin lore, composed
virtue-vs-gauntlet matchups). Campfire boss portrait NOT wired:
snapshot() phase-gates sins[] to Hagiography and CampaignView carries no
defeat mask. Scaffold integration fix: TooltipText is now
Cow<'static, str> (&'static-only forced Box::leak per battle rebuild for
composed tips - caught pre-merge). ui_shots untouched (no observer Step
plumbing; E3 World::trigger tests cover hover; tips are hover-only). +6
shell tests (x2 bins): exact rite compositions, all-42-rite ASCII/132-char
sweep, concede arm table, lore gates, mirror-card matchup strings. 97
green under bevy-ui, 45 plain, --smoke exit 0, harness byte-identical.

### 4.15 E4 playback wave: three-beat arcs (DONE 2026-10-15, $0.00 art reuse)

The 48 E4 frames ship to assets (stage_sprites.py anim pass pose-list
extended; 95 per-champion frames; idempotent reruns; MANIFEST keys were
folded at 0941da4). Rite arcs are three beats - windup(telegraph) ->
strike(contact) -> followthrough(settle) - with ARC_HOLD_TICKS 45 -> 60
(20/20/20 lone cast, 15-tick floor at 4 beats/exchange; pure tick math,
zero wall-clock, harness stays exact). Defender hit-flash stays locked to
the flourish's opening frame (its chronicle line), attacker-only windup/
followthrough; bosses flash-only by species gate; missing handles degrade
to idle+flash, never a broken texture; net-duel roster_count==0 replay
suppression covers the new beats. Verdict machinery battle::verdict_row
built + tested (11_victory gold-frame / 12_defeat plain-fallen, bosses
static, headless-safe) - call sites wired in the follow-up commit: both over-screens show the
verdict row; the net surface's chronicle caps at 4 freshest lines
(measured overflow: banner clipped + Amen flush at the full 8). +3 battle anim tests (beat order/
timing, replay suppression sweep, verdict gates); 51 plain / 111 featured
green, check clean, smoke 0, harness twice byte-identical; battle.png
proves Audric's windup opposite Aldwyn's flash mid-arc.

### 4.16 Vesture picker + practice mirror-avoidance (DONE 2026-10-15)

Company cards now carry a navy/gold "Vesture" chip (absolute overlay,
bottom-left of the plate - a side column blew the 194px card budget;
geometry pinned by test) cycling base -> alt-a -> alt-b per champion.
24 alt gallery plates staged in-tree (LANCZOS from the committed E5
masters; crusade-app/assets/sprites/gallery/NNN--alt-{a,b}.png);
MANIFEST.json folded +24 keys (192 total; hrothwyn-b cites the pinned v2).
Persistence: settings key roster.variant.<species_id> (u8 ring, automerge
settings map, additive, wasm-safe, survives save/load; clamped, garbage
reads base). ChampionCard tail field variant + ClientInput::CycleVariant
(Roster-gated, sin-spawn ids refused). Battle keeps canon base art (alt
sets ship no anim families - hover tip says so). ui_shots roster leg
drives a CycleVariant on Bastien so roster.png proves the swap.
Practice foe trio root-fix: theme [3,8,10] minus party-held species,
backfilled 1..=12 in stable order (deterministic from party; +4 tests).
The "Foe" tag stays as the net-duel backstop. Tests 45->51 plain, 97->111
featured; wasm32 check clean; harness twice byte-identical (roster.png
differs from goldens exactly as intended; battle* differ via the new
Aldwyn-led trio; five screens untouched).

### 4.17 Adversary art lane — M7a (GENERATION DONE 2026-10-16, 15/15, $0.00)

`docs/ART-ADVERSARIES.md` holds the full prompt table for the historical-foes
lane (10 archetypes 108-117, 4 commander static boss portraits 120-123, 1
"road east" scene header), descriptors verbatim from HISTORICAL-FOES.md §4
wardrobe column, sin-depth tiers read from landed `adversaries.rs`, banned
cliches as the shared negative tail.

Generation run 2026-10-16 on this box via a scratch driver
(`/tmp/opencode/sprite-bakeoff/adv_batch.py`) built from the ART-ADVERSARIES
table (STYLE/NEG tails verbatim, plus the 4.5 caption-suppression wording).
nano-banana-2-1 only; gpt-image-1.5 contingency never needed. **Per-cell
status:**

| cell | final id | status |
|---|---|---|
| 108 turcoman-bowrider | v1 | generated, eyeball OK |
| 109 ghulam-lancer | v1 | generated, OK |
| 110 sudani-shieldbearer | v1 | generated, OK |
| 111 armenian-bowman | v1 | generated, OK |
| 112 daylamite-axeman | v1 | generated, OK |
| 113 bedouin-outrider | v1 | generated, OK |
| 114 kurdish-ghazw-rider | v1 | generated, OK |
| 115 naffatun-sapper | v1 | generated, OK |
| 116 askari-alamdar | v1 | generated, OK |
| 117 muqaddam-of-the-tulb | v1 | generated, OK |
| 120 kilij-arslan-i | v1 | generated, OK |
| 121 nur-ad-din | v1 | generated, OK |
| 122 asad-al-din-shirkuh | v1 | generated, OK |
| 123 saladin | v1 | generated, OK |
| road-east-header | **v2** | **regenerated** — v1 foul: sprite framing produced a red-coated figure instead of a landscape; v2 passed with a scene-tile style wrapper |

Zero cells failed twice. No banned motifs (scimitar/turban-helm/barbary
robes/camel/caricature) in any accepted cell; all faces calm/dignified; read
is kit-contrast per HF. Mechanical gates: all 15 q128s are 128x128 with **0
off-palette px and 0 semi-transparent px** (PIL sweep); q64s likewise
palette-clean. Staged at `art/adversaries-staging/nano-banana-2-1/<subject>/
{raw,q64,q128}.png` (roster-staging layout + version.txt pin). +15 MANIFEST
keys `kirk-adv-nano-banana-2-1-<subject>-vN` (207 keys total). Spend
unchanged at **$0.664**.

stage_sprites.py ADVERSARIES table + icons stay UNWIRED until the HF §8
human eyeball pass approves cells.

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
- Known cosmetic debt (updated 2026-10-14): mutex `.expect()` hardening only.
  CLOSED today: UI tofu glyphs (embedded font is ASCII-only by design; all
  rendered strings swept to ASCII, ui-shots-verified), lobby/relay test port
  races (retry+jitter at every MockRelay::run call site), RECV_TIMEOUT flake
  (diagnosed as box-saturation budget pressure on the 30s deadline, not a
  race — deadline deliberately left at 30s to keep liveness regressions loud).

## 6. Secrets & shelved threads

- PayPerQ key: user's personal store only; scripts read `PPQ_API_KEY` env.
  Nothing with the key may be committed (grep before pushes).
- Bedrock image lane: shelved (no AWS creds on box); quant.py pipeline is
  provider-agnostic if creds ever arrive.
- The serving gallery (box-local): `python3 -m http.server 8901 --directory
  /tmp/opencode/sprite-bakeoff` may still be running on the origin box;
  kill `pkill -f "http.server 8901"`.

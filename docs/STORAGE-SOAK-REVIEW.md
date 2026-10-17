---
title: "Storage-soak review: G16 soak tests + report (measurement-only)"
summary: >
  Review of the uncommitted G16 storage-growth soak (crusade-app/tests/
  storage_soak.rs, docs/STORAGE-SOAK.md). The soaks drive the real client
  API, every measured table cell reproduces exactly under my own rerun
  except two off-by-one cells in the baptism table, ALL claimed growth
  slopes recompute from the printed data, the hex-at-2x and 8 MiB claims
  check out in code, the projection arithmetic holds, and the flagged
  zeal-carryover bug is genuine (verified end-to-end in client.rs/types.rs/
  engine). Three doc-accuracy nits; none move any conclusion.
tags: [review, crusade-app, storage, soak, g16, persistence]
status: review
date: 2026-10-17
---

# Storage-soak review (2026-10-17)

## Scope

- Untracked additions only: `crusade-app/tests/storage_soak.rs` (748 lines)
  and `docs/STORAGE-SOAK.md`. `git status` confirms nothing else in the
  tree was touched — no new APIs in client.rs, no behavior changes.
- Card constraints: `#[ignore]`d soaks through the real
  `ClientGame::handle` API; battle fabrication per existing test recipes;
  mid-journey save/load; measured trail JSON + automerge doc bytes; growth
  fit + projection to the ~5 MB localStorage budget; recommendation
  preserving memorial data.

## Verification performed

- **Soak rerun (self):** `cargo test -p crusade-app --test storage_soak --
  --ignored --nocapture`: 4/4 pass in 11.44s. Result 1 (worst-case road),
  Result 2 (25 pilgrimages, all 25 rows printed), and Result 3 (write
  amplification, all 8 rows printed) reproduce **cell-for-cell** against
  the doc tables, including trail JSON lengths, doc bytes, hex bytes,
  scenario tallies (8 days / 8 battles / 1 conceded fall / 0 calls / 0
  refused), and the terminal "day 10, book 20/20, 0 living" line. Result 4
  reproduces at 100 (4,566 / 9,132) but see NIT 1.
- **Real-API drive:** the journey driver uses only
  `ClientInput::{StartTrail, Travel, EngageChoice, PickRite, PickSwitch,
  Concede, CallToRepentance, AcknowledgeResult, AbandonTrail}` plus
  `game.snapshot()`/`game.store()`. `fight_or_concede` is the same
  best-attack / pick-switch recipe as `tests/client_tests.rs` and
  `tests/campaign_tests.rs` (mkchamp pattern verbatim, max-power rite
  pick, `<idx>:<name>` switch parsing identical). The conceded champion
  fall routes through the real `on_trail_battle_end` succession path
  (client.rs:1433-1528). No private-field fabrication in the game lane.
- **Determinism pin:** `soak_store()` uses `set_champion` with the
  lexicographic minimum UUID; `pilgrim_seed` (client.rs:245-286) really is
  derived from sorted roster ids and persisted under `trail.seed` at first
  embark (client.rs:1178-1179), and `start_trail` really does take
  `roster[0]` after id sort (client.rs:1147-1148). The pin is load-bearing
  and honest. Reruns are byte-identical.
- **Measurement integrity:**
  - `trail_json_bytes`: `persist::save_trail` is literally
    `serde_json::to_string` + `set_setting(TRAIL_SAVE_KEY, ...)`
    (persist.rs:223-227); the soak re-serializes the same DTO, same
    serializer — the byte count is the stored string's length.
  - `doc_bytes` is `RosterStore::save_bytes()`; `hex_bytes` the claim is
    verified: wasm `web_save_document` stores `hex::encode(bytes)` under
    `deusvult.roster.v1` (persist.rs:24-32), called with the store's save
    bytes at bevy_shell/input.rs:166-167. The "browser sees ~2x doc bytes"
    unit claim is correct.
  - The 8 MiB `MAX_DOC_BYTES` load cap exists (crusade-data/src/roster.rs:41).
- **Slope recompute from printed rows:**
  - Book page rate: (3152-2067)/(20-1) = 57.1 B/page — doc's "~57" ✓
    (bounded at 20 pages → ~3.2 KB ✓).
  - Per-pilgrimage: (23068-1937)/24 = 880.46 — doc's "880.5" ✓. Per-run
    deltas from the full printed table decline from ~950 (runs 2-4) to
    ~872-876 (runs 15-25): asymptotically below the quoted average, so the
    assertion "cleanly linear" is slightly overstated but the projection
    built on 880.5 is *conservative* (the true long-run slope is lower).
  - Write amplification: segment slopes 35.1 / 35.2 / 34.6 / 35.4 per
    write across the four 250→2,000 segments; endpoint fit 34.92 ✓ — this
    one IS linear.
  - Baptisms: (4566-1471)/90 = 34.4 ✓ ("~34" ✓).
  - Projections: (2621440-1937)/880.46 ≈ 2975 → "~2,980" ✓ (doc's own
    5 MB → 2.5 MiB doc-byte conversion is right: 5 MiB hex = 2,621,440
    doc bytes); 8388608/880.46 ≈ 9528 → "~9,500" ✓; codex "75,000 writes"
    = 2621440/34.9 ≈ 75,100 ✓. "~25 save writes per pilgrimage" =
    880.5/34.9 ≈ 25.2 ✓ consistent.
- **Scenario plausibility:** 24↔25 both carry `scripted_gate`
  (overworld.rs:121-122) and are adjacent, so the terminus bounce fights a
  scripted gate per entry as claimed; gates field escort_lv = mantle-2
  (clamp 5) ×2 plus commander at mantle+2 (client.rs:1278-1285) — a 1v3
  against peers/above, since `t.party` is a single-file mantle
  (client.rs:1160, 1522). Wild/ambush mixes are 2-3 foes at mantle level
  (`trail_foe_mix`, client.rs:191-210). All adversary archetypes are
  `Typing::Valor` — the "no effectiveness edge" claim holds. `spend_band
  (1000)=2`, `band_of(255)=2`, 20-man file (`STARTING_MEN`), so "every
  soul ashore" is the true best crossing. Historical/extinction outcome
  follows from the engine, not fabrication.
- **Lanes larger than modeled?** Checked the un-modeled write lanes:
  `scan_defeated_foes` writes `foes.defeated` only when the mask actually
  changes (client.rs:1725-1727) — bounded by species count, then silent;
  campaign-lane writes (sins mask, shard items) are outside the trail
  story and tiny/bounded; `trail.seed` is minted once and kept
  (persist.rs:246-250 semantics). No unbounded lane escapes the model.
- **Memorial-preservation rule:** the recommendation's compaction sketch
  is op-history re-base only, explicitly excluding Book pages and roster
  entries ("a page is never silently dropped"). Honored.
- **Normal suite:** orchestrator-verified 76/76 unchanged; nothing but the
  two new files exists in the tree, and all four soaks are `#[ignore]`d,
  so CI is untouched by construction.

## Findings

### NIT 1 — Result 4 table: two cells off by one byte from the rerun

Doc rows: `10 → 1,472 / 2,944` and `50 → 2,881 / 5,762`. My rerun prints
`1,471 / 2,942` and `2,880 / 5,760` (the 100-row, `4,566 / 9,132`,
matches exactly). One byte low in the doc on doc_bytes, two on hex. The
slope (34.4 B/baptism), the per-baptism claim, and every projection are
unaffected — this is a transcription slip on two cells. Correct the two
rows to the measured values before commit.

### NIT 2 — "exactly the four writes ClientGame::baptize performs" overstates fidelity

The real `baptize` (client.rs:1803-1827) additionally sets
`convert.converted = true` and `convert.converted_from = Some(species_id)`
on the stored instance, and writes the per-species `baptismal_name()` as
the setting value. The soak's fabricated convert leaves both flags default
and writes a fixed 20-char name. Direction: the real baptism is a few
bytes *larger* per event (~5-10 B on a ~34 B base), so the measured slope
is a slight underestimate — immaterial (the baptism axis sits 2+ orders
of magnitude under budget even at double the rate), but the doc should
say "the four write *sites*", not "exactly the four writes".

### NIT 3 — two honest-reporting wording slips

- "0 battle entries refused" in Result 1 is accurate, but the intro
  ("documented here because the soak had to tolerate them") implies the
  zeal path was exercised; it never fired (0 wins). The tolerant driver
  arm is defensive dead code in this soak — the finding stands on the
  code read (below), not on observation. Say so.
- The Result 2 table shows `book_pages` pinned at 20 across all 25
  pilgrimages because a fresh `TrailState` is built per embark — the v1
  Book **does not accumulate across pilgrimages at all**. The doc never
  states this plainly; a skimmer could read "append-only memorial" as
  cross-pilgrimage. One sentence would close it (and it *strengthens*
  the no-cap conclusion).

## Finding verification: the two flagged behaviors

**Zeal carryover (finding 1) — GENUINE BUG, fully reproduced by code
read.** The chain, link by link:

1. Engine spends zeal per rite use: `slot_ref.zeal_left -= 1`
   (crusade-core/src/engine/attack.rs:76).
2. Trail victory path clones survivors verbatim into `t.party` and saves
   (client.rs:1411-1418) — resolve carryover is documented intent ("march
   on UNRESTED"), but zeal goes along with it.
3. Battle entry requires full zeal: `validate_instance` fails
   `IllegalRite` unless `slot.zeal_left == rite.zeal`
   (types.rs:358-360); `Battle::new` maps that to
   `BattleError::IllegalChampion` (engine/mod.rs:203-216);
   `enter_trail_battle` clones `t.party` straight into `Battle::new`
   (client.rs:1360-1361).
4. The campaign lane restores zeal on victory precisely because "battle
   entry requires full-zeal rites" (client.rs:1616-1618, 1640-1644) —
   the trail victory path lacks the mirror.

So any road-battle win that spent zeal leaves the survivor unable to enter
the next road battle; and the failure surfaces *after* the day's
save (client.rs:1289-1305 save before `enter_trail_battle`), matching the
soak's described tolerance. Not a misread of the fabrication — the soak
never wins, so it never matters there; this is a latent v1 trail bug in
M8b's battle chaining, correctly handed to a separate fix card. The
suggested fix (field-reset zeal for road survivors on victory, mirroring
the campaign lane) is the right shape.

**Road structural unwinnability (finding 2) — supported.** 1v3 gate
composition (escorts at mantle-2, boss at mantle+2), 2-3-deep wild/ambush
mixes at mantle level, all-Valor adversary typing, wound carryover without
rest: verified above, and the empirical soak confirms extinction by day
10 with 0 wins for a fully-invested L50 veteran under the best crossing.
Stated as a "worth saying in the design docs" note, not a storage defect.

## Verdict

**SHIP** — the soaks drive the real public API with honest fixtures, every
load-bearing table and slope reproduces exactly under rerun, the
hex-at-2x and 8 MiB claims are true in the persistence layer, the growth
fits and ~3k-pilgrimage projection recompute from the printed data, the
no-cap-for-v1 recommendation is sound and preserves the memorial contract,
and the flagged zeal-carryover bug is genuine and correctly routed to its
own card. `#[ignore]` gating keeps CI and the normal suite untouched (76/76
re-verified by the orchestrator). Before commit, fix the two off-by-one
cells in the Result 4 table (NIT 1) and, in the same polish pass, the
three wording slips (NITs 2-3) — none changes a number that matters or a
conclusion.

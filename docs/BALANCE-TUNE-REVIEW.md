---
title: "Balance-tune review: adversary kits, rites 58-62, fuzz post-tune report"
summary: >
  Review of the uncommitted balance-tuning diff (archetype learnsets, five new
  adversary rites 58..=62, Comet of Naphtha 60->80, post-tune sections in
  FUZZ-ADVERSARIES / HISTORICAL-FOES / CONVERSION). Stat/typing/id/lore lane
  is clean, rites 1..=42 and the engine untouched, and every reported
  post-tune fuzz number reproduces exactly under my own rerun. One MEDIUM:
  the Armenian Bowman (111) now has no learnset entry below level 5, opening
  a narrow zero-rite convert lane and contradicting the card's kit-legality
  constraint (and the worker's own claimed invariant).
tags: [review, crusade-core, adversaries, balance, fuzz, historical-foes]
status: review
date: 2026-10-17
---

# Balance-tune review (2026-10-17)

## Scope

- `git diff` (working tree): crusade-core/src/data/adversaries.rs,
  docs/FUZZ-ADVERSARIES.md, docs/HISTORICAL-FOES.md, docs/CONVERSION.md.
  Exactly four files — the card's lane, no more.
- Card constraints: HF section 4 stat lines/typings/ids/names/lore/obstinacy/
  sin_depth binding; only kits + adversary rites 43+ tunable; 117 and
  120..=125 untouchable; rites 1..=42 and engine untouchable.

## Verification performed

- **Diff audit (lane):** the adversaries.rs diff touches only learnset
  entries (archetypes 108..=116), the five new rite blocks 58..=62, Comet of
  Naphtha's `power` field (60 -> 80 — in-lane, rite 54 is archetype-only),
  the module doc's id range, and the rite-count guard test. Zero changes to
  any stat line, typing, id, name, lore, obstinacy, or sin_depth; zero
  changes inside the 117/120..=125 rows; rites.rs and every engine/sim/app
  file untouched. HF diff is the section-4 kit column plus the dated kit note
  and one `43..=57` -> `43..=62` range line; CONVERSION.md diff is the
  balance-debt note only.
- **Fuzz rerun (self):** `crusade-sim fuzz 2000 {5,20,40} adversaries`
  (release build, working tree). *Every* post-tune number in the doc
  reproduces exactly: aggregates 884/1116 (L5), 783/1217 (L20), 791/1209
  (L40); matchup-1 avg turns 14/13/14; all 28 per-id rows at all three
  levels in both matchups match the reported tables cell-for-cell (spot of
  the load-bearing ones: Shieldbearer 23->51/29->56 vs champions, mirror
  37/36 -> 61/66; Sapper 29->39/39; Bowman 25->49/28->43; failures 0
  everywhere). The mirror-matchup adversaries-vs-adversaries tables
  (914/1086, 892/1108, 916/1084 side splits and all per-id rows) also match.
- **Before-columns** in the post-tune tables match the HEAD baseline tables
  in FUZZ-ADVERSARIES.md cell-for-cell.
- **Arithmetic cross-checks in the new prose:** archetype L20 average
  35.4 -> 50.0 ("36 -> 50" ✓); boss five (117, 120..=123) average exactly
  77 at L20 (("~77" ✓) and exactly 79 at L5 ✓; "44% sits 0.8pp under the 45%
  floor" = 884/2000 = 44.2% ✓; "L5 per-id now 35-60" ✓ (verified range
  35-60).
- **New rites 58..=62 vs claimed intent:** ids sequential after 57, unique,
  `Typeless`; power/accuracy/zeal/secondaries match the FUZZ post-tune
  account exactly (58: 65/90/15 Guard+1 self; 59: 85/90/12 HighCrit; 60:
  85/90/12 Guard-1 target; 61: 80/95/15 flinch 30%; 62: 85/90/12 no riders).
  No name collision with champion rites 1..=42 or adversary rites 43..=57
  (full name lists compared). Neighbor comments are grave, specific, and
  unit-attributed (e.g. "the short thrusting spear taking a horseman out of
  the saddle; the shield never leaves the line") — section 6 register holds.
  The count guard was correctly widened 15 -> 20 with the id sequence
  asserted, and the `RITES.len() == 42` champion-table pin kept.
- **Learnset legality:** levels ascending in every edited learnset; all
  referenced rite ids exist (`learnset_rites_exist_levels_ascending_in_range`
  covers both); the `level <= 5` damaging-rite guard predates this diff and
  was not weakened (verified at HEAD). L5 built kits: every archetype still
  fields a damaging rite (108: {1,44}, 109: {1}, 110: {1,2}, 111: {48},
  112: {1}, 113: {1}, 114: {1}, 115: {1}, 116: {1}) — see the MEDIUM finding
  for 111's lane below.
- **HF section 4 kit column vs code:** all nine changed archetypes
  spot-checked against adversaries.rs — order and every `@N` learn level
  matches (110: MB, SW, 58@6, 46@10 ✓; 111: 48@5, 47@6, 2@9, 59@12 ✓; 112:
  1, 49@6, 50@8, 60@12 ✓; 113: 1, 51@6, 52@7, 61@10 ✓; 114: 1, 15@6, 53@7,
  7@10 ✓; 115: 1, 54@6, 55@6, 5@9 ✓; 116: 1, 18@6, 56@9, 62@12 ✓; 108 and
  109 likewise ✓). 117's row is genuinely unchanged.
- **CONVERSION.md debt note:** truthful on every checkable claim — the
  Shieldbearer's 23/29 -> 51/56 fix via kit (not a Valor STAB lane), the
  champions-only-pool limitation already resolved, the L5 de-tuning via
  learn levels.
- **Orchestrator-verified (trust, not rerun):** cargo test 102/102; champion
  golden `fuzz 2000 5` byte-identical pre/post. Consistent with the
  mechanism — the champion pool draws via the preserved
  `gen_range(1..=12)` path and adversary data cannot leak into it.

## Findings

### MEDIUM — Armenian Bowman (111) now has no learnset entry below level 5

108's old `level: 1, rite_id: 47` slot was the only L1 entry on 111; the diff
deletes it and does not replace it (adversaries.rs:391-394: first entry is
now `level: 5, rite_id: 48`). Consequences:

- **Violates the card constraint** the reviewer was asked to check ("each
  archetype still has a legal L1 kit"): a levels 1-4 Armenian Bowman builds
  with *zero* rites. Every other archetype gained `LearnEntry { level: 1,
  rite_id: 1 }`; 111 did not — and the worker's own account section claims
  "each gaining Measured Blow at level 1", which is false for 111.
- **Reachable in-game:** `crusade-app/src/client.rs:1806` (`baptize`) builds
  the convert via `practice_champion(species_id, pc.foe_level.clamp(1,
  MAX_LEVEL))`, and `foe_level` mirrors `t.champion_level`
  (client.rs:1347), which sinks toward 1 on defeats
  (`.saturating_sub(5).max(1)`). Wild foes are clamped to >=5
  (`build_trail_foes`, client.rs:1316), but the baptism path is not — so a
  road fight won at mantle level 1-4 over the zone-3/4 mix (both contain
  111) can baptize a Bowman with a completely empty rite array.
  `validate_instance`/(`types.rs`) passes an all-`None` rites array
  vacuously, so the man joins the roster legal-but-bricked: in battle he
  can only Yield. Pre-diff this archetype had a legal kit at every level.
- Fuzz/tests are blind to it (fuzz runs at 5/20/40, damage guard keys on
  <=5 via rite 48), so nothing automated squawks.

Fix: add `LearnEntry { level: 1, rite_id: 1 }` to 111's learnset (Measured
Blow, 40 power, is dormant under the max-power policy while Volley in Turn
has zeal — 65 > 40 — so the L5+ play pattern is unchanged; only the
zeal-exhaustion tail, ~20+ turn battles, differs, and avg turns is 13-14).
Then re-run the three fuzz passes and re-sync the doc if any cell moves
(expected: identical or within rounding). Cheap; do it now rather than
papering over the one species with a sub-5 hole.

### LOW — residual rationale for 109/114 overstates its facts

The FUZZ post-tune "Residuals" bullet says Ghulam (62/65) and Ghazw (60/62)
"read above the band ... despite byte-identical kits ... no legal kit lever
exists that does not touch the boss tier". Neither claim survives the diff
itself: 109's learnset `[15@1, 18@7, 45@12]` became `[1@1, 15@6, 18@7,
45@12]`, and 114's `[15@1, 53@6, 7@10]` became `[1@1, 15@6, 53@7, 7@10]` —
at L20/L40 both now carry a fourth rite (Measured Blow) where the old kits
had three, i.e. *not* byte-identical (behaviorally near-identical, since
the added slot is dormant under the max-power policy — 40 < 45 < 80/70 —
but the doc says what the doc says). And a legal lever *does* exist:
109/114 learnsets are card-tunable (this same diff edited them), so a
future owner decision could de-level or drop a stroke on exactly those two
ids without touching 117/120..=125's learnsets or the shared rites'
numbers. The underlying judgment is fine — 60-65 is 0-5pp over the band
edge, inside the documented +/-6-7pp noise floor, and the card set no
ceiling target — but the "no legal lever" / "byte-identical" framing should
be rewritten as "outcome accepted within the noise floor; levers exist but
were not needed."

### NIT — HF kit-note annotation and band claim slightly overstate

- "the `@N` suffixes above mark learn levels after the fuzz pass" — only
  *changed* levels carry suffixes; unchanged mid-level entries (Dust of the
  Plain @5, Zhupin Cast @8, Mirage Veil @7, Wet Hide Ward @6, Litany of
  Stillness @9, Steady Ward @9, Hold the Banner @9) are unmarked, so a
  reader cannot reconstruct full learnsets from the table alone. Ordering
  matches learnset order, which is the load-bearing property; reword the
  note to say `@N` marks the level where a suffix appears.
- "rites 58-62 ... lifted the chronic L20/L40 stragglers into the 40-60%
  band" — the Naffatun Sapper sits at 39/39 and the Outrider at 41/39,
  which the FUZZ doc itself reports honestly ("at the accepted >=38 line").
  "toward the band" (the card's own wording) is the accurate phrase.

## Register check (docs/HISTORICAL-FOES.md section 6)

- New rite names are functional martial terms ("The Banner Advances",
  "Lance at the Gallop"), ASCII-clean, no stolen champion-rite names.
- New code comments and FUZZ prose are chronicle-firm and material ("the
  last sheaf, a shaft finding the eye-slit"; "the naphtha pot is his whole
  trade") — violence in service of the register, attribution to craft and
  circumstance, no mockery. Pass.

## Verdict

**FIX** — everything the fuzz report claims reproduces exactly under rerun,
the binding lane (stats/typings/lore/rites 1..=42/engine/117/120..=125) is
provably untouched, and the targets are met within the documented noise
floor. But the Armenian Bowman ships with no sub-5 learnset entry, breaking
the card's own kit-legality requirement and opening a real (if narrow)
zero-rite convert lane through `baptize` (MEDIUM); the one-line fix plus a
fuzz re-confirm is cheap. The LOW residual-rationale overstatement and the
NIT doc wording ride along for the same polish pass.

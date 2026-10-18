# L5-SIM-REVIEW — adversarial review of docs/L5-SIM.md

Status: REVIEW COMPLETE, 2026-10-18. Reviewer: independent subagent, no
authorship overlap with the sim or the doc. Method: read
`crusade-app/tests/l5_commander_sim.rs` against the shipped data tables,
the reference harnesses (crusade-sim, storage soak, trail depth sim), and
the four source docs; re-ran both lanes in release mode.

## Verdict: FIX (minor) — three mechanism sentences need correction; the numbers, the before/after tables, and the REJECT recommendation are sound and reproduced exactly.

The analysis is decision-quality except for three prose claims about
WHAT the lever does to kits at levels >= 6. Those sentences are factually
wrong (one of them claims a sim verification that never happens), even
though the error does not move any measured cell or the recommendation.
An owner asking "what exactly would this edit do?" would be misinformed
by them; they are cheap to fix.

## What was verified, and how

### 1. Override mechanism — faithful

- `kit()` in the sim is byte-equivalent in logic to crusade-sim's
  `build_champion` and the client's `practice_champion`: entries with
  `LearnEntry.level <= level`, newest-first pick, dedup, 4-rite cap,
  reversed. Learn-level semantics (`learn_level <= L`) are honored
  verbatim. crusade-sim's `ADVERSARY_IDS` pin (108..=117 + 120..=123)
  and champion pool (1..=12) confirmed at
  `crusade-sim/src/main.rs:31-40`.
- `applied_learnset` entries were diffed against the committed
  `crusade-core/src/data/adversaries.rs` learnsets by hand: for all six
  commanders the override is exactly "pushed strong L1-lane rite(s) to
  6" + "rite 1 (Measured Blow) filled at level 1 where absent" — i.e.,
  the same shape the shipped archetype treatment used (committed
  archetype learnsets carry `(1, rite 1)` + pushed rite at 6; e.g. 111
  fields `[1, 48, 47]` at L6).
- `Battle::new` validates rites against the COMMITTED learnset only at
  construction (`engine/mod.rs:209` via `validate_instance`); `Battle`
  and `RiteSlot` have plain serde derives, no custom deserialization
  validation, and the serde keys used by `patch_kit_json`
  (`rite_id`/`zeal_left`) match `types.rs:264-267`. The patch rewrites
  all four slots (excess slots nulled) and sticks — proven empirically:
  the applied lane reads 74.0%, which is impossible if the patch
  silently kept the old rites. Zeal is set to full `r.zeal`, exactly as
  the practice recipe does.
- Overrides are applied only where `kit(applied) != kit(current)` at the
  fought level, so escorts/archetypes are untouched and unswept
  non-commanders can't be silently altered.
- Latent wart, not triggered: `fight()` maps `Battle::new` refusal to
  `(true, 0)` — a champion-side win with zero turns. No species in the
  pools is riteless at 5 (all learnsets have an entry at 1..=5) and the
  printed avg turns (14.1/15.9) prove all 2000 battles ran; but a future
  edit reusing this lane could silently bias counts. Worth a `panic!`
  or an explicit counter.

### 2. Lane A — like-for-like, margins safe

- Pool is 16 ids = ALL adversaries (the table has exactly 16; no
  118/119 "sub-commanders" exist). The 124/125 extension beyond
  crusade-sim's pinned 14 is disclosed in the doc.
- Before/after is a PAIRED comparison: identical seeds
  (`[s as u8; 32]`), identical rng streams for team draws, identical
  per-battle `Battle::new` seed — the only varying input is the kit
  patch. crusade-sim uses the same seeds-as-`[s as u8;32]` convention,
  so the aliasing (256 unique seeds for n=2000) mirrors the reference
  harness rather than exceeding it. The doc's absolute-vs-delta caveat
  (rng is `BattleRng`, not crusade-sim's ChaCha8) is stated and correct.
- Cross-harness sanity: lane A "before" per-commander rates (94.6/77.1/
  82.4/86.1) are close to the FUZZ post-tune 14-id-pool readings
  (92/78/78/80), consistent with adding two more above-band ids and a
  different rng. Aggregate avg turns 14.1 matches FUZZ's 14.
- Margins vs the ±6-7pp correlated-seed caveat: aggregate delta is
  +36.9pp; the SMALLEST per-commander delta is Nur ad-Din at 21pp —
  three times the noise floor even if it applied (it is the wrong model
  for paired columns anyway). All six commanders' moves are decisive.
- Nit: "appearances ~336-427 each, noise ~±2pp" understates the
  unpaired sampling noise (only 256 unique battles, ~44 unique
  appearances per commander) — but pairing makes this moot for the
  decision. The word "noise" is the only loose word in the table block.

### 3. Lane B — cells and offsets match the shipped gates, and identical results are genuinely structural at the swept cells

- GATES constant verified against `data/overworld.rs` ScreenDefs (101→
  120@+2, 102→121@+2, 103→122@+2, 104→124@+2, 105→125@+3, 108→123@+3),
  GATE-CONTENT.md §0 escort columns, and the client's gate build
  (`client.rs:1477`: `escort_lv = lv-2 clamped 5..=50`, `boss_lv =
  lv+offset min MAX`, boss pushed last = party index 2). All match.
- Reproduction: all 30 cells before == after exactly, including turns.
- WHY that is genuinely structural, checked cell by cell:
  - cap 15+ cells (boss >= 17): the 4-rite cap has bound, so applied and
    committed kits are the same SET (only 121's slots 0/1 swap order —
    45 vs 47, powers 80 vs 30, never a greedy tie, inert). Identical
    output is byte-expected.
  - cap-3 cells (boss 5/6): the override IS applied where the kit
    differs... and the captain at 3 dies to two L5 escorts before the
    boss acts (turn counts 4.8-5.8 both ways). So the canary proves the
    lever reached the battle and changed nothing, not that the lever
    failed — exactly what the doc's reading says in its second clause.
- So the conclusion "the lever cannot move any gate at any captain
  level" survives scrutiny.

### 4. Recommendation and counter-dials — logic sound, one overstatement to soften

- The 40% floor framing is accurate: FUZZ §4 sets the 40-60% band on
  archetypes and explicitly designs the named tier ABOVE it; dropping
  4/6 named men through even the archetype floor is correctly called a
  double breach. The REJECT is fully supported by reproduced numbers.
- "Scope the fuzz target to the archetype pool" is consistent with
  precedent: FUZZ already accepted an aggregate pinned below the 45%
  target by boss-tier composition (44% at L5) and above it at L20/L40,
  and its residual makes the commander proposal conditional on ">=45%
  champions-side" being mandatory. Recommended counter-dial 1 is
  analytically honest, not a trick: the full-pool number is disclosed in
  the same doc.
- Counter-dial 2 inwardly consistent (Kilij-only measured 94.6→31.1, so
  "not actually a gentle dial" is supported by the same table).
- Wording nit: "accept the ~79% named-tier inflation as composition,
  as the doc already does at L20/L40" — 79% is the FUZZ L5 figure; the
  L20/L40 acceptance rides ~77. Cosmetic.

### 5. Nits against the FUZZ residual — none material

- Residual quote ("their 390-420 stat lines should hold them above the
  band at every level") verbatim-accurate, including the conditional
  framing ("if the L5 aggregate must reach >=45%").
- 124/125 extension-by-analogy is flagged in both the doc and the sim
  header; stat totals claimed 390-420 spot-ckecked (Kilij 400, Nur 415).
- "No commander GAINS a rite" is true of the residual itself; the
  Measured Blow backfill added by the applied learnsets is disclosed in
  §0 and matches the archetype precedent.

## The three sentences to FIX before the owner sees this

All in the explanatory prose; none touches a number, a table, or the
recommendation. Corrected wording given.

1. §0: "Kits at every level >= 6 are set-identical to the committed
   learnsets (verified in the sim): the lever acts ONLY on kits at
   levels 1..=5."
   FALSE twice. The added Measured Blow (rite 1) persists in applied
   kits past 5: 121 at 6-9, 122 at 6-11, 123 at 6-13, 124 at 6-13, 125
   at 6-9 all carry one extra rite vs committed (hand-verified against
   the committed learnsets and `kit()`'s newest-4 rule; the 4-rite cap
   only evicts it later). The shipped archetype treatment has the same
   property (111 fields rite 1 at L6), so the override is still a
   faithful "exactly as the archetypes were treated" — but the sentence
   misstates both the edit's footprint and the sim: nothing in the sim
   prints or asserts kits >= 6, so "verified in the sim" never happened.
   Correct to: "kits >= 6 differ from committed only by the backfilled
   Measured Blow entry where present (same shape as the shipped
   archetype learnsets); under the power-max foe policy that rite
   (power 40) scores below every lane it shadows (45-80 power), so
   applied behavior >= 6 is unchanged."

2. §2 reading: "the pushed lanes land at level 6, so a boss at any
   level >= 6 fields the identical kit".
   Same overstatement. At boss 6-13 some applied kits carry the extra
   dormant rite (and 121's >=14 kit swaps two slot orders). Results are
   identical because the differing rite never wins the greedy score, not
   because the kit vectors are identical. Correct to "fields the same
   fighting rites; the only diffs are a dormant Measured Blow / slot
   order, invisible to the power-max policy".

3. §2 reading: "the diagnostic cap-3 cells (boss at 5/6, where the
   applied boss truly is disarmed)". Half wrong: at the +3 gates the
   boss stands at 6, where the pushed lanes have ALREADY re-landed
   (applied 125 at 6 = [1, 45, 53]; applied 123 at 6 = [1, 47, 57]) —
   that boss is not disarmed at all. The flat 0.0%/0.0% there is fully
   explained by the next clause (L3 captain dies to L5 escorts in ~5
   turns), so the conclusion stands; only "truly is disarmed" should be
   scoped to the +2 gates' boss-5 rows — and even there 121 keeps
   Storm of Arrows (47).

If the owner wants the ">=6 invariance" claim enforced rather than
prosed, the honest fix in the sim is one extra print/assert pass:
`kit(id, L, applied) == kit(id, L, committed)` as SETS minus the
`rite 1` backfill, swept over L in 6..=50. As written today, no such
check exists.

## Bottom line

The measured claim — 37.1% -> 74.0% champions-side, 4/6 commanders
under even the archetype floor, zero gate matchlet moved — reproduces
bit-for-bit under
`cargo test -p crusade-app --test l5_commander_sim --release -- --ignored --nocapture`,
the harness faithfully mirrors crusade-sim/practice-champion semantics,
and REJECT-as-documented with counter-dial 1 (scope the aggregate to
the archetype pool, per FUZZ's own composition acceptance) is the
correct owner recommendation. Fix the three mechanism sentences above
and ship the doc.

---
title: "M7 Review: Historical Adversary Lane (adversaries.rs + Call to Repentance seams)"
summary: >
  Reviewer report for the M7 worklist: historical adversary data tables
  (ids 108-117, commanders 120-123, rites 43-57), Typing::Valor chart seam,
  conversion_chance contested roll, and serde-additive instance fields.
  All build/test gates verified green; five MINOR findings, no MAJOR or
  CRITICAL. Verdict: SHIP.
tags: [review, m7, crusade-core, adversaries, conversion-mechanic]
date: 2026-10-16
status: review
---

# M7 Review — Historical Adversary Lane

Scope: diff only (`git diff` on the working tree + the untracked
`crusade-core/src/data/adversaries.rs`, read in full). Contracts checked:
`docs/HISTORICAL-FOES.md` sections 4-7, `docs/CONVERSION.md` sections 3 and
7, `docs/DOCTRINAL-SINS.md` section 2.

## 1. Verification results (self-report claims, checked)

| Claim | Result |
|---|---|
| `cargo test -p crusade-core` passes | CONFIRMED — 75 passed, 0 failed |
| `cargo test --workspace` passes | CONFIRMED — all suites green, 0 failed (75 core + 133 + 26 + others) |
| Single-crate wasm check fails pre-existing on getrandom | CONFIRMED — `cargo check -p crusade-core --target wasm32-unknown-unknown` fails with getrandom 0.2.16 E0433; verified **pre-existing** by `git stash -u` on clean HEAD (identical failure). Cargo.toml/lock untouched by the diff, so the dep graph cannot have changed. Tree restored cleanly after pop |
| Multi-crate wasm check passes | CONFIRMED — `cargo check -p crusade-app --target wasm32-unknown-unknown` (kirk lib + core + net + data + bridge + app) finishes clean |
| Workspace warnings | All `unused import/variable` warnings trace to `deps/cdk/**` and `kirk/src/**` files outside this diff — pre-existing, not worker's |

## 2. Data fidelity vs HISTORICAL-FOES.md section 4 (all 14 rows)

Verified per-stat against the section 4 table (order resolve/valour/piety/
guard/aegis/alacrity, matching adversaries.rs:13). Every row matches
exactly: 108: 40/55/35/40/45/85 (300); 109: 65/85/30/70/50/45 (345);
110: 80/55/25/85/45/30 (320); 111: 55/60/35/50/55/65 (320);
112: 70/80/25/65/40/40 (320); 113: 45/65/30/40/40/85 (305);
114: 60/75/35/60/50/55 (335); 115: 50/45/70/55/60/40 (320);
116: 75/60/40/70/65/45 (355); 117: 85/85/45/80/60/50 (405, rides the
commander band by design per the section 4 note and the code comment at
adversaries.rs:390). Commanders: 120=400, 121=415, 122=415, 123=420 —
all inside the 390-420 band (adversaries.rs:408-476).

Kit fidelity: all 15 `[NEW]` rites from the kit sketches are present as
ADVERSARY_RITES 43..=57 with matching names and effect flavor
(Wheeling Shots multi-hit, Dust of the Plain evade, Couched Lance
high-crit, Testudo of Hides, Storm of Arrows, Volley in Turn, Cleave of
Axes, Zhupin Cast +1 priority, Harass and Fade flinch 30%, Mirage Veil,
Spear of the Hills, Comet of Naphtha Brand 30%, Wet Hide Ward,
Hold the Banner, Wheel the Line). Legacy rite references resolve to the
sketched names: 1 Measured Blow, 2 Steady Ward, 5 Litany of Stillness,
7 Zealous Cry, 15 Sundering Blow, 18 Bulwark's Stand — every learnset
entry cites an existing rite (also asserted by
`learnset_rites_exist_levels_ascending_in_range`, adversaries.rs:534).

Lore lines for 108/115/117 match the section 4 register samples verbatim;
commander lore (120-123) matches section 5's historical record.

## 3. Formula fidelity vs CONVERSION.md section 3 (exact)

`conversion_chance` (crusade-core/src/data/mod.rs:125) implements the
roll term for term:

- `obstinacy: None` → `None` (commanders 120-123 carry `obstinacy: None`;
  champions and sin-spawn likewise — verified in data and by
  `non_convertible_foes_return_none`).
- hardness = obstinacy + 15 iff `foe_level > caller_level` + sin_depth*10
  (ranging 0..30, the spec's sin_depth_bonus band).
- conviction = caller piety + 20 iff virtue-kill + min(fervor, 15).
- chance = conviction*100/(conviction+hardness), integer-floored (matches
  the project's documented "exact integer formula, no floats" discipline),
  clamped to 3..=85; total==0 guarded.
- Every worked example in the tests was re-arithmetized by hand: 72%,
  66%, 78/68 seniority pair, 72% fervor cap, 3% floor, 85% ceiling,
  84/56 tier-ordering — all correct.

## 4. Register (HISTORICAL-FOES.md 6 + DOCTRINAL-SINS.md 2)

Read every name and lore string in adversaries.rs plus the new module and
function doc comments. No slurs, no mock-Arabic, no physical/dietary
mockery, no "heathen horde" framing, no "Saracen" (not even in quotation —
permitted but not required). Names are attested office/unit terms or
recorded personal names (KilijArslan I, Nur ad-Din, Asad al-Din Shirkuh,
Saladin). The chronicle verbs are yield/stand/hold class ("stood his
ground", "the line stood", "would not be turned"); honor recorded from
both sides' chroniclers (Saladin: "he kept his safe-conducts, and at
Jaffa his courtesy crossed the lines"). The theological verdict is
doctrine-directed only, in module docs ("fight for a false god",
adversaries.rs:8) — the Peter-Ketton line of DOCTRINAL-SINS 2.2. PASS.

## 5. Seams (HISTORICAL-FOES.md 7 + CONVERSION.md 7)

- `species_by_id` / `rite_by_id` widen with `.or_else` over the adversary
  tables (data/mod.rs:20-31); ids < 108 resolve SPECIES-first exactly as
  before; `species_by_id_scans_adversaries_after_species` proves
  1..=12 and 101..=107 unchanged and the 118/119/124 gap stays empty.
- Locked tables genuinely untouched: `species.rs` main table content is
  byte-identical at HEAD except the additive `None` pairs; the
  `SPECIES.len() == 19` assertion (species.rs:338) and the rites.rs
  locked battery (42 rites, `typeless == 7` at rites.rs:697) sit in
  files/regions this diff never edits. No stale negative-id test ever
  existed for the 108/43 gap at HEAD (checked `git show HEAD:`), so the
  widened lookup breaks nothing.
- Serde: `converted` has `#[serde(default)]`, `converted_from` adds
  `skip_serializing_if = "Option::is_none"` (types.rs:278-284); the
  old-save deserialization test (`OLD_SAVE`, data/mod.rs serde_tests)
  round-trips a pre-M7 instance JSON and passes. ChampionInstance
  equality round-trip holds. Truly additive.
- GOLDEN_DIGEST: the only engine-tree changes are the mkchamp field
  additions and the constant; `engine/attack.rs`, `battle.rs` etc. are
  untouched. The digest's docstring now explains the recalibration as
  purely the additive-field serialization effect (engine/tests.rs:470-477).
  This is a legitimate recalibration, not a logic drift.
- `Typing::Valor` is appended last in the enum with the forwards-only
  serde constraint documented (types.rs:56-66).

## 6. Chart

`Typing::Valor` returns 100 in both the Virtue-attacker and Sin-attacker
match arms (chart.rs:58,75); the Typeless attacker arm already returned
100 for all defenders. New test covers every virtue and sin. Wheel
entries (`virtue_vs_virtue` tables) untouched. STAB path
(engine/attack.rs:29,112) untouched, and adversary rites are all
`RiteType::Typeless`, so no STAB lane exists or could sneak in — the
adversaries.rs rite test asserts Typeless for every entry. PASS.

## 7. Balance note

Accepted: the Sudani Shieldbearer deficit stands as documented debt per
the M1 Sabina precedent. Spot check (single-seed `crusade-sim duel`,
level 25, elite champion trio 10/9/11 — totals 360/360/370): archetypes
108-116 all lose, mini-boss 117 and commanders 120/123 win — consistent
with the 300-355 archetype totals vs the champion band. Nothing else
outside the cited 40-60% band surfaced. Caveat logged as M7-4 below.

## 8. Issues

All findings are MINOR; none block.

- **[MINOR] M7-1 — Doc self-inconsistency in the contract.**
  docs/HISTORICAL-FOES.md:210-211 says "new rites 43..=54 likewise
  additive", but section 4's kit sketches ship 15 `[NEW]` rites spanning
  43..=57, which the code implements (adversaries.rs:3, module doc, is
  correct). Fix the stale "43..=54" text in section 7.
- **[MINOR] M7-2 — Function signature drift vs CONVERSION.md 7.**
  The spec gives `conversion_chance(caller: &InstanceView, foe:
  &SpeciesBase, foe_level: u8, fervor: u8) -> u8`; no `InstanceView` type
  exists anywhere in the workspace, and the shipped fn
  (crusade-core/src/data/mod.rs:125) takes primitive parameters and
  returns `Option<u8>` (None = not rolled, which encodes the commanders
  rule more honestly than a sentinel). Semantics match section 3 exactly
  (see section 3 above). Recommend amending CONVERSION.md 7 to the shipped
  signature rather than reshaping the code.
- **[MINOR] M7-3 — Test comment arithmetic slip.**
  crusade-core/src/data/mod.rs:253: comment reads "400 / 422 = 94%" but
  the actual values are conviction 400+20+15 = 435 over total 457
  (95.2%). The asserted clamp result (85) is correct either way; the
  comment math is wrong.
- **[MINOR] M7-4 — Fuzz harness cannot reproduce the section 4 band.**
  HISTORICAL-FOES.md:87-88 makes `crusade-sim fuzz` the balance proof
  (40-60% inside the champion band), but the fuzz pool is champions-only
  (crusade-sim/src/main.rs:22-23: SPECIES_MIN..=SPECIES_MAX = 1..=12), so
  adversary win rates are not measurable with the shipped command.
  Follow-up: widen `gen_team` to draw 108..=117 (and widen the stats
  array at main.rs:199, which is indexed by species id and would
  **panic** on adversary ids if the pool alone changes), or add an
  adversary-vs-band duel mode.
- **[MINOR] M7-5 — §7 client seams deferred without a doc note.**
  CONVERSION.md section 7 (titled "M7 contract") also lists the
  post-victory dialog, `campaign.foes_defeated` and
  `campaign.converts_baptized` settings keys; none landed (only
  `SINS_DEFEATED_KEY` exists, crusade-app/src/client.rs:70). HF section 9
  slots those under HF-3/HF-4, so the deferral is defensible — but the
  "M7 contract" heading should carry a one-line note that the
  crusade-app seams land with M8/HF-3/HF-4.

## Notes

- No dead code, no commented-out blocks, no dbg!/println!/todo! in the
  diff; every new public item carries a doc comment; style matches the
  module conventions (statics, per-entry comments citing archetype ids,
  exhaustive match-arm updates in the two display helpers).
- PLAN.md / docs/HISTORICAL-FOES.md edits were committed to master by the
  orchestrator mid-review (commit ebf0db0); content identical to what was
  reviewed (post-commit diff empty).

**VERDICT: SHIP** — 0 CRITICAL, 0 MAJOR, 5 MINOR.

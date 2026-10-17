---
title: M7b Review — Conversion UI + Codex (crusade-app)
summary: >
  Review of the M7b diff: Call to Repentance client flow, Roll of Worthy
  Adversaries and Testimonies codex books, Firstfruits/Mercy economy.
  Verdict: SHIP — 0 high, 0 medium, 2 low, 4 nits.
tags: [review, m7b, conversion, crusade-app]
date: 2026-10-16
status: review
---

# M7B-REVIEW — conversion UI + codex

Scope: `git diff` only — crusade-app (conversion.rs new; client.rs,
view.rs, pvp.rs, bevy_shell/screens.rs, lib.rs edits) plus PLAN.md status
lines. crusade-core/crusade-data untouched in the diff (correct: items
are free-form additive store keys; instance seams landed in 4fa9137).
The art/MANIFEST.json churn in the same working tree belongs to the M7a
lane and is out of scope here.

Contracts checked: docs/CONVERSION.md sections 2-8,
docs/HISTORICAL-FOES.md section 6, PLAN.md section 3, core seams at
4fa9137.

## Verification results

- `cargo test -p crusade-app --lib`: 38 passed, 0 failed (matches the
  worker's claim of 38 lib tests; +6 are the new conversion_tests).
- `cargo test --workspace`: all green (98 core incl. the serde/serde-default
  save tests, all other packages green; no new failures).
- `cargo check -p crusade-app --target wasm32-unknown-unknown`: clean.
- crusade-app compiles warning-free; the unused-import/variable warnings
  in the build are pre-existing `kirk`/cdk-lane noise, none in the diff.

## Worklist findings

### 1-2. Formula wiring — PASS

`decide_call(chance, roll) = roll <= chance` with roll = splitmix64 %
100 + 1 in 1..=100 (deterministic, pinned by test at the 3% floor and 85%
ceiling). `scan_defeated_foes` maps `conversion_chance(caller_piety,
caller_level, false, 0, sp, foe_level)` correctly. The v1 deviations —
no final-blow attribution (virtue-kill bonus forfeited), unwired fervor
(0), caller = standing MY_SIDE champion — are honestly commented in both
`PendingCall` and `scan_defeated_foes`. The shown odds equal the odds the
sealed draw resolves against (same captured `chance_pct`). 

### 3. Bit packing — PASS

`foe_bit`: 108..=117 -> 0..=9, 120..=123 -> 12..=15 exactly, all other
ids None (pinned by test). Masks are u32 per HF 6.5; key strings
(`campaign.foes_defeated` / `campaign.converts_baptized`) are disjoint
from `SINS_DEFEATED_KEY` (`campaign.sins_defeated`, u8). Additive: absent
keys degrade to 0 (sealed books), new ViewSnapshot fields are all
`#[serde(default)]`, instance `converted`/`converted_from` have serde
defaults (core serde test present). Serde round-trip through
save_bytes/load_bytes is tested at the client level.

### 4. Economy — PASS

Firstfruits x8 (`firstfruits-shard`) on baptism only; Mercy x3
(`mercy-shard`) on refusal or release-without-call exactly once; no
path grants both. `RosterStore::add_item` accumulates (saturating), so
ids are additive with no crusade-data registry change needed. Firstfruits
> Mercy > 0 honors CONVERSION 4/5 ("the economy pays for mercy, not
slaughter"; contrition bounties are 1..=7, so Firstfruits at 8 tops the
Contrition band as required).

### 5. Register gate — PASS (with nits below)

All 10 testimonies read in full: first-person, grief-sober, melodrama-free,
no coercion verbs, no slurs, no "Saracen" (quoted or otherwise), the
fallen "yield/stand/hold". Apt per-archetype scripture beats (Thomas for
the Shieldbearer, Saul/Paul for 117 — matching the name table comment).
The log lines and the new screen copy use CONVERSION 8's exact frame
("his soul was commended unto God", "would not turn"). The banned-terms
test sweeps baptismal names, testimonies, and the adversary lore of all
14 ids.

### 6. Bevy shell — PASS (with one latent low below)

`adversary_roll_panel` / `testimony_panel` mirror the
Manuscript-of-Sins gating: phase gate in `snapshot()`, mask gate in
`client.rs` card builders, and a doubled per-row render gate — sealed
rows carry empty text and render only "{name} - Unmet" / "- Not yet
called". Snapshot defaults in pvp.rs seal all three new fields
(foes/testimonies empty, conversion None); net duels never convert.
`AcknowledgeResult` matches `Phase::BattleOver` only — from
ConversionChoice it returns an error, and queue drain (not ack) is the
sole path out of ConversionChoice, so a pending queue cannot be acked
away mid-decision from the choice phase.

### 7. Sloppiness hunt

No dbg!/todo!/unimplemented!; unwraps are test-only; every `expect` in
product code is static-table-covered with a justification message;
bad settings values degrade to sealed masks, never panic.

## Issues

- [LOW] `client.rs::on_battle_end`, campaign branch: pending Calls are
  consulted only on the practice/no-campaign path. A won campaign node
  sets `Phase::CampaignRest` / `Phase::BattleOver` unconditionally, so a
  queued Call would be stranded (and silently cleared by the BattleOver
  ack). Unreachable today — campaign foes are sin bosses only (ids earn
  no `foe_bit`, queue stays empty) — but the doc comment on
  `on_battle_end` claims "a non-empty queue takes the phase to
  ConversionChoice before BattleOver" for won battles generally. Insert
  the same `pending_calls.is_empty()` check on the campaign exit, or
  scope the comment, before any campaign node ever fields adversaries.
- [LOW] Determinism seam drift: CONVERSION 3 and the core doc on
  `conversion_chance` say the draw "reuses the RngFacade" / the kirk
  commit-reveal per-battle seed discipline. The implementation uses a
  bespoke salt `(foes_defeated_mask << 32) | calls_issued` hashed by
  splitmix64. This is deterministic, replica-stable, and adequate for the
  single-player lane (PvP never converts), and `conversion_roll`'s own
  doc presents it as the client's discipline — but the stale core
  comment and the contract line now overclaim. Amend CONVERSION 3 /
  core docs, or wire the RngFacade.
- [NIT] CONVERSION 5's hagiography statistics call for "calls issued /
  converts / souls released"; only `campaign.calls_issued` and the
  converts mask exist — souls released are not counted anywhere.
- [NIT] view.rs `ViewSnapshot::sins`: the four-line doc comment is
  duplicated verbatim (compiles as concatenated docs; delete one copy).
- [NIT] The register-gate test does not sweep the new log lines and
  screen strings (all manually verified compliant); folding them into
  the corpus would lock it in.
- [NIT] Testimony 111 "that Water-priest's grief" — stray capitalization
  mid-sentence; and the table test's failure message says "2-3 beats"
  while the bound is 2..=4.

## Verdict

**SHIP.** Counts: 0 high, 0 medium, 2 low, 4 nits. The two lows are a
latent campaign-routing gap (unreachable with shipped data) and a
doc-drift on the draw seam — both cheap follow-ups, neither blocks the
milestone.

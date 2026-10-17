---
title: "Core seam review: typing_override, adversaries 124/125, Hattin gate rewiring"
summary: >
  Review of the uncommitted crusade-core seam (ChampionInstance.typing_override
  routed through a new Battle::active_typing; adversaries 124 Gokbori / 125 Taki
  al-Din Umar; overworld gates 19/23 re-pointed) plus the five orchestrator
  fixture one-liners. Core seam is clean; the gate rewiring silently orphans two
  new commanders from the app-side Roll of Worthy Adversaries bitfield.
tags: [review, crusade-core, conversion, historical-foes, overworld, seam]
status: review
date: 2026-10-16
---

# Core seam review (2026-10-16)

## Scope

- GLM worker: `ChampionInstance.typing_override` (types.rs), `Battle::active_typing`
  engine seam (attack.rs), adversaries 124/125 (adversaries.rs), gates 19/23
  (overworld.rs), guard-test moves, serde tests.
- Orchestrator (in-scope-lite): mechanical `typing_override: None` in
  crusade-data/roster.rs, crusade-bridge adapter_tests, crusade-net duel /
  nostr_duel / reorder fixtures.

## Verification performed

- `cargo test -p crusade-core` — **102 passed, 0 failed** (claim confirmed).
- `cargo clippy -p crusade-core --all-targets` — zero warnings (the only stderr
  line is an unrelated `~/.cargo` registry permission error from cache
  auto-clean, exit 0).
- Trinity-of-care audit (self-run): every `typing`/`species_by_id` read under
  `crusade-core/src/engine/`:
  - attack.rs:41 `stab_pct` → `active_typing` ✓ (STAB reads override)
  - attack.rs:100 defender effectiveness in `apply_turn` → `active_typing` ✓
    (incoming effectiveness; outgoing is the same `data::effectiveness` call)
  - attack.rs:37 the species fallback inside `active_typing` itself ✓
  - engine/mod.rs:183 (`Side::computed`) and :205 (`Battle::new` validation)
    are stat/validation reads — correctly NOT routed through the override;
    `compute_stats` (data/mod.rs:69) does not touch typing at all ✓
  - engine/resolve.rs, engine/trail.rs: no typing reads ✓
  - Species-level reads (data/ encounter tables, witnesses, chart) preserved ✓
- Serde additivity: `#[serde(default, skip_serializing_if = "Option::is_none")]`
  on the new field; `old_instance_save_deserializes_with_defaults` (pre-M7 JSON
  literal) now asserts `typing_override == None` and passes; round-trip test
  extended with a `Some` value ✓
- Golden digest: `golden_digest` (constant `0xb96cc6525dc772a4`, fixtures all
  `typing_override: None`) green — rationale holds: `skip_serializing_if`
  keeps the serialized Battle blob byte-identical and `active_typing(None)`
  resolves to exactly the species value read before. No recalibration needed ✓
- Guard tests correctly moved: gap probe 124 → 126 in
  `species_by_id_scans_adversaries_after_species`; `non_convertible_foes_return_none`
  (commanders never convert) extended to 124/125 ✓
- Card B values re-derived by hand: 124 Gokbori 80+80+45+70+60+70 = **405**;
  125 Taki al-Din Umar 85+80+50+75+65+45 = **400**; both `Typing::Valor`,
  `obstinacy: None`, `sin_depth: Some(3)`; kits (51/43/57/56 and 45/53/47/57)
  all within rites 43..=57 and covered by the pre-existing
  `learnset_rites_exist_levels_ascending_in_range` ✓
- The five orchestrator one-liners read line-by-line: each is a struct-literal
  `typing_override: None` in test fixtures; inert ✓

## New tests read skeptically (non-vacuous?)

- `typing_override_resolves_incoming_effectiveness`: end-to-end via
  `apply_turn` → `BattleEvent::Damage` with observed `effectiveness_pct`; plain
  lane asserts non-empty and all-100, humbled lane asserts all-50 (the wheel's
  self-resist). Real assertion, not vacuous. **Nit:** the `humbled` vec has no
  `!is_empty()` assert — an empty soundness hole closed only by mirrored
  construction with `plain`.
- `typing_override_grants_stab_and_outgoing_damage`: direct `stab_pct` pin
  (100 → 150), non-leak asserted on side 1, then an end-to-end replay under the
  same seed asserting (a) identical lengths, (b) equal effectiveness (STAB is
  the only shifted term), (c) every humbled damage strictly greater. Not
  vacuous.
- `hattin_wing_commanders_match_spec` and
  `hattin_wing_gates_reference_the_wing_commanders` pin names, totals, typing,
  never-convertion, kit lane, and the 19/23 → 124/125 mapping directly.

## Findings

### MEDIUM — gates 19/23 now point at commanders the app cannot record

`crusade-app/src/conversion.rs` still pins `COMMANDER_IDS = 120..=123`;
`foe_bit(124)`/`foe_bit(125)` return `None`, so at
`crusade-app/src/client.rs:1702` a won gate battle over Gokbori or Taki al-Din
is `continue`d past the `campaign.foes_defeated` mask write, and
`foe_cards` (client.rs:2082) — built from `CONVERTIBLE_IDS.chain(COMMANDER_IDS)`
— never renders either man on the Roll of Worthy Adversaries; his lore is
unreachable in-game. This is a regression introduced by this diff: before the
rewire, gates 19/23 pointed at 122/121, which are recorded. The
`roll_and_testimonies_gate_by_phase_and_masks` test pins `len() == 14`, which
now pins the gap green. Fix is one worker card: widen `COMMANDER_IDS` to
120..=125 (bit layout `id - 120 + 12` extends to bits 16/17 for free in the
u32), update the test and the `14 foe ids` / FoeCard doc comments.

### LOW — Hattin wing assignment inverted vs. the attested order of battle

Lore says Gokbori "rode the Sultan's right wing" and Taki al-Din's was "the
Sultan's left wing". The standard OOB at Hattin (Baha al-Din; reproduced in
the Wikipedia article the project's own HISTORICAL-FOES.md:264 cites) has
**Gökböri on the left and Taqi al-Din Umar on the right**. The card prescribed
the inversion and the implementation matched the card, but the codebase's
register section binds to attestation — the lore strings are a one-line swap
each. Register itself is fine (see below).

### NIT — AI / sim re-derive instance typing outside the seam

`crusade-app/src/policy.rs:24` (`greedy_action`) and
`crusade-sim/src/main.rs:163` (`policy_action`) compute the opponent's active
typing from the species table, bypassing `typing_override`. Battle outcomes
stay correct (the engine honors the override); only the AI's
power×effectiveness scoring misprices a converted opponent. No divergence
today (no converted foe can stand on side 1 of a practice/sim battle), but the
seam should expose a public accessor before any cross-crate reader needs it.

### NIT — stale comments after the id-band growth

`crusade-app/src/client.rs:609` and `crusade-app/src/view.rs:251` still say
"14 foe ids" / "120..=123"; adversaries.rs module docs were updated, these
were not (they belong to the fix for the MEDIUM finding).

## Register check (docs/HISTORICAL-FOES.md section 6)

- Gokbori: "He rode the Sultan's right wing at Hattin, folding our flank in all
  day long; shrewd in the spending of men, and grave with it." — grave,
  honors the man's craft, own-name usage, no mockery ✓
- Taki al-Din Umar: "The Sultan's left wing barred the springs of Hattin from
  us; from the ninth hour the thirst did his work, and the day went with the
  water." — chronicle voice, attribution to circumstance not ethnicity,
  no mockery ✓
- Both pass §6 rules 1–3 except the attestation wrinkle in the LOW finding.

## Verdict

**FIX** — the crusade-core seam itself is SHIP-clean (trinity-of-care holds,
serde additive, digest rationale verified, tests pin what they claim), but the
gate rewiring orphans the two new commanders from the app's defeat-mask / Roll
of Worthy Adversaries (MEDIUM), and the lore inversion (LOW) rides the same
content line and is cheapest to fix now.

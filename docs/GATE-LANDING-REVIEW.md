---
title: "Gate-landing review: GATE-CONTENT placement map, chronicle lines, boss-offset plumbing"
summary: >
  Review of the uncommitted gate-landing diff (crusade-core/src/data/overworld.rs,
  crusade-app/src/client.rs, docs/OVERWORLD.md) against docs/GATE-CONTENT.md.
  The placement map pins row-for-row, all fifteen authored chronicle lines are
  character-verbatim, the serde default round-trips a genuine legacy blob, and
  every flagged deviation holds up on close reading. Verdict: SHIP.
tags: [review, crusade-core, crusade-app, trail, gates, gate-content]
status: review
date: 2026-10-17
---

# Gate-landing review (2026-10-17)

## Scope

- `git diff` (working tree): crusade-core/src/data/overworld.rs,
  crusade-app/src/client.rs, docs/OVERWORLD.md. Exactly the card's three
  files. (`docs/RECOVERY.md` and `tools/__pycache__/` sit untracked in the
  tree but are NOT part of this diff and were not reviewed; flag to the
  owner in case the former was meant to ride along.)
- Card: implement docs/GATE-CONTENT.md — sections 0 (placement map) and
  1-5 (chronicle text), plus the `boss_level_offset` plumbing its
  orchestrator note 3 prescribes.

## Verification performed

- **Map fidelity, row for row (GATE-CONTENT section 0):** every landed
  `ScreenDef` gate row matches the table: 9:101/120/+2 (SHIPPED reference
  row; only the field was defaulted in), 13:102/121/+2, 16:103/122/+2,
  19:104/124/+2, 20:105/125/**+3** (the 123 -> 125 commander move of
  orchestrator note 1 is landed; Saladin no longer stands at 20),
  25:108/123/**+3**; 23/24 carry `scripted_gate: None` exactly as
  orchestrator note 2 prescribes (stand-ins cleared, not restaffed).
  The escort arms in `gate_escort_mix` match the escorts column exactly:
  101 [108,109], 102 [114,116], 103 [110,116], 104 [116,114],
  105 [116,117], 108 [116,117]. The core test
  `gate_placement_matches_the_content_map` pins the ENTIRE map — all eight
  screens including the screen-9 reference row and both cleared rows as
  `None` — not a subset. `hattin_wing_gates_reference_the_wing_commanders`
  was updated coherently (19->124, 20->125, docstring reworded).
- **Text verbatim (GATE-CONTENT sections 1-5):** extracted all 15
  blockquoted texts from the content doc programmatically, joined the
  wrapped lines, and substring-checked each against client.rs: 15/15
  character-identical (five pre-battle, five victory, five defeat).
  Gate 101 keeps its shipped intro/victory via `gate_text`'s
  `_ => None` fallback, as the content doc and the code comment agree.
  Defeat lines wire through `gate_defeat_phrase` into the Book's
  pre-rendered latest page: `"{name}, day {day} — {where-phrase}"`, the
  phrase itself opening "fell in battle upon the pilgrim road, ..." —
  the `where_fell` family, correct shape.
- **Serde/legacy:** `#[serde(default = "default_boss_level_offset")]` with
  the default fn returning 2. The legacy test deserializes a concrete
  JSON blob `{"battle_ref":101,"commander_id":120}` (no offset field) and
  asserts the parsed struct equals the full struct at offset 2 — a real
  old-blob round-trip, not a vacuous type check.
- **Deviation 4a (seed fix):** test-only. The pre-mint lives in
  `trail_tests::fixture_store` (`set_setting(TRAIL_SEED_KEY, 0x5E*32)`);
  the production mint in `pilgrim_seed`/embark is untouched by the diff.
  The diagnosis is plausible by code reading: `pilgrim_seed` FNV-mixes
  sorted roster ids, `add_champion` assigns uuid v4 keys (documented at
  bevy_shell/ui_shots.rs:814), so without a pre-minted key every fixture
  run would walk a different road — including which day events fire on
  the gate-entry day (a ConvertOpportunity there pre-empts the gate; see
  4b). Deterministic tests per door is the correct fix shape.
- **Deviation 4b (Convert pre-empts Gate):** confirmed at client.rs:1306
  — the FollowUp selection checks `out.convert_ref` before
  `dest.scripted_gate`, so a willing hearer on the gate-entry day skips
  the funnel for that entry. The diff did not change this ordering (only
  the Gate tuple arity). Pre-existing behavior; the tests accommodate it
  by stepping back a screen and re-entering. Owner decision pending;
  noted for the record, not held against this card.
- **Deviation 4c (defeat phrase keyed screen+cause):** NOT a live lie in
  the Book. Every gate screen (9/13/16/19/20/25) carries
  `encounter_zone: None` and `props: &[]`; `FollowUp::Gate` pre-empts
  `FollowUp::Ambush` for that destination; trail events resolve at the
  DESTINATION screen (trail.rs); `FallenCause::SlainInBattle` pages are
  written only from `on_trail_battle_end`'s defeat branch at the current
  screen. So the only reachable SlainInBattle page at a gate screen is
  the gate battle itself, and re-entry refires the gate (pre-existing),
  which keeps even that page truthful. Screens 23/24 fall through to the
  shipped screen-and-cause row. The Kilij negative pin is covered by a
  test (`a_gate_defeat_pages_the_book_with_the_maps_where_phrase`
  asserts the shipped row at screen 9).
- **Deviation 4d (106/107 escort arms removed):** clean. Grep: no
  battle_ref 106/107 anywhere — `TRAIL_EVENTS` battle verbs reference
  only 201/203; every `106`/`107` hit in the tree is the sin-boss
  SPECIES id (campaign gauntlet, sprite keys), an unrelated number space.
- **Level math:** escorts keep the shipped
  `lv.saturating_sub(2).clamp(5, MAX_LEVEL)`; boss is
  `lv.saturating_add(boss_offset).min(MAX_LEVEL)` — offset from data,
  saturated, matching the content doc's "(capped at MAX_LEVEL by the
  shipped clamp)" note.
- **Data surface:** the diff touches only screen rows (+ the BattleGate
  struct field) in core, and text tables/routing in the app. No roster,
  stat, rite, encounter-mix, or trail-event edits anywhere.
- **Orchestrator-verified (trust, not rerun):** workspace tests green
  (373 passed/0 failed/4 ignored) and register_lint 0 findings. The new
  prose comments are chronicle-firm and ASCII, consistent with the
  register lane content this file carries.

## Findings

### LOW — the authored chronicle lines are pinned by review, not by a test

The 15 strings are character-verbatim TODAY (verified by paste-check),
but the client tests only assert short fragments
(`"spears in good order..."`, etc.). A future caret-touch — a rewrapped
hyphen, a "fixed" archaism — would pass the suite and desync from
GATE-CONTENT silently. Optional hardening: a test pinning the full
verbatim strings (as the core map pin does for the placement table).
Not a ship blocker; the content doc remaining the diff-reference is an
acceptable discipline.

### NIT — screen+cause defeat keying is reachability-safe today but fragile to future content

The phrase lookup reasons about possibility ("a gate screen") rather
than about the actual battle that produced the page (`trail_gate` is
already gone by view time by design). Today's invariants (no zones or
props on gate screens, gate pre-empts ambush at routing) hold, but a
future content pass that, e.g., staffs a ChoiceEncounter on the old
stand-in screens (23/24) or lifts the Gate/Ambush preemption would
silently mislabel pages written by a different fight on a gate screen.
If the where_fell family ever gains new members on gate screens, re-key
against the recorded cause context rather than the screen. A one-line
comment already gestures at this; leave it as-is unless those doors open.

## Register check (docs/HISTORICAL-FOES.md section 6)

All new prose came verbatim from the register-reviewed content doc;
new code comments are chronicle-firm, material, ASCII. Pass.

## Verdict

**SHIP** — the placement map lands row-for-row with a full core pin
(including the cleared stand-ins and the +3 escalations), all fifteen
authored lines are character-verbatim with the defeat phrases wired
through the `where_fell` family, the serde legacy path is genuinely
exercised, and level math keeps the shipped clamps. The flagged
deviations all resolve: the seed fix is test-only with a plausible
diagnosis, the Convert-preempts-Gate ordering is pre-existing and
unchanged (owner decision pending, noted), the defeat-phrase keying
cannot produce a false page under any reachably-constructed battle, and
the 106/107 arm removal has no referents. The LOW (verbatim text pins)
and NIT (keying fragility to future content) are polish-lane items, not
ship blockers. Convert-preempts-Gate remains the one open owner
decision from the worker's report; it is orthogonal to this landing and
should be tracked where trail-routing decisions live.

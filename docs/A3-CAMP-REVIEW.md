---
title: "A3 battle-depth + Make Camp review: the van, the camp dials, and the shimmed sims"
summary: >
  Review of the uncommitted A3 (docs/TRAIL-BATTLE-DEPTH.md) + Make Camp
  (docs/RECOVERY.md / RECOVERY-SIM.md) diff: vanguard cache, defeat
  routing, camp engine + client wiring, props, save/view serde tails.
  Every load-bearing number is spec-faithful and pinned by client tests;
  both ignored sim sweeps reproduce every doc-claimed row verbatim under
  my own release rerun; the mend-cap repair is correct. Two LOWs ride
  along: both sims fight camp ambushes with the 201 brigand mix where the
  spec and the shipped code use the screen's zone mix (immaterial at 10%
  x ~20 camps, but the mirror lies about the ambush lane), and no
  legacy-load test pins that pre-vanguard v2 blobs still load.
tags: [review, crusade-app, crusade-core, trail, vanguard, make-camp, sims]
status: review
date: 2026-10-17
---

# A3 + Make Camp review (2026-10-17)

## Scope

- `git diff` (working tree): crusade-core/src/{data/overworld.rs,
  engine/trail.rs}, crusade-app/src/{client.rs, persist.rs, view.rs,
  bevy_shell/overworld.rs, bevy_shell/screens.rs},
  docs/{TRAIL-BATTLE-DEPTH.md, RECOVERY-SIM.md}; untracked
  crusade-app/tests/camp_policy_sim.rs. Exactly the card's lane.
- Specs under review: docs/TRAIL-BATTLE-DEPTH.md section 3 (A3) and
  docs/RECOVERY.md section 5 (Candidate A), both owner-ruled and
  sim-validated.

## Verification performed

- **Spec fidelity, line by line.** Slots are `min(VANGUARD_MAX=2, living
  non-champion men)` (sync_vanguard's retain-then-fill loop; depleted
  file and lone captain both client-tested). Minting is
  `captain_level.saturating_sub(2).clamp(5, MAX_LEVEL)`, species = the
  mantle's (test pins level 18 under a level-20 captain and the Order's
  breed). Gates remain single file: `enter_trail_battle` skips sync and
  party extension when `trail_gate` is set (client-tested, no muster
  line at battles of record). Camp dials: engine consts 1/4 mend, 3/4
  cap, +5 morale, 10% ambush, double upkeep, day+1, no trail-event roll —
  all in `trail::make_camp` with boundary guard tests at resolve
  0 / 40 / 3/4-1 / 3/4 / max-2 / max-1. Camp props at screens
  {3, 8, 12, 15, 18, 21} exactly (3 pre-existed as scenery; five added).
- **Defeat routing.** The van-death loop reads the whole post-battle
  line (`[mantle, van...]`, felled stand present at 0 resolve), writes
  pages and strikes the roster on BOTH outcomes, and runs before the
  mantle routing; the captain's own page, `champion_falls`, and the
  level banding are byte-for-byte the shipped path. The hole-hunt asked
  for: (a) a wounded-but-alive van man carries wounds across days —
  yes, his body is written back at battle end; that is the spec
  ("wounds persist between battles"), and zeal is field-refilled on
  victory as before; (b) a trail-event death between battles is struck
  before the next mint — sync's retain plus the view's living-filter
  both check `alive && !champion`; (c) double pages — none: the settle
  loop is the only writer of the van cause, felled men leave the cache
  immediately, and the risen man's old van body retires on the
  champion flag (client test pins the re-band and the strike). The
  Book cause maps both directions in persist.rs (`as_str` /
  `from_str`), and the two battle causes differ past the shared
  prefix, so no parse collision.
- **Camp endgame discipline.** The `camp_mend` ephemeral has the same
  one-battle lifetime as `trail_gate`: set before the ambush entry,
  taken at battle end, applied only on a kept dawn to the standing
  mantle. A crash mid-watch loses the mend but keeps the spent day —
  the flagged precedent; I agree with it (the alternative is either a
  persisted mid-battle fact or a free mend on reload, both worse). The
  cap repair `(r+mend).min(cap).max(r)` is right and the engine tests
  pin it: camping at max-1 is a no-op, never a wound.
- **Determinism / chronicle.** The camp day stream is
  `trail_day_rng(seed, pre-increment day)` — travel's exact recipe; days
  are spent exclusively (a camped day can never also be a marched day),
  so no alias with a march on "day N". The ambush battle seeds on the
  post-increment day, mirroring a march battle's `trail_battle_seed`
  timing. `BattleRng` is per-call; nothing in the diff touches
  crusade-core golden-path RNG signatures. Log lines match the spec
  samples verbatim modulo format placeholders: quiet camp, capped mend,
  "A cry from the watch — the camp is fallen upon in the dark!", "The
  night is kept. By dawn the wounded are tended.", the lean-stores
  refusal, the muster lines, the lone-captain line, the van-death line.
- **View/UI.** `TrailView.can_camp` / `mantle_resolve` / `vanguard` are
  serde(default) tails; the pane shows wounds and the van; the walker
  queues `MakeCamp` only in engine mode behind `mirror.can_camp`
  (identical to the ChoiceEncounter precedent; the demo-stub camp verb
  at the else arm predates this diff). `cargo check -p crusade-app
  --target wasm32-unknown-unknown`: clean.
- **Sim reruns (self, release).** `trail_depth_sim` reproduces every
  post-implementation claim verbatim: A3 = 85.7% road (77 battles),
  1.9% gates (261), 25/48 extinct at median day 17.0, 60% reach,
  median remnant 5.0, 100% >=5-page arrivals, 38 van pages; A3+Camp =
  86.2% / 1.9% / 26-48@18.0 / 60% / 5.0 / 100% / 19 camps / 42 pages.
  `camp_policy_sim` reproduces every RECOVERY-SIM section 1-2 row
  verbatim: soak 10.0 / 48-48 / 0% reach; rational 17.0 (25/48), 60%;
  always 18.0 (27/48), 58%, 30 camps; prudent 18.0 (27/48), 58%, 7.0
  remnant, 20 camps; knob lane +1029 supplies and nothing else; both
  probes 17.0 / 27% / 60%.
- **Sim-vs-client mirror audit (the docs admit the harness is
  self-contained).** Verified cell-for-cell: `trail_day_rng`,
  `trail_battle_seed`, `trail_foe_mix` (all six zone rows + 201/203
  arms), `gate_escort_mix`, the `practice_champion` learnset recipe,
  `sync_vanguard` discipline, `settle_mantle_fallen` order (captain's
  page, then THE MANTLE). One real divergence remains — see LOW-1.
- **Orchestrator-verified (trust, not rerun):** workspace tests
  383 passed / 0 failed; register_lint 0 findings.

## Findings

### LOW — both sims fight camp ambushes with the wrong foe mix

In both `trail_depth_sim.rs:camp_day` and
`camp_policy_sim.rs:Jour::camp`, the night-ambush mix is
`trail_foe_mix(201, zone)` — which matches the `201 => &[112, 113]`
arm unconditionally, i.e. brigands at every camp screen. RECOVERY.md
section 5 says the ambush payload is the screen's encounter-zone id
(None -> 201), and the shipped client does exactly that
(`trail_make_camp`: `Some(z) => trail_foe_mix(0, Some(z))`,
`None => trail_foe_mix(201, None)`). Consequences: at screens
8/12/15/18/21 the shipped night watch fights the zone table — up to
`[114, 116, 117]` at screen 18 — while the sweeps that "re-ran against
the landed tree" fought brigands. Measured impact is immaterial
today: prudent camping fires ~19-21 camps across 48 runs and the
ambush bit is 10%, so ~2 ambush battles exist in the entire sweep, and
the doc's rows reproduce whether or not the lane is mirrored (they
did). But the harness claims to mirror the shipped camp and does not;
the fix is one line per sim (zone mix, None -> 201) plus a re-run to
confirm no row moves (expected: none). Also ride along: `camp_day` in
trail_depth_sim keys its ambush rng on the POST-increment day where
the client keys PRE-increment (mirroring travel); both are
alias-free, but the mirror should match.

### LOW — no legacy-load test for pre-vanguard v2 blobs

`TrailSave.vanguard` is a `#[serde(default)]` tail, no
`deny_unknown_fields` anywhere in persist.rs, `TRAIL_SAVE_VERSION`
stays 2 — so a v2 document written before the van genuinely loads
(the missing tail deserializes to an empty vec, and the next road
entry's sync re-mints from the roster anyway; the doc comment says as
much). True by construction, but unpinned: the persist tests only
round-trip captures built by the new signature. A five-line test
deserializing a hand-written v2 JSON without the `vanguard` key (the
suite already hand-writes JSON for the version-1 probe) would make the
discipline load-bearing against a future field reorder or a
`deny_unknown_fields` accident.

### NIT — Pyrrhic camp-ambush win prints a chronicle-false line

If a night-ambush battle is won with the mantle felled (the van
finishes it), `on_trail_battle_end` still applies `camp_mend` to the
stale pre-battle mantle body (immediately discarded by succession) and
pushes "The night is kept. By dawn the wounded are tended." over the
captain's corpse. Harmless — the mend lands on a dead file — but the
line lies once. Gate the mend and the line on `mantle_stands`.

### NIT — one A3 sample line not shipped verbatim

TRAIL-BATTLE-DEPTH section 3 lists "the watch cries; the van stands
to" as a register-kept sample; the shipped camp night-watch is the
RECOVERY.md ambush intro plus the muster line that follows it
("...bears the mantle; with him stand ..."), which carries the same
fact. Accept; worth knowing the sample list is aspirational there.

## Register check

New strings are chronicle-firm and unit-attributed ("The van held, and
{name} did not rise with it."; "The balms have done what
field-dressing can; the rest must knit of itself."); the Book cause is
grave, scene-true, and maps both directions. The quiet-camp no-mend
line ("...Day {n} passes quietly.") is new but in-register. The
bevy-walker prompt trail-dressing ("a day, double rations, and the
watches posted") matches the spec's sample. Pass.

## Verdict

**SHIP.** Every load-bearing number of both specs is implemented as
ruled and pinned by green client tests: the van's slots/mint/gate
exclusion/defeat routing both ways, the camp's mend curve with the
no-bleed cap, the double-upkeep day, the 10% unmended night watch with
the mend riding a kept dawn, and the prop placements at
{3, 8, 12, 15, 18, 21}. Both ignored sweeps reproduce every claimed
row verbatim under my own rerun, and the persistence discipline is
genuine tail-serde with the cause mapping reversible in both
directions. The two LOWs (sim ambush-mix mirror; missing legacy-load
pin) and the two NITs ride along on the next polish pass — none of
them touch the shipped behavior's correctness.

# TRAIL-BATTLE-DEPTH — the fighting line of the pilgrimage

Status: SHIPPED post-implementation, 2026-10-17. A3 is wired in
crusade-app (van cache on `TrailCampaign`, `sync_vanguard`, road-battle
party `[mantle, van...]`, gates captain-alone, van defeat routing with
the Book cause "fell in battle upon the pilgrim road, shoulder to
shoulder in the van", serde-tail save/view additions — no DTO version
bump), and Make Camp rides on top per docs/RECOVERY.md section 5.

Post-implementation verification: the sweep harness re-ran against the
landed tree (`cargo test -p crusade-app --test trail_depth_sim --release
-- --ignored --nocapture`) and reproduces the table below exactly — A3:
85.7% road won (77 battles), 1.9% gates (261), extinction 25/48 at
median day 17.0, 60% reach 25 ≤d40, median arrival remnant 5.0, 100% of
arrivals with ≥5 Book pages, 38 vanguard pages; A3+Camp: 85.0% / 1.9% /
58% / 5.0 / 100% / 20 camps / 41 pages.[^1] Every headline metric holds
within seed noise of the simmed acceptance profile (the harness is
self-contained; the client wiring mirrors it, and the routing seams are
pinned by the new client tests: minting at captain-2, slot arithmetic,
defeat routing both ways, gates never field the van).

[^1]: Review-pass harness repair (2026-10-17): `camp_day` previously
      fought night ambushes with the brigand table `trail_foe_mix(201,
      zone)` and keyed its day rng on the post-increment day. It now
      fights the screen's encounter-zone mix (`Some(z) -> zone table,
      None -> 201`) on the pre-increment day, cell-for-cell with client.rs
      `trail_make_camp`. The re-rerun moves only the A3+Camp row:
      86.2→85.0 road win, 60→58% reach, 19→20 camps, 42→41 van pages
      (a different seed on the ambush roll re-rolls the ~2 ambush battles
      in the sweep; nothing that gates acceptance changed). All other
      rows are byte-identical.

## The pre-implementation measurement report (2026-10-17)

The sims live in `crusade-app/tests/trail_depth_sim.rs`, every
test `#[ignore]`d. Run: `cargo test -p crusade-app --test
trail_depth_sim -- --ignored --nocapture`. 48 seeded pilgrimages per
configuration (the camp-soak seed set and journey loop, rational
posture: no elective grinding, no seeded falls), REAL battles through
the crusade-core engine, the client's own foe mixes and gate level
rules (escorts at captain-2, commanders at captain+offset), the storage
soak's fight recipe (two Valour-stack openers, then best damaging
rite), full forced-switch handling on both sides so multi-body parties
fight honestly. Defeats route through the real THE MANTLE succession;
the Book of the Fallen is written by the sim exactly as client.rs
writes it.

## Why this card exists

docs/RECOVERY-SIM.md §4: road battles are fought single-file against
2-3 near-peer foes; ~90% are lost (measured again here: 10.4% won), a
loss is a dead captain, and no recovery dial can rescue the win target.
The owner ruled on 2026-10-17: battle depth first, then Make Camp.
This document is that card.

Register note, binding on whatever ships: every chronicler of the
expedition — and the rule's own doctrine of NAMED men — writes of a
company that fights shoulder to shoulder, not a lone duellist
battering down three men in a row. A candidate that puts the named men
on the field is MORE faithful to the source register than the shipped
single file. The single file was a v1 scaffolding, not a canon fact.

## 1. The comparative tables (48 runs each)

| candidate | road win % | gate win % | extinct | med extinct day | reach 25 ≤d40 | med living @arrival | arrivals ≥5 Book pp | van pages |
|---|---|---|---|---|---|---|---|---|
| CURRENT (shipped) | 10.4 | 1.9 | 25/48 | 17.0 | 60% | 5.0 | 100% | 0 |
| A van-of-2 @cap-2, van at gates | 88.6 | **55.6** | 40/48 | 17.0 | 38% | 3.0 | 100% | 294 |
| A2 van-of-2 @cap-0, van at gates | 90.7 | **66.7** | 38/48 | 17.0 | 46% | 3.0 | 100% | 256 |
| **A3 van-of-2 @cap-2, gates captain-alone (RECOMMENDED)** | 85.7 | 1.9 | 25/48 | 17.0 | 60% | 5.0 | 100% | 38 |
| A4 van-of-2 @cap-0, gates captain-alone | 93.5 | 1.9 | 24/48 | 17.0 | 60% | 5.0 | 100% | 26 |
| A-rej swoon (felled van recover, no pages) | 96.2 | **89.3** | 3/48 | 14.0 | 94% | 14.0 | **76%** | 0 |
| B asymmetric tables | **37.5** | 1.9 | 21/48 | 18.0 | 73% | 5.0 | 100% | 0 |
| C skirmish (1v1 duels off gates) | 59.3 | 1.7 | 11/48 | 22.0 | 98% | 7.0 | 100% | 0 |
| A5 van-of-ONE @cap-2, gates captain-alone | 54.5 | 1.9 | 24/48 | 17.0 | 60% | 4.0 | 100% | 32 |
| A3 + prudent Make Camp (spec dials) | 85.0 | 1.9 | 26/48 | 18.0 | 58% | 5.0 | 100% | 41 |

## 2. Acceptance-profile scorecard

Profile: road fights won ≥50% under prudent play; gates stay bloody
(≤35%); median arrival remnant 5-12; ≥80% of arrivals carry ≥5 Book
pages.

| candidate | road ≥50% | gates ≤35% | remnant 5-12 | Book ≥80% | verdict |
|---|---|---|---|---|---|
| CURRENT | ✗ 10.4 | ✓ 1.9 | ✓ 5.0 | ✓ 100% | fails the core |
| A (at gates) | ✓ 88.6 | ✗ 55.6 | ✗ 3.0 | ✓ 100% | rejected: the van bleeds the roster at gates; commanders cease to be the wall |
| A2 (@cap-0) | ✓ | ✗ 66.7 | ✗ 3.0 | ✓ | rejected: strictly worse along the same axis |
| **A3** | ✓ 85.7 | ✓ 1.9 | ✓ 5.0 | ✓ 100% | **passes** |
| A4 (@cap-0, gates alone) | ✓ 93.5 | ✓ | ✓ 5.0 | ✓ | rejected on tone: the road gives nothing at 93.5%; the -2 offset keeps the van men mortal |
| swoon | ✓ | ✗ 89.3 | ✗ 14.0 | ✗ 76% | rejected: see §4 |
| B asymmetric | ✗ 37.5 | ✓ | ✓ | ✓ | rejected: misses the win target outright |
| C skirmish | ✓ 59.3 | ✓ | ✓ 7.0 | ✓ 100% | rejected: see §4 (numbers say yes; canon says no) |
| A5 (van of one) | ✓ 54.5 | ✓ | ✗ 4.0 | ✓ | held as the difficulty dial (§3) |

## 3. Recommendation: A3 — the company fights the road; the captain fights the battle of record

**Road battles** (wild rolls, event-raised ambushes, camp watches) field
the mantle plus up to two VANGUARDSMEN — named men drawn from the
living roster, real instances, perma-dead at 0 resolve, each a page of
the Book. **Gates** (the battles of record: Dorylaeum, Harim, Shirkuh,
the Horns, the King's road) remain the captain's own fight, single file
against the escorts and the commander, exactly as shipped.

The spec, implementable:

- **TrailCampaign party shape.** Today `party: Vec<ChampionInstance>`
  holds ONE mantle bearer. It becomes the mantle bearer plus
  `vanguard: Vec<VanguardMan>` where `VanguardMan { name: String,
  instance: ChampionInstance }`, length ≤ 2. The roster (`state.men`)
  remains the truth of who lives; the vanguard vec is the battle-body
  cache, keyed by name.
- **Minting.** A vanguardsman's instance is minted from the named man
  with the client's practice recipe (last-4 learnset rites, no
  devotion investment) at `captain_level.saturating_sub(2).clamp(5,
  MAX_LEVEL)`, species = the mantle's species (the Order's own breed).
  No XP lane in phase 1: a vanguardsman's level is re-banded to the
  current captain's -2 each time he is minted (a fresh hire under a
  risen captain joins at the new band). Wounds persist between
  battles; zeal is field-refilled on victory as now.
- **Mercy is arithmetic.** Vanguard slots are `min(2, living
  non-champion men)`. A depleted file fights short; a lone captain
  fights alone. No morale/day scaling switch — the roster itself is
  the dial, and A5/A4 show the knob's span should the owner want a
  harder road later (van of one: road 54.5%; van at captain level:
  road 93.5%, remnant unchanged).
- **Battle construction.** Road battle entries build
  `[mantle, vanguard...]` vs the client's existing foe mix at captain
  level; forced switches are handled on both sides; the active order
  is mantle first.
- **Defeat routing.** Any vanguardsman at 0 resolve after the battle —
  victory or defeat — died where the line stood: his `ManEntry.alive =
  false`, and the Book gains his page with cause
  `"fell in battle upon the pilgrim road, shoulder to shoulder in the van"`.
  The mantle at 0 resolve routes EXACTLY as today: his page first
  (`"fell in battle upon the pilgrim road"`, gate `where_fell` family
  unchanged), then `champion_falls` over the file. A risen man's
  vanguard body, if he had one, retires with the old mantle; he takes
  up the new minted mantle instance.
- **Succession interaction: none new.** The sim runs the shipped
  `champion_falls` untouched; extinction under A3 equals CURRENT
  (25/48, median day 17) because the gates still kill captains at the
  shipped rate, and the reorder still bleeds what it bled. What
  changes is WHERE the Book's battle pages come from: road deaths land
  on named vanguardsmen (38 pages across the sweep, ≈1.3 per arrived
  pilgrimage) instead of being priced solely in captains. The funnel
  is preserved — perma-death on companions is what keeps it so; see
  §4's swoon row for the measured alternative.
- **Gates.** No change: foe mix, level offsets, party of one,
  pre/victory/defeat chronicle text all stand. Gate win stays 1.9% —
  the commanders remain the pilgrimage's true examination, and the
  60%/-arrival shape of the campaign is untouched.
- **View/UI.** The trail battle intro lists the van by name when it
  stands: "Anselm of Beauvais bears the mantle; with him stand Baldric
  of Metz and Drogo of Nesle." The battle screen shows the party row
  as now (the engine already renders parties). The Book page line
  above is the only new ledger text. Sample log lines, register-kept:
  - van death: "the van held, and Baldric of Metz did not rise with it"
  - night watch, company present: "the watch cries; the van stands to"
  - lone captain: "the captain marches on with no man beside him"
- **Data seams.** None in `crusade-core/src/data/**`: foe tables,
  gates, zones all unchanged. The change is party construction in
  client battle-entry plus the vanguard cache on TrailCampaign.

Phase-2 sketch, after owner approval (NO code in this card):
1. `TrailCampaign::vanguard` field + `sync_vanguard()` (the sim's
   function, moved, with roster-name identity as here).
2. `enter_trail_battle` builds the party per §3; gate path unchanged.
3. `on_trail_battle_end` gains the vanguard-death loop before the
   existing mantle routing; Book cause string added to the cause
   table (engine `&'static str` lane).
4. Trail view: van names in the intro/muster block; Book renders the
   new cause verbatim (it already renders any cause string).
5. Guard tests: perma-death → page + roster death; risen man's
   vanguard body retires; depleted file fights short; gate entries
   never include the van.
6. Re-run this sim harness against the wired client (metrics must hold
   within seed noise).

## 4. Rejections, and why

- **CURRENT**: 10.4% road win rate; RECOVERY-SIM proved no camp dial
  moves it. It is the problem statement, not a candidate.
- **A at gates (van stands in battles of record)**: gates soften to
  55.6-89.3% won AND the vanguard bleeds ~6 souls per run (294/256
  pages), median arrival 3.0, extinction RISES to 40/48. The
  commanders must stay the wall, and the named men must not be spent
  against them. Rejected.
- **Swoon (unconscious vanguardsmen)**: the honest measurement of the
  "they're only wounded" mercy. Gates at 89.3%, median arrival 14.0
  (A4 territory and beyond — the remnant stops being a remnant), and
  the Book testifies less: only 76% of arrivals carry ≥5 pages, the
  one candidate to fail that bound. Unconsciousness is the one dial
  that breaks the funnel. Rejected, measured.
- **B asymmetric tables**: road win 37.5% — a single file at captain
  level still loses the mid-road 2v1s (practice foes at even level
  trade acceptably before the veteran build asserts itself). It misses
  the ≥50% target outright. And the tone cost is real: an army
  spanning two faiths reduces to one cutpurse on the home road.
  Rejected.
- **C skirmish**: the numbers pass (59.3% road, 1.7% gates, remnant
  7.0, Book 100% — propped up by sea and trail-event losses), and it is
  still rejected, on canon: OVERWORLD §2's wild encounters are
  Pokemon-shaped BATTLES against the region's table, not duels that
  cannot kill; under C the road takes no named man at arms anywhere
  except the six funnels, extinction falls to 11/48 with median day
  22, and 98% of runs reach the City — the Oregon Trail pressure
  collapses into a boss gauntlet with pleasant walks between. "The
  loser's side withdraws" also leaves no Book page for a battle the
  company LOST, which the doctrine of named men forbids the road
  from forgiving. Rejected on register and pacing, numbers noted.

## 5. Make Camp interaction (the waiting recovery layer)

A3 + the spec'd prudent camp (25% mend to a 3/4 cap, +5 morale, 10%
ambush, double upkeep — the dials RECOVERY-SIM cleared as safe): 20
camps pitched across 48 runs (the trigger — a wounded mantle standing
on a camp screen — now actually fires, because road battles leave
wounded survivors to mend), and every headline metric is unchanged
within noise (85.0% road, 1.9% gates, 58% reach, 5.0 remnant, 100%
Book, extinction 26/48 at day 18 — see the header footnote for the
review-pass harness repair that drifted this row 85.0/58/20/41). The reading: with battle depth
fixed first, the camp engages (wounded winners exist) and remains
weak-side safe (no overshoot, no never-camp regression by
construction). Phase order stands as the owner ruled: ship A3, then
wire the camp as specced. Note the camp mends the MANTLE only; whether
the vanguard shares the balms is a phase-2 dial — the sim says the
game does not need it (the van is re-banded with the roster; its
wounds are the road's honest cost), and RECOVERY-SIM's ledger argues
against widening the heal surface.

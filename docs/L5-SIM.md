# L5-SIM — commander learn-level L5 proposal: measured before/after

Status: SIM ANALYSIS ONLY, 2026-10-18. No data tables touched; no tests
edited. The sim is `crusade-app/tests/l5_commander_sim.rs` (every test
`#[ignore]`d). Run:

```text
cargo test -p crusade-app --test l5_commander_sim --release -- --ignored --nocapture
```

## 0. What the proposal actually is (discrepancy note)

The card described "a learn-level L5 third rite" for the named
commanders (ids 120..=125). The only documented, unapplied commander
learnset proposal in the tree is docs/FUZZ-ADVERSARIES.md § Residuals
("Unapplied proposal (owner decision required)"), and it is the
OPPOSITE shape: no commander GAINS a rite. It pushes the strong L1-lane
rites off the named commanders to learn level 6, mirroring the
2026-10-17 archetype treatment:

- Kilij Arslan I (120): `Wheeling Shots` (43) 1 -> 6
- Nur ad-Din (121): `Couched Lance` (45) 1 -> 6
- Asad al-Din Shirkuh (122): `Cleave of Axes` (49) 1 -> 6 and
  `Harass and Fade` (51) 1 -> 6
- Saladin (123): `Storm of Arrows` (47) 1 -> 6
- Gokbori (124): `Harass and Fade` (51) 1 -> 6 (extension by analogy —
  the doc names 120..=123 only; 51 is his L1 lane)
- Taki al-Din Umar (125): `Couched Lance` (45) 1 -> 6 (same extension)

Archetype precedent fills the level-1 slot with `Measured Blow` (rite
1) so no L1..=5 kit is riteless; the applied learnsets in the sim do
the same for the five commanders who lack rite 1 today. Kits at levels
>= 6 differ from the committed learnsets only by that backfilled
Measured Blow entry where present — it persists in applied kits at
6-9 for 121/125, 6-11 for 122, and 6-13 for 123/124 until the 4-rite
cap evicts it (the same shape as the shipped archetype learnsets, e.g.
111 fields rite 1 at L6); under the power-max foe policy that power-40
rite scores below every lane it shadows (45-80 power), so applied
behavior at >= 6 is unchanged.

L5 kits, before -> after (printed by the lane itself):

| id  | name               | L5 kit today | L5 kit applied |
|-----|--------------------|--------------|----------------|
| 120 | Kilij Arslan I     | [1, 43]      | [1]            |
| 121 | Nur ad-Din         | [45, 47]     | [1, 47] (47 @5 holds) |
| 122 | Asad al-Din Shirkuh| [49, 51]     | [1]            |
| 123 | Saladin            | [47]         | [1]            |
| 124 | Gokbori            | [51]         | [1]            |
| 125 | Taki al-Din Umar   | [45]         | [1]            |

## 1. Lane A — L5 pool fuzz mirror (the lane the proposal was written for)

crusade-sim `fuzz adversaries` semantics: n=2000 battles at level 5,
random 3-man teams, champions (1..=12) vs the committed adversary pool
108..=117 + 120..=123 EXTENDED with 124/125 (16 ids — crusade-sim pins
120..=123; the wings are measured here so the whole named tier reads).
Same seeds and same team draws in both columns; applied kits patched
in-memory only. NOTE: this lane's team-draw rng differs from
crusade-sim's (the workspace `BattleRng`, not ChaCha8) and the pool has
two more ids, so the absolute aggregates are NOT comparable to the
fuzz report's 44%; the before-to-after DELTA is the signal.

| metric | before | after |
|--------|--------|-------|
| champions-side aggregate | 742/2000 = 37.1% | 1479/2000 = 74.0% |
| avg battle length (turns) | 14.1 | 15.9 |

Per-commander team-win rate at L5 (appearances ~336-427 each, noise
~±2pp):

| id  | name                | before | after |
|-----|---------------------|--------|-------|
| 120 | Kilij Arslan I      | 94.6%  | 31.1% |
| 121 | Nur ad-Din          | 77.1%  | 56.2% |
| 122 | Asad al-Din Shirkuh | 82.4%  | 29.0% |
| 123 | Saladin             | 86.1%  | 12.4% |
| 124 | Gokbori             | 70.0%  | 14.7% |
| 125 | Taki al-Din Umar    | 79.9%  | 20.9% |

Reading: the doc predicted "their 390-420 stat lines should hold them
above the band at every level." The numbers say NO at L5. A stat line
armed only with `Measured Blow` collapses: four of the six named men
fall to 12-29% — below even the 40% archetype band FLOOR — and only
Nur ad-Din (who keeps `Storm of Arrows` at 5) hangs on at 56%. The
lever as written is not a trim; at L5 it disarms the named tier.

## 2. Lane B — gate matchlets (the "gates are the wall" check)

Each landed gate of docs/GATE-CONTENT.md §0 fought exactly as shipped:
escorts at captain-2 (clamped 5..=50), boss at captain+offset, lone
veteran captain (the storage soak's recipe, species 7 learnset kit at
his own level), n=1024 per cell, identical seeds in both columns.
Captain levels {3, 15, 25, 40, 50} — 3 is the diagnostic (the only
level where a boss stands at 5/6 with the pushed lanes still off his
kit); 15-50 span plausible captain states at the screens 13..=25 gates.

| ref | boss (gate screen)              | cap L | boss L | captain win% before | after | turns before | after |
|-----|---------------------------------|-------|--------|--------------------|-------|--------------|-------|
| 101 | Kilij Arslan I (9)              | 3     | 5      | 0.0 | 0.0 | 5.8 | 5.8 |
|     |                                 | 15    | 17     | 0.2 | 0.2 | 5.6 | 5.6 |
|     |                                 | 25    | 27     | 1.7 | 1.7 | 6.0 | 6.0 |
|     |                                 | 40    | 42     | 1.5 | 1.5 | 6.1 | 6.1 |
|     |                                 | 50    | 50     | 1.8 | 1.8 | 6.1 | 6.1 |
| 102 | Nur ad-Din (13)                 | 3     | 5      | 0.0 | 0.0 | 4.8 | 4.8 |
|     |                                 | 15    | 17     | 42.0| 42.0| 8.2 | 8.2 |
|     |                                 | 25    | 27     | 35.1| 35.1| 8.7 | 8.7 |
|     |                                 | 40    | 42     | 39.4| 39.4| 8.9 | 8.9 |
|     |                                 | 50    | 50     | 65.4| 65.4| 8.5 | 8.5 |
| 103 | Asad al-Din Shirkuh (16)      | 3     | 5      | 0.0 | 0.0 | 5.8 | 5.8 |
|     |                                 | 15    | 17     | 12.0| 12.0| 10.3| 10.3|
|     |                                 | 25    | 27     | 11.2| 11.2| 11.2| 11.2|
|     |                                 | 40    | 42     | 9.9 | 9.9 | 11.9| 11.9|
|     |                                 | 50    | 50     | 24.0| 24.0| 12.0| 12.0|
| 104 | Gokbori (19)                  | 3     | 5      | 0.0 | 0.0 | 5.8 | 5.8 |
|     |                                 | 15    | 17     | 68.1| 68.1| 8.0 | 8.0 |
|     |                                 | 25    | 27     | 65.4| 65.4| 8.5 | 8.5 |
|     |                                 | 40    | 42     | 70.4| 70.4| 8.6 | 8.6 |
|     |                                 | 50    | 50     | 93.5| 93.5| 8.2 | 8.2 |
| 105 | Taki al-Din Umar (20)         | 3     | 6      | 0.0 | 0.0 | 5.8 | 5.8 |
|     |                                 | 15    | 18     | 47.9| 47.9| 8.6 | 8.6 |
|     |                                 | 25    | 28     | 63.5| 63.5| 8.7 | 8.7 |
|     |                                 | 40    | 43     | 64.2| 64.2| 9.0 | 9.0 |
|     |                                 | 50    | 50     | 64.7| 64.7| 9.2 | 9.2 |
| 108 | Saladin (25)                  | 3     | 6      | 0.0 | 0.0 | 5.8 | 5.8 |
|     |                                 | 15    | 18     | 14.3| 14.3| 8.6 | 8.6 |
|     |                                 | 25    | 28     | 19.1| 19.1| 8.8 | 8.8 |
|     |                                 | 40    | 43     | 25.8| 25.8| 9.1 | 9.1 |
|     |                                 | 50    | 50     | 30.3| 30.3| 9.3 | 9.3 |

Reading: before == after in EVERY cell, exactly. This is policy-
behavioral identity, verified under the same seeds in both columns,
not kit-vector identity: at boss levels 6-13 some applied kits carry
the extra dormant backfilled Measured Blow (and 121's >=14 kit swaps
two slot orders, 45 vs 47), but the differing rite never wins the
greedy power-max score, so the fought behavior is identical. The
diagnostic cap-3 cells read 0.0% either way because an L3 captain
against two L5 escorts falls in ~5 turns, before the boss's kit
matters — and only the +2 gates' boss-5 rows field a trimmed boss at
all (keeping Measured Blow alone, except Nur ad-Din who still holds
Storm of Arrows at 5); at the +3 gates the boss stands at 6 with the
pushed lanes already re-landed (applied 125 at 6 = [1, 45, 53];
applied 123 at 6 = [1, 47, 57]). The lever cannot move any gate at
any captain level.

Caveats on absolute numbers (not the delta): these matchlets field a
FRESH full-resolve veteran captain; on the road the captain arrives
bloodied, which is why the journey soak (docs/TRAIL-BATTLE-DEPTH.md)
reads 1.9% across gates while these fresh-captain cells read higher.
The cells flag one pre-existing observation for the owner, unchanged
by this proposal either way: gates 104/105 (the Hattin wings) read
48-94% captain-win in the fresh-captain matchlet at captain >= 15,
above the ≤35% band the trail doc targets; the journey-number band
(1.9%) still governs, but the wings are the softest named gates in
isolation.

Sim caveat (latent wart, untriggered): the `fight()` driver maps
`Battle::new` construction refusal (an illegal/riteless entry) to a
side-0 win with zero turns. No species in either pool is riteless at
any fought level, and the printed average turn counts (14.1/15.9 in
lane A, 4.8-12.0 in lane B) prove every battle ran in the recorded
results; the sim now panics on refusal so a future edit reusing the
lane cannot silently bias counts.

## 3. Recommendation

**REJECT as documented.** The proposal's goal was the L5 fuzz
aggregate (champions-side floor ~45%). Measured:

- It overshoots wildly: 37.1% -> 74.0% champions-side in this pool.
- It breaks its own invariant: the doc assumed the commander stat
  lines would hold the tier above band at L5; instead four of six
  named men fall to 12-29%, under the archetype band floor. They
  become feeble men with famous names — a coherence problem as much as
  a balance one.
- It buys nothing in the shipped game: commanders field ONLY at gates
  at captain+2/+3 (boss level >= 8 for any captain >= 6, and the
  cap <= 3 edge is a 0%-either-way stomp), so no gate matchlet moves
  at all. The lever's entire effect lives inside the L5 fuzz lane.

If the owner still wants the L5 fuzz aggregate raised, the minimal
counter-dials short of this lever:

1. **Scope the fuzz target, not the kits (recommended).** The named
   tier sits above band by design (mini-boss/boss reading, FUZZ section
   4). Read the L5 aggregate against the archetype pool only, or accept
   the ~79% named-tier inflation as composition, as the doc already
   does at L20/L40.
2. **Single-rite subset: Kilij only.** `Wheeling Shots` (43) 1 -> 6
   leaves him [1] at L5 but he keeps `Harass and Fade` at 6 — measured
   94.6% -> 31.1% though, still below the floor; not actually a
   gentle dial.
3. If a future card wants commanders TOUGHER at low levels (the
   direction the original card phrasing implied), that is a different
   proposal: a real third rite learned at 5. Nothing of that shape is
   documented today, and S1/S2 give it no numeric footing either way.

The decision rests with the OWNER; the numbers above are the full
before/after record.


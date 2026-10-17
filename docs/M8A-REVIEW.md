---
title: "M8a Review — Overworld schema + trail engine (The Mantle, Crossing)"
summary: "Review of crusade-core/src/data/overworld.rs and src/engine/trail.rs against docs/OVERWORLD.md §§1-6 and HISTORICAL-FOES.md §6. 97/97 tests pass; wasm clean for core crates. Verdict: SHIP with minor follow-ups."
tags: [review, m8, overworld, pilgrim-road, trail-engine, the-mantle]
date: 2026-10-16
status: review
---

# M8A REVIEW

Scope: `git diff HEAD` — new `crusade-core/src/data/overworld.rs` (294 ln),
new `crusade-core/src/engine/trail.rs` (742 ln), `pub mod overworld;` and
`pub mod trail;`. Unrelated working-tree files (M7b conversion-lane edits in
crusade-app, art ledgers) were excluded from review.

## 1. Build & tests

- `cargo test -p crusade-core`: **97 passed, 0 failed.** ✓ (expected 97)
- Multi-crate wasm check (`crusade-core/data/net/bridge`): **all compile clean**
  for `wasm32-unknown-unknown`. ✓
- Multi-crate check as written fails only inside **crusade-app** (7 errors:
  `crate::conversion` import, `scan_defeated_foes`, `resolve_call`, the new
  `ViewSnapshot` fields). Every error sits in `crusade-app/src/client.rs`,
  `view.rs`, `pvp.rs` — uncommitted M7b conversion-lane WIP untouched by this
  diff. Not attributable to M8a; no stash performed (git mutation, and
  attribution is unambiguous from the error paths).
- Single-crate `cargo check -p crusade-core --target wasm32-unknown-unknown`
  fails in the `getrandom` dependency at feature-resolution — identical to the
  documented pre-existing edge; the diff adds no rng/entropy/Cargo.toml change,
  so it cannot be diff-caused. ✓ pre-existing, confirmed by inspection.

## 2. Contract fidelity

- **§3 travel_day economics** — upkeep `living/5` floor 1 charged before the
  day's event ("the men eat first"), starvation on empty stores costs
  morale (−5), never men; morale clamped −100..=100 on every write path;
  events filtered by `min_day` and region, weighted pick without replacement
  bias. The doc specifies no exact numbers; constants are named, documented,
  and unit-tested at their rails. ✓
- **§6.2 THE MANTLE** — `champion_falls`: highest-devotion living non-champion
  at/above `DEVOTION_RISE_MIN = 192` (top quartile of the hidden 0..=255 seed —
  "rare enough to feel like grace") rises; else each man is kept against
  `50 + morale/2` percent (clamp 0..100), losses written to the Book with a
  chronicle-register cause ("lost in the rout after their captain fell"). Skill
  banding (−5..+2) correctly delegated to the caller, documented in-module. ✓
- **§6.4 The Crossing** — charter/provisioning banded into 0..=2, bend
  losses/supplies/morale-scar monotonically (strict ordering proven by test at
  the rails), ≥5 souls always land, falls recorded in the Book at day 0. ✓
- **Gate commanders (§4)** — screen 9 = Kilij Arslan (120, Anatolia ✓),
  screen 16 = Shirkuh (122, Roads of Egypt ✓), screens 20/25 = Saladin (123,
  Hattin finale + Holy Land finale ✓). Commander ids verified against
  `data::adversaries` (120=Kilij Arslan I, 121=Nur ad-Din, 122=Shirkuh,
  123=Saladin). Guard test enforces membership in the named set.
- **Register (HISTORICAL-FOES §6)** — event causes and comments are
  chronicle-firm, not cruel ("the road took him", "lost at sea in the
  Crossing"). No mockery, no exonyms outside quotes, no slurs. ✓

## 3. Determinism

- Every roll path consumes the caller-passed `BattleRng`; grep confirms zero
  `thread_rng`/entropy/time reads in crusade-core. Per-travel-day seeding
  discipline (`BattleRng::for_turn(seed, day)`) is exercised by the golden sim,
  matching §5's seeded-per-day contract. ✓
- The 30-day golden sim genuinely proves determinism: full-state FNV-1a hash
  over the serialized state after a Crossing + 30 marches, run twice; per-day
  invariant checks (morale clamp, dead == Book length) along the way. ✓

## 4. Schema quality

- Edge symmetry test maps N↔S/E↔W correctly via `(dir+2)%4`; direction
  ordering `[N,E,S,W]` matches the edge comment. ✓
- BFS from screen 1 asserts all 25 screens reachable and every region touched. ✓
- Region ids 1..=7 and screen ids 1..=25 asserted contiguous against position. ✓
- No `dbg!`, `todo!`, dead code; style matches existing data tables. ✓

## 5. Deviations audit

All four worker deviations are justified and documented in-module:

1. **No serde on ScreenDef** — static table discipline like Rite/SpeciesBase;
   journey state stores ids only (overworld.rs:60-61). Sound. ✓
2. **Serialize-only state** — `FallenEntry.cause` is `&'static str` from the
   fixed cause table, so `Deserialize` is impossible (trail.rs:11-14). This
   will NOT silently break the M8c persistence card: any attempt to round-trip
   `TrailState` directly fails loudly at compile time. **Note for M8c:** the
   correct shape is an explicit save DTO in crusade-app (owned Strings; cause
   re-expressed as an enum keyed to the fixed cause table) plus the pilgrimage
   seed + day count persisted so per-day rng streams can be rebuilt. Do not add
   `Deserialize` to `TrailState`. ✓
3. **Single-file roster** — multi-file assignment/distribution deferred to
   crusade-app's card; documented (trail.rs:7-10); state otherwise carries the
   full §5 vector (screen, roster + Devotion seeds, supplies, morale, day,
   region flags, Book of the Fallen). ✓
4. **wasm note** — module claims wasm-safe; verified: all four core crates
   compile for wasm32 in the workspace build (see §1). ✓

## 6. Issues

1. **[minor]** §5's fixed verb set includes `convert_opportunity`; `TrailVerb`
   omits it (LoseSupplies/LoseMen/MoraleDelta/Battle/Forage/Rest/Nothing only).
   Add-only seam, but "converts replenish the ranks" (§3) needs it — schedule
   for M8b before event-table densification.
2. **[minor]** Gate commander geography: screen 19 assigns Shirkuh (122) to a
   Horns-of-Hattin wing (§4 names Gokbori / Taki al-Din, absent from the
   adversary table; Shirkuh died 1169); screen 23 puts Nur ad-Din (121, d. 1174)
   on the Arsuf/Jaffa approach. Constrained by the 4-commander table; flag for
   the next adversary-data card (add ids 124+), then rewire gates.
3. **[minor]** §3's Book records "name, where he fell, how" — `FallenEntry`
   carries day + cause but no screen/region. Add `screen: u16` (or region id)
   before persistence lands; cause strings alone don't carry place.
4. **[nit]** overworld.rs:82-84 references "the region macros below" — no
   macros exist (table hand-written, which is fine); stale comment.
5. **[nit]** `LoseMen` cause strings keyed by hard-coded event id
   (`match event_id`, trail.rs:177-182) couples engine to table rows; brittle
   as the table densifies. Move cause onto `TrailEvent` (also fixes the
   persistence shape from §5.2).
6. **[nit]** `champion_falls` applies succession but does not write the fallen
   champion's own Book entry — caller duty, but say so on the type; M8c must not
   drop the page.
7. **[info]** Starvation fires when supplies hit 0 mid-upkeep even if partially
   paid (supplies 1, cost 2). Documented in-module (trail.rs:15-16); acceptable
   judgment; keep consistent with M8c tuning.
8. **[info]** Multi-crate wasm check currently blocked by uncommitted M7b
   crusade-app WIP (§1). Re-run once M7b lands; do not attribute to M8a.

## Verdict

**SHIP.** All contractual mechanics are faithful or explicitly documented as
judgment calls; tests are strong (97/97, real determinism proof). The three
minor issues are data-card follow-ups, not M8a defects.

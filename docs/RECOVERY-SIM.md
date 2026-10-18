# RECOVERY-SIM — measured phase-1 results for Make Camp (docs/RECOVERY.md)

Status: SHIPPED post-implementation, 2026-10-17. Make Camp is wired in
crusade-app (`ClientInput::MakeCamp` on Camp-prop screens; camp props at
screens {3, 8, 12, 15, 18, 21}; the night ambush fought from the unmended
stance, balms at dawn on a kept watch) on top of A3 (docs/TRAIL-BATTLE-
DEPTH.md). One engine-semantics repair landed with the wiring: the
prototype's cap applied to the STANCE, so camping above 3/4 resolve would
have bled the captain; the shipped `make_camp` caps the MEND only
(`resolve_after = (r+mend).min(cap).max(r)` — top-of-bar camps are a
no-op, never a wound). The prudent policy never camps above 40%, so no
reported number moves; the sweep re-ran against the landed tree
(`cargo test -p crusade-app --test camp_policy_sim --release --
--ignored --nocapture`) and reproduces every section-1 and section-2 row
verbatim: never-camp soak posture 10.0/48-48-extinct/0% reach; never-camp
rational 17.0 (25/48), 60% reach; prudent 18.0 (27/48), 58% reach, 7.0
median remnant, 100% Book, 20 camps.[^1] No DTO version bump (serde tail
fields only).

[^1]: Review-pass harness repair (2026-10-17): `Jour::camp` previously
      fought night ambushes with the brigand table `trail_foe_mix(201,
      zone)`; it now fights the screen's encounter-zone mix (`Some(z) ->
      zone table, None -> 201`), cell-for-cell with client.rs
      `trail_make_camp`. Re-ran in release: every section-1 and section-2
      row reproduced unchanged.

Status (original phase-1 report, preserved below): the measurements that
followed justified the ordering A3-then-camp; the tables stand as
measured.
Run: `cargo test -p crusade-app --test camp_policy_sim -- --ignored --nocapture`
Harness: `crusade-app/tests/camp_policy_sim.rs` — the real journey math
(`trail_engine::travel_day`, `make_camp` semantics) with REAL battles via
the crusade-core battle engine, the client's own foe mixes / gate level
rules, and the storage soak's fight recipe. 48 seeded pilgrimages per
configuration. Camp screens modeled at {3, 8, 12, 15, 18, 21} per
RECOVERY.md §3A. Battles counted as entered; losses route through the
real THE MANTLE succession (champion page, rise-in-righteousness /
reorder, level banding).

## 1. Policies × metrics (48 runs each, dials as specced)

Postures: "soak posture" = engage electives to 3/day + concede every 8th
battle (docs/STORAGE-SOAK.md scenario, for baseline comparability).
"Rational" = fight what the road raises, no elective grinding.

| policy | med extinct day | ≥1 win | reach 25 ≤d40 | med living @arrival | arrivals ≥5 Book pp | camps | extinct ≤d15 |
|---|---|---|---|---|---|---|---|
| never-camp, soak posture | 10.0 (48/48 extinct) | 25% | 0% | — | — | 0 | 100% |
| never-camp, rational posture | 17.0 (25/48) | 27% | 60% | 5.0 | 100% | 0 | 4/48 |
| always-camp | 18.0 (27/48) | 27% | 58% | 5.0 | 100% | 30 | 5/37 non-camping runs |
| prudent-camp (<40% resolve, >2d supplies) | 18.0 (27/48) | 27% | 58% | 7.0 | 100% | 20 | 5/37 |
| prudent + roguelite knob (+10% start supplies) | 18.0 (27/48) | 27% | 58% | 7.0 | 100% | 20 | 5/37 |

## 2. Sensitivity probes (would stronger/recover-free dials matter?)

| variant | med extinct day | ≥1 win | reach 25 | med living @arrival |
|---|---|---|---|---|
| prudent, heal 50% & cap lifted to 100%, 10% ambush | 17.0 | 27% | 60% | 7.0 |
| prudent, spec heal, 0% ambush | 17.0 | 27% | 60% | 6.0 |

Even a full, risk-free mend moves nothing that matters.

## 3. Acceptance-target scorecard (RECOVERY.md §5)

| target | verdict | measured |
|---|---|---|
| Never-camp extinction 8–12d (regression guard) | **HOLDS** | median 10.0, 100% extinct ≤15, 0% reach 25 — under the soak posture the mechanic is invisible to a non-camper |
| Prudent: ≥50% record ≥1 battle win | **FAILS** | 27% — and NO recovery dial moves it (both sensitivity probes: 27%). Firsts wins are decided inside the battle engine, not between battles |
| Prudent: ≥25% reach screen 25 by day 40 | **holds** | 58% — but never-camp rational reaches 60%, so this is the road's existing shape, not the camp's doing |
| Median living at arrival ≤12 | holds | 7.0 (camp) vs 5.0 (never-camp rational) — the camp's one true effect: a slightly larger remnant arrives |
| ≥80% of arrivals carry ≥5 Book pages | holds | 100% everywhere |
| Never-campers 100% extinct by day 15 | holds ONLY under soak posture | the rational never-camper crosses 60% of seeds — the doc's premise "every soaked pilgrimage went extinct" was an artifact of the soak's fight-everything posture, not of the road itself |

## 4. What the numbers actually say

1. **The recovery mechanic as specced mostly does not engage.** 20 camps
   across 48 prudent runs (~0.4/run). The trigger — a mantle at <40%
   resolve still alive to camp — barely exists: road battles against
   2–3 peers are usually *lost outright* (~4% win rate), and a loss is a
   dead captain, not a wounded one. Recovery economics assume wounded
   winners; the road produces almost none. RECOVERY.md §2's ledger
   ("a bar-costly but winnable battle") does not survive contact with
   the real battle engine at MVP foe-party depth.
2. **Recovery is not the binding lever; party-depth is.** The ≥50% win
   target cannot be met by any heal/cap/ambush tuning (probes above).
   It is a battle-difficulty card (foe count/level at gates and camps),
   separable from this one.
3. **The camp is otherwise well-behaved.** Never-campers are untouched;
   campers gain no free wins, pay days, and arrive with 2 more souls.
   It is chronicle-true and cheap. If recovery ships for tone/canon,
   the spec'd dials are *safe* — they err on the weak side, not the
   strong side. There was no need to turn the cap to 2/3 or the heal
   to 20%; if anything the numbers under-fire.

### Dial verdicts (for a hypothetical phase 2)

| dial | spec | sim verdict |
|---|---|---|
| heal 25% max | keep | adequate within noise; no dial value rescues the win target |
| cap 3/4 max | keep | as above |
| morale +5 | keep | morale is not the tight resource at best provisioning |
| ambush 10% | keep | 10% vs 0% indistinguishable in outcomes |
| double-upkeep day cost | keep | supplies are not binding at charter 255/prov 1000 (~900 median end-day supplies) |

## 5. Roguelite-knob analysis ("does meta-progression change the picture?")

Modeled: +10% starting supplies as a legacy perk. Result: **every metric
identical** to the plain prudent run; only the end-day supplies median
rose (+100, exactly the grant). Supplies and days are slack resources at
a good crossing; deaths are battle losses, and a supply/season meta-bonus
never enters the battle. A roguelite lane that *would* change the picture
is one that alters battle structure (e.g., the mantle inheriting level
banding upward, or a second fighting body) — but that is the succession /
multi-file card, not a recovery knob, and RECOVERY.md §4 already rules
the healer-body variant out for canon reasons. Recommendation: no
meta-progression attached to the camp; if the owner wants roguelite
growth, aim it at battle depth, not at the journey ledger.

## 6. Phase-1 recommendation

**Stop before implementing as-is.** The spec is not dangerous but it
does not solve the problem it was aimed at: with the mantle alone in the
fighting line against 2–3 foes, there are almost no wounded survivors to
mend, so recovery capacity cannot reach its own acceptance target. The
honest sequence is:

1. A battle-depth fix first (gate/wild mixes fought at odds a single
   file can survive, or the deferred multi-file lane) — only then does a
   wound economy exist for a camp to serve.
2. Ship the camp with the spec'd dials as the recovery layer at that
   point; phase-1 here pre-clears its safety (no never-camp regression,
   no overshoot at any dial strength tested).
3. If the camp ships alone anyway for canon/tone, ship spec dials — but
   set expectations: it will show in the Book as ~2 extra souls at
   arrival, not as won battles.

The engine prototype (`trail::make_camp` + consts + guard tests) shipped
with the A3 client wiring (2026-10-17): `ClientInput::MakeCamp` on camp
screens, camp props at {3, 8, 12, 15, 18, 21}, `TrailView.can_camp` /
`mantle_resolve` tail fields, the register text of RECOVERY.md §5, and
the mend-cap repair noted above. The acceptance profile of §5 reads as
measured — the camp remains weak-side safe, and the wound economy it was
aimed at now exists because A3 produces wounded survivors on the road.

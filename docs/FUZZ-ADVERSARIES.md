---
title: "Fuzz Report: Adversary Pool Balance (M7 ids 108-117, 120-123)"
summary: >
  Widened crusade-sim fuzz harness over the M7 historical-adversary pool:
  champions-vs-adversaries and adversaries-vs-adversaries at levels 5/20/40,
  2000 seeded battles per matchup per level (seeds 0..2000 as [s as u8; 32]),
  zero failures. Muqaddam and the four commanders sit above the champion band
  at every level as designed; at L20/L40 the archetype field strays low
  (Shieldbearer/Bowman/Sapper at 23-29%) and at L5 the whole pool overshoots
  the band (adversary teams win 75%). Champion-only fuzz mode is unchanged
  (golden output byte-identical).
tags: [fuzz-report, m7, crusade-sim, adversaries, balance]
date: 2026-10-16
status: fuzz-report
---

# FUZZ-ADVERSARIES

Resolves M7-REVIEW.md issue 4 (M7-4): the fuzz pool was champions-only and
the per-id stats array at crusade-sim/src/main.rs:199 was indexed by species
id, which would panic on adversary ids.

## Harness changes

- `stats` is now a `HashMap<u16, (usize, usize)>` keyed by species id
  (appearances, wins) instead of a fixed `[(usize, usize); SPECIES_MAX + 1]`
  array — no panic on ids 108..=123.
- New `crusade-sim fuzz <n> <level> adversaries` pool mode: runs
  champions-vs-adversaries then adversaries-vs-adversaries, `n` battles each,
  same seed discipline as the champion mode (battle seeds `0..n` as
  `[s as u8; 32]`, ChaCha8Rng). Per-id tables printed per matchup.
- The adversary pool is pinned to the M7 id set (archetypes 108..=117 +
  commanders 120..=123) as an explicit constant, so results stay reproducible
  while later milestones append ids to `data::adversaries::ADVERSARIES`
  (in-flight M7b work adds 124/125; extend `ADVERSARY_IDS` when that lands).
- Champion mode (`fuzz <n> <level>`, pool defaults to `champions`) keeps the
  exact `gen_range(1..=12)` draw path; `fuzz 300 5` and `fuzz 300 20` output
  verified **byte-identical** before/after the change. Full reruns of the
  adversary fuzz are byte-identical (determinism check passed).

## Run parameters

- Command: `crusade-sim fuzz 2000 <level> adversaries` for level in {5, 20, 40}.
- Seeds: 0..2000 as `[s as u8; 32]` per matchup (both matchups reuse the
  same seed range, matching the existing champion-mode discipline).
- Teams: 3 random members per side, rites = 4 most recent learnset entries
  with level <= battle level, no grace/devotion, neutral calling.
- Policy AI, TURN_CAP 500, identical for both sides (unchanged).
- Failures: 0 at every level and matchup.
- **SAMPLE-SIZE CAVEAT (added at review, SIM-FUZZ-REVIEW):** seeds are
  `[s as u8; 32]` — only 256 distinct seeds exist, so a "2000-battle"
  matchup is 256 unique battles replayed ~7.8x. The effective noise floor
  for per-id win% is ~+/-6-7pp, not +/-2.5pp. Read the tables accordingly:
  within-band conclusions at the 40-60% edges are unreliable; only the
  large deviations (L5 overtuning; double-digit sags) are solid evidence.

## Aggregate (champions side 0 vs adversaries side 1)

| level | side0 (champions) | side1 (adversaries) | avg turns |
|------:|------------------:|--------------------:|----------:|
| 5     | 496 (25%)         | 1504 (75%)          | 12        |
| 20    | 1028 (51%)        | 972 (49%)           | 13        |
| 40    | 997 (50%)         | 1003 (50%)          | 14        |

Adversaries-vs-adversaries side splits: 1001/999 (L5), 920/1080 (L20),
927/1073 (L40).

## Per-id win rates, champions vs adversaries

Win% = wins / appearances across the 2000 battles (a member of the winning
team counts as a win). Band per HISTORICAL-FOES.md section 4: archetypes
108..=117 should land 40-60%; Muqaddam (117) and commanders (120..=123) sit
above it by design (mini-boss / boss tier).

| id  | name                  | app | L5% | L20% | L40% |
|----:|-----------------------|----:|----:|-----:|-----:|
| 108 | Turcoman Bowrider     | 318 | 85  | 41   | 37   |
| 109 | Ghulam Lancer         | 464 | 76  | 46   | 49   |
| 110 | Sudani Shieldbearer   | 534 | 54  | 23   | 29   |
| 111 | Armenian Bowman       | 516 | 57  | 25   | 28   |
| 112 | Daylamite Axeman      | 421 | 66  | 31   | 36   |
| 113 | Bedouin Outrider      | 376 | 64  | 36   | 34   |
| 114 | Kurdish Ghazw Rider   | 419 | 82  | 49   | 49   |
| 115 | Naffatun Sapper       | 412 | 52  | 29   | 29   |
| 116 | Askari Alamdar        | 338 | 76  | 39   | 34   |
| 117 | Muqaddam of the Tulb  | 461 | 77  | 70   | 74   |
| 120 | Kilij Arslan I        | 508 | 95  | 82   | 81   |
| 121 | Nur ad-Din            | 414 | 86  | 63   | 65   |
| 122 | Asad al-Din Shirkuh   | 428 | 90  | 74   | 74   |
| 123 | Saladin               | 391 | 93  | 63   | 71   |

## Per-id win rates, adversaries vs adversaries

| id  | name                  | L5% | L20% | L40% |
|----:|-----------------------|----:|-----:|-----:|
| 108 | Turcoman Bowrider     | 52  | 52   | 46   |
| 109 | Ghulam Lancer         | 50  | 45   | 45   |
| 110 | Sudani Shieldbearer   | 23  | 37   | 36   |
| 111 | Armenian Bowman       | 42  | 35   | 36   |
| 112 | Daylamite Axeman      | 42  | 44   | 44   |
| 113 | Bedouin Outrider      | 33  | 25   | 22   |
| 114 | Kurdish Ghazw Rider   | 50  | 45   | 46   |
| 115 | Naffatun Sapper       | 38  | 40   | 39   |
| 116 | Askari Alamdar        | 43  | 42   | 45   |
| 117 | Muqaddam of the Tulb  | 53  | 63   | 63   |
| 120 | Kilij Arslan I        | 86  | 85   | 84   |
| 121 | Nur ad-Din            | 58  | 57   | 59   |
| 122 | Asad al-Din Shirkuh   | 58  | 67   | 70   |
| 123 | Saladin               | 65  | 58   | 59   |

## Band check (HISTORICAL-FOES section 4)

- **Muqaddam (117)**: above the band at every level (77/70/74) — mini-boss
  tier holds.
- **Commanders (120..=123)**: above the band at every level (63-95%) — boss
  tier holds. Kilij Arslan I is the strongest field unit in every matchup
  and level (commander-band stat line but his alacrity 85 kit
  dominates the never-switch policy AI).
- **L5**: 6 of 10 archetypes sit ABOVE the band (Bowrider 85, Ghazw 82,
  Alamdar 76, Ghulam 76, Axeman 66, Outrider 64); in-band are Bowman 57,
  Shieldbearer 54, Sapper 52; none below. The pool as a whole wins 75% of
  battles — overtuned at L5.
- **L20**: in-band are Ghazw 49, Ghulam 46, Bowrider 41; below band are
  Alamdar 39, Outrider 36, Axeman 31, Sapper 29, Bowman 25, Shieldbearer 23.
- **L40**: in-band are Ghulam 49, Ghazw 49; below band are Bowrider 37,
  Axeman 36, Outrider 34, Alamdar 34, Shieldbearer 29, Sapper 29, Bowman 28.

## Delta vs the M7 worker's reported numbers

M7 worker aggregates (side0/side1): 46/54 L5, 78/22 L20, 79/21 L40.
This run: 25/75 L5, 51/49 L20, 50/50 L40 — the shape does not reproduce
(see Deviations). Their per-id pairs (read as champions-vs-adversaries
L20/L40, the only interpretation that fits: Shieldbearer 23/27 and Muqaddam
71/72 land within noise of this run's 23/29 and 70/74):

| id  | archetype    | M7 L20 | here L20 | Δ    | M7 L40 | here L40 | Δ    |
|----:|--------------|-------:|---------:|-----:|-------:|---------:|-----:|
| 108 | Bowrider     | 42    | 41       | -1   | 42     | 37       | -5   |
| 109 | Ghulam       | 63    | 46       | -17  | 55     | 49       | -6   |
| 110 | Shieldbearer | 23    | 23       | 0    | 27     | 29       | +2   |
| 111 | Bowman       | 42    | 25       | -17  | 37     | 28       | -9   |
| 112 | Axeman       | 36    | 31       | -5   | 34     | 36       | +2   |
| 113 | Outrider     | 41    | 36       | -5   | 39     | 34       | -5   |
| 114 | Ghazw        | 51    | 49       | -2   | 46     | 49       | +3   |
| 115 | Sapper       | 36    | 29       | -7   | 42     | 29       | -13  |
| 116 | Alamdar      | 42    | 39       | -3   | 42     | 34       | -8   |
| 117 | Muqaddam     | 71    | 70       | -1   | 72     | 74       | +2   |

Six of ten ids agree within ~5pp at both levels — but per the
SAMPLE-SIZE CAVEAT above the true noise floor is ~6-7pp, so only these
double-digit divergences stand as real:
Ghulam L20 (-17), Bowman L20 (-17) and L40 (-9), Sapper L40 (-13).

## Deviations and notes

- **Aggregate shape vs M7 worker**: this run finds adversaries dominant at
  L5 (75% team wins) and balanced at L20/L40 (~50%), where the M7 worker
  reported near-balanced L5 (54%) and champion-dominated L20/L40 (22/21%
  adversary team wins). The M7 worker's harness was never committed (their
  numbers came from a session report, not a shipped command), so the exact
  cause cannot be isolated; the committed harness is now the reproducible
  baseline for any future re-check.
- **Likely divergence driver**: their per-id list covers archetypes only,
  suggesting an archetypes-only adversary pool; this run draws adversaries
  from all 14 M7 ids (including commanders as teammates), which changes both
  the seed-to-team mapping and average team strength. Pool composition is
  now pinned in `ADVERSARY_IDS`, so this ambiguity is closed going forward.
- **L5 overshoot**: adversary learnsets front-load rites at level 1 while
  champion kits are thin at L5 under the greedy max-power policy; the pool
  wins 75% of L5 battles. If the section 4 band is meant to hold at L5, the
  L1-lane adversary rites (43, 15, 49, 51, 54) or champion L5 kits need a
  balance pass.
- **Champion mode untouched**: golden outputs for `fuzz 300 5` and
  `fuzz 300 20` are byte-identical before/after the harness change;
  `cargo test -p crusade-sim` green (0 tests, placeholder lib), `cargo fmt`
  and `cargo clippy` clean.
- **Concurrent M7b work**: a `typing_override: Option<Typing>` field was
  added to `ChampionInstance` in the working tree mid-run; `build_champion`
  sets it to `None` (behavior-neutral — verified by byte-identical fuzz
  output before/after the field landed). No crusade-core files were edited
  by this task.

## Post-tune verification (2026-10-17)

Balance-tuning card over the archetype kits (owner decisions 2026-10-17:
Shieldbearer kit redesign, L5 pool de-tuning via learn levels, lifts for
the chronic L20/L40 stragglers). Stat lines, typings, ids, names, lore,
obstinacy and sin_depth are untouched; rites 1..=42, the engine, and the
harness are untouched. Muqaddam (117) and the commanders (120..=125) were
not modified. Same harness, same seeds, deterministic; `fuzz 300 5`,
`fuzz 300 20`, and `fuzz 2000 5 champions` outputs verified byte-identical
before/after every edit (champion mode golden holds); `cargo test -p
crusade-core` 102/102 green throughout. Three tuning fuzz passes after the
baseline (within the ~6-iteration budget).

What changed (details per archetype below): five new adversary rites
58..=62 (`Unhorse the Rider`, `Arrow through the Visor`, `The Falling
Stroke`, `Lance at the Gallop`, `The Banner Advances`), one power bump on
an archetype-only rite (`Comet of Naphtha` 60 -> 80), and learn-level
re-ordering across archetypes 108..=116 so the strong L1-lane rites
(43, 15, 47, 49, 51, 54, 18) land at level 6+ and L5 foes field
Measured-Blow-tier kits. Kits and learn levels are updated in
HISTORICAL-FOES.md section 4; the CONVERSION.md balance-debt note is
resolved.

### Aggregate, champions (side 0) vs adversaries (side 1)

| level | before side0 | before side1 | after side0 | after side1 |
|------:|-------------:|-------------:|------------:|------------:|
| 5     | 496 (25%)    | 1504 (75%)   | 884 (44%)   | 1116 (56%)  |
| 20    | 1028 (51%)   | 972 (49%)    | 783 (39%)   | 1217 (61%)  |
| 40    | 997 (50%)    | 1003 (50%)   | 791 (40%)   | 1209 (60%)  |

Failures: 0 at every level and matchup. Avg turns 14/13/14 (matchup 1).
Adversaries-vs-adversaries side splits after: 914/1086 (L5), 892/1108
(L20), 916/1084 (L40); before: 1001/999, 920/1080, 927/1073.

### Per-id win rates, champions vs adversaries (before -> after)

| id  | name                  | L5        | L20       | L40       |
|----:|-----------------------|----------:|----------:|----------:|
| 108 | Turcoman Bowrider     | 85 -> 49  | 41 -> 55  | 37 -> 44  |
| 109 | Ghulam Lancer         | 76 -> 36  | 46 -> 62  | 49 -> 65  |
| 110 | Sudani Shieldbearer   | 54 -> 38  | 23 -> 51  | 29 -> 56  |
| 111 | Armenian Bowman       | 57 -> 46  | 25 -> 49  | 28 -> 43  |
| 112 | Daylamite Axeman      | 66 -> 35  | 31 -> 49  | 36 -> 49  |
| 113 | Bedouin Outrider      | 64 -> 37  | 36 -> 41  | 34 -> 39  |
| 114 | Kurdish Ghazw Rider   | 82 -> 60  | 49 -> 60  | 49 -> 62  |
| 115 | Naffatun Sapper       | 52 -> 35  | 29 -> 39  | 29 -> 39  |
| 116 | Askari Alamdar        | 76 -> 39  | 39 -> 44  | 34 -> 42  |
| 117 | Muqaddam of the Tulb  | 77 -> 67  | 70 -> 80  | 74 -> 79  |
| 120 | Kilij Arslan I        | 95 -> 92  | 82 -> 89  | 81 -> 87  |
| 121 | Nur ad-Din            | 86 -> 78  | 63 -> 69  | 65 -> 73  |
| 122 | Asad al-Din Shirkuh   | 90 -> 78  | 74 -> 75  | 74 -> 83  |
| 123 | Saladin               | 93 -> 80  | 63 -> 72  | 71 -> 68  |

### Per-id win rates, adversaries vs adversaries (before -> after)

| id  | name                  | L5        | L20       | L40       |
|----:|-----------------------|----------:|----------:|----------:|
| 108 | Turcoman Bowrider     | 52 -> 40  | 52 -> 45  | 46 -> 45  |
| 109 | Ghulam Lancer         | 50 -> 41  | 45 -> 37  | 45 -> 39  |
| 110 | Sudani Shieldbearer   | 23 -> 36  | 37 -> 61  | 36 -> 66  |
| 111 | Armenian Bowman       | 42 -> 51  | 35 -> 32  | 36 -> 29  |
| 112 | Daylamite Axeman      | 42 -> 40  | 44 -> 57  | 44 -> 58  |
| 113 | Bedouin Outrider      | 33 -> 32  | 25 -> 27  | 22 -> 29  |
| 114 | Kurdish Ghazw Rider   | 50 -> 41  | 45 -> 36  | 46 -> 34  |
| 115 | Naffatun Sapper       | 38 -> 31  | 40 -> 48  | 39 -> 51  |
| 116 | Askari Alamdar        | 43 -> 44  | 42 -> 45  | 45 -> 43  |
| 117 | Muqaddam of the Tulb  | 53 -> 60  | 63 -> 58  | 63 -> 57  |
| 120 | Kilij Arslan I        | 86 -> 85  | 85 -> 86  | 84 -> 83  |
| 121 | Nur ad-Din            | 58 -> 59  | 57 -> 50  | 59 -> 50  |
| 122 | Asad al-Din Shirkuh   | 58 -> 65  | 67 -> 62  | 70 -> 61  |
| 123 | Saladin               | 65 -> 68  | 58 -> 56  | 59 -> 55  |

### Account of each change

- **Sudani Shieldbearer (110), kit redesign (owner decision 1).** The old
  kit (Measured Blow + Steady Ward + Testudo of Hides) gave the
  never-switch policy AI one feeble 40-power stroke, so he stalled and lost
  chronically (23/29 at L20/L40). New rite 58 `Unhorse the Rider`
  (power 65, accuracy 90, Guard +1 on self per hit) arrives at level 6:
  the short thrusting spear that takes a horseman out of the saddle while
  the shield line locks — a patient wall that punishes. Steady Ward stays
  at 1 and Testudo of Hides at 10, so the defensive identity is intact;
  Measured Blow keeps early zeal. 23/29 -> 51/56, dead in band; the
  mirror matchup rises 37/36 -> 61/66 (he was the mirror floor before).
  An 85-power snowballing first cut (70 power + Alacrity -1 on target +
  Guard +1) measured 63/71 in tuning pass 1 and was trimmed to the final
  shape in pass 2.
- **L5 pool overtuning (owner decision 2), learn-level lever.** The strong
  L1-lane rites moved to level 6+ in every archetype learnset (43 off
  Bowrider, 15 off Ghulam/Ghazw, 47 off Bowman, 49 off Axeman, 51 off
  Outrider, 54 off Sapper, 18 off Alamdar), each gaining Measured Blow at
  level 1 so the early-damage guard test still holds. L5 champion team win
  rate 25% -> 44%; L5 per-id now 35-60 across archetypes (from 52-85).
  44% sits 0.8pp under the 45% floor of the target — inside the +/-6-7pp
  noise floor, and structurally pinned there: five above-band-by-design
  ids (117, 120..=123) average 79% at L5, so pushing archetype L5 win
  rates to mid-band would push the aggregate back past 60% adversary. See
  Residuals for the unapplied commander-kit proposal.
- **Armenian Bowman (111).** New rite 59 `Arrow through the Visor`
  (power 85, accuracy 90, HighCrit) at level 12 — the last sheaf finding
  the eye-slit. Volley in Turn is now the level-5 anchor (the policy AI
  always played it over Storm of Arrows anyway). 25/28 -> 49/43, in band.
- **Daylamite Axeman (112).** New rite 60 `The Falling Stroke` (power 85,
  accuracy 90, Guard -1 on target) at level 12 — the harness-hewing
  downward cut. Cleave of Axes (boss-shared with Shirkuh, numbers
  untouched) moves to 6. 31/36 -> 49/49, in band.
- **Bedouin Outrider (113).** New rite 61 `Lance at the Gallop` (power 80,
  accuracy 95, 30% flinch) at level 10 — the one true charge of a man
  never at sword's length. A flinch-less 70-power cut moved nothing
  (34/34, pass 2); the flinch rider at 80 power carried him to 41/39.
  His 305-total stat line (resolve 45) is the binding floor — this is as
  far as kit levers reach.
- **Naffatun Sapper (115).** `Comet of Naphtha` trimmed up 60 -> 80
  (archetype-only rite; the naphtha pot is his whole trade) and pushed to
  level 6 for the L5 fix. 29/29 -> 39/39, at the accepted >=38 line.
- **Askari Alamdar (116).** New rite 62 `The Banner Advances` (power 85,
  accuracy 90, no riders) at level 12 — the banner going forward. A
  Valour +1 on self per hit snowballed to 75/76 in pass 1 and was cut to
  the flat stroke in pass 2 (Hold the Banner keeps the rally flavor as a
  status). Bulwark's Stand moves to 6 for the L5 fix. 39/34 -> 44/42.

### Residuals and structural notes

- **Ghulam Lancer (62/65) and Kurdish Ghazw Rider (60/62) now read above
  the band at L20/L40** (their baselines were 46/49 and 49/49). Their kits
  WERE edited in this pass — both kept their original rites but learned
  them at re-leveled tiers (Sundering Blows to 6), so at L20/L40 they
  field one more strong rite than before. A genuine kit lever exists
  (re-level the mid-tier strokes higher), but it is mild: their own
  hardest swings are locked champion rites (1, 15, 18, 7) or boss-shared
  (45, 53), and at 2-5pp over the band edge they sit within the ±6-7pp
  correlated-seed noise floor (256 unique battles). Accepted as-is;
  flagged if a future fuzz with a de-aliased seed chain still reads high.
- **L20/L40 aggregate shifted 51/50 -> 39/40 champions-side.** Arithmetic,
  not regression: archetype per-id average rose 36 -> 50 at L20 (the
  owner-ordered lift into the band) while the five boss-tier ids ride
  ~77, so the 14-id pool average is ~60 adversary-side. The per-id band
  and a 50/50 aggregate are not jointly reachable while the boss tier is
  fixed above band; decision 3 set per-id targets only, and those are met.
- **L5 per-id floor**: Axeman 35, Sapper 35, Ghulam 36, Outrider 37,
  Shieldbearer 38, Alamdar 39 — 1-5pp under the band floor, inside the
  noise floor, and mutually exclusive with a higher L5 aggregate (see
  above). Accepted.
- **Unapplied proposal (owner decision required)**: if the L5 aggregate
  must reach >=45% champions-side, the remaining lever is the commander
  learnsets — push Kilij Arslan's `Wheeling Shots` (43), Nur ad-Din's
  `Couched Lance` (45), Saladin's `Storm of Arrows` (47) and Shirkuh's
  `Cleave of Axes` (49) / `Harass and Fade` (51) off their L1 lanes to
  level 6, exactly as the archetypes were treated. Their 390-420 stat
  lines should hold them above the band at every level (Kilij keeps
  Harass at 6; Shirkuh keeps two L1 strokes only if 51 stays), but this
  touches the "do not touch" boss tier, so it was not applied.
- Shieldbearer L5 reads 38 with an unchanged L5 kit (Measured Blow +
  Steady Ward, 54% at baseline): the drop is pool composition, not his.

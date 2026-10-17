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

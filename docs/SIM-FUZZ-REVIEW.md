---
title: "Review: crusade-sim adversary fuzz mode (GLM worker diff) + FUZZ-ADVERSARIES.md"
summary: >
  Review of the uncommitted crusade-sim/src/main.rs diff (HashMap tallies,
  new `fuzz <n> <level> adversaries` pool mode, champion mode preserved)
  and docs/FUZZ-ADVERSARIES.md. Code passes every check on the review card:
  champion-mode draw path is byte-identical logic, ADVERSARY_IDS matches the
  M7 id set exactly (108..=117, 120..=123; excludes in-flight-M7b 124/125),
  seed discipline unchanged, no new panic paths, determinism verified
  empirically. One MAJOR doc-level issue: the report treats 2000 runs per
  matchup as statistically independent, but seeds `[s as u8; 32]` alias mod
  256 — only 256 distinct battles exist per run, so the ±2.5pp noise claim
  and the Delta section's five-point agreement thresholds are understated
  ~3x. Two MINOR doc inaccuracies (Kilij stat total, avg-turns range).
tags: [review, fuzz, m7, crusade-sim, adversaries]
date: 2026-10-16
status: review
---

# SIM-FUZZ-REVIEW

Scope: uncommitted diff in `crusade-sim/src/main.rs` +
`docs/FUZZ-ADVERSARIES.md` (untracked). Verdict and issue counts at the end.

## Card checks

1. **Champion mode bit-for-bit preserved — PASS.** In `gen_team`
   (main.rs:141-152) the `Pool::Champions` arm calls
   `rng.gen_range(SPECIES_MIN..=SPECIES_MAX)` — the exact pre-change call.
   The `pool.ids()` allocation happens before the draw loop and consumes no
   rng, so the ChaCha8 stream for champion mode is unchanged. Verified
   empirically: `fuzz 20 5` (default pool) and `fuzz 20 5 champions`
   output identical; two back-to-back runs identical. (The doc's claim that
   `fuzz 300 5` / `fuzz 300 20` golden output is byte-identical to the
   pre-change binary cannot be independently reproduced here — the old
   main.rs no longer compiles against the mutated `ChampionInstance` —
   but the surviving logic is identical by direct hunk inspection.)
2. **New adversaries mode — PASS.** `fuzz <n> <level> adversaries` runs
   champions-vs-adversaries then adversaries-vs-adversaries, fresh `Stats`
   per matchup, failures summed into one exit code. Ran green at n=20 L5
   (exit 0, L5 adversary dominance reproduces the doc's direction).
3. **HashMap tally pairing — PASS.** `appear` in the all-members loop,
   `win` only for the winning team's members, keyed by `c.species_id` —
   identical attribution semantics to the old array; zero-division guarded
   by the `app > 0` filter in `print_stats_table`.
4. **ADVERSARY_IDS — PASS EXACT.** Constant (main.rs:39-41) =
   108..=117 + 120..=123; `adversaries.rs` contains 108..=117 and 120..=125.
   124/125 (Gokbori, Taki al-Din Umar) are Hattin sub-boss texture named
   without ids in HISTORICAL-FOES.md section 5 and are in-flight M7b work —
   correctly excluded and the exclusion is documented in both code comment
   and fuzz report.
5. **Seed discipline unchanged — PASS.** `let seed = [s as u8; 32];` is an
   untouched line; both matchups reuse seeds 0..n as documented.
6. **No panics on unknown ids — PASS.** `build_champion` returns `Err` for
   unknown species/rite ids; `print_stats_table` name lookup uses
   `map_or("?")`; `gen_range(0..ids.len())` indexes a non-empty pinned
   list. Pre-existing edge retained: `side_wins[w as usize]` would panic on
   an engine winner value > 1 (unchanged behavior, not this diff's risk).
7. **Build/test/run — PASS.** `cargo test -p crusade-sim` green (0 tests,
   as reported); `cargo clippy -p crusade-sim` clean;
   `cargo run -p crusade-sim -- fuzz 20 5 champions` exit 0, deterministic.
8. **Doc numbers internally consistent — PASS with exceptions below.**
   Matchup aggregates each sum to 2000 (496+1504, 1028+972, 997+1003;
   AvA splits 1001/999, 920/1080, 927/1073); percentages round correctly;
   matchup-1 appearances sum to exactly 6000 (= 2000 × 3); band-check text
   matches its own table at all three levels; Muqaddam/commander min-max
   (63-95) matches the tables.

## Issues

### [MAJOR] 1. Seed aliasing invalidates the report's noise analysis

`[s as u8; 32]` truncates mod 256, so a 2000-battle fuzz run contains only
**256 distinct battles**, each replayed 7-8 times. Empirically confirmed:
`fuzz 512 5 champions` side wins (232/280) are exactly 2× `fuzz 256 5`
(116/140). Consequences for FUZZ-ADVERSARIES.md:

- Run parameters claim "2000 seeded battles... ~318-534 appearances per
  adversary id... roughly +/-2.5pp of seed noise" — the ±2.5pp figure
  assumes ~428 independent observations; effective independent appearances
  are ~55/id, so 1σ noise is roughly **±6-7pp**, not 2.5.
- The Delta section's "six of ten ids agree within ~5pp" and
  "beyond-noise divergences" classification uses the understated noise
  floor. Robust at the corrected floor: Ghulam L20 (-17) and Bowman L20
  (-17) remain beyond noise; the ±5pp "agreements" (e.g. Bowman L40 -9,
  Sapper L40 -13) are no longer clearly within/beyond and the section's
  confidence language should be qualified.

This is a **doc problem, not a code regression**: the harness implements
exactly the seed discipline the review card pins (which is also how the
M7 worker's numbers were generated, so cross-run deltas remain apples-to-
apples), and champion mode must keep the aliasing to preserve historical
goldens. Doc fix only: restate unique-sample size, noise floor, and Delta
thresholds. (Widening seeds past 8 bits is a separate harness decision and
would break golden comparability.)

### [MINOR] 2. Kilij Arslan I stat total wrong

FUZZ-ADVERSARIES.md:113: "his 305-total stat line is mid-band." Actual
base (adversaries.rs:414) 70/75/45/65/60/85 sums to **400** — inside the
section-4 commander band (390-420), lower quarter, not 305. The "alacrity
85 kit dominates" claim is correct. Typo / stale number only.

### [MINOR] 3. "Avg turns 12-17" not backed by the doc's own tables

Run parameters states "Avg turns 12-17" but the aggregate table reports
12/13/14 for matchup 1 and gives no avg-turns figures for matchup 2, so
the 17 upper bound is unverifiable from within the document. Add the AvA
avg-turns numbers or drop the bound.

### [NIT] 4. Dead allocation in champion draw path

`let ids = pool.ids();` (main.rs:142) allocates a 12-element Vec on every
`gen_team` call even when `pool == Champions` and `ids` is never read.
Zero behavior impact; hoist into the Adversaries arm if touched again.

### [NIT] 5. Avg turns divides by n including failed battles

`total_turns / n` underweights when failures > 0. Pre-existing in champion
mode, unchanged; only matters on failure runs.

## Verdict

**FIX** — but the fix is doc-only. The code diff ships clean: all five
code-bearing card checks pass, champion mode is provably unchanged in draw
logic and determinism, and the new mode runs green. Issue 1 (MAJOR) lives
entirely in FUZZ-ADVERSARIES.md's statistical interpretation and must be
corrected before the report is cited as the section-4 band evidence; it
weakens but does not overturn the report's headline findings (L5
overshoot, commander dominance, L20/L40 low-stray all survive a corrected
noise floor).

Counts: 1 major, 2 minor, 2 nit (5 total).

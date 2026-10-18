# G8 Review — old-codec fixture / serde forwards-compat pins

Reviewer: subagent (kimi-k3-abliterated), 2026-10-18.
Scope reviewed: `git diff crusade-core/src/data/mod.rs crusade-core/src/types.rs docs/BUILDING.md docs/ROADMAP.md` only.

## Uncommitted files NOT reviewed (other workers in flight)

- `crusade-app/src/{bevy_shell/battle.rs,bevy_shell/main.rs,bevy_shell/screens.rs,client.rs,conversion.rs,pvp.rs,view.rs}`
- `crusade-app/tests/{net_client_tests.rs,pvp_tests.rs}` + untracked `crusade-app/tests/l5_commander_sim.rs`
- `crusade-bridge/src/{attest.rs,lib.rs}` + untracked `crusade-bridge/src/ladder.rs`
- `docs/{PRODUCTION-ARCHITECTURE.md,SHIPPING.md,ZAP-MONETIZATION.md}` + untracked `docs/LADDER-REVIEW.md`, `docs/LADDER.md`
- Untracked `crusade-core/src/data/baptismal_names.rs`
- ALSO NOT REVIEWED: non-G8 hunks inside two of my scoped files —
  `pub mod baptismal_names;` + `baptismal_names()` / `baptismal_name_for()`
  in `crusade-core/src/data/mod.rs`, and the V12-1 ladder paragraph rewrite
  in `docs/ROADMAP.md`. Both clearly belong to other in-flight cards
  (G6/M7-4 table stub; V12-1/LADDER-REVIEW). See "Editorial hygiene" below.

## Verdict: SHIP

(with two non-blocking flags at the bottom)

## 1. v1 shadow struct is field-exact vs the pre-M7 instance shape — PASS

`git show 4fa9137~1:crusade-core/src/types.rs` (the commit immediately
before M7's additive `converted` / `converted_from` fields; `typing_override`
arrived later in 4f5e2a7) yields exactly 9 fields:
`species_id: u16, level: u8, xp: u32, grace: GraceStats,
devotion: DevotionStats, calling_id: u8, rites: [Option<RiteSlot>; 4],
current_resolve: u16, ailment: Option<Ailment>`.

The test-local `ChampionInstanceV1` shadow (`data/mod.rs`) matches names,
types, and order exactly, and carries no `#[serde(default)]` /
`deny_unknown_fields` attributes — so the "field-exact" claim is
load-bearing-accurate: the test proves a real pre-M7 codec skips
`converted` / `converted_from` / `typing_override` wholesale and never
inspects the `"Valor"` string. The `VALOR_SAVE` fixture itself matches the
current serializer's emitted key set (the three additive keys present,
`Some`-valued), so it is a genuine v2 row, not a strawman.

## 2. Pins cover the real persisted paths — PASS

- Roster rows: `crusade-data/src/roster.rs` persists champions via
  `serde_json::to_string` / `serde_json::from_str::<ChampionInstance>`
  inside the automerge doc — the same derive-based serde_json entry point
  the tests exercise.
- Trail lane: `crusade-app/src/persist.rs` `TrailSave.party:
  Vec<ChampionInstance>` round-trips via `serde_json::to_string(save)` /
  `serde_json::from_str::<TrailSave>` — same derive path.
- There is no parallel codec (no bincode/postcard/hand-rolled reader) for
  these rows. `load_trail` additionally degrades malformed JSON / unknown
  `save_version` to `Ok(None)`, so the real app's failure mode is even
  softer than the pinned `Err` — consistent with, not contrary to, the docs.

## 3. Docs consistent with tests; no contradicting claims — PASS

- `docs/BUILDING.md` "Save-format compatibility" section accurately
  restates exactly what the three new tests prove (loads / `unknown
  variant` error / `missing field` error; no panic, no UB).
- `deny_unknown_fields` appears nowhere in the tree except the new test
  comment and a pre-existing guard comment in `persist.rs` (its
  pre-vanguard legacy-load test pins against a future
  `deny_unknown_fields` accident) — both point the same direction.
- `docs/ROADMAP.md` G8 row closure matches the tests. Note this supersedes
  the row's old suggested mitigation ("document 'v2 saves not loadable by
  v1'") — the pinned behavior is the opposite and better, so the closure
  text is the correct call.
- `crusade-core/src/types.rs` doc edit removes the stale justification
  ("old builds never meet it because instances serialize `species_id`
  only") and replaces it with the proven mechanism. Good.

## 4. Scope discipline — PASS for G8, with one caveat for the committer

`types.rs` and `BUILDING.md` diffs are purely G8. But the `mod.rs` and
`ROADMAP.md` diffs each contain one other worker's hunks (baptismal_names
production wiring; V12-1 ladder prose). That makes the GLM claim "no
production code changes" false at the file level — the G8 hunks themselves
are test+docs only. **Flag for orchestrator:** commit-split `mod.rs` and
`ROADMAP.md` by hunk; the ROADMAP batch discipline ("one card, disjoint
files") is violated by the interleaving as-is.

## Tests run

`cargo test -p crusade-core serde_tests` — 5/5 pass (including the three
new pins and the two pre-existing M7 fixtures).

## Non-blocking flags

1. **dead_code warning**: the shadow struct's unread fields trigger a
   `#[warn(dead_code)]` in the test build (fields must exist for the mirror
   to mean anything). No CI `-D warnings` gate found
   (`.github/workflows/platform-checks.yml` has no deny/clippy step), so
   harmless, but `#[allow(dead_code)]` on the shadow struct would keep the
   warning log clean.
2. **Pre-existing stale rationale, not introduced here**:
   `docs/HISTORICAL-FOES.md` §7 still says "old builds never meet it;
   instances serialize `species_id` only" — factually stale since M7
   (instances also serialize `converted`; since M8b, `typing_override`).
   The conclusion (old builds are safe) is now *proven* rather than
   asserted, so the section is not wrong, just under-justified. A one-line
   pointer to BUILDING.md's compatibility section would close the loop on
   a future docs pass; not required for this card.

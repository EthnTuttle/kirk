---
title: "V12-1 ladder-service review, round 3: peer-binding remedy verification (SHIP/FIX verdict)"
summary: >
  Re-review of the uncommitted V12-1 ladder after the round-2 FIX (R1
  agreeing-sockpuppet retro-laundering). The worker implemented the
  preferred remedy exactly: serde-default `peer` on kind-9265 content,
  mutual peer-naming in the fold, gates/docs re-scoped, new pins. I
  re-ran the R1 probe against the public fold_attestations MYSELF
  (scratch integration test, since deleted) — R1's single-sock costume
  is dead. Workspace 415/0 green (412 + my 3 probes), wasm32 gate
  passes. One residue survives in canonical folds ONLY: TWO colluding
  socks mutually naming each other can still squat an honest battle id
  and outrank the honest pair — the docs call this
  "impossible-in-practice", which is wrong (it takes one attacker with
  two keys); what is true is that it is fail-safe-deterministic,
  cursor-inert in production, and publicly observable. Plus two doc
  typos. Verdict: FIX (docs-only; no mechanism change exists or is
  needed).
tags: [review, v12-1, ladder, pvp, attestations, nostr, elo]
status: review
date: 2026-10-18
---

# V12-1 ladder review — round 3 (2026-10-18)

## Verification performed (self-ran, not trusted)

- Wrote a scratch integration probe
  (`crusade-bridge/tests/ladder_r3_probe.rs`, since deleted) against the
  PUBLIC `crusade_bridge::fold_attestations`, replaying R1 exactly:
  host seals at `T_h`, honest loser finalizes at `T_h + 3`, fresh sock
  publishes a fully AGREEING seal (same digest/winner/host) backdated to
  `T_h`. Result: **honest pair settles once (host 1016 / loser 984),
  sock `None`, in both stream orders** — and the table equals the clean
  fold byte-for-byte. A second variant (sock naming the LOSER as its
  counterparty, the maximal misnaming) is likewise inert.
- A third probe (below) exercised the two-pairs-squatting-one-battle-id
  residue: deterministic, cursor-protected — but NOT impossible.
- `cargo test --workspace` — **415 passed, 0 failed** (412 worker +
  my 3 probes). Includes the full ladder battery: 19/19
  `ladder::tests` (all four new pins green: `sockpuppet_retro_laundering_is_inert`,
  `peer_misnaming_breaks_the_pair`, `forged_second_seal_is_inert_for_everyone`,
  `old_seals_without_host_or_peer_never_fold`), `pvp_tests`
  (settle battery + two-device convergence at the exact 1031),
  `net_client_tests` (e2e duel: host/peer fields asserted on the wire).
- `cargo check -p crusade-app --target wasm32-unknown-unknown --features
  bevy-ui` — clean. The android CI lane is not locally reproducible
  (no NDK clang in this environment; the failure is in third-party
  `ring`'s build script before any ladder code compiles) — CI-gated, not
  a regression signal.
- Clippy: no gate exists; three NEW pedantic warnings in the diff
  (`needless mut` on the `named` closure, collapsible `if` at
  ladder.rs:290, 8/7 args on `NetDuelDriver::host`) sit among dozens of
  pre-existing workspace warnings. Cleanliness nit, listed below.

## (1) R1 — CLOSED, probe-verified

`try_settle` now requires each seal's `peer` to name the OTHER seal's
author (ladder.rs:215-224). In R1's costume the honest host seal names
J while the sock's seal names H: candidate (H, S) dies on
non-mutuality regardless of timestamp, agreement, or key grinding. My
independent probe (not the worker's pin) confirms: the honest
settlement stands, the sock stays `None`, and the fold table is
identical to the un-attacked fold in both stream orders. The worker's
own pin (`sockpuppet_retro_laundering_is_inert`) covers the same
ground permanently.

The mechanism remedy was implemented as recommended: `peer` is a
serde-default `Option<String>` tail on the SAME kind 9265 (no new
kind); seals lacking `peer` (round-1 vintage) or `host` (pre-V12-1)
never fold and that is pinned both ways
(`old_seals_without_host_or_peer_never_fold` loops both vintages). Gate
2's equivocation comparison now includes `peer` (ladder.rs:285-288), so
an author can't reseal with a different counterparty without voiding
themselves. Shell wiring is correct: `run_duel` sets
`host_pubkey`/`peer_pubkey` from the lobby outcome per role
(we_host: host=us; else host=outcome.peer; counterparty = outcome.peer
both ways), `finalize` signs both fields into the content, and the
e2e test asserts `host`/`peer` on both published seals.

## (2) Previously-closed findings — no regression

The mob battery is genuinely UPGRADED, not just re-aliased:
`mobbed_battle_still_settles_the_honest_pair` now feeds mob seals that
name the honest host `a` as their counterparty (maximal form) in both
agreeing and disagreeing flavors, and asserts mobbers stay `None`. The
retro-poison pin still proves void==withholding in both stream orders;
rolling==genesis on mixed history (damped 5-match series, mobbed,
equivocated, disagreeing) at every duel-boundary split; store-level
settle battery covers forge/tamper, self-sign, replay idempotence,
missing counter-seal, wrong-identity; two-device merge converges to
the genesis fold at exactly 1031 (pre-fix path says 1033). HIGH-1/2
(disagreeing costume)/HIGH-3/MEDIUM-1/2 and the round-2 nits remain
closed. Round-2's doc nit ("a later successful fetch completes it") is
reworded honestly ("if the seals are ever re-presented by hand the
settle completes exactly once").

## (3) R2-residue — the squatting pair is NOT impossible; it IS fail-safe-deterministic → docs must stop overclaiming

ladder.rs:36-39 and LADDER.md gate 3 call "two disjoint mutually-named
pairs squatting one battle id" an "impossible-in-practice residue" with
the (settled_at, a, b) rank as the safety net. The first half is wrong,
in a way that matters for the canonical-fold claim:

- One attacker with **two sock keys** (S, T) can fabricate a fully
  agreeing, mutually-naming pair on ANY published battle id: digest and
  host are relay-public, the pair need only agree with ITSELF, and
  `settled_at` can be backdated (skews are intra-pair) to strictly
  precede the honest pair's `max(created_at)` — no pubkey grinding
  needed. In every canonical (genesis/audit) fold, the squat pair then
  outranks and displaces the honest settlement: R1's launder survives
  at one extra key of cost. Probe-verified deterministically: fold picks
  the squat pair when it ranks earlier.
- What IS true (probe-verified): the fold's answer is a TOTAL ORDER —
  one settlement per battle, identical table in every stream order; and
  production is unaffected — the persisted cursor means a settled
  battle is never reprocessed (probe: refold over the squatting stream
  WITH the cursor reproduces the honest 1016/984 table exactly), and
  device sync carries the automerge blob, so stored rows keep the honest
  settlement everywhere it was actually earned.
- The squat is also publicly observable: two disjoint mutually-agreeing
  pairs on one battle id cannot arise from real duel flow, so an audit
  lane (or the future read-only standings recomputation of
  PRODUCTION-ARCHITECTURE.md) can flag the forgery class precisely. And
  the same two socks could mint unlimited rating on FRESH battle ids
  regardless (sybil self-minting is inherent to any pure seal-fold
  ladder) — squatting adds exactly one extra power: erasing an honest
  settlement from canonical refolds.
- No fold-side mechanism retires this (folds can't verify a transcript
  ever existed behind a digest; participant-binding the digest at seal
  time is a client-side convention the fold cannot check). The honest
  remedy is docs: drop "impossible-in-practice", describe the residue
  accurately (two colluding keys, deterministic canonical choice,
  cursor-protected stores, observable anomaly), and say so in LADDER.md
  "What this does not solve".

## (4) Doc-vs-code mismatches found

| # | Where | Claim | Truth |
|---|---|---|---|
| F1 | LADDER.md gate 3 + ladder.rs:36 | squatting pair "impossible-in-practice" | possible with two colluding keys; deterministic + cursor-inert + observable (see (3)) |
| F2 | LADDER.md gate 3 line: "(59 s skew cap: `MAX_PAIR_SKEW_SECS` = 1800 s)" | "59 s" | constant is 1800 s — the sentence contradicts itself inline; typo introduced in this round's re-scope |
| F3 | SHIPPING.md limitation 4 | "counter-seal fetch retry window is ~18 s" | 3 attempts × 6 s timeout + 2 × 4 s gaps = worst case ≈26 s (round-2's own arithmetic said ≤26 s); typical is much faster, but the number is wrong |
| — | clippy nits | ladder.rs needless-`mut`, collapsible `if`; pvp.rs 8/7 args | 3 pedantic warnings in new code, no lint gate, pre-existing noise dwarfs them; tidy while touching the file |

What DOES now match exactly: the gate texts (1-4) against
`read_verified`/`AuthorSeal`/`try_settle`/damping, the persistence
section against `settle_ladder`/`load_ladder_state` (single atomic
`ladder.state`, legacy seed-only, fail-open-never-mints), the
prompt-vs-canonical divergence text, the ROADMAP V12-1 entry (accurate,
review-cross-referenced), PRODUCTION-ARCHITECTURE ("mob seals are inert
extras" is finally TRUE — third-party seals cannot form a pair at all),
and ZAP-MONETIZATION's link update. The `fetch_counter_seal` flow
matches doc step 2 (author=peer, d=battle, retry-bounded,
signature-pre-screened, client's fold arm runs the real gates).

## Disposition table

| Item | Disposition |
|---|---|
| R1 single-sock retro-laundering | **CLOSED**, independently probe-verified + worker pin |
| HIGH-1 retro-poison | still **CLOSED** (pin intact, peer now in the conflict check) |
| HIGH-2 disagreeing mob | still **CLOSED**, battery upgraded to maximal peer-naming mobs |
| HIGH-3 rolling≠genesis | still **CLOSED**, incl. exact-1031 two-device convergence |
| MEDIUM-1 fetch window | mechanism closed (bounded retry + idempotent refold); **new nit F3** on the stated window size |
| MEDIUM-2 atomic state | still **CLOSED** |
| Round-2 doc nits | both reworded accurately |
| Old-seal inertness (both vintages) | **PINNED** (`old_seals_without_host_or_peer_never_fold` loops host-absent AND peer-absent) |
| Misnaming / forged maximal agreement | **PINNED** (`peer_misnaming_breaks_the_pair`, `forged_second_seal_is_inert_for_everyone`) |
| Squatting two-pair residue | **R2-residue**: deterministic & cursor-safe but practical with two socks — **F1** doc overclaim |
| "59 s skew cap" | **F2** typo |
| clippy pedantry | nit, non-blocking |

## Verdict

**FIX — docs-only.** The code remedy is complete and probe-verified:
R1's costume is dead, the previous battery holds upgraded, and
everything pinned green (workspace 415/0, wasm gate clean). What
remains is that the docs currently claim MORE than the peer-binding
actually delivers: fix F1 (reword the squat residue + honest-limits
entry), F2, F3, and sweep the three clippy pedantries while editing.
No mechanism change is possible or required for the residue; the
determinism + cursor + observability properties are the settlement —
but the owner must read them as they are, not as "impossible".

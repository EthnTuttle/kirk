---
title: "V12-2 wager-escrow review: round 1 (SHIP/FIX verdict)"
summary: >
  Adversarial review of the uncommitted V12-2 wager escrow
  (crusade-bridge/src/escrow.rs, new; ladder.rs refactored to export
  canonical_settlements + AttestedSettlement; docs/WAGERS.md; SHIPPING
  limitation-2 edit). All six claims (a)-(f) verified BY RERUN and by
  reading the vendored CDK mint's NUT-11 verifier, not by trusting the
  worker's tests: 39 bridge tests green on my own run, ladder gate
  bodies byte-diffed against HEAD (equivalent modulo rename + a
  needless-mut removal), wasm gate re-run WITH escrow.rs touched and
  clean, and I wrote three scratch probes of my own (since deleted):
  terms-now-skew diverges the mint lock conditions (real finding),
  squat-pair settle-blocking fails CLOSED with full post-window refunds
  (inherited LADDER R2-residue, no theft class). Verdict: FIX —
  docs-only. One MEDIUM doc gap: WAGERS.md never states that the two
  clients must agree on byte-identical WagerTerms (refund_after derives
  from the local `now` at terms time), so independently-composed terms
  yield a pot that can lock but never settle. No mechanism change is
  needed anywhere.
tags: [review, v12-2, wagers, escrow, cashu, nut-11, ladder]
status: review
date: 2026-10-19
---

# V12-2 wager-escrow review — round 1 (2026-10-19)

## Verification performed (self-ran, not trusted)

- `cargo test -p crusade-bridge` — **39 passed, 0 failed** (includes the
  full escrow battery on the real in-memory CDK mint: honest
  lock/settle/payout, no-counterseal window → full refunds with a real
  3 s wall-clock sleep, forged/mob/retro-poison all inert, replay
  rejected, winner-alone 1-of-2 mint-rejected, fold-equality pin).
- `cargo test -p kirk` (133) and `crusade-app` `pvp_tests` +
  `net_client_tests` (6) — green; the ladder consumers are untouched.
- **Wasm gate re-run adversarially**: `touch escrow.rs ladder.rs &&
  cargo check -p crusade-app --target wasm32-unknown-unknown --features
  bevy-ui` — recompiled crusade-bridge FOR WASM with escrow in the
  module tree, **clean**. (cdk was already in the app's wasm graph via
  the root kirk crate `GameMint`, so no new crate class entered.)
- **Gate-body equivalence diff (claim f)**: extracted the HEAD
  fold-attestations gate 1-3 body and the new `canonical_settlements`
  body; after the mechanical `Settlement`→`AttestedSettlement` rename
  they are identical line-for-line except doc comments and the
  `already_settled`-cursor skip (which moved with the function, still
  parameterized). `fold_attestations` now does exactly
  `canonical_settlements()` + the unchanged gate-4/Elo tail. The only
  other ladder.rs change is `let mut named` → `let named` on a pure
  closure — no capture mutation, no semantic change. The single clippy
  warning in crusade-bridge (collapsible-if at ladder.rs:332) exists
  verbatim in HEAD (line 295) — pre-existing, not introduced here.
- **CDK mint semantics read at the source** (`deps/cdk/crates/cashu/
  src/nuts/nut11/mod.rs`): `verify_p2pk` enforces, before locktime,
  `num_sigs` (2) signatures over the authorized set {data} ∪ {pubkeys}
  = the two duel keys; at/after locktime refund keys SUPERSEDE with
  `num_sigs_refund.unwrap_or(1)` — i.e. depositor-alone refund is
  mint-enforced, and the mint (not the client) owns the clock check.
  This matches every claim in WAGERS.md/escrow.rs docs.
- Three scratch probes I wrote myself (since deleted):
  1. `probe_terms_now_skew_changes_lock_conditions` — PASS: two
     clients composing `WagerTerms` 1 s apart produce DIFFERENT NUT-11
     lock conditions (locktime = refund_after feeds the secret). See
     M1.
  2. `probe_mismatched_terms_escrow_rejects_other_sides_locks` — PASS:
     an escrow object holding the other side's terms sees honest
     depositor locks as non-matching (proof matching compares parsed
     conditions to `terms.deposit_conditions`, nonce-ignoring but
     locktime-sensitive).
  3. `squat_pair_blocks_settle_refunds_both_sides` — PASS: two
     colluding sock keys, mutually naming, backdated, self-agreeing on
     the escrowed battle id, DO displace the honest settlement in the
     canonical fold (inherited LADDER R2-residue) and the escrow
     verdict is **Pending — never Settle, never mis-pay** — and
     post-window both depositors refunded in full on the real
     in-memory mint. Fail-closed.

## Claims (a)-(f) disposition

| Claim | Verdict |
|---|---|
| (a) NUT-11 P2PK 2-of-2 over the two duel nostr pubkeys + timelocked depositor refund, mint-enforced | **VERIFIED** — `deposit_conditions`: data=counterparty, pubkeys=[depositor], num_sigs=2, locktime=refund_after, refund=[depositor]; CDK verifier semantics match the doc text; `winner_alone_cannot_spend_the_pot` proves 1-of-2 rejection at raw mint level; post-window single-sig refund proven against the real mint with real wall-clock sleep |
| (b) Adjudication consults the SAME fold gates via shared helpers | **VERIFIED** — escrow calls `ladder::canonical_settlements` (extracted, not copied; diff-proven), and `adjudication_and_ladder_agree_on_winner_identity` pins verdict-winner == fold-winner == ladder rating direction |
| (c) honest settle pays winner once; no-counterseal refunds both; forged/mob/equivocating move nothing | **VERIFIED** — all four legs green on my run, including mint-level double-spend rejection on replay and post-settle refund rejection |
| (d) winner alone can't spend the pot | **VERIFIED** — mint-level rejection of a single witness on a 2-of-2 lock |
| (e) clock only via `now` params (wasm-safe); CRUSADE_MINT_URL with test-mint sentinel default | **VERIFIED** — module takes `now: u64` everywhere; tests use real time only because the in-memory MINT enforces locktime against real time (documented in the test kit); `config_defaults_to_test_mint` pins the sentinel default |
| (f) ladder lane unbroken | **VERIFIED** — gate bodies byte-equivalent (see above); all ladder/pvp/net tests green; no behavioral drift found |

## Chases from the brief

- **Malfunctioning pot via under-funding / wrong conditions**: `lock`
  enforces exact-balance funding (`AmountMismatch`) and builds the
  conditions itself — the depositor cannot inject conditions, so the
  depositor side of this class is dead. The attendant-fed `locked` pot
  in `settle`/`refund` goes through `check_locked` (exact total + every
  proof must match one side's exact lock conditions, nonce-ignoring)
  before any signature/mint contact, and the mint would reject anything
  else anyway — fail-closed twice. The ONE live trap is M1 below.
- **Timeout arithmetic**: `now.saturating_add(grace)` (saturating);
  all window compares are pure comparisons. The `2 * sats_per_side`
  and `proofs_total` sums could, in a debug build, panic on sums ≥ 2^64
  — that requires a mint issuing ≥ 2^63-sat denominations (no sane
  keyset does; test mint caps at 2^31); release wraps → comparison
  mismatch → fail-closed error. N2 nit, theoretical only.
- **Grace window mint-enforced?** Yes — CDK's `verify_p2pk` checks
  `now >= locktime` against the mint's own clock; the client's
  `now`-gated pre-checks only type the error early. Post-locktime the
  2-of-2 is superseded refund-side (depositor key ∈ refund keys), so a
  late cooperative spend succeeds only as a refund to depositors —
  which is the intended outcome anyway. No theft class.
- **Empty-cursor `adjudicate`**: read-only over the fold, mutates no
  ladder state. It CAN settle a battle the ladder cursor already
  folded — correct: the pot is still locked and both duelists sign;
  replay is stopped by the mint's spent-input set (proven by
  `settled_escrow_cannot_pay_twice`). Worker's "intentional" claim
  holds.

## Findings

### M1 (MEDIUM, docs) — WagerTerms identity is an unenforced, undocumented precondition

`WagerTerms::new` derives `refund_after = now + grace_secs` from the
CALLER's local `now`, and `deposit_conditions` embeds `refund_after` as
the NUT-11 locktime. Probe-verified: two clients composing terms
independently — 1 s of clock difference, or `CRUSADE_WAGER_GRACE_SECS`
overridden on only one side — produce **different lock conditions**;
the attendant's `check_locked` then sees one side's honest deposit as
`ForeignLock`, and `settle` can never run. The pot is recoverable only
if each depositor refunds through an escrow object carrying THEIR OWN
terms — an attendant holding one canonical object cannot do it for
them. Nothing negotiates or even compares terms today (`WagerTerms` is
`Serialize + Eq`; the machinery to pin this exists and is unused).

The escrow is not UI-wired yet, so nothing ships broken — but
WAGERS.md is the spec the wirer will follow, and it is silent. Fix:
add a "terms agreement" precondition section: ONE canonical
`WagerTerms` must be proposed (by the host, alongside the lobby/challenge
flow), exchanged, and verified byte-identical (e.g. hash-compared) by
the joiner BEFORE any deposit; deposits under divergent terms must be
refused client-side. Optionally note that deriving `refund_after` from
a duel-start timestamp both parties already agree on would make the
terms deterministic; that is a v1.1 design nicety, NOT required for
this verdict — the doc precondition alone closes the trap.

### N2 (nit) — squat-pair griefing interaction deserves one line

The LADDER R2-residue (two colluding keys squatting a battle id) maps
onto the escrow as **settle-blocking, never mis-payment** (probe 3
above): the pot degrades to post-window full refunds, i.e. exactly the
already-documented "dispute = refund both sides" outcome, reachable
without the loser even participating. Since a withholding loser already
reaches the same outcome, the residue adds no NEW power — but a reader
of the "Failure modes" list could reasonably wonder. One sentence
("colluding-pair squat: blocks settlement, funds refund in full —
never pays the wrong key") closes it.

### N3 (nit, cosmetic) — `num_sigs_refund: None` vs doc's "n_sigs_refund = 1"

Code passes `None` and relies on the NUT-11 default (verified: mint
uses `unwrap_or(1)`); WAGERS.md states the effective value. Accurate
as prose; if the docs get touched for M1 anyway, write "(mint default)"
once.

## What checks out clean

- Secret hygiene: each swap output re-converts conditions through
  `Nut10Secret` whose `From` impl mints a fresh random nonce (verified
  in vendored cashu), so no shared-Y collisions across denominations —
  the code comment's claim is true. Payout/refund outputs are fresh
  bearer secrets (`assert_bearer` pinned).
- `cashu_pubkey` (02 + x-only) round-trips through actual Schnorr
  spends end-to-end on the mint.
- `check_locked` ordering: custody bounded before signing, before mint
  contact.
- SHIPPING.md limitation-2 edit is accurate in every verifiable
  particular (escrow exists, not wired into the client, end-to-end
  test coverage on the in-memory mint, no rake, refund-on-dispute,
  deployment-time env config). ROADMAP G2 resolution is the owner's
  call and out of review scope.
- Escrow edges verified: terms validation (0 sats, bad id, same key
  twice, uppercase hex, third-party depositor), settle-before-window
  blocked, refund-before-window blocked, settle-after-window blocked,
  other-battle settlements in the same stream inert.
- No new clippy warnings attributable to this diff (the one
  crusade-bridge warning is pre-existing in HEAD).

## Verdict

**FIX — docs-only.** Every mechanism claim (a)-(f) verified by rerun,
source-read of the vendored mint, byte-level gate-diff, and three
independent probes; the escrow is conservative in exactly the right
direction (every novel failure mode I could construct degrades to
full-refund). What remains before wiring is one paragraph of spec:
**M1** (canonical-terms agreement precondition in WAGERS.md), plus the
two one-line nits N2/N3 while the file is open. No code change is
needed or recommended for this round.

# Wager Escrow (V12-2)

Duel attendant escrow: two duelists lock N sats per side; a mutual-peer
attested battle settlement (the kind-9265 machinery of docs/LADDER.md)
releases the whole 2N pot to the winner; anything else refunds both
sides after the window. **v1: no fee, no rake.** Code:
`crusade-bridge/src/escrow.rs` (`WagerEscrow`, `WagerTerms`,
`EscrowConfig`, `EscrowVerdict`, `EscrowError`).

## Construction (chosen + why)

**NUT-11 P2PK 2-of-2 multisig over the two duel pubkeys, with a
timelocked depositor-only refund.** Each deposit proof carries:

- authorized pubkeys = `{depositor, counterparty}`, `n_sigs = 2`
  (before `refund_after` the mint requires BOTH duelists' Schnorr
  witness signatures to spend);
- `refund = {depositor}`, `n_sigs_refund = 1` (after `refund_after` the
  depositor alone refunds — mint-enforced: the refund path supersedes);
- `sig_flag = SIG_INPUTS`.

Why this over the card's alternative (hashlock-to-result + timelocked
refund): an HTLC has ONE preimage but a duel has TWO possible winners,
so the preimage would have to live with a custodial adjudicator key —
strictly more trust and strictly more machinery. The 2-of-2 uses the
same two nostr keys that already seal kind-9265 attestations, and CDK
verifies it natively. An honest sealed settlement is by construction
the 2-of-2 signature pair; a withheld or poisoned seal folds to "no
settlement", and then the refund window pays both sides back.

Adjudication NEVER trusts a bare claim: `WagerEscrow::adjudicate` folds
the seal stream through `ladder::canonical_settlements` — the exact
shared gate product (well-formed, per-author equivocation, mutual
peer-naming, agreeing digest/winner/host, skew window) the ladder rates
on — refactored out of `fold_attestations` for reuse, not copied. The
escrow pays only when a settlement whose participant pair IS the two
escrowed duelists folds for THIS battle id. Fold-equality pin (test):
the winner the ladder would rate up is the winner the escrow pays.

## Terms agreement (hard precondition)

`refund_after = now + grace` is derived from the *local* clock and env at
`WagerTerms::new`. If the two sides compose terms independently with even
a 1-second skew, they get different NUT-11 lock conditions: the pot locks
but can never settle (mismatched locks are `ForeignLock`), and each side
must refund separately after the window. Therefore: **terms are composed
once, exchanged, and byte-compared (WagerTerms is Eq + serde) before
either deposit.** The caller is responsible for that handshake; the
escrow layer refuses to paper over mismatched terms.

## Failure modes (all fail-closed, tested end-to-end on a test mint)

- **Withheld counter-seal / timeout**: `adjudicate` stays `Pending`,
  `settle` returns `NoSettlement`, `refund` before the window returns
  `RefundNotOpen`; after the window both sides refund in full.
- **Disagreeing / forged seals, equivocation (retro-poison), mob
  seals**: fold to nothing; `settle` refuses; refunds on timeout.
  (Dispute = plain refund both sides. v1 burns nothing: simplest
  defensible — see "v1 limits".)
- **Replay**: a settled pot cannot pay twice — the mint rejects the
  re-spend of the burned locked proofs; a post-settle refund of the
  spent lock is likewise rejected.
- **Winner alone**: a single witness signature on a 2-of-2 lock is
  rejected by the mint.
- **Two-key squat grief**: an attacker who fabricates a disjoint
  mutually-named seal pair on the escrowed battle id (the ladder's
  documented residue) can *block* settlement — `adjudicate` fails closed
  to `Pending` and both depositors refund in full after the window.
  Funds are never mis-paid; the grief costs time only.
- **Late settlement**: once `refund_after` passes, `adjudicate` returns
  `Expired` and `settle` is dead (`SettlementClosed`) — the refund path
  supersedes at the mint anyway.

## Config seam (G2 resolution)

`EscrowConfig::from_env`:

- `CRUSADE_MINT_URL` — Cashu mint base URL. Default:
  `test-mint://in-memory` (the in-process test mint; the caller maps
  the sentinel to a freshly constructed in-memory CDK mint, the
  `GameMint::new_test` precedent).
- `CRUSADE_WAGER_GRACE_SECS` — escrow lifetime override (settle-before
  / refund-after window). Default 48 h.

G2 resolved 2026-10-19: the owner runs a self-hosted CDK mint.
**Pointing the escrow at that production mint is a config line
(env var) — never a code change.** The `WagerEscrow` token plumbing
takes a `cdk::Mint` handle from the caller; nothing about the
construction, the terms, or adjudication depends on which mint that is.

## API surface

```
WagerEscrow::new(WagerTerms, EscrowConfig)          // validates terms
WagerTerms::new(&config, battle, sats, a, b, now)   // refund_after = now + grace
escrow.adjudicate(seals, now)  -> EscrowVerdict::Pending | Settle(winner) | Expired
escrow.lock(mint, depositor, funding, now)          -> locked proofs (2-of-2)
escrow.settle(mint, seals, locked_pot, sk_a, sk_b, now) -> winner payout proofs
escrow.refund(mint, locked_side, sk_depositor, now) -> refund proofs
```

Clock discipline: unix time enters only via caller-passed `now`
(wasm-safe; the mint enforces the locktime, the module pre-checks so a
rejected spend is a typed `EscrowError`).

## v1 limits (stated plainly)

- **No rake.** Pot = exactly `2 * sats_per_side`; winner takes it
  whole. Adding a house cut later is an explicit protocol change.
- **Refund-on-dispute, not slash.** Conflicting seals do not burn or
  forfeit; both sides are made whole after the window. The loser who
  refuses to sign forfeits NOTHING monetarily — the wager is void, his
  ladder rating and reputation still take the documented LADDER.md hit
  for a withheld seal. If that incentive hole proves real, the upgrade
  path is NUT-11 refund-key policy (e.g. escrow-held adjudicator refund
  key replacing depositor-only refunds), a deliberate v2 decision.
- **Cooperative release.** Settling before the window requires BOTH
  duelists' witness signatures over the spend — same keys that seal.
  An honest loser signs; the timeout exists for everyone else.
- **Mint trust.** Cashu ecash implicitly trusts the mint for solvency/
  unforgeability (standard ecash caveat); the escrow adds no mint-side
  logic beyond stock NUT-11 enforcement, so ANY conforming mint —
  including the owner's — is a drop-in.
- **Attendant custodies the locked proofs between lock and settle/
  refund.** The custodian cannot spend them (2-of-2 / refund-to-owner
  conditions), only lose them; a v2 should keep per-side proof custody
  with the depositors' clients.

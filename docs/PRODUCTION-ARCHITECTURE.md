---
title: Production Architecture — Server-Side Requirements for DEUS VULT
summary: >
  What running this game for real players actually needs server-side:
  Cashu mint decision, nostr relay topology, iroh rendezvous, ladder and
  transcript arbitration, distribution/ops, minimum-vs-full topologies with
  cost/ops burden, and a verdict on nonfungible.cash.
tags: [architecture, production, cashu, nostr, iroh, ops, monetization]
date: 2026-10-16
status: architecture-assessment
---

# Production Architecture

Ground truth sources: `PLAN.md` §5 (trust model), `docs/SHIPPING.md`
(roadmap v1.0–v2.0), `docs/V1-SHIP-PLAN.md` (WS-A/B/C), `docs/
ZAP-MONETIZATION.md` (§2 integrity firewall), kirk `src/cashu/mint.rs`
(GameMint = CDK-mint wrapper), `src/events/mod.rs` (kind map),
`crusade-bridge/src/attest.rs` (kind 9265), `crusade-net/src/
nostr_transport.rs` (kind 9264, wasm fallback) and `iroh_transport.rs`
(ALPN `/deus-vult/duel/0`, `presets::N0`).

The stack is deliberately near-serverless: game truth is a deterministic
engine + countersigned transcripts, and "we run no money servers"
(ZAP-MONETIZATION §3). Production server load is therefore a question of
trust choices, not compute. The big fork is whether Cashu wagers (v1.2)
ship — that is the only feature that forces us to operate a money service.

## 1. Do we need to run a Cashu mint?

**Game tokens (RNG commitments).** The kirk trust model needs *a* mint
that blind-signs tokens whose proofs players unblind; the C value is the
commit-reveal entropy. Crucially, the mint does not need to know the token
value — game tokens can be 1-sat (or any unit's) denominations. Any
honest public mint works for entropy: a player with tokens from *any*
mint gets unpredictable-until-reveal C values. **No dedicated mint needed
for RNG.** (Caveat: cross-mint tokens in one challenge need a per-mint
keyset allowlist/verification step; simplest is the challenge event naming
the accepted mint.)

**Reward tokens (P2PK, kind 9263).** `GameMint::mint_reward_tokens` and
`publish_game_result` are NOT implemented — both return
`MintError("requires full CDK integration")` today. Whoever operates the
mint rewards come from is the payout authority and must custody sats.
The `winner_pubkey` P2PK lock is real (NUT-11) but the wrapper is a
skeleton.

**Wagers (v1.2).** Escrow + "mint arbitration" (kirk `Game` trait
adapter replays duel chains to verdict) means the arbitrator is the mint
operator, or a service the operator runs. This is where "no central game
server" in the README stops being true: the mint is a trusted custodian
and referee. Honest reading of the codebase: arbitration code (replay to
verdict) is done; escrow wiring is explicitly deferred (SHIPPING limits
2, plan M6 note "wager escrow wiring").

**Zaps.** Fully independent of any mint (NIP-57, external LNURL). Verdict:
no mint needed.

### Verdict (mint)

- **v1.0–v1.1: do NOT operate a mint.** Point at an established public
  mint for game-token entropy; zaps cover monetization.
- **v1.2 wagers: operating a mint is unavoidable if we want settlement.**
  Obligations/risks: (a) **custody** — the mint holds sats backing all
  outstanding proofs; insolvency/deletion = player loss, reputational and
  possibly legal (money-transmitter/MSB exposure varies by
  jurisdiction; gambling features multiply it); (b) **Lightning backend**
  — CLN/LND/LDK node, liquidity, channel management, uptime, or a paid
  LN backend (LNbits-style) with its own custody trade-off;
  (c) **key management** — the mint's signing seed signs everything;
  HSM/KMS or `cdk_signatory` + offline backup, keyset rotation policy;
  (d) **deniability** — a wager/settlement mint should not be the public
  random number mint, to limit blast radius. Use `cdk`/`nutmix` with a
  documented runbook; budget this as a *financial service*, not a game
  server. Cheapest honest alternative: **skip Cashu wagers; keep zaps
  as the entire monetization path** (ZAP-MONETIZATION §4 already notes
  zaps alone are complete). Recommended: defer wager decision until
  there is real PvP volume demanding stakes.

## 2. Nostr relay topology

Kind map: 9259 challenge, 9260 accept, 9261 move, 9262 final, 9263
reward (all kirk), 9264 duel transport (NIP-44 encrypted),
9265 attestation (public, `d`-tagged by battle id), 9734/9735 zaps.

Persistence requirements:

- **9264** (duel transport): live delivery only; ephemerality acceptable.
  `nostr_transport.rs` caps relay latency at 30s. Public relays work.
- **9259/9260** (lobby): need short retention (minutes–hours so a
  challenger can be found). Public relays store everything by default;
  contest is fine.
- **9265 attestations + 9261/9262 move/final rail: need durable
  persistence** — they are the dispute-resolution record. Public relays
  garbage-collect, prune, and rate-limit custom kinds. For v1.1 PvP
  without wagers, attestations double as the spectator/witness record
  and the (future) arbitral record; loss degrades, but does not break,
  honest play.

Topology recommendation: **default public relay list (already in
`crusade-app/src/client.rs`: relay.damus.io, relay.nostr.band) + one
operator-run relay** (strfry or nostr-rs-relay, ~$5–10 VPS or free tier;
wss only) added to the client default list, with NIP-42 auth optional
and a whitelist of kinds 9259–9265. Client already supports relay-list
override (`net.relays` setting). Self-hosted relay's job: guaranteed
retention of 9265 and lobby kinds, and a known-good peer for 9264.

## 3. iroh rendezvous/relay

`iroh_transport.rs` binds with `presets::N0` = **n0's public relay
servers + n0 DNS discovery**, used as QUIC hole-punch relays when direct
paths fail. So NAT traversal currently depends on a third party's
production relays at production load. Options: (a) accept the n0
dependency (documented; iroh degrades to no-ticket/connect failure),
(b) run our own iroh relay + DNS discovery node (single cheap VPS,
iroh's `iroh-relay` binary; tickets then carry our relay URL), or (c)
lean on the nostr relay transport (kind 9264) which is wasm-safe, always
works through NAT, and already the default for browser builds. **MVP
answer: nostr transport as primary, iroh with default n0 preset as a
native-only upgrade; do not operate an iroh relay until latency becomes
a measured player complaint.** If we do run one later it's one small
stateless daemon + DNS record.

## 4. Matchmaking / ladder / transcript archive

- **Matchmaking**: nostr lobby (9259/9260) per V1-SHIP-PLAN WS-B — no
  central server, relay-mediated. Fine.
- **Ladder**: Elo lives in the *player's own automerge store*
  (`pvp.rs: record_ladder_result`, `ladder.rating` setting, default 1000)
  = self-scored honor system (SHIPPING limitation 4). A credible public
  ladder needs an aggregator: read kind-9265 attestations (both pubkeys,
  consistent digests/winner) from the retention relay and recompute Elo —
  that is a **small read-only cron/worker against our relay, not a trusted
  service**: it publishes a signed ranking event anyone can recompute.
  Do not build an API with private ladder state; it would violate the
  trust model's spirit and create an oracle players must trust.
- **Transcript archive**: for mint-side replay arbitration (v1.2), the
  arbiter needs the full duel transcript, not only the digest. Today
  transcripts live in players' CRDT stores. The honest design is
  **fetch-on-dispute** (both parties already hold the countersigned
  transcript; a claimant who withholds it loses the dispute), not a
  permanent archive of all matches — smaller storage, smaller attack
  surface, fits the privacy register. Only persist 9265 attestations
  (tiny, public).

## 5. Distribution / ops

- **Web**: trunk static build → GitHub Pages via `.github/workflows/
  web-pages.yml` (pinned `trunk@0.21.14`), `--public-url /kirk/`,
  PWA + SW. No servers. One human gate: Pages source toggle (WS-A A0).
- **Mobile**: cargo-mobile2 Android/iOS shells (v1.3); signing keys in
  CI secrets, Play/App-Store listings, and per ZAP-§5 risk 4 the donation
  link must be reviewed against store external-payment rules.
- **Update channel**: PWA redeploys atomically on push (SW takes next
  load). CRDT roster format versioned by storage key
  (`deusvult.roster.v1`) — bump the key and migrate in code on schema
  change; never mutate in place. Mobile: store release trains.
- **Crash/analytics**: none by default — the privacy register (ecash,
  pseudonymous nostr keys, E2E duel transport) makes a third-party
  SDK telemetry pipeline discordant. Use opt-in, self-hosted, aggregated
  crash reports (event-free; release hash + panic message only) or none
  at v1; player-facing bug report link (GitHub issues) instead.

## 6. Topologies

**Minimum viable (honest, v1.0–v1.1) — ~$0–15/mo, ~0.5 day/mo ops:**

| Component | Choice |
|---|---|
| Game hosting | GitHub Pages (free) |
| Lobby/duel/attest | Public relays (defaults) + 1 cheap VPS running strfry (retention for 9259–9265) |
| RNG entropy | Existing public Cashu mints; none operated |
| Money | Zaps only; owner's external LNURL/NIP-05; zero custody |
| iroh | n0 public relays (dep documented); nostr transport primary |
| Ladder | Honor-system local Elo; |

**Full (v1.2 wagers + public ladder) — ~$50–300/mo + real legal/ops
burden:** everything above plus a CDK mint (signatory custody, LN
backend + liquidity, backups, key rotation), the arbitration adapter
deployed as a service consuming 9265 attestations and challenged
transcripts, a signed public ladder recomputation service, pref. own
iroh relay, monitoring/on-call, and legal review of custodial wagering.

| Burden | Min | Full |
|---|---|---|
| Monthly cost | $0–15 | $50–300 + LN liquidity |
| Weekly ops time | <1 h | 5–15 h (payments incident response) |
| Custody risk | none | player funds |
| Legal surface | donations only | money transmission + gambling |
| Signing keys to protect | release keys | release + mint seed + arbiter keys |

# TASK B — nonfungible.cash evaluation

**What it is.** A Cashu-adjacent collectible scheme: the NFT *is* the
image file. Asset id `h = SHA-256("Cashu_PS_Asset_v1"‖len‖file) mod r`
over normalized bytes; ownership is a **Pointcheval–Sanders credential**
(BLS12-381) blind-issued by a mint over `(h, owner secret)`; ownership
"showings" are fresh unlinkable PS presentations + nullifier
`N = s·G_null` + BIP-340 profile signature; transfers are **bearer
transfer files** (image + credential, first redeemer owns); the mint
answers only "is N spent?"; identities optional NIP-07/nostr. No sats,
no gas; the minter's custodial mint is the single root of issuance and
spentness.

**Fit with our game.**

- *Composability with kirk*: it is **orthogonal but same-ecosystem** —
  PS credentials over BLS12-381, not Cashu ecash proofs over secp256k1.
  It does not compose with kirk's C-value commit-reveal (different curve,
  different primitive) and cannot serve as game tokens, wager escrow, or
  attestation. No conflict — but no integration gain either.
- *Collectible Champions / codex pages*: our collection grammar is
  **conversion earned in battle**, recorded in the player's own CRDT
  store; making Champions transferable bearer assets (a) breaks the
  roster's CRDT ownership model, (b) routes around the battle-earned
  design, (c) adds a churn/asset-transfer surface the replay/attestation
  trust model has no need for.
- *Cosmetics / supporter badges*: this is the one genuine adjacency —
  a hat/badge as a PS credential would be a nicer integrity story than
  zap-gated cosmetics, but it hits the same storefront/fulfillment
  complexity and app-store risk that got zap-gated cosmetics deferred to
  v1.3+ (ZAP-MONETIZATION §3 surface #5), plus a **new centralized mint
  we would have to run** (issuance + nullifier set) — exactly the
  custody/light-infra the min topology avoids.

**Verdict: REJECT for core, note for later.** Champion collection,
RNG, wagers, and attestations stay on existing kirk/crusade primitives.
Revisit only if v2 wants tradeable cosmetic memorabilia with
provenance; even then, weigh a plain signed-badge scheme (nostr event
attesting a receipt) against a PS-mint — the latter's privacy is only
worth the mint ops if unlinkability between showings really matters.

# Ops runway (prioritized)

1. [human] Flip GitHub Pages source (WS-A A0); run real-browser QA (C1)
   — the only unverifiable-by-CI surface.
2. Decide perpetually: **wagers yes/no.** If no forever, ops stays at
   ~$15/mo and this doc's "Full" column is archival.
3. Donation address: owner-controlled NIP-05/lud16; load from signed
   kind-0 profile so rotation needs no rebuild (ZAP risk 1).
4. Provision the retention relay (strfry, kinds 9259–9265, wss, ~$5–10
   VPS) before v1.1 PvP ships; add to default relay list in `client.rs`.
5. Write relay-incident fallback: client relay-list override setting is
   already in place; document it for players.
6. Document release-key + (future) mint-seed custody runbook; separate
   roles.
7. If wagers ship: separate wager mint from any entropy mint; legal
   review of custodial wagering; dispute = fetch-on-dispute transcript;
   no permanent archive.
8. If public ladder ships: read-only recomputation service over 9265
   attestations publishing signed rankings; never a private-state API.
9. Mobile signing (v1.3): CI-signing secrets, store review of donation
   link against external-payment rules.
10. Crash reporting: either none, or opt-in minimal self-hosted; no
    third-party SDK without a privacy-note update to this doc.

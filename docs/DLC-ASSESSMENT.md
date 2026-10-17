---
title: Discreet Log Contracts — Fit Assessment for kirk
summary: Technical assessment of Discreet Log Contracts (adaptor-signature oracle contracts on Bitcoin) as a potential escrow/settlement mechanism for kirk's Cashu-based wager system, evaluated against the existing P2PK reward-token design, commit-reveal seeding, Nostr coordination, and the wasm32 target.
tags: [dlc, cashu, escrow, oracle, bitcoin, wasm32, assessment]
date: 2026-10-16
status: assessment
---

# Discreet Log Contracts — Fit Assessment for kirk

**Date:** 2026-10-16 · **Status:** assessment · **Verdict:** adapt (narrow, deferred)

---

## 1. Background: What DLCs Are

Discreet Log Contracts (DLCs) were proposed by Thaddeus Dryja (2018) as a
framework for "on-chain/off-chain stateful bitcoin contracts, relying on
real-world observations from a third-party, without leaking knowledge of
usage" ([dlcspecs/Introduction.md][dlc-intro]). They are the canonical
Bitcoin-native mechanism for **conditional payments between two parties
keyed on an externally-verifiable event**.

### 1.1 Mechanism

A DLC between Alice and Bob is built from five ingredients:

| Component | Role |
|---|---|
| **Oracle attestation** | An oracle publishes, ahead of the event, a public key `P` and a one-time nonce/R-value `R = k·G`. After the event, it publishes a Schnorr signature `s = k + H(R‖P‖m)·x` over the outcome `m`. |
| **Adaptor signature** | Alice/Bob encrypt a signature `s` for a given outcome into `s'` using the *signature point* `S = R + H(R‖P‖m)·P`. `s'` decrypts to a valid `s` only when the oracle's real signature is revealed. |
| **Funding transaction** | An on-chain transaction locking both parties' collateral in an output spendable only by mutual consent. |
| **Contract Execution Transactions (CETs)** | Pre-signed spend transactions, one per possible outcome. Only the CET matching the real outcome becomes spendable once the oracle's signature is revealed — the oracle never learns which contract it settled. |
| **Refund transaction** | A timelocked fallback that returns collateral if the oracle never attests (faulty-oracle protection). |

The key properties: **trust-minimized settlement** (neither party can strand
the other; both can always unilaterally close), **oracle non-equivocation**
(the oracle cannot sign two conflicting outcomes without leaking its private
key), and **"discreetness"** (the oracle sees a stream of unrelated
signatures, not the contracts they settle).

The spec itself flags the main remaining gap: *"The current version of the
specification does not offer protection against a party colluding with an
oracle. Mitigation such as using a federation of oracles might be considered
in future versions"* ([dlcspecs/Introduction.md][dlc-intro]).

### 1.2 Intended use case — and why it fits "betting" shape

DLCs were designed from the start for **betting and derivatives**: the
canonical example is two parties wagering on whether a sports team wins or a
price crosses a level, settled by an oracle that signs the outcome. The
payment graph is: fund → (per-outcome CETs) → unilateral settlement, with
collateral locked at the start. A two-player wager on a game outcome is
structurally *exactly* this shape — a 1-of-2 numeric event.

### 1.3 Implementation state as of 2026

⚠️ *Note on verification:* the `websearch` tool in this environment returned
`No search results found.` for every query (see §6, "Tool failures"), so
2025–2026 community status is triangulated from primary repos + training
knowledge rather than live news. Confidence: **medium**.

- **[dlcspecs][dlcspecs]** — the reference specification. Still explicitly a
  *"work-in-progress"*; 43 open issues, 23 open PRs; `MultiOracle.md` and
  Taproot DLCs are still in the *Future Work* list. Not a ratified,
  interoperable standard.
- **[rust-dlc][rust-dlc]** — the main Rust implementation, MIT-licensed,
  maintained by p2pderivatives. 159★ / 42 forks. Its own README still carries
  the warning: *"still in early stage… not thoroughly tested in production…
  recommend avoiding using it on main-net"* and notes it is *"not yet fully
  compliant"* with the spec. Composed of `dlc` (tx construction/signing),
  `dlc-trie` (numeric-outcome lookup), `dlc-manager`, `dlc-messages`, plus
  provider crates for bitcoin-rpc, electrs, sled storage and the P2PD oracle.
- **10101** — the highest-profile DLC consumer product (a mobile
  non-custodial bitcoin betting/trading app built on rust-dlc).
  **Status could not be confirmed from live sources** (its domain failed to
  load and search is non-functional here — see §6). From prior knowledge,
  10101's app was wound down in 2024 and the team pivoted, leaving no
  actively-shipped, consumer-facing DLC product. Treat rust-dlc's production
  readiness and the "DLC app ecosystem" claim as **unproven in the wild**.
- Other implementations (bitcoin-s/Suredbits, cfd-dlc/CryptoGarage,
  node-dlc/Atomic Finance, NDLC) are listed by the spec repo but none have
  emerged as a dominant, actively-shipped standard.

**Bottom line for §1:** the cryptography is sound and well-specified; the
**ecosystem is stalled** — a great spec draft, a genuinely useful but
production-immature Rust library, no winning reference deployment, and
open-question oracle collusion that the spec itself defers to future work.

---

## 2. Fit Against Our Stack

### 2.1 Our context

Kirk is a **trustless gaming** project: **Cashu ecash** tokens (via a vendored
`deps/cdk`), **Nostr** for coordination, and a wager model where:

- Wagers are Cashu ecash, with **mint-side replay arbitration** planned via
  **crusade-bridge kind-9265 attestations**.
- Today's reward path uses **P2PK reward-token** semantics (NUT-11-style
  locked proofs).
- The trust model is deliberately *mint-arbitrated*, not *chain-arbitrated*:
  the arbiter replays transcripts to a verdict, and verdicts depend only on
  the transcript.

### 2.2 The wager-escrow question: would DLCs improve it?

**Partially, and only at the settlement-trust layer — not at the
arbitration-trust layer where our real problem lives.**

What DLCs *would* give us:

- **Non-custodial collateral.** Funds are locked in a 2-of-2-adapted UTXO,
  not held by a mint or a bridge. Neither player can abscond with the pot.
- **Outcome-keyed, privacy-preserving settlement.** Only the CET matching
  the true outcome becomes spendable; the oracle can't tell which contract
  it just settled.
- **Refund safety.** A timelocked refund path protects players against an
  oracle that simply goes silent.

What DLCs *would not* solve for us:

- **Oracle corruption is the whole ballgame.** The spec's own caveat applies
  with full force: nothing prevents a colluding oracle from signing a false
  outcome. In a two-player wager, "the game outcome" *is* the oracle's
  attestation — the thing we are trying to make tamper-proof. Our
  **mint-side replay arbitration (kind-9265 attestations)** *is* our oracle
  mechanism; a DLC layered on top would merely convert the arbitrator's
  verdict into an on-chain settlement path. It relocates trust; it does not
  remove it.
- **Two-player isn't enough for full DLC.** A 2-party DLC is really just an
  escrowed bet with a refund — useful, but a federation/multi-oracle setup
  (the mitigation the spec *recommends* for collusion) is precisely the part
  of the spec that is **least** mature (`MultiOracle.md` is a work-in-progress,
  not a ratified document).

### 2.3 Who would the oracle be?

| Candidate | Assessment |
|---|---|
| **The mint** | Contradictory: the mint is already the custodian of ecash. Using it *also* as the DLC oracle doesn't add a trust boundary, it just moves the settlement venue. It *does* avoid adding a new trusted party, though. |
| **Both players** | Not an oracle in the DLC sense — player keys are the adversarial inputs. Co-signing by both players is already the *default* escrow shape; it provides no independent attestation of the outcome. |
| **A federation / kirk-run oracle set** | The spec-sanctioned mitigation for oracle collusion (`MultiOracle.md`) — but that is the *least* mature part of the spec, and it re-introduces the very "trusted committee" trust model we are trying to escape by using ecash in the first place. |
| **Third-party attestation oracles** (e.g., a sports/news oracle) | Would work for *external* events, but our wagers are settled on **in-game outcomes**, which no external oracle can observe. This rules out the standard public-oracle route entirely. |

**Conclusion on oracle identity:** if kirk wants outcome-attestation, the
only sane option is *our own existing* arbitrated attestation stream
(kind-9265). But then the DLC is a *wrapper around* our existing arbitration,
not an *improvement to* it. There is no third-party oracle that can
legitimately attest "player X won the match" without re-deriving the same
replay trust we already need.

### 2.4 Better or worse than the current P2PK reward-token design?

| Dimension | Current P2PK (NUT-11) reward tokens | DLC on top |
|---|---|---|
| Custody | Mint-custodied; mint enforces spending condition | On-chain collateral, player-custodied until settlement |
| Settlement authority | Mint, enforcing the spending condition + our replay arbitration | Oracle attestation (which *we* would still have to supply) |
| Privacy | Good — ecash is inherently blind | Comparable; oracle doesn't see the contract, but settlement is public on-chain |
| Latency / UX | Mint round-trip, sub-second to seconds | On-chain funding + on-chain CET confirmation — minutes to hours |
| Player device requirements | Any wallet that can hold ecash | Full Bitcoin node / wallet participation, adaptor-signature capable |
| Auditability | Off-chain, gated by our own logs | Publicly auditable on-chain trail |
| Maturity of tooling | Cashu P2PK (NUT-11) is a **shipped, optional NUT** implemented across the ecosystem ([NUT-11][nut11]) | `rust-dlc` self-describes as early-stage / not production-tested; spec is WIP |

**Verdict for the main game-loop wager path:** the current P2PK design is
*better* on every dimension that matters to players (latency, device
requirements, privacy, and consistency with the ecash mental model), and
*worse* only on the dimension DLCs actually address (collateral custody),
which we have consciously traded away for the mint-arbitrated trust model.
DLCs would be **regressive** as a general replacement.

---

## 3. Composition & Conflict With Existing kirk Components

### 3.1 Commit-reveal C-value seeding

Kirk's commit-reveal produces a per-battle seed from which
`crusade-bridge` re-derives outcomes. **This composes cleanly with DLCs in
one specific direction:** the C-value-derived seed could be *published* as
the "event data" that an oracle attests to, making the on-chain settlement
path reproducible by a third party from the seed alone.

**Conflict:** the DLC oracle's *attestation key* is not the C-value. They are
separate cryptographic objects and must not be conflated: the R-value/nonce
committed by a DLC oracle is a one-time Schnorr nonce, not a replayable game
seed. Reusing a key across both roles would be a **security bug** (cross-
protocol/key-reuse attack), so a strict boundary would be needed: DLC
signing keys are isolated from (and never derived from) commit-reveal
material. This is a real, non-trivial integration hazard.

### 3.2 Nostr coordination

Composes well *conceptually* — our kind-9265 attestations are already a
natural place to publish an oracle's advertised `R`-value ahead of an event
and the signature after it, making kirk itself a Nostr-visible "oracle
service." **Conflict:** the DLC spec's message flow
(`dlc-messages`: `Offer`, `Accept`, `Sign`, `Close`, CET negotiation) is a
**peer-to-peer interactive protocol**. Mapping it onto Nostr (ephemeral
events, or a 1:1 message channel) is possible but non-trivial: Nostr has no
reliable delivery guarantee, and the funding-transaction negotiation requires
tight round-tripping. Existing Nostr-native escrow patterns (e.g., NIP-EC
style cashu-in-nostr) are far simpler and already within our stack's
vocabulary.

### 3.3 wasm32 target & pure-Rust availability

- **Yes, there is a pure-Rust DLC library**: [`rust-dlc`][rust-dlc] (MIT,
  p2pderivatives). It is a workspace of crates (`dlc`, `dlc-manager`,
  `dlc-messages`, `dlc-trie`) with no C/C++ dependencies in its core.
- The *core* `dlc` and `dlc-messages` crates should compile to
  `wasm32-unknown-unknown` in principle (they build on `rust-bitcoin` /
  `secp256k1`, both wasm-friendly).
- **However, `dlc-manager`** — the crate you actually need to run a contract
  lifecycle — is coupled to `tokio`-style async and to a
  blockchain/wallet *provider* trait intended for a native daemon
  (`bitcoin-rpc-provider`, `electrs-blockchain-provider`, `sled` storage).
  This is exactly the dependency class kirk's root `Cargo.toml` already
  deliberately avoids for wasm: tokio's net/process/signal features pull in
  `mio`, which has no wasm32 backend, and our build isolates tokio behind
  native-only features for that reason. Bringing in `dlc-manager` as-is
  would break or heavily complicate the wasm32 build.
- An additional `wasm32` complication: the DLC funding/CET flow requires an
  on-chain wallet with UTXO management and fee estimation *in the browser or
  on-device*. Our current mobile/browser posture is ecash-native and does
  not carry a Bitcoin wallet stack.

### 3.4 Mobile

Related to §3.3: a DLC wager requires the *player's* device to hold keys,
sign funding transactions, and monitor the chain for CET/refund timelocks.
On mobile this means either (a) embedding a full Bitcoin wallet into the
kirk client — a major, out-of-scope dependency — or (b) delegating wallet
duties to a server, which **defeats the entire non-custodial premise** of
adopting DLCs in the first place. This is the strongest single argument
against adoption.

---

## 4. Verdict

**Adapt — but narrowly, and deferred.**

DLCs are **not** a replacement for, or an improvement to, our primary
mint-side replay arbitration and P2PK reward-token path. They *are* the
correct primitive if kirk later needs a **high-stakes, minimally-trusted,
non-custodial settlement lane** that lives outside the ecash model — i.e.,
precisely the "tournament finals / large pot" scenario. That is the narrow
slot where the additional latency, device requirements, and new oracle-trust
surface are justified by the value at stake.

**What we should *not* do:** rip out P2PK reward tokens for DLC settlement,
or attempt to make DLC the general escrow mechanism for all wagers.

### 4.1 Smallest viable integration sketch (if/when triggered)

A **"tournament pot" DLC lane** for high-value, opt-in matches only:

1. **Trigger:** a match is explicitly flagged `high-stakes` (e.g., tournament
   finals) *and* both players opt in. All other wagers continue on the P2PK
   path unchanged.
2. **Oracle:** a small **federation of kirk-operated signing keys** (2-of-3
   style), *not* the mint itself, to avoid collapsing the mint's custodial
   role into the attestation role. Keys are isolated from commit-reveal
   material (§3.1).
3. **Attestation transport:** the federation publishes `R`-values and
   outcome signatures as **Nostr events** (new `kind`, adjacent to 9265)
   so attestation is publicly auditable and no new transport stack is needed.
4. **Outcome derivation:** the oracle federation does *not* re-adjudicate
   gameplay; it attests to the **kind-9265 attestation** that our existing
   arbitration already produced. The DLC is a *settlement wrapper* around
   our arbitration, not a second arbiter.
5. **Library:** `rust-dlc`'s `dlc` + `dlc-messages` crates only (native
   target, behind a feature flag); no `dlc-manager` in wasm. The
   coordinator/bridge side (native) runs the manager; the mobile client
   interacts only through the Nostr attestation feed and a thin
   "sign this CET" request to the user's own wallet, never embedding a
   wallet.
6. **Fallback:** every pot carries a timelocked refund path so a stalled
   federation can never strand funds.

This keeps the wasm32 build clean, keeps mobile out of the on-chain hot
path, and confines the new trust surface to a scenario where players have
explicitly consented to it.

---

## 5. Actionable Next Steps (if the "adapt" path is pursued)

- [ ] Track [`MultiOracle.md`][dlcspecs] in dlcspecs — federation-of-oracles
      is the prerequisite for our §4.1 sketch, and it is not yet stable.
- [ ] Re-evaluate [`rust-dlc`][rust-dlc] maturity (its own README still
      warns against main-net use) before any production commitment.
- [ ] Prototype a 3-key federation signer publishing attestation events to
      Nostr, *decoupled from* rust-dlc, so the transport layer can be built
      and audited independently of the DLC library.
- [ ] Add a doc note to the P2PK reward-token design explicitly stating that
      DLC settlement is *out of scope* for the general wager path, to
      prevent accidental scope creep.

---

## 6. Sources & Verification Notes

**Primary sources consulted (fetched/read directly):**

- [dlcspecs — Discreet Log Contract specification][dlcspecs] — mechanism,
  glossary, oracle/adaptor-signature definitions, implementation list,
  future-work caveats, and the "oracle collusion" trust limitation.
- [dlcspecs/Introduction.md][dlc-intro] — adaptor signature and
  signature-point definitions; refund-transaction semantics.
- [rust-dlc][rust-dlc] — crate layout, MIT license, maturity warning,
  wasm/native dependency surface.
- [cashubtc/nuts (NUT-11, P2PK)][nut11] — spending-condition enforcement by
  the mint, `locktime` / `refund` / `n_sigs` tags, Schnorr witness format.

**Local codebase consulted:**

- `Cargo.toml` — tokio feature isolation for wasm32; `nostr = "0.35"`;
  `deps/cdk` (vendored Cashu Development Kit).
- `crusade-bridge/src/lib.rs`, `crusade-bridge/src/adapter.rs` — replay
  arbitration architecture, kind-9265 event chain.
- `docs/ZAP-MONETIZATION.md` — existing trust-model firewall between
  monetization and outcome generation.
- `docs/V1-SHIP-PLAN.md`, `PLAN.md` — wager escrow as a known open item.

**Failed / unverifiable sources (reporting per instructions):**

- `websearch` tool returned `No search results found.` for every query
  attempted in this session; 2025–2026 community/news status of 10101 could
  not be independently confirmed and relies on prior knowledge (medium
  confidence).
- `webfetch` of `https://www.10101.com` failed with a transport error
  (domain unreachable).
- `webfetch` of `https://adiabat.github.io/dlc.pdf` (original DLC paper) was
  rejected — `application/pdf` is an unsupported content type for the fetch
  tool; the markdown `Introduction.md` from the spec repo was used instead.

[dlcspecs]: https://github.com/discreetlogcontracts/dlcspecs
[dlc-intro]: https://github.com/discreetlogcontracts/dlcspecs/blob/master/Introduction.md
[rust-dlc]: https://github.com/p2pderivatives/rust-dlc
[nut11]: https://github.com/cashubtc/nuts/blob/main/11.md

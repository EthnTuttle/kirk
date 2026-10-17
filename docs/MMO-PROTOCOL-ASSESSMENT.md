---
title: Cyberspace (arkin0x) as MMO-Lane Protocol Backbone — Assessment
summary: Evaluation of the Cyberspace v2 nostr-based thermodynamic spatial protocol as a possible backbone for a future shared-world lane of the kirk game stack; includes fit analysis against kirk/crusade constraints, an alternatives survey (MUD/Dojo, Matrix, yjs/automerge world servers), and a verdict.
tags: [assessment, mmo, protocol-research, nostr, cyberspace, shared-world, crusade-net, kirk]
date: 2026-10-16
status: assessment
---

# MMO-Protocol Assessment: arkin0x/cyberspace

Scope: a *future* MMO-style shared-world lane. The scenario/campaign lane stays single-player and is unaffected. Target repo reviewed at commit dated 2026-10-14 (shallow clone, spec + docs read in full enough to characterize design).

## 1. What Cyberspace actually is

**A protocol spec, not a world engine.** Cyberspace v2 (`CYBERSPACE_V2.md`, "Design complete (spec); reference implementation in progress") defines a 256-bit coordinate space (three 85-bit axes interleaved, plus a plane bit: `dataspace` = GPS-mapped to Earth, `ideaspace` = abstract) where:

- **Identity = location.** Your nostr pubkey *is* your spawn coordinate. No spawn choice, no registry.
- **Movement = structured PoW.** Moving from A to B requires computing a Cantor-pairing-tree over the intermediate coordinates ("hop"), or a SHA-256 Merkle-tree "sidestep" across walls where Cantor intermediate values become infeasible to store. Work scales with ultrametric distance; decomposition invariance means you cannot subdivide a long move into cheap short ones. A temporal axis (hash-chained fresh work seeded by the previous event id) prevents caching/replay.
- **State = nostr events.** Movement is a linear hash chain of signed `kind:3333` events (spawn/hop/sidestep) on ordinary nostr relays; location-encrypted content ("bags", `kind:33330`) derives keys from region Cantor roots — "chalk on a sidewalk" readable only by those who did the region's work. DECKs extend with new kinds (`33331` SNO objects, hyperspace/landfall transit via Bitcoin blocks in DECK-0001).
- **Consensus:** there is none, deliberately. Validity of a chain is a local function (proof verification). "Knowledge of global state is not possible" is an explicit design tenet. A chain may fork; the newest spawn resets position. There is *no* shared mutable world state, no tiles, no inventory, no combat — only presence proofs, movement history, and location-gated crypto.
- **Games are explicitly out-of-band.** `docs/territory-conflict-game-layer.md` (2026-08/09 design record) rules that territory, combat ("derezz"), and virtual spawns live in the "game box": each game defines its own liveness rules over protocol-valid chains, honoured only by that game's clients. Conflict resolution, if any, must be work-based ("D2" reorg-attack model), eventual-consistency, `created_at` never consulted. DECK-0002 (Games) is reserved but unwritten; the derezz DECK was ruled unworkable and withdrawn 2026-09-15.

**Stack/maturity:**

- Language: spec in English + Python reference pseudocode; `cyberspace-cli` (Python) is the reference implementation, explicitly "in progress". No Rust implementation exists.
- License: CC BY-SA 4.0 (a *document* license covering the spec; there is effectively implementation freedom, but no code to reuse).
- Activity: small (~66 stars, 1 core author), but genuinely active in 2026 — spec last updated 2026-10-08, design records through Sept 2026, funded (OpenSats grantee). Conceptually sophisticated (game-box invariants, work-as-clock reasoning are unusually rigorous).
- Wallet/money: none natively; DECK-0001 uses Bitcoin blocks only as a clock/transit anchor. No cashu integration.

## 2. Fit analysis against our constraints

| Constraint | Fit | Notes |
|---|---|---|
| Deterministic battle truth via kirk commit-reveal | **Composes cleanly** | Cyberspace owns *validity* of movement, never battle outcomes. Battles would stay exactly as-is: crusade-core simulation + kirk commit-reveal, invoked from a shared-world context. The "game box" doctrine explicitly endorses this layering. |
| nostr-based discovery | **Strong fit** | Cyberspace *is* nostr-native (NIP-01, standard relays, `cyberspace.nostr1.com` etc.). Presence/movement gossip would reuse our existing nostr transport's relay infra conceptually. |
| automerge CRDT offline-first personal data (crusade-data) | **Compatible, orthogonal** | Personal data (rosters, campaign, inventory) stays automerge-local. Cyberspace would only cover *public* presence/encounter rendezvous. Their models don't overlap, which is good. |
| wasm32 target | **Hard blocker for direct reuse** | All reference code is Python. Movement proofs need bignum Cantor pairing on 185 GB integers for deep moves — utterly hostile to wasm/browser even in Rust. Only short-range hops are consumer-feasible (spec §9.9 "consumer benchmarks" concedes nation-state-scale moves need serious hardware). |
| Rust workspace | **No code reuse** | Adoption = writing a Rust (partial) implementation of the spec ourselves. |
| iroh QUIC transport (crusade-net) | **Redundant but not conflicting** | Cyberspace is relay-based (nostr) by design; iroh stays the transport for fast battle session traffic. |

**Where it would compose:** as a *rendezvous and presence layer* for the MMO lane — keys-as-positions, anti-teleport movement cost, location-gated "chalk" (world flavor text, encounter hooks), public movement chains as tamper-evident audit trails. This matches our `OVERWORLD.md` ambitions in spirit.

**Where it conflicts or falls short:**

1. **No world state.** There are no tiles(and no possibility of any) — no persistence, no economy, no shared mutable anything. An MMO lane needs all of that; cyberspace provides none of it. We'd build 95% of the game box ourselves anyway.
2. **Movement economics fight gameplay.** Hops scale with distance in a way tuned for philosophical robustness, not game feel. A shared world where relocation costs minutes-to-hours of CPU is a design constraint we'd have to route around (virtual spawn / game-defined spawn zones are anticipated by the spec but unspecced — DECK-0002 unwritten).
3. **No consensus on anything shared.** "Knowledge of global state is not possible" is a feature for them, a dealbreaker as a *backbone*. An encounter between two players requires both to have independently worked into the same region, with no shared sequencing.
4. **Immature + single-vendor.** Spec says "implementations are in progress"; the one client (ONOSENDAI, JS/React) and CLI are prototypes by one grantee. Key game-layer designs (derezz) were recently ruled unworkable — the conflict model we'd need most is the least-settled part.

## 3. Alternatives (same rigor, brief)

- **MUD (Lattice) / Dojo (Starknet) — onchain autonomous worlds.** Full world state is on-chain ECS tables; every state transition is a consensus-verified transaction; clients are thin indexers. Strengths: this actually *is* a shared-world backbone — persistence, permissionless composability, atomic conflict resolution. Weaknesses for us: requires a chain footing (EVM for MUD, Cairo/Starknet for Dojo) that clashes with our cashu/nostr identity model; every action costs gas/latency; Rust-side integration means vendor SDKs; wasm feasibility poor for Dojo's Katana/Torii stack; philosophically substitutes one admin (chain validators) for another. Would only fit if we wanted a credibly-neutral *auction house/persistence* layer, and even then it pulls the whole game into its ECS paradigm.

- **Matrix (application-service/bot-driven world state).** Matrix is a decentralized, eventually-consistent event-graph protocol with real implementations in Rust (`matrix-sdk`, wasm-capable). A shared world can be modeled as rooms = regions, state events = world state, with an appservice or bot as the enforcing server. Strengths: mature spec, federation, E2EE, Rust crates, offline-tolerant merges (state resolution is CRDT-like). Weaknesses: world truth is ultimately held by whichever homeserver runs the room — no trustless conflict resolution for competitive play; state resolution is designed for chat, not game physics; per-1v1-battle truth still must be our own commit-reveal. Viable as a *lobby/social layer*, heavier than plain nostr for no extra trust.

- **yjs/automerge-based world servers (e.g. partykit/yjs-style CRDT rooms, or "sync-server-per-region").** Run the shared world as a set of automerge documents (one per region/shard) synced through hosted sync servers, with presence via ephemeral messages. Strengths: perfectly aligned with crusade-data (same CRDT, same offline-first story, wasm-friendly automerge); trivially composable with existing crates; eventual consistency is acceptable for non-competitive world state (map, decorations, houses); battle truth still kirk commit-reveal over iroh/nostr. Weaknesses: CRDTs give convergence, not *authority* — cheating (forged writes) must be filtered by signature-checked ops and/or an honest-sync-server assumption; no existing off-the-shelf "autonomous world" framework — we'd write the shard semantics ourselves. This is the least glamorous but most engineerable path.

## 4. Recommendation

**Reject cyberspace as a backbone; adapt one idea (optional, deferrable).**

Reasoning: cyberspace is a brilliant *presence/movement* proof protocol with (by design) zero shared state, a Python-only reference implementation, wasm-hostile work functions, and an explicitly unsettled game-conflict layer. As a "protocol backbone" for an MMO lane it provides maybe 10% of what the lane needs (anti-teleport presence proofs) while adding a hard correctness burden (implementing and pen-testing Cantor-tree PoW verification in Rust, wasm constraints, sybil movement) for benefits a game-defined spawn policy could grant for free. Its own design record concedes that all game rules live in a bespoke "game box" — i.e., we'd build the MMO either way; cyberspace would only decorate it.

**Adapt (minimal, future, optional):** borrow the narrow primitive of *location-gated content and presence rendezvous over nostr*:

- If/when the MMO lane exists, add a `crusade-world` crate beside `crusade-net`:
  - nostr-native presence/encounter adverts over the existing `nostr_transport` seam (new custom kinds, not cyberspace's kind 3333 namespace);
  - optionally honor cyberspace *region key* derivation for flavor content ("chalk") only — read-compatible, never movement-compatible (no PoW hops on wasm);
  - 1v1 truth unchanged: kirk commit-reveal, crusade-core deterministic sim, iroh QUIC session, nostr fallback.
  - Shared world state (if ever): automerge-per-region docs with signature-gated writes (option 3 above), not cyberspace, not on-chain.
- What stays kirk-native: battle truth (commit-reveal), payments (cashu), identity (nostr keys as-is — *not* keys-as-coordinates), personal data (automerge).

Bottom line: **adopt = no; adapt = one small read-compatible idea behind a new crate seam, only after the MMO lane is actually greenlit; reject as backbone.**

## References

- Spec: https://github.com/arkin0x/cyberspace (`CYBERSPACE_V2.md`, `RATIONALE.md`, `docs/territory-conflict-game-layer.md`, `decks/README.md`), reviewed 2026-10-16 at master (2026-10-14).
- Implementation: https://github.com/arkin0x/cyberspace-cli (Python, WIP); client: https://github.com/arkin0x/ONOSENDAI.
- Local clone used for review: /tmp/opencode/cyberspace (shallow).

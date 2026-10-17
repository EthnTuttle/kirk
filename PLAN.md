# DEUS VULT: Champions of the Cross

Master plan. This file is the single source of truth for the orchestrator thread.
Sub-threads receive self-contained prompts; they do not read this repo's history.

## 1. Vision

A monster-battler (Pokemon-style mechanics, zero Nintendo IP) with a reverent
Christian Crusades theme. You are the **Marshal of an Order** (fictional Order of
the Pilgrim Star, Ordo Peregrinae Stellae). You rally, train, and lead
**Champions** — knights, serjeants, hospitallers, clerics — who battle
manifestations of the **Seven Deadly Sins** (PvE campaign) and rival Orders in
**PvP Tournaments of Virtue**.

Tone rules (avoid heresy, glorify God):
- Christ is Lord, God is our Father, and the Holy Spirit descended to build
  up the Church and advance the Kingdom of God on earth. God is never a
  game mechanic, stat, or collectible. Prayer, virtue, and repentance are.
- The Cross and Crusades are honoured; no mockery, no syncretism, no occult
  player-facing mechanics. Collection grammar is CONVERSION: "call them all
  to repentance" (see section 3 and docs/CONVERSION.md).
- Foe lanes (owner direction 2026-10-16): TWO lanes, one canon.
  1. ALLEGORY: the Seven Deadly Sins remain the recurring boss tier
     (grotesque allegories in the Spenser/Dante tradition).
  2. HISTORY RESTORED: field adversaries are the Muslim armies of
     1095-1192 (docs/HISTORICAL-FOES.md, reinstated). The game's voice is
     the chronicle voice of the age: they fight for a false god, and the
     work of mercy is to call them to repentance and baptism into Christ.
     The register is grounded in attested doctrinal conflict
     (docs/DOCTRINAL-SINS.md), never caricature: no slurs, no mock-Arabic
     names, no dehumanizing sprites. The deeper a foe lives in his sin the
     more corruption shows in his portrayal; the common soldier stands as
     a man, redeemable. All men can be saved through repentance and faith
     in Christ Jesus — that is the whole point of the mechanic.

## 2. IP Safety

- No Pokemon names, creature designs, or trademarked terms. Not "type", we say
  **Virtue**; not "moves", we say **Rites**; not "catch", we say **Call to
  Repentance** (converts are **baptized** into the roster); not "gym", we
  say **Chapter House**; not "Pokedex", we say **Hagiography**.
- All names original Latin-flavoured inventions.

## 3. Mechanics (contract for crusade-core)

- **Virtues (7)**: Humility, Charity, Chastity, Diligence, Patience, Temperance,
  Kindness. Each opposes one Sin: Pride, Envy, Lust, Sloth, Wrath, Gluttony,
  Greed. Champions have 1 (rarely 2) virtues. Sin-spawn have 1 sin.
- **Effectiveness chart**: virtue rites deal 2x vs their opposed sin (and vice
  versa); among virtues a fixed 7-wheel makes each virtue hit the next one 2x,
  resisted by itself and the previous one (0.5x). Dual typing multiplies.
  Exact percent rules documented on `data::effectiveness`; table in
  `crusade-core/src/data/chart.rs`. Sin-typed rites exist for sin-spawn.
- **Stats** (per Champion species): Resolve (HP), Valour (Atk), Piety (SpA),
  Guard (Def), Aegis (SpD), Alacrity (Spe). Base stats 20..130.
- **Instances**: level 1-50, XP curve (medium-slow), IV-equivalent
  "Grace" (0-15 per stat), EV-equivalent "Devotion" (0-63 per stat, 130 total),
  4 Rite slots, 25 natures -> 5x5 "Callings" (+10%/-10% two stats).
- **Rites**: name, virtue, power 0-120, accuracy, Zeal cost (PP), category
  (Martial/Spiritual/Status), priority, secondary effects (burn equivalent
  "Brand", paralysis "Chains of Conscience", etc.).
- **Battle engine**: 1v1 duels first, squad of 6 with switching; turn order by
  priority then Alacrity; deterministic RNG seeded per-battle from kirk
  commit-reveal (see 5). Damage formula: classic multiplicative (stat ratio *
  power * STAB 1.5 * effectiveness * rand 0.85-1.0 * level factor). Fainting ->
  "Yield"; battle ends when a squad yields.
- **Conversion (collection)**: "Call them all to repentance." Defeated
  human adversaries may be issued a **Call to Repentance**. Conversion is a
  contested roll: the issuing Champion's Piety against the foe's
  **Obstinacy** — stronger practitioners, and those steeped deeper in sin,
  convert less readily; named commanders never do. On success the foe
  repents, is baptized, and joins the roster as a new Christian (his
  typing shifts to a virtue; his kit is retained). On refusal his soul is
  released unto God and he departs the field (Mercy-tier reward). Full
  spec: docs/CONVERSION.md. Defeated sin-spawn still drop Contrition
  Shards; campaign NPC Champions still join by trial (scripted).

## 4. Architecture (cargo workspace)

```
kirk/                    # existing: trustless protocol (cashu+nostr). Untouched API.
crusade-core/            # pure logic, deterministic, no I/O, wasm-safe. Types+data+
                         # engine+RngFacade (seed in, decisions out). Target: 100% unit test
crusade-net/             # Transport trait; iroh QUIC impl (native), nostr-relay impl
                         # (fallback, works in browser wasm). Signed battle transcripts.
crusade-data/            # automerge CRDT: roster, inventory, settings, battle history
                         # summaries. Sync over crusade-net. Works offline-first.
crusade-app/             # Bevy client: android/ios targets + wasm-bindgen web build.
                         # Headless `sim` binary first (console battles) to prove core.
crusade-sim/             # tiny CLI harness: AI-vs-AI battles, balance fuzzing, seeds.
```

Build targets: `wasm32-unknown-unknown` (core+data+net-nostr+app),
`aarch64-linux-android`, `aarch64-apple-ios` (app w/ bevy platform shells).

## 5. Trust model

- Battle RNG: kirk C-value commit-reveal. Both players hash-commit token sets;
  revealed C values + battle id -> HKDF -> per-battle seed -> per-turn sub-seeds.
  Transcript signed by both keys; verifiable by any observer.
- Wagers/rewards: existing kirk token types (game tokens, P2PK reward tokens).
- Matchmaking/discovery: nostr (existing kinds 9259-9263) + iroh topic tickets.
- CRDT sync ONLY for one's own collection across devices; never for battle truth
  (battles are event-sourced, countersigned).

## 6. Milestones

- [x] M0: plan + workspace scaffold, domain type contract in crusade-core.
- [x] M1-core: 42 rites, 19 species, 25 callings, effectiveness wheel,
      deterministic engine with full unit coverage; review pass 1 +
      adjudicated fixes.
- [x] M1-tail: crusade-sim harness (fuzz + duel). Balance debt noted:
      Cantor Sabina underperforms across levels; revisit in M6 content pass.
- [x] M2: crusade-net: Transport trait, turn commit-reveal DuelSession,
      rolling transcript digests; iroh QUIC (M2b, feature-gated, localhost
      test) + NIP-44 nostr relay fallback (embedded-relay test). Signing of
      transcripts deferred to M4 (kirk binding).
- [x] M3: crusade-data automerge roster store (merge/save/load tested).
      Wire-sync of doc bytes over crusade-net deferred to app milestone.
- [x] M4: crusade-bridge: seed derivation from revealed C values, kind-9265
      dual attestation of transcript digests. Defers: full kirk Game-trait
      adapter for mint-side replay validation; wager escrow wiring.
- [x] M5a: crusade-app client core: frontend-agnostic ClientGame API,
      serde view snapshots, input handling, headless AI duels (wasm-ready,
      no native deps).
- [x] M5b: Bevy 0.18 desktop shell (menu/roster/battle/campaign screens,
      smoke harness), verified compiling to wasm32-unknown-unknown;
      trunk web harness + mobile guidance in docs/BUILDING.md.
- [x] M6: 7-boss campaign gauntlet (persistent damage, contrition shards),
      PvP NetDuelDriver over any Transport with dual attestation + Elo
      ladder, device roster sync, kirk Game-trait adapter for mint-visible
      battle replay arbitration.

Launch roster: 12 Champions, 42 rites (incl. 2 per sin for sin-spawn),
7 sin-bosses. Numbers live in data tables, not code.

## 7. Status notes

- Milestone status: M0..M6 complete (M5b Bevy shell done; M6 campaign/
  PvP/ladder/kirk adapter done). Review rounds converged: R1 (10 findings,
  fixed), R2 (SHIP), R3 holistic (3 majors, fixed), R4 (SHIP), R5 final
  audit (1 critical + 2 majors in bridge replay, fixed), R6 (SHIP).
- wasm32-unknown-unknown verified for the full stack (core, data, net,
  bridge, app incl. Bevy shell).
- Pre-existing debt (inherited kirk lib) CLEARED 2026-10-15: the 9
  `cashu::services::*` tests now run against a real in-memory CDK test mint
  (`GameMint::new_test` mirrors cdk's own `create_mint`), and
  `extract_c_value_from_token` parses the first proof's C x-coordinate
  (same convention crusade-bridge uses). Workspace fully green.
- Kind map: kirk 9259..=9263, duel transport 9264, attestation 9265.
- Delegation reality: glm/deepseek flash models failed silently as coding
  subagents in this environment; all workers ran as tff/kimi-k3 with
  contract-first skeletons + compact prompts; reviewer tff/kimi-k3#high.
- Forward lanes: M7 (adversary lane + conversion engine — IN FLIGHT
  2026-10-16, data tables landed, under review), M8 "The Pilgrim Road"
  overworld (docs/OVERWORLD.md), v1.2 wagers (BLOCKED on mint ops
  decision, docs/PRODUCTION-ARCHITECTURE.md), v1.3 mobile signing/SDK.
- New lanes (owner direction 2026-10-16): M7 = adversary data tables
  (HISTORICAL-FOES ids 108-117 archetypes + 120-123 commanders, new rites
  43+, Obstinacy stat) + the Call to Repentance conversion engine
  (docs/CONVERSION.md) + art lane per HISTORICAL-FOES section 8 with the
  sin-depth visual register. M8 = "The Pilgrim Road" overworld
  (docs/OVERWORLD.md): top-down pixel tiles, Zelda-style screen-by-screen
  exploration (not scrolling), random encounters in the wild, scripted
  encounters at funnel points, choice encounters the player elects,
  wrapped in Oregon-Trail pilgrimage logistics (supplies, attrition,
  morale) en route to the Holy Land. M8 design begins immediately,
  implemented after M7's data seams land.

## 7bis. Orchestration protocol

- Workers: `tff/kimi-k3` (default variant) ONLY for code — flash-tier models
  provably failed as coding subagents here (re-confirmed 2026-10-14).
  Datapoint 2026-10-16: `tff/glm-5.3-flash` completed a bounded
  research/write task (docs/DLC-ASSESSMENT.md) successfully; flash remains
  unproven for code. Planning/review: `tff/kimi-k3#high`.
- Workers get: exact files, contracts, style rules; must run `cargo test -p <crate>`.
- Reviewer gets: diff only + contract; returns issue list. Worker fixes.
- Orchestrator commits after each green milestone. Concise conventional commits.
- Parallel workers never share a file; orchestrator writes shared contracts first.
- Research/exploration happens INSIDE agents, never in the orchestrator thread:
  workers/reviewers read sources and search the web themselves and return
  distilled reports. Orchestrator context holds decisions and state only.

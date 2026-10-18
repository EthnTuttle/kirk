---
title: "Forward Roadmap — M7 Completion through the Pilgrim Road v2.0"
summary: >
  Milestone ladder with concrete, independently-assignable work cards from the
  current state (M0-M6 done, M7 adversary tables + conversion engine in flight)
  to a shippable "Pilgrim Road" v2.0 overworld campaign, v1.2 wagers, v1.3
  mobile, v2.x live-ops, and a speculative v3.0 shared-world lane. Includes a
  gap-analysis register, critical-path analysis, risk table, and a two-week
  punch list.
tags: [roadmap, planning, milestones, m7, m8, overworld, conversion, pilgrim-road, gap-analysis]
date: 2026-10-16
status: roadmap
---

# Forward Roadmap — Deus Vult: Champions of the Cross

Ground truth cross-checked against the tree on 2026-10-16:

- M0–M6 are done and review-converged (R6 = SHIP). The shell has animated
  battle sprites, tooltips, gallery roster, Manuscript of Sins, PvP lobby +
  attestation + alms CTA, vesture picker. Workspace tests green.
- **M7 is IN FLIGHT by owner direction (working tree, 2026-10-16):** adversary
  data tables (ids 108–117 + commanders 120–123), rites 43+, `Obstinacy`,
  the Call-to-Repentance conversion engine (docs/CONVERSION.md §7).
- **M8 is DESIGN-SIGNED:** "The Pilgrim Road" overworld (docs/OVERWORLD.md):
  screen-graph Zelda layer + Oregon-Trail journey layer + THE MANTLE
  succession mechanic + Book of the Fallen.
- **Caveat of record (see Gap G1):** the last *committed* docs-pivot
  (`ede0db4`) retired the historical lane in favor of a fictional
  heresy-cult; the *uncommitted* working tree reinstates the historical lane
  and adds CONVERSION.md / OVERWORLD.md / DOCTRINAL-SINS(-2).md /
  MMO-PROTOCOL-ASSESSMENT.md. This roadmap is written against the working
  tree (owner direction 2026-10-16) but the churn itself is a gap item.
- M7 landed 2026-10-16 (adversaries.rs, `Typing::Valor`, obstinacy/
  sin_depth, conversion_chance; 75 core tests green, under review):
  remaining M7 work is UI (M7b) and art (M7a). M8 (`overworld.rs`)
  untouched. `witnesses.rs` (lore-only, non-combatant roll) is unrelated
  to the conversion Testimonies pages (collision resolved 2026-10-16:
  pages renamed "Testimonies").

Orchestration constraints that bind every card below (PLAN §7bis):
workers = `tff/kimi-k3` default only; reviews = `tff/kimi-k3#high`; cards in
one batch touch disjoint files; orchestrator commits after each green card.

---

## 1. Milestone ladder

| Milestone | Ships | Target lane |
|---|---|---|
| **v1.2 (open, parallel)** | Wager escrow over a live mint; optional two-line practice-foe taste (HF-§9 "b") | net/econ |
| **M7 complete** | Adversary roster playable in practice/gauntlet; conversion loop closed end-to-end in engine + UI; fuzz-proven | core content |
| **M7a** | All adversary art produced, staged, wired | art ($0 lane) |
| **M8a** | Overworld data schema + deterministic trail engine (headless-playable) | core |
| **M8b** | THE MANTLE succession + Book of the Fallen | core |
| **M8c** | Bevy tile renderer, encounters wired, campaign save-state v2 | shell |
| **v2.0 "The Pilgrim Road"** | Full campaign: rally → Crossing → 7 regions → Jerusalem; conversion + succession + attrition live | ship |
| **v1.3** | Android/iOS shells via cargo-mobile2; wasm/mobile CI lanes | platform |
| **v2.x live-ops** | New screens/regions as data drops; Chapter-House narrative; seasonal (participation-only) ladders; spectator replay viewer | live-ops |
| **v3.0 (SPECULATIVE)** | Shared-world lane: `crusade-world` nostr-native presence/rendezvous crate. Per MMO-PROTOCOL-ASSESSMENT verdict: cyberspace rejected as backbone; not greenlit | speculative |

### M7 — Adversary data + conversion engine (completion cards)

**M7-1 — Typing::Valor + species_model seam** — *M*
- Files: `crusade-core/src/types.rs` (new `Valor` arm), `data/chart.rs`
  (1.0× both ways for Valor), `data/mod.rs` (`species_by_id` scans
  SPECIES then ADVERSARIES; ids < 108 unchanged behavior).
- Deps: none. Done-test: existing 53 core tests green unchanged; new chart
  tests pin Valor==100% both ways vs all 15; serde roundtrip test proves
  additive encoding.
- Note (Gap G8): must prove via test that an *old* binary reading a
  Valor-typed instance fails gracefully (Serializes-forwards-only claim in
  HF §7 is asserted, not yet proven).

**M7-2 — Adversary table** — *M*
- Files: `crusade-core/src/data/adversaries.rs` (new): `pub static
  ADVERSARIES: &[SpeciesBase]`, ids 108..=117 + 120..=123 with stats/kits
  per HF §4–5; new `obstinacy: Option<u8>` + `sin_depth: Option<u8>` fields
  on `SpeciesBase` (serde-default Options; CONVERSION §7).
- Deps: M7-1, M7-3 (rite ids 43+ must exist to reference; or kit list
  references by id with the rites card landing same batch — prefer M7-3
  first). Done-test: table tests: sorted ids, 10 archetypes + 4 commanders,
  totals 300–380 / 390–420, commanders carry no obstinacy, ASCII names.

**M7-3 — Rites 43..=54** — *M*
- Files: `crusade-core/src/data/rites.rs` (append-only), widen count tests.
- Content per HF §4 [NEW] marks: Wheeling Shots, Dust of the Plain, Couched
  Lance, Testudo of Hides, Storm of Arrows, Volley in Turn, Cleave of Axes,
  Zhupin Cast, Harass and Fade, Mirage Veil, Spear of the Hills, Comet of
  Naphtha (Brand), Wet Hide Ward, Hold the Banner, Wheel the Line.
- Deps: none (rites are id-keyed). Done-test: all rites referenced by
  adversary learnsets exist; effect claws (multi-hit, evade, high-crit,
  flinch, protect, buff) reuse existing `Secondary`/`EffectKind` machinery —
  if a clause needs a new engine effect, that effect lands in a small engine
  card first (Gap G10).

**M7-4 — Conversion engine (core)** — *M*
- Files: `crusade-core` (new `src/engine/convert.rs`): pure
  `conversion_chance(caller, foe, foe_level, fervor) -> u8` (3..=85 clamp,
  exact formula CONVERSION §3), plus instance fields `converted: bool` +
  `converted_from: Option<u16>` (serde defaults), `baptize()` transform
  (typing shift rule, nickname default via `baptismal_names` table — Gap G6,
  ship a stub table now), post-battle draw reusing RngFacade sub-seed
  (crusade-bridge seed discipline), forced-result seam for scripted nodes,
  item rewards: Firstfruits / Mercy shards (Gap G7: reconcile with HF's
  "Commendation Shards" naming).
- Deps: M7-2. Done-test: boundary-tier unit tests (20/35/50/70/90 obstinacy
  × caller piety bands), determinism golden (same seed → same verdict),
  PvP path asserts conversion unavailable.

**M7-5 — Balance fuzz** — *S*
- Files: `crusade-sim` fixtures only (no API change). Run fuzz at levels
  5/20/40 vs champion band; target 40–60% adversary win rate (HF §4 "C2
  Sabina method"); fold in the open Sabina rebalance (C2 debt) if in scope.
- Deps: M7-2, M7-3. Done-test: fuzz report numbers recorded in commit
  message; table adjustments until inside band.

**M7-6 — M7 review** — *S* — reviewer `kimi-k3#high`, diff of M7-1..5 vs
CONVERSION.md contract. Deps: all above. Done-test: SHIP verdict.

### M7a — Adversary art lane (art, parallel, $0)

**M7a-1 — SUBJECTS prompt table** — *S*
- Files: `tools/artbake/` new ADVERSARIES table (subjects, descriptors per
  HF §8 wardrobe lists incl. per-era kit canon; sin-depth visual gradient
  per CONVERSION §6 using existing palette slots: Rum red #A62639, Ayyubid
  palegold #F4D35E, Zengid ink #0B0E1B accents; HF §8 seed descriptor for
  108 is the style anchor).
- Deps: none. Done-test: table reviewed against HF §6 register rules (no
  caricature, no mock-Arabic, corruption reads as spiritual condition).

**M7a-2 — Generation pass (~15 cells)** — *S*, one weekend worst case
- 10 archetypes + 4 commander portraits + 1 "road east" scene header
  (+ derived 32×32 icons) via nano-banana-2-1 at $0; gpt-image-1.5
  contingency ≈ $0.35.
- Deps: M7a-1. Done-test: q128s 0 off-palette px, eyeball pass, MANIFEST
  ledger entries.

**M7a-3 — Staging + shell wiring** — *S — serialize after any in-flight
shell card*
- Files: `tools/artbake/stage_sprites.py` ADVERSARIES pass;
  `crusade-app/assets/sprites/adversaries/…`, icons; shell resolver extends
  `battle.rs::champion_sprite_path` table reverse-lookup to the new ids.
- Deps: M7a-2 + M7-2 (ids must exist). Done-test: idempotent staging rerun;
  bin tests pin id→path mappings; ui-shots byte-identical on untouched
  screens.

### M7b — Conversion UI + Testimonies pages / Roll of Worthy Adversaries

**M7b-1 — Post-victory Call-to-Repentance dialog** — *M*
- Files: `crusade-app/src/client.rs` (post-battle action input), `view.rs`
  (dialog snapshot), `bevy_shell/{battle.rs,screens.rs,input.rs}` (the
  [Call to Repentance] / [Release his soul] chooser, text-only liturgical
  beat for release — no animation budget).
- Deps: M7-4, M8c-unrelated. Done-test: client tests cover success/refusal/
  commander-NEVER/sin-spawn-NEVER/PvP-absent paths; ui-shots new leg;
  harness byte-identical elsewhere.

**M7b-2 — Roll of Worthy Adversaries + Testimonies pages (Hagiography books)** — *M*
- Files: `crusade-core/src/data/` codex/witness-text tables (chronicle-voice
  prose per archetype + testimony per convertible id); `client.rs` settings
  keys `campaign.foes_defeated` + `campaign.converts_baptized` masks (Gap G5
  width problem must be resolved here: 14 ids > u8 bit width);
  `bevy_shell/screens.rs` third/fourth Hagiography books mirroring the
  ART-STATUS §4.7 Manuscript-of-Sins seam exactly (sealed rows carry empty
  text until mask bit set).
- Deps: M7-2 (ids), M7b-1 (unlock writes). Done-test: phase-gate + mask-gate
  tests (both directions), save/load persistence test, sealed-vs-revealed
  ui-shots.

### M8a — Overworld data schema + trail engine

**M8a-1 — Schema (data contract first, orchestrator-written)** — *S*
- Files: `crusade-core/src/data/overworld.rs` (new): `ScreenDef { id,
  region, tile_layout_ref, edges: [Option<ScreenId>; 4], encounter_zone,
  props }`, `RegionDef`, `TrailEvent { condition, weight, effect: TrailVerb }`
  where `TrailVerb` is a closed enum (`LoseSupply, LoseMen, MoraleDelta(i8),
  Battle(id), ConvertOpportunity, Shrine, Nothing`) — effects are
  data-selected verbs, never closures.
- Deps: none. Done-test: schema serde roundtrip; empty-region fixtures
  validate.

**M8a-2 — TrailRng + journey engine** — *L*
- Files: `crusade-core/src/engine/trail.rs` (new): per-travel-day seeded
  `TrailRng` (sub-seed chain from the kirk seed discipline; day replays are
  byte-stable — Gap G9 on future PvP/coop sync claims), movement verbs
  (consume supplies per screen), wild-encounter weighted roll per zone
  table, trail-event resolution, morale bands → battle stat penalty
  application at the `start_battle(context)` gate, season-clock advance,
  company-disperses fail state.
- Deps: M8a-1. Done-test: golden-day replay digests (same seed+route →
  identical event stream); property tests: supplies/morale never go
  negative, dispersal only at rock-bottom morale, day-gating respected.

**M8a-3 — Region/content authoring, wave 1** — *L (content)*
- Files: data-only: 7 `RegionDef`s + per-region wild pools (Anatolia→Rum
  Seljuk archetypes 108/113; Coasts→Zengid 109/111/114; Egypt roads→
  110/111/112/115; Hattin gauntlet→117/commanders; per OVERWORLD §4 +
  HF §2 geography), trail-event tables, funnel screens with commander
  battles (Dorylaeum 120, Harim 121, Egypt 122, Hattin/Arsuf/Jaffa 123).
- Deps: M8a-2, M7-2. Done-test: schema validation test walks all 280
  screens (40×7): every edge target exists, every zone table entry resolves
  a species id, every funnel's scripted battle reachable, no orphan screens.

### M8b — THE MANTLE succession + Book of the Fallen

**M8b-1 — Named-men roster state** — *M*
- Files: `crusade-core` new engine state vector (additive serde, OVERWORLD
  §5): named-men roster + file assignments + hidden per-man Devotion seed
  (rolled at recruitment from TrailRng), supplies, morale, day, region
  flags, Book of the Fallen vec.
- Deps: M8a-2. Done-test: additive-serde forward/backward compat tests (old
  saves load with empty world state).

**M8b-2 — `champion_succession()` pure fn** — *M*
- Files: `crusade-core/src/engine/succession.rs` (new): exact OVERWORLD §6.2
  rules — file scan for qualifying Devotion; on rise, new Champion with
  seeded random kit/level band (former level −5..+2); on reorder, per-man
  loss rolls vs morale, survivors distributed; every death appends a Book
  of the Fallen entry (name, screen id, cause).
- Deps: M8b-1. Done-test: band tests (qualifying/none), rarity proof (rise
  rate within "rare grace" envelope across fuzzed Devotion distributions),
  golden digests; Book append monotonicity property.

**M8b-3 — Book of the Fallen UI (Hagiography fifth element)** — *S*
- Files: `view.rs`, `screens.rs` — memorial pages, same sealed/unsealed
  discipline as §4.7/§M7b-2.
- Deps: M8b-2. Done-test: ui-shots leg; persisted-entries test.

### M8c — Bevy overworld renderer + wiring + saves

**M8c-1 — Tile renderer** — *L*
- Files: `crusade-app/src/bevy_shell/` new `overworld.rs` (+ state/input
  seams): one tilemap chunk per `ScreenDef`, tile-stepped movement,
  sub-tile centring only, edge-step → wipe transition → adjacent load (the
  NES-Zelda constraint from OVERWORLD §5). Screen metrics LOCKED with the
  art pass (Gap G4).
- Deps: M8a-1 (schema), tile art placeholders available day one (flat
  palette tiles acceptable for the whole dev loop). Done-test: headless
  scripted traversal test walks a fixture screen graph; ui-shots of 2+
  fixture screens byte-identical twice.

**M8c-2 — Encounter wiring** — *M*
- Files: shell + `client.rs`: wild (per-step roll → `start_battle` with
  zone-picked adversary), scripted (funnel fires on screen entry, no skip),
  choice (prop interaction prompt with scout-odds display), sin-boss
  corruption sites (elective; ignored corruption = morale drain per region
  until cleansed).
- Deps: M8a-2, M8c-1, M7b-1 (post-battle conversion dialog is the same
  seam). Done-test: end-to-end scripted run: enter zone, roll fires battle,
  victory returns to trail with rewards/morale deltas applied.

**M8c-3 — Campaign save-state extension** — *M*
- Files: `crusade-data` roster/schema (additive world-state doc section),
  `crusade-app/src/persist.rs` (localStorage shim) — version the save
  (`deusvult.roster.v2` with v1 migration path: v1 → v2 = empty world
  state + existing roster).
- Deps: M8b-1. Done-test: v1→v2 migration test, wasm persistence roundtrip,
  CRDT merge test for world state across two device fixtures.

### v1.2 — PvP ladder hardening + wager escrow (open items, parallel lane)

**V12-1 — Ladder service** — *M*: **DONE (2026-10-18).** Replaced the
self-scored honor-system Elo (SHIPPING limitation 4) with a
relay-published, mutually-signed attestation ladder
(`crusade-bridge/src/ladder.rs` fold, `pvp::settle_ladder`, docs/LADDER.md;
kind 9265 extended in-place, no new kind). Done-test landed:
unsigned/tampered, replayed, self-signed, stale, disagreeing, and
mobbed attestations are all proven inert; honest two-client folds
converge byte-for-byte.

**V12-2 — Wager escrow** — *L*: escrow via existing kirk token flow,
settlement via the replay adapter (M6, already mint-visible). Files:
`crusade-bridge/src/adapter.rs`, kirk `src/cashu/`. Deps: **G2 mint
decision** (who runs a production GameMint) — the only true blocker.
Done-test: end-to-end test-mint escrow→settle→slash roundtrip.

**V12-3 (DONE 2026-10-18, b75bd7f)** — *S*: rotate one adversary id into
`PRACTICE_FOE_SPECIES` (HF §9, two lines + a bounds test). Deps: M7-2.

### v1.3 — Mobile shells + CI

**V13-1 — Android shell** — *L*: cargo-mobile2 scaffolding, NDK job on a
dev machine, per BUILDING §(c). Deps: none technical. Done-test: APK on
hardware runs smoke-equivalent leg.

**V13-2 — iOS shell** — *L*: Xcode + signing identity (owner-provided).
Done-test: TestFlight build.

**V13-3 (DONE 2026-10-18, d79da89) — wasm/mobile CI lanes** — *M*: wasm32 check is currently covered
only by the pages workflow deploy build; add a dedicated `cargo check
--target wasm32-unknown-unknown` CI gate + android `cargo check
--target aarch64-linux-android` (no SDK link step). Files: `.github/`
workflows. Deps: none. Done-test: gates red on a deliberately broken PR.

### v2.x live-ops (post-2.0, data-only where possible)

- New regions/screens as data drops (M8a schema makes this code-free).
- Chapter-House narrative beats, seasonal *participation-only* ladders
  (ZAP-MONETIZATION §1.6 forbids sats-weighted rankings), spectator replay
  viewer (transcripts already portable), zap-receipt display (§3.3).
- Register/canon lint as CI (see Gap G11): every new prose/table row
  machine-checked for banned register terms.

### v3.0 — Shared-world (SPECULATIVE, per MMO-PROTOCOL-ASSESSMENT.md)

- Trigger condition only: explicit owner greenlight. If lit: new
  `crusade-world` crate — nostr-native presence/encounter rendezvous over
  the existing `nostr_transport` seam (custom kinds), battle truth/cashu/
  identity/personal data stay kirk-native; shared state if ever:
  automerge-per-region with signature-gated writes. Cyberspace is REJECTED
  as a backbone; at most one read-compatible flavor idea (region-key
  "chalk"), never movement-compatible on wasm.

---

## 2. Gap-analysis register

| # | Gap / unknown | Blocking | Owner-facing question / proposed resolution |
|---|---|---|---|
| G1 | **Committed vs working-tree canon divergence.** HEAD (`ede0db4`) retires the historical lane; uncommitted working tree reinstates it and adds CONVERSION/OVERWORLD/DOCTRINAL-SINS-2/MMO-PROTOCOL-ASSESSMENT. | All M7/M8 commits | Owner: commit or discard the working-tree docs. Proposal: commit as one "owner direction 2026-10-16 final" commit with the reinstatement note verbatim; the flip-flop is otherwise invisible to future sessions. |
| G2 | **Mint custody.** V12-2 needs a deployed Cashu mint + GameMint production config; nobody runs one. | v1.2 wagers only | Owner: self-host a CDK mint, delegate to an existing mint, or descope wagers to zaps-only (ZAP-MONETIZATION §4 already calls zaps a complete path alone). Roadmap proceeds as if wagers stay v1.2-blocked until answered. |
| G3 | ~~**"Witness" naming collision.**~~ RESOLVED 2026-10-16 (pages renamed Testimonies; witnesses.rs comment cleaned). `crusade-core/src/data/witnesses.rs` (lore-only roll) already exists; CONVERSION.md §4's "Testimonies pages" (convert testimonies) is a different thing. | M7b-2 | Rename in CONVERSION-driven cards: codex table = `testimonies.rs`, Hagiography pages = "Testimonies of the Baptized". One-line doc edits. |
| G4 | **Screen metrics + tileset capacity unset.** OVERWORLD locks internal res/palette discipline but defers exact metrics to "the art pass"; 280 screens of tile art is an art-production scale the $0 pipeline has never faced (roster was ~110 portrait cells). Tile capacity/tile-count budget unknown. | M8c-1 visuals (not engine) | Proposal: lock metrics in M8a-1 (e.g. 16×11 tiles of 16px = 256×176 scaled) and prototype one region entirely from flat palette tiles + ~10 reusable stamp tiles before any generation spend. Art card only after the prototype proves the loop. |
| G5 | **Settings bitmask width.** `campaign.sins_defeated` is u8 (7 bits used). `campaign.foes_defeated` needs 14 bits (108–117 + 120–123); `campaign.converts_baptized` needs 10. u8 masks overflow. | M7b-2 | Use u32 masks in new keys (settings map stores strings, so width is a display choice, not a format break); or key per archetype (`campaign.foe.<id>`). Decide at M7b-2 contract time; audit that no sibling key ever reuses a bit position (separate keys today — latent width bug only). |
| G6 | **`baptismal_names` data table** referenced by CONVERSION §4 but never specified (sources, register, per-archetype forms). | M7-4 (soft; stub ships) | Proposal: stub one generic default ("Brother " + office-stem) at M7-4; author the real table as a content card with register review, pre-v2.0. |
| G7 | ~~**Shard economy naming conflict.**~~ RESOLVED 2026-10-16 (owner: keep Firstfruits/Mercy split; "Commendation" retired; HF §7/§10 updated). CONVERSION §4–§5 mints "Firstfruits Shards" + "Mercy Shards"; HF §7/§10-q4 mints "Commendation Shards". Three new item types unresolved. | M7-4 | Owner: one shard type (reuse Contrition economy) or three? Proposal: two — Firstfruits (baptism) and Mercy (release) — and edit HF §7's "Commendation" mention to match; document the economy delta in the item table. |
| G8 | **Serde forwards-compat claim unproven.** `Typing::Valor` new arm — old builds "never meet it", but no test proves a new save can't hard-crash an old binary (matters for v1.0→v2.0 players on the same origin/localStorage). | M7-1 | Add an explicit old-codec fixture test in M7-1 verifying defined behavior (error, not UB); document "v2 saves not loadable by v1" in BUILDING/SHIPPING. |
| G9 | **TrailRng future-sync claim.** OVERWORLD §5 says per-day seeds are "PvP/coop-syncable later" but no protocol exists; risk of designing the seed chain wrong for a future lane. | None today | Accept as aspiration; spec only that the day-seed derivation is a pure function of (campaign_root_seed, day), so any future sync lane can replay. Pin in the M8a-2 contract. |
| G10 | **New engine primitives for rites 43+.** HF §4 kits need multi-hit, evade, priority, flinch, protect, buff, Brand clauses — most likely covered by existing `Secondary` machinery (42 rites already exercise it) but unverified until M7-3. | M7-3 (discovery) | M7-3 begins with an audit: map each [NEW] rite to existing effect kinds; any missing primitive gets a small engine card before the rite table lands. |
| G11 | **Register discipline at scale.** M7/M8 adds ~14 adversary prose sets + codex pages + trail-event text + Book of the Fallen lines — the largest register surface yet, under HF §6 and CONVERSION §8 banned-register rules, with DOCTRINAL-SINS-2 (untracked, unreviewed in-tree) as the grounding volume. | All content cards | Owner: ratify DOCTRINAL-SINS/2 as canon references (commit decision rides with G1). Proposal: a machine-checkable banned-terms list in `tools/` run as a CI gate over `crusade-core/src/data/**` and codex tables; every prose card gets kimi-k3#high register review in addition to code review. |
| G12 | **Fervor item economy undefined.** CONVERSION §3's `fervor_bonus (0..15)` from "litany items / chapter-house blessings" is marked OPTIONAL lane; no item table, source, or economy exists. | Nothing (formula takes fervor=0) | Decision: implement M7-4 with `fervor: u8` plumbed but always 0 at call sites; defer the item economy to a v2.x card with an explicit economy design doc. |
| G13 | **HF §10 open questions.** q1/q5/q6 answered by owner direction (in-file). Still open: q3 (attributed "Saracen" quotations), q7 (boss nodes persistent damage vs fresh). | M8a-3 (q7), prose cards (q3) | Owner: rule on both. Defaults if unanswered: q3 = quotation allowed only attributed to a named in-world chronicler (HF middle course); q7 = funnel battles fresh full-hp (shorter, replayable; persistent-damage reserved for the sin gauntlet). |
| G14 | **Morale-penalty stat band unspecified** ("battle stat penalties band" at low morale — size and shape undefined). | M8a-2 | Provisional: −10% Resolve/Valour below 25% morale, route-blocked below 10%; tune in the M8 fuzz pass. Contract pin in M8a-2. |
| G15 | **Real-browser QA still unrun** (SHIPPING limitation 3); v2.0 adds a whole renderer class (tilemap) never QA'd in browser. | v2.0 ship gate | Extend the C1 browser checklist with an overworld leg; v2.0 go/no-go mirrors v1.0's rule. |
| G16 | **Storage growth.** Named-men roster + Book of the Fallen + world state in automerge/localStorage — unbounded memorial growth vs ~5 MB localStorage on some browsers. | M8c-3 | Cap: Book entries append but doc compaction (automerge) + cap named-men roster at a fixed max company size (e.g. 40) in M8b-1; measure doc bytes in a soak test. |

---

## 3. Critical path

```
M7-1 → M7-2 → M7-4 → M7-5 → M7-6 ─┐
M7-3 ────────────→ M7-2           ├→ M7b-1 → M7b-2 ─┐
G1 (owner commit decision) → everything             ↓
M8a-1 → M8a-2 → M8a-3 (content) → M8c-2 → v2.0 ship
              └→ M8b-1 → M8b-2 → M8b-3 ────────────→│
M8c-1 (renderer) ───────────────────────────────────┘
M8c-3 (saves) gates any public v2.0 deploy.
```

The spine: **G1 → M7-1 → M7-2 → M7-4 → (M7b UI) → M8a-1 → M8a-2 → M8a-3 ∥
M8b ∥ M8c-1 → M8c-2 → M8c-3 → v2.0**.

**Parallelizable:** M7a art lane (all three cards) with all of M7 core;
V12-1 ladder with M7; V13-3 CI gate anytime; M7a-3 serializes after M7b-1
only because both touch `battle.rs`/`screens.rs` (card-disjointness rule).
M8c-1 can start on placeholder flat tiles the day M8a-1 lands.

**Blocked only by owner decisions right now:** G1 (canon commit),
G2 (mint → wagers), G7 (shard naming), G13 (q3/q7). Everything else is
executable.

---

## 4. Risk table

| Risk | Class | Severity | Likelihood | Mitigation |
|---|---|---|---|---|
| wasm32 regressions on mobile-Safari-class browsers (wgpu fallback for tilemap renderer) | tech | High | Medium | M8c-1 placeholder-flat fallback art path; G15 browser QA gate; V13-3 CI gate before v2.0 |
| Determinism break: TrailRng/succession seeds diverge from battle seed discipline | tech | High | Low | Golden-day digests at M8a-2/M8b-2; review requires digest-stability tests; day-seed is a pure fn of (root, day) per G9 |
| CRDT/save scale: named-men + Book of the Fallen bloat automerge doc; v1→v2 migration bug wipes rosters | tech | High | Medium | G16 caps + soak test; migration tested both directions (M8c-3); v1 key untouched on migration failure |
| v3.0 shared world (automerge-per-region) has no authority model — forged writes | tech | Medium | (speculative) | Signature-gated writes per MMO assessment §3; no greenlight without an authority design |
| Register drift at content scale: one mocking line ships in 14 adversary codex pages | content | High | Medium | G11 banned-terms CI gate + #high register review on every prose card; DOCTRINAL-SINS citations required per claim |
| Sin-depth visual gradient reads as ethnic caricature despite guardrails | content | High | Medium | M7a-1 style anchor reviewed against HF §6.4 before batch; corruption = palette/shadow only; per-cell eyeball gate |
| Mint custody / key compromise (alms address rotation, wager mint) | ops | Medium | Medium | ZAP §5.1 mitigations (NIP-05-signed rotation); G2 descope option; treat the address like a release-signing key |
| Art/content authoring is the long pole (280 screens, ~15 adversary cells, codex prose) even at $0 | schedule | Medium | High | G4 prototype-tile-first; regions authorable incrementally; v2.0 can ship 3 regions and densify as v2.x data drops (OVERWORLD §5) |
| Owner-direction churn (historical ↔ fictional pivot twice in one day) | schedule | High | Medium | G1 single canon commit; PLAN.md updated in the same commit; committed tree is truth thereafter |
| Fuzz band failure: adversaries outside 40–60% win band | content | Low | Medium | M7-5 before UI wiring; stats-only iteration is cheap (data tables) |

---

## 5. Next two weeks — punch list

**Day 0 (owner, blocking):**
1. G1: commit or discard the working-tree canon docs; land the PLAN.md §6
   M7/M8 lines in the same commit.
2. G7: Firstfruits + Mercy shard naming (or consolidate).
3. G13: q3 (quotation rule) and q7 (funnel battles fresh).

**Week 1 (workers, kimi-k3; #high reviews on M7-1/M7-4):**
4. M7-1 Typing::Valor + species seam (+ G8 old-codec fixture test).
5. M7-3 rites 43+ with the G10 effect-audit first (split if primitives
   missing).
6. M7-2 adversaries.rs table (stats/kits/obstinacy/sin_depth).
7. M7-4 conversion engine (fervor plumbed, always 0; baptism transform;
   testimony-name stub per G6).
8. M7a-1 adversary SUBJECTS descriptor table (solo content card; register
   review).

**Week 2:**
9. M7-5 fuzz at 5/20/40; iterate tables into the 40–60% band; record report.
10. M7-6 #high review gate → M7 closes; orchestrator commits.
11. M7b-1 conversion dialog (post-victory chooser + release beat).
12. M7b-2 Roll of Worthy Adversaries + Testimonies (u32 masks per G5).
13. M7a-2 adversary art gen pass (~15 cells, $0) once M7a-1 is reviewed;
    stage via M7a-3 after M7b-1 merges.
14. M8a-1 overworld schema contract (orchestrator-written, incl. G4 screen
    metrics proposal for owner sign-off).
15. G16 doc-size soak spike and the G11 banned-terms CI list drafted
    alongside the first prose card.

Overflow order if week 2 outruns the list: M8a-2 trail engine (start) >
V13-3 CI gate > V12-1 ladder. Do not start M8c-1 (tile renderer) before
the M8a-1 schema and G4 metrics are locked.


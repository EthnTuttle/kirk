# Shipping Status — Deus Vult: Champions of the Cross

Last updated: this session's end. Plan and milestones: `PLAN.md`.

## What the app IS today

A complete, deterministic, cryptographically-honest monster-battler core with
a Bevy front-end shell:

| Surface | State |
|---|---|
| Battle engine (`crusade-core`) | **Done.** Fully deterministic, fuzzed, 53 tests, replay-stable (golden digests). |
| Practice duels (vs AI) | **Done and playable** in the Bevy shell. |
| Campaign (7 sin-bosses, persistent damage, shards) | **Done and playable.** |
| Roster/collection store | **Done** (automerge CRDT); browser persistence via localStorage shim (wasm-only). |
| PvP transport layer | **Done** (iroh QUIC native + NIP-44 nostr relay fallback), digested transcripts, dual attestation (kind 9265). |
| Mint-side arbitration | **Done** (kirk `Game` adapter replays duel chains to verdict; review-verified). |
| wasm32-unknown-unknown | Entire stack compiles; trunk release bundle + PWA wrapper built. |

## What v1-ship-in-browser includes (current push)

- **Playable offline PWA**: menu → roster → practice duel → campaign gauntlet,
  install-to-homescreen via service worker + manifest. Progress persists in
  localStorage on web.
- Deployed automatically by `.github/workflows/web-pages.yml` to GitHub Pages
  on each push to `master` (owner must flip Settings → Pages → Source:
  "GitHub Actions" once).
- No servers required for gameplay.

## Known limitations of the current bundle (honest list)

1. **PvP not exposed in the UI.** Transports, duel protocol, and attestation
   exist and are tested, but there is no lobby/challenge-accept screen wiring
   the kirk nostr challenge flow into a `NetDuelDriver` session. Sims/tests
   prove the machinery end-to-end over localhost QUIC and an embedded relay.
2. **No wagers in v1.** Attestations are produced; escrow claim flow against a
   live Cashu mint is not wired (needs a mint + GameMint services; kirk's
   `GameMint::new_test` stub failures are pre-existing tech debt).
3. **Browser runtime was verified headless** (build + asset presence + smokes).
   First real-browser run should sanity-check font rendering, wgpu
   (WebGL2/WebGPU fallback) and audio.
4. **Leaderboard is self-scored** Elo in the player store (honor system until
   the mint arbitration path is live).
5. Balance debt: "Cantor Sabina" underperforms in fuzz data (M6 content pass).

## Roadmap to full ship

| Phase | Work | Depends on |
|---|---|---|
| **v1.0 (now)** | PWA on GH Pages: practice + campaign, offline, persistent roster | pages owner toggle |
| **v1.1 — PvP wiring** | Nostr lobby UI: post challenge (existing kirk kinds 9259–9260), accept → seed → `NetDuelDriver` (iroh ticket upgrade, nostr fallback), in-game battle screen reuse, donation/rating on finish | code only, ~1 milestone |
| **v1.2 — Wagers** | GameMint deployment config, escrow via existing kirk token flow, settlement via replay adapter + attestation | a running mint |
| **v1.3 — Mobile shells** | cargo-mobile2 Android/iOS wrappers around the same `bevy-ui` shell; app-store assets | SDKs, signing |
| **v2.0 — Content live-ops** | More Champions/rites via data tables (code-free), Chapter Houses narrative, seasonal ladders, spectator replay viewer (transcripts are portable) | design |

## Operational notes

- Reproducible build: `trunk build --release crusade-app/web/index.html --public-url /kirk/`
  (see `docs/BUILDING.md` § web).
- Determinism guarantees mean any two honest clients derive identical battle
  outcomes from the same seed + actions; disputes resolve by publishing the
  transcript (already countersigned).
- Event kind registry: kirk 9259–9263, duel transport 9264, attestation 9265.

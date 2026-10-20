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

1. **PvP.** Wired since v1.1: the Tournament of Virtue lobby screens
   (challenge posting/acceptance over nostr) drive `NetDuelDriver`
   sessions with dual attestation (kind 9265).
2. **Wagers: escrow exists, not in the shipped client.** The V12-2 wager
   escrow is implemented and test-covered end-to-end on the in-memory CDK
   mint (docs/WAGERS.md): NUT-11 2-of-2 duel-pubkey locks with timelocked
   depositor refunds, adjudicated on the ladder's shared attestation fold;
   no rake, refund-on-dispute. Not yet wired into the app UI, and
   production use needs the owner's self-hosted mint pointed in via
   `CRUSADE_MINT_URL` (deployment-time config only). The old
   `GameMint::new_test` stub debt is closed (2026-10-15):
   the test mint is a real in-memory CDK mint and
   `extract_c_value_from_token` parses for real — workspace is fully green.
3. **Browser runtime was verified headless** (build + asset presence + smokes).
   First real-browser run should sanity-check font rendering, wgpu
   (WebGL2/WebGPU fallback) and audio.
4. **Leaderboard settles only on relay-published, mutually signed
   attestations** (V12-1; docs/LADDER.md): kind-9265 seal + counter-seal,
   folded with replay/self-sign/stale/equivocation gates, inert mob
   extras, persisted same-pair damping, and an atomic rolling state that
   equals the genesis fold. Remaining soft spots (all in
   docs/LADDER.md): a peer can forfeit the settle by never
   countersigning; the counter-seal fetch retries up to ~26 s worst case on a
   live duel (3 attempts, 6 s relay timeout each, 4 s gaps); an equivocator's retro-poison voids the battle for fresh
   auditors while the participants' stores keep the prompt settle.
5. Balance debt: "Cantor Sabina" underperforms in fuzz data (M6 content pass).

## Roadmap to full ship

| Phase | Work | Depends on |
|---|---|---|
| **v1.0 (now)** | PWA on GH Pages: practice + campaign, offline, persistent roster | pages owner toggle |
| **v1.1 — PvP wiring** | **DONE.** Nostr lobby UI: post challenge, accept → seed → `NetDuelDriver`, in-game battle screen reuse, rating on finish | shipped |
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

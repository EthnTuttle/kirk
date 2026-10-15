# V1 Ship Plan — Deus Vult: Champions of the Cross

Written 2026-10-14 by the ship planner. Ground truth (verified by orchestrator
this session): HEAD `0318dce` is the shippable-PWA commit; M0–M6 done;
`cargo test --workspace` = 231 green + only the 9 known pre-existing kirk
`cashu::services::*` panics (`src/cashu/mint.rs:47`); all 6 PWA files exist;
`web-pages.yml` audited and coherent; this dev box cannot run the wasm build
(no wasm32 target) nor the bevy smoke (no X/Wayland dev libs) — CI and a real
browser are the only remaining verifiers. Details: `docs/SHIPPING.md`.

## Orchestration amendment (PLAN §7bis, supersede in place)

Flash-tier models provably failed as coding/verification subagents here.
New rule for this plan and onwards: **workers = `tff/kimi-k3` default
variant only; anything subtle/review = `kimi-k3#high`.** No flash workers.
(PLAN.md §7bis should be edited to say this — folded into card A2.)

Card rule per §7bis: cards in the same batch touch disjoint files.

---

## WS-A — v1.0 SHIP (PWA live at github.io)

### A0 — Owner gates [HUMAN ONLY, no worker]
1. Repo Settings → Pages → Source: **GitHub Actions** (one-time flip).
2. Push/merge to `master` touching `crusade-*/` (or re-run the workflow);
   confirm the deploy job is green and the site renders at
   `https://<owner>.github.io/kirk/`.
3. Browser QA checklist (decision: none of this has ever run in a real
   browser — headless only): boot splash → menu → starter roster of 6 →
   practice duel to completion → campaign node 1 → **reload the tab and
   confirm roster persisted** (localStorage `deusvult.roster.v1`) →
   Chrome install icon → launch installed PWA → DevTools → Network →
   Offline → reload (must boot from the service worker cache) → console
   clean (watch for wgpu WebGL2/WebGPU fallback warnings).

### A1 — PWA static-asset audit [worker kimi-k3, review kimi-k3#high]
- **Files:** `crusade-app/web/sw.js`, `manifest.webmanifest`, `index.html`.
- **Contract:** read-only audit + tiny fixes only. `sw.js` discovers hashed
  js/wasm by regex-parsing index.html at install — confirm the regex
  matches trunk 0.19's emitted naming, `CACHE` version is sane, and the
  runtime-cache fallback covers regex misses. Confirm manifest
  `start_url`/`scope`/`id` are relative (`./`) and thus correct under the
  `/kirk/` path prefix, and icons line up with `data-trunk copy-file`s.
- **Acceptance:** eyeball pass + `grep -n` evidence in the session log;
  if edited, `node --check crusade-app/web/sw.js` (or `npx esbuild --check`)
  passes and the workflow file is untouched.

### A2 — Build docs + §7bis amendment [worker kimi-k3]
- **Files:** `docs/BUILDING.md`, `PLAN.md` (§7bis only).
- **Contract:** BUILDING.md is missing the one gate that broke this
  environment: add, top of file and in §(b), "run
  `git submodule update --init deps/cdk` first — any build fails without
  it." In PLAN.md §7bis, replace the flash-model worker roster line with
  the kimi-k3-only rule stated above. Two sentences each, no restructuring.
- **Acceptance:** `grep -n "submodule" docs/BUILDING.md` hits;
  `grep -n "kimi-k3" PLAN.md` shows §7bis updated.

### A3 (optional, v1.0+ε) — "Support the Order" donation button
**[BLOCKED on human decision: the owner must supply a lightning address /
NIP-05 lud16 they control. Firewalled: zaps never touch RNG, matchmaking,
Elo, or arbitration — see `docs/ZAP-MONETIZATION.md` §2.]**
- **Files:** `crusade-app/src/bevy_shell/screens.rs` (menu section only).
- **Contract:** per ZAP-MONETIZATION §3 surface #1: a
  `const SUPPORT_LIGHTNING_ADDRESS: &str` placeholder at the top, one menu
  button "Support the Order" below "Campaign", copy-framed as voluntary
  alms. On wasm it copies to clipboard / opens a `lightning:` URI via
  `window.open`; natively it prints/copies the address. No new deps, no
  events, no Phase change (~30 lines). Owner swaps the constant before
  release.
- **Acceptance:** `cargo check -p crusade-app --features bevy-ui` +
  `cargo test -p crusade-app` green; menu renders the new button (code
  inspection — smoke can't click it).

Batch: A1, A2, A3 are file-disjoint → parallel. A0 is independent (human).

---

## WS-B — v1.1 PvP UI (biggest functional gap)

Existing and tested, reuse as-is: `kirk` kinds 9259/9260
(`src/events/mod.rs`), `derive_battle_seed_from_reveal`
(`crusade-bridge/src/seed.rs`), `NetDuelDriver` incl. `attestation_event()`
(`crusade-app/src/pvp.rs`), nostr transport (wasm-safe websockets,
`crusade-net/src/nostr_transport.rs`).

### B1 — Lobby orchestrator (client lib) [worker kimi-k3, review kimi-k3#high]
- **Files:** `crusade-app/src/lobby.rs` (new), `crusade-app/src/lib.rs`.
- **Contract:** an async `Lobby` handle the UI drives: connect to a
  configurable relay list; publish a challenge (kind 9259 with the
  player's roster commitment); subscribe for accepts (kind 9260) and
  challenges targeting us; on accept, run the kirk commit→reveal handshake
  to both players' C values, then compute `battle_id =
  sha256(challenge_id || accept_id)` and
  `derive_battle_seed_from_reveal` — the seed/battle_id/rosters the UI
  feeds `NetDuelDriver::host/_join`. Default to the nostr transport
  (browser path); iroh upgrade stays native-only.
- **Acceptance:** `cargo test -p crusade-app` green incl. a new lobby test
  over the embedded-relay fixture (host+joiner exchange → identical seeds).

### B3a — Zap module [worker kimi-k3]
(renamed so net-crate work is one file-owner — parallel-safe with B1)
- **Files:** `crusade-net/src/zaps.rs` (new), `crusade-net/src/lib.rs`.
- **Contract:** per ZAP-MONETIZATION §3: build a NIP-57 kind-9734 zap
  request tagging an attestation event id (`e`) + recipient pubkey (`p`),
  and the LNURL-pay switchboard calls (fetch params from the recipient's
  endpoint, POST callback). Failure is always cosmetic; never follow
  cross-domain redirects. `nostr-sdk 0.35` ships the NIP-57 types.
- **Acceptance:** `cargo test -p crusade-net` green incl. zap-request
  build-tag-roundtrip tests.

### B2 — Duel screen wiring (UI phases + shell) [worker kimi-k3, review kimi-k3#high]
- **Files (lib):** `crusade-app/src/view.rs`, `client.rs`. **Files
  (shell):** `src/bevy_shell/{state.rs,screens.rs,battle.rs,input.rs,main.rs}`,
  `src/pvp.rs` only if its API needs a seam.
- **Contract:** new `Phase::Lobby` (list challenges / post / accepting…)
  fed by B1's `Lobby`, and net-duel phases that reuse the existing battle
  renderer verbatim — `NetDuelDriver::snapshot()` already emits the same
  `ViewSnapshot` shapes. The shell spawns one async task per lobby/duel
  (main.rs gets a channel-based bridge to the sync `ClientGame` rebuild
  loop). `finalize` flow unchanged. **Depends on B1; must NOT overlap A3**
  (both touch screens.rs) — run after WS-A merges.
- **Acceptance:** `cargo test --workspace` green; `cargo check -p
  crusade-app --features bevy-ui --target wasm32-unknown-unknown` (on CI
  or a box with the target); headless smoke still exits 0.

### B3b — Post-duel attestation + zap CTA [worker kimi-k3] (after B2+B3a)
- **Files (shell):** `src/bevy_shell/screens.rs` (battle-over section).
- **Contract:** on `BattleOver` after a net duel, publish the driver’s
  kind-9265 attestation to the relay, show "You may offer alms to the
  victor" (loser-optional, spectator-open; copy never says wager/pot/
  winnings) wiring into B3a. Firewall: zero inputs to RNG/Elo.
- **Acceptance:** `cargo test -p crusade-app` green; code review confirms
  attestation id + winner pubkey tag the 9734 request.

### B4 — Elo display [worker kimi-k3] (after B2)
- **Files:** `crusade-app/src/view.rs`, `client.rs`, and shell menu/lobby
  render.
- **Contract:** read `LADDER_RATING_KEY` from the store
  (`pvp::record_ladder_result` writes it) and render "Ladder: NNNN"
  on menu + lobby. Honor-system disclaimer line (SHIPPING limitation 4).
- **Acceptance:** `cargo test -p crusade-app` green.

Sequence: B1 ∥ B3a → B2 → (B3b ∥ B4). Every B card gets a kimi-k3#high
review pass.

---

## WS-C — DEBT / POLISH

- **C1 — Real-browser QA [HUMAN].** The WS-A checklist, run on the deployed
  URL, plus: WebGL2 AND WebGPU/Lavapipe-class fallback, portrait phone
  viewport, private-browsing (SW install fails — game must still play),
  two-machine persistence behavior. This is release-blocking in spirit.
- **C2 — Cantor Sabina balance [worker kimi-k3, review kimi-k3#high].**
  Files: `crusade-core/src/data/` species/rite tables only (the
  underperformer from the M1-tail fuzz note). Acceptance: re-run the
  crusade-sim fuzz harness (the same command used at M1-tail /
  `cargo run -p crusade-sim` per its README/help); Sabina's win rate must
  land within the band the fuzz report treats as competitive across
  levels; `cargo test -p crusade-core` green.
- **C3 — the 9 kirk test panics: RESOLVED 2026-10-15** (post-plan,
  dedicated worker + `#high` review): real in-memory CDK test mint, real
  `extract_c_value_from_token` parse, workspace fully green. Was declared
  OUT of scope for this plan; landed anyway right after ship.
- **C4 — audio: leave excluded** (bevy `audio` already minus-`bevy_audio`
  for CI alsa reasons; documented in BUILDING.md §a). Revisit at v1.3
  mobile shells, which need signing/SDK work anyway.

---

## Recommended execution order

1. **A1 + A2 in parallel immediately** — pure de-risk, bytes only, unblocks
   nothing but burns the two known landmines (precache drift, submodule
   footgun) while they're cheap.
2. **Owner fires A0** the moment the runner shows the repo state is
   Pages-ready; C1 rides on the deployed URL.
3. **A3 only when the owner names a lightning address** — don't let a
   placeholder address ship to a deployed URL.
4. **B1 ∥ B3a next** (v1.1 is the real product), then B2 → B3b ∥ B4.
5. **C2 whenever a worker is idle** — independent table change.

Estimate: **~9 worker-sessions** (A1, A2, A3, B1, B3a, B2, B3b, B4, C2)
plus ~5 kimi-k3#high review passes (A1, B1, B2, B3b, C2) plus human time
for A0/C1 and the donation-address decision.

**Single riskiest item:** the entire *real-browser* surface (C1) — wgpu's
WebGL2/WebGPU path, font rendering, SW precache, and localStorage have
never executed outside headless CI, and neither this box nor CI can prove
them. A broken first paint discovered post-deploy is the one failure mode
with no automated net. Run A0 quickly and treat C1 as the go/no-go gate.

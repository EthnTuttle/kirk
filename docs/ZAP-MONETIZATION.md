# Zap Monetization — NIP-57 value-for-value design

Status: design only. No zaps ship today. Context: `PLAN.md` §5 (trust model),
`docs/SHIPPING.md` (ship status). Cashu wagers remain the v1.2 roadmap item;
zaps are independent of the mint and can ship now.

Design constraints (not negotiable):

- **Voluntary almsgiving, never coercion.** Every zap is a free gift after
  value was delivered. Nothing is paywalled that affects play. Tone rule from
  `PLAN.md` §1 applies to money too: no gambling-mechanic glorification.
- **Soft money, hard outcomes.** Sats are soft signals (thanks, support,
  spectacle). Battle results, XP, roster, and ladder state are hard outcomes
  under the replay-verification trust model. The two must never touch.
- **Piggyback, don't rebuild.** We already publish signed nostr events from
  known player keys (kinds 9264/9265). Zaps reference those events; we add a
  client, not a server.

## 1. Zap surfaces (ranked by cost/benefit)

1. **Menu donation panel — "Support the Order" (SHIPPED 2026-10-15, full
   flow).** The menu button toggles a shell-local alms panel: copyable
   address, an offline-rendered QR of the `lightning:` URI (v3 matrix,
   6 px/cell, ISO quiet zone), a wallet hop (wasm `window.open`; native
   scan-or-copy note), and 21/100/1,000-sat chips that draw LNURL-pay
   bolt11 invoices — browser build only (reuses `zaps::fetch_pay_params`
   validation + a plain `callback?amount=` GET, no kind-9734 from the
   menu); desktop chips are inert dim slabs by NoHttp design. Failures are
   cosmetic dims, never panics. Dep: `qrcode 0.14.1` (default-features off,
   zero transitive deps, matrix rendered as bevy UI rects). Firewall
   intact: pure shell observers + one cosmetic mailbox; nothing touches
   RNG, matchmaking, Elo, or attestation.
2. **Zap the winner after a duel ("tip the victor").** At duel end,
   `NetDuelDriver::attestation_event()` (`crusade-app/src/pvp.rs:246`)
   already yields a signed kind-9265 event id. Once PvP UI lands (v1.1), the
   battle-over screen offers both players and any spectator: "You may offer
   alms to the victor." The zap request (kind 9734) tags the attestation
   event id and targets the winner's pubkey; the winner's NIP-05/LNURL
   endpoint handles invoices. Pure P2P — no escrow, no stake, no dispute
   surface. This is the **stake-light alternative to Cashu wagers**: value
   flows on merit after settlement, so nothing rides on the outcome in
   advance and there is nothing to arbitrate.
3. **Zaps on attestation publication ("witness rewards").** Kind-9265
   attestations are public relay events (`crusade-bridge/src/attest.rs`).
   Any nostr client anywhere can zap them without game code at all. A
   spectator/replay viewer (SHIPPING v2.0) can show zap totals per
   attestation — a crowdsourced "matches worth watching" signal that costs
   us zero trust surface because attestations are already
   observer-verifiable.
4. **Campaign boss-win prompt — "tip the Order".** After a sin-boss falls,
   one honest line: "If this campaign blessed you, you may support its
   makers." Opens the same static address as #1. Shown only after wins,
   dismissible permanently. Cheap; second to ship.
5. **Zap-gated cosmetics — DEFERRED, design with care.** Selling
   cosmetics (banner, title, champion palette) is legitimate value-for-value,
   but delivery requires receipt validation (verifying kind-9735 receipts
   against our LNURL public key) and fulfillment tracking — real code, real
   fraud surface, and it edges toward storefront mechanics. Not before v1.3
   at earliest; never gate gameplay, stats, Champion species, or rites.
6. **Zap-weighted community events / seasonal ladders — DO NOT BUILD as
   proposed.** The moment any published ranking reads zap totals, the wallet
   becomes a game input and the firewall in §2 breaks. Community events may
   track *participation* (attested duels played), never *sats*. If seasonal
   ladders ever aggregate anything, they aggregate replay-verified results
   only. See risks §5.

## 2. Integrity firewall

Systems that must NEVER observe zap existence, amounts, or senders:

| System | Where it lives | Why it must stay blind |
|---|---|---|
| Battle RNG / seed derivation | kirk commit-reveal → `crusade-bridge` → `Battle::new(seed)` | Outcomes must be reproducible from seed + actions by any observer. A money input is a hidden input. |
| Matchmaking / challenge flow | kirk kinds 9259–9263 | Pairing on wealth destroys competitive integrity and invites griefing-through-free-matches. |
| Elo ladder | `crusade-app/src/pvp.rs` `settle_ladder` + crusade-bridge `ladder.rs` | Attestation-folded since V12-1 (docs/LADDER.md); adding purchasable signal makes it a vanity metric, not a rating. |
| Arbitration / mint replay | kirk `Game` adapter, `crusade-bridge` replay | The arbiter replays transcripts to verdict. Its verdicts must depend only on the transcript — that is the whole trust model (PLAN §5). |

Rule of thumb: **zaps may influence what a player SEES (supporter badge,
thank-you note); never what a player IS (rating, roster, odds, matchups).**
Soft money buys gratitude and visibility. Hard outcomes are earned in battle
and proven by transcript. The firewall is also theological cover: almsgiving
buys nothing — that is what makes it almsgiving.

## 3. Technical sketch

What already exists and is reused as-is:

- Signed nostr events from player keys: duel transport kind 9264
  (`crusade-net/src/nostr_transport.rs`, NIP-44 encrypted JSON), attestation
  kind 9265 with battle id in an indexable `d` tag
  (`crusade-bridge/src/attest.rs`).
- nostr/nostr-sdk 0.35 workspace deps (workspace `Cargo.toml:22-23`),
  wasm-safe over relay websockets (crusade-net/Cargo.toml comment is
  explicit). nostr-sdk ships NIP-57 types (kind 9734 zap requests, 9735
  receipts) in this version line.
- `NetDuelDriver::attestation_event()` — the exact event id a zap request
  should tag (`e` tag) alongside the recipient `p` tag.

What stays OUTSIDE the repo (by design — we run no money servers):

- **The lightning address / LNURL-pay endpoint.** Hosted by a wallet
  provider the owner controls. The game only ever displays or links to it.
- **Invoice generation, payment, receipt publication.** Handled by the
  user's own LN wallet (zap via share-intent/`lightning:` URI) or by the
  recipient's LNURL server.
- **NIP-05 identifier** (e.g. `order@deusvult.example`) — DNS + a JSON
  file, not game infra. It is what makes the displayed address verifiable.

New code inside the repo, all small:

| Piece | Crate | Size |
|---|---|---|
| Static donation address constant + menu/about button opening a `lightning:` URI | `crusade-app/src/bevy_shell/screens.rs` (menu section, `Phase::Menu`) | ~30 lines, v1.0+ε — SHIPPED, then expanded 2026-10-15 to the full panel above (QR + invoice chips + wallet hop) |
| `crusade-net/src/zaps.rs` (thin module): build a NIP-57 kind-9734 zap request tagging an attestation event id + recipient pubkey; URL-encode LNURL-pay switchboard flow (fetch LNURL params from the recipient's endpoint, POST callback with the signed zap request) | `crusade-net` new module; re-export from `lib.rs` | ~150 lines + tests, v1.1 |
| Battle-over screen zap CTA wiring `NetDuelDriver::we_won()` / spectator view to the zap module | `crusade-app` (pvp surface + bevy shell) | ~60 lines, v1.1 |
| Optional: fetch/display kind-9735 zap receipts on a spectator transcript view | deferred to the v2.0 replay viewer | later |

Note the honest asymmetry: **sending** a zap from the client is easy (sign
9734 with the player's existing keys, hand invoice to an external wallet via
`lightning:` URI — the PWA holds no funds). **Receiving** requires the owner
to operate nothing; the wallet/LNURL provider does. Nobody's keys for money
ever touch the game.

## 4. Phased plan

| Phase | Ships | Code cost |
|---|---|---|
| **v1.0+ε** | Static donation lightning address in the menu/about screen (surface #1), campaign boss-win prompt optional (surface #4, same address) | Near-zero: one constant + one button. No new deps. |
| **v1.1** (with PvP UI) | "Tip the victor" zap flow on the battle-over screen, zap requests tagging kind-9265 attestations (surface #2). `crusade-net::zaps` module. | One small module + UI wiring, ~1 week. |
| **v1.2** (wager roadmap) | **Zaps and Cashu wagers COEXIST, cleanly separated:** zaps = voluntary, post-hoc, un-arbitrated, any amount, no stake; wagers = pre-commitment, escrowed, mint-settled via replay arbitration. Neither supersedes the other — wagers are for consenting players who want stakes; zaps are for everyone else, including spectators. The firewall (§2) already forbids either from touching outcomes. If wagers slip further, zaps alone remain a complete monetization path. | None for zaps. |
| **v1.3+** | Optional cosmetics storefront (surface #5) only after receipt-verification design is reviewed; spectator zap display (surface #3) with the replay viewer. | Needs design review. |

## 5. Risks (honest list)

1. **Custody of the static address.** A lightning address baked into a
   static PWA pays whoever controls that LNURL domain. If the wallet
   provider / NIP-05 host is compromised or the owner loses the account,
   alms go elsewhere and there is no on-chain signal. Mitigations: use a
   long-lived, owner-controlled NIP-05 identifier as the display name;
   treat the address like a release-signing key (document rotation);
   if rotated, old PWAs keep paying the old address until updated — make
   the address loadable from a signed nostr profile (kind 0 lud16) so
   rotation propagates without a rebuild.
2. **Zap farming / self-dealing.** Any ranking, badge, or event that reads
   zap totals invites wallets zapping themselves (cheap through a custodial
   wallet; real fees only on receipts). §2 forbids rankings from reading
   zaps — enforce it in review, not just in this doc. Surface #3's "zap
   totals per match" is a censorship-soft signal, not a leaderboard; it
   must never feed matchmaking or rewards.
3. **Appearances of gambling if zaps are ever framed as stakes.** The
   "tip the victor" flow must stay loser-optional and spectator-open, and
   copy must never say "wager", "pot", or "winnings". Pre-commitment of
   any kind drifts toward the Cashu wager feature's regulated territory
   without its escrow/arbitration machinery.
4. **App-store rules.** Apple/Google restrict external payment links in
   apps that sell digital content. A donation link with nothing sold is
   generally tolerated; zap-gated cosmetics (surface #5) would not be —
   one more reason it is deferred and optional. Web PWA has no such gate.
5. **Relay noise.** Zap-themed infrastructure adds kind-9734/9735 traffic
   to relays we already use for 9259–9265. Trivial volume today; revisit
   relay selection (paid/whitelisted relays) if a relay starts rate-
   limiting zaps but not duel events or vice versa.
6. **LNURL dependency surface.** The v1.1 zap client makes two HTTP calls
   to the *recipient's* LNURL endpoint. A hostile recipient endpoint can
   fail or serve garbage; the client must treat zap failure as purely
   cosmetic (toast, never block the duel flow) and never follow redirects
   off the original domain.

## Recommendation

Ship surface #1 first: **a static donation lightning address on the
menu/about screen of the existing PWA**, copy-framed as "Support the Order",
~30 lines in `crusade-app/src/bevy_shell/screens.rs`, no new dependencies,
no custody in-repo (the address belongs to an external wallet/NIP-05 the
owner controls), and fully inside the tone rules — a voluntary alms-box at
the chapel door, visible on day one, months before any wager infrastructure
or PvP UI exists. Everything else in this doc builds on the attestation
plumbing that v1.1 will expose anyway.

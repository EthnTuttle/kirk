# The Attested Ladder (V12-1)

The ladder is a **pure fold over the kind-9265 battle-attestation
stream** (crusade-bridge `attest.rs` + `ladder.rs`). A rating moves only
on the strength of signed attestations; there is no honor system, no
local say-so, and nothing else in the game can write it.

## Flow

1. A net duel ends with the FinalDigest exchange (crusade-net). Both
   replicas hold the identical transcript hash; each side signs a
   kind-9265 attestation `{"digest", "battle", "winner", "host", "peer"}`
   with
   its own nostr key, `battle` duplicated into the `d` tag for relay
   indexing. `winner` is an engine *side* index (0 = host); `host` names
   the host's pubkey so any stream reader can map `winner` to a player.
   (`host` and `peer` are serde-default tails on the 9265 content —
   same kind, no new kind minted. Seals lacking either field simply
   never fold, and a test pins that.)
2. Each shell publishes its own seal to the duel's relay set, then
   fetches the *counter-seal* (`author = peer`, `kind = 9265`,
   `d = battle id`) with a bounded retry — the peer legitimately signs
   seconds later — and hands both events to the client as
   `LobbyNet::LadderSettled`.
3. The client folds the pair (`pvp::settle_ladder` →
   `crusade_bridge::fold_attestations`) over its persisted rolling
   state, then writes the result back **atomically** as one
   `ladder.state` JSON value (see below).

## Verification gates (every recipient MUST apply all of them)

Gate any candidate event before it can influence a rating:

1. **Well-formed** — kind is 9265; the nostr event id + Schnorr
   signature verify (`event.verify()`); exactly one `d` tag holding a
   64-**lowercase**-hex battle id (case-explicit: uppercase hex is
   gate-1 inert); content parses as `Attestation` with `winner <= 1`,
   64-lowercase-hex digest, and `battle` equal to the `d` tag.
2. **Equivocation voids** — identical duplicates of one author's seal
   (relay republication) fold to one. One author sealing the same battle
   with *disagreeing* content (digest, winner, host, or peer differs)
   marks
   that author conflicted on this battle: every candidate settlement
   involving them dies, in every fold view, forever. A **retro-poison**
   — a loser later signing a backdated seal claiming the win — is thus
   incentive-equal to withholding: the battle never settles canonically
   and the author cannot pick a preferred outcome (proven:
   `retro_poison_folds_identically_to_withholding`).
3. **Only a mutually-named pair settles** — a battle yields at most one
   settlement: two unconflicted authors whose seals agree on
   digest/battle/winner/host, `host` naming one of them (30 min skew cap:
   `MAX_PAIR_SKEW_SECS` = 1800 s), and **each seal's `peer` naming the
   other seal's author**. Mutual peer-naming retires the whole
   mob/sockpuppet class: a third party's seal — agreeing, disagreeing,
   or backdated into the skew window — names a counterparty the honest
   seals never name back, so no candidate pair containing it exists
   (proven: `sockpuppet_retro_laundering_is_inert`,
   `mobbed_battle_still_settles_the_honest_pair`,
   `peer_misnaming_breaks_the_pair`). Should two disjoint mutually-named
   pairs ever squat one battle id, the (settled_at, author_a, author_b)
   minimum keeps every fold's choice identical.
4. **Damping** — per unordered pair per `LADDER_EPOCH_SECS` (7-day)
   window, the first `FULL_WEIGHT_PER_EPOCH` (2) settlements count at
   full weight; each further one counts at `1 / (excess + 1)`. The
   counters are part of the persisted fold state, so damping is live on
   the rolling path, not only for genesis auditors.

## Determinism and rolling == genesis

Settlements apply in canonical order — (`max(created_at)` of the pair,
battle id) — over canonical pair keys (`a < b` by pubkey bytes). Any two
callers folding the same events over the same fold state derive equal
`LadderTable`s, in any input order.

The rolling state is the *complete* fold state: (full ratings table for
every rated player, settlement cursor = folded battle ids, per-pair
damping counters). Given exactly that state, folding a suffix of the
stream equals folding from genesis — proven on a mixed, non-trivial
history (three players, interleaved pairs, a damped five-match series,
mobbed/equivocated/disagreeing battles) at every duel-boundary split
point (`rolling_fold_matches_genesis_fold_on_mixed_history`). The two
production-relevant caveats:

- **Split granularity**: rolling works at whole duels. A split that
  leaves one seal orphaned is the "missing counter-seal" case and is
  inert, not divergent.
- **created_at ties**: nostr timestamps are seconds; the canonical
  settlement order breaks ties by battle id. Two same-second battles
  folded in the same call are fine (the sort applies); the
  rolling==genesis property assumes suffix events outrank prefix events
  in canonical order, which same-second ties could violate. Real duels
  settle minutes apart, so the production path does not produce such
  folds; a genesis refold over full relay history remains the tiebreak
  audit.

## Persistence: one atomic value

`settle_ladder` writes a single automerge settings key, `ladder.state`,
as one JSON blob: `{ratings: {hex pubkey -> u64}, folded: [battle ids],
damping: [[a, b, epoch, count]...]}`. Atomicity is the point: automerge
settings are last-writer-wins per key, and split rating/cursor keys
could merge into a new-rating-with-old-cursor chimera; a single value
cannot. Two devices settling different duels while forked merge to one
whole blob (never a mix), and re-presenting each device's seals after
the merge converges both devices to the genesis fold — the cursor keeps
the re-fold idempotent (proven:
`two_devices_converge_after_merge_and_resettle`).

The pre-atomic keys `ladder.rating` / `ladder.folded` are read once as a
migration seed (legacy rating into the local player's row, legacy cursor
into the folded set) and never written again. Absent or unparseable
state reads as a fresh ladder — fail-open to unranked; corrupt state can
never mint rating, and since the state is one opaque blob a corrupt
rating cannot persist alongside a healthy cursor.

`ClientGame::ladder_rating` reads our row out of the blob for the
menu/lobby chrome; it never reads the legacy key.

## What this does not solve (honest limits, current)

- A peer can **forfeit rating movement** by never countersigning: the
  duel settles for neither side. They keep no untainted claim either,
  and the transcript is replay-verifiable by an arbiter via the M6
  adapter if a dispute ever matters.
- **Prompt-vs-canonical divergence on equivocation**: if both sides
  settle live and one later retro-poisons the battle, the participants'
  stored rating keeps the honest settlement while every fresh fold sees
  a void. The attacker chose between "honest settlement recorded" and
  "void"; they could never pick a preferred win. The cursor means our
  own store never re-processes the poisoned battle.
- **Relay visibility**: a relay that withholds the counter-seal stalls
  the settle (bounded retry, then the roll stays unmarked this session)
  rather than letting a wrong mark be made. The fold itself is
  idempotent, so if the seals are ever re-presented by hand the settle
  completes exactly once.
- **created_at ties / skew**: an author controls their own created_at
  only within the 1800 s skew window and the canonical tie-breaks; it
  cannot mint movement (gate audit date: 2026-10-18, LADDER-REVIEW.md).
- **Two-key squat residue**: one attacker holding two colluding keys can
  fabricate a disjoint mutually-naming pair on a published battle id. It
  rates nobody but the socks, is publicly observable, and folds
  deterministically (every auditor picks the same
  (settled_at, author_a, author_b) minimum; any store that already folded
  keeps the honest settlement). Not fold-fixable; bounded and documented.
  Epoch-boundary farming nets ~4 full-weight settlements across the
  boundary edge — inherent to bucketing, bounded, zero-sum between the
  pair.

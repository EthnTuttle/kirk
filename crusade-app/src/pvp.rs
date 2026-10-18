//! Network PvP driver (M6 wire): turn-by-turn UI driving of a live
//! [`crusade_net::DuelSession`] over any [`crusade_net::Transport`].
//!
//! `ClientGame` is a synchronous state machine while transports are async,
//! so the driver's owner is the UI's async task, NOT `ClientGame`. A net
//! duel UI composes the two: `ClientGame` keeps the roster store and the
//! menu flows; `NetDuelDriver` runs the duel and renders via
//! [`NetDuelDriver::snapshot`] into the same in-battle [`ViewSnapshot`]
//! shapes, so one renderer serves practice, campaign, and net play.
//!
//! Party/seed plumbing: BOTH parties, the battle seed, and the battle id
//! arrive here as plain parameters (the kirk challenge flow — challenge /
//! accept nostr events, roster exchange, `derive_battle_seed_from_reveal` —
//! integrates above this layer). Both replicas construct the identical
//! `Battle`: the host is side 0 (`Battle::new(my, peer, seed)`) and the
//! joiner side 1 (`Battle::new(peer, my, seed)`).
//!
//! Turn protocol (canonical side-0-first, mirroring crusade-net's contract):
//! [`NetDuelDriver::my_turn`] runs one commit-reveal turn, then walks the
//! forced-switch chain so side 0's round completes fully before side 1's.
//! Our OWN switch can't resolve inside `my_turn` (the UI must pick the
//! replacement), so it defers: `my_turn` returns with
//! [`NetDuelDriver::pending_forced_switch`] true and the UI calls
//! [`NetDuelDriver::my_switch`] with the chosen index, which announces it
//! and then finishes the rest of the chain (relevant only when we are side
//! 0 and side 1 owes next).
//!
//! End of duel: [`NetDuelDriver::finalize`] exchanges the FinalDigest; once
//! that verified, it signs a `crusade_bridge` transcript attestation (when
//! nostr keys were supplied). The ladder moves ONLY when [`settle_ladder`]
//! later folds that seal together with the peer's relay-published
//! counter-seal (V12-1; docs/LADDER.md) — never on a local say-so.

use std::cell::RefCell;

use crusade_bridge::{sign_attestation, Attestation};
use crusade_core::engine::{Action, Battle, BattleEvent};
use crusade_core::types::ChampionInstance;
use crusade_data::{RosterStore, StoreError};
use crusade_net::zaps::ZapRequest;
use crusade_net::{DuelSession, Transport};
use nostr::{Event, JsonUtil, Keys, PublicKey};

use crate::client::{champion_view, render_events, rite_cards, species_name, LOG_CAP, LOG_TAIL};
use crate::view::{ClientInput, Phase, ViewSnapshot};
use crate::AppError;

/// Settings key the attested ladder rating lives under (u64 as decimal
/// string). LEGACY since V12-1r: superseded by [`LADDER_STATE_KEY`]; read
/// only as a migration seed for the local player's rating.
pub const LADDER_RATING_KEY: &str = "ladder.rating";
/// LEGACY: the pre-atomic rolling cursor; read only as a migration seed.
pub const LADDER_FOLDED_KEY: &str = "ladder.folded";
/// The attested ladder's rolling state, ONE automerge settings value
/// (JSON): full ratings table (hex pubkey -> rating), settlement cursor
/// (folded battle ids), and per-pair damping counters. Written atomically
/// by [`settle_ladder`] in a single `set_setting`, so automerge's
/// per-key LWW can never pair a new rating with an old cursor.
pub const LADDER_STATE_KEY: &str = "ladder.state";
/// A fresh store's implicit ladder rating.
pub use crusade_bridge::DEFAULT_LADDER_RATING;

/// Serde shape of the `ladder.state` JSON. All collections sort
/// canonically on write and defaults make every field a tail: a legacy
/// doc with no ladder state parses as empty.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct LadderState {
    /// hex pubkey -> folded rating (every rated player, not just ours).
    #[serde(default)]
    ratings: std::collections::BTreeMap<String, u64>,
    /// Settled battle ids (64-hex) the ratings already reflect.
    #[serde(default)]
    folded: std::collections::BTreeSet<String>,
    /// Per-(unordered pair, epoch) settlement counts:
    /// (hex pubkey a < b, epoch, count).
    #[serde(default)]
    damping: Vec<(String, String, u64, u32)>,
}

/// Load the rolling state, migrating the legacy split keys
/// (`ladder.rating`/`ladder.folded`, pre-atomic V12-1) on first read:
/// the legacy rating seeds `my_pubkey`'s row, the legacy cursor the
/// folded set. Absent or unparseable state reads as a fresh ladder
/// (fail-open to unranked — a corrupt blob can never mint rating).
fn load_ladder_state(
    store: &RosterStore,
    my_pubkey: &PublicKey,
) -> Result<LadderState, StoreError> {
    if let Some(json) = store.setting(LADDER_STATE_KEY)? {
        return Ok(serde_json::from_str(&json).unwrap_or_default());
    }
    let mut state = LadderState::default();
    if let Some(r) = store
        .setting(LADDER_RATING_KEY)?
        .and_then(|s| s.parse::<u64>().ok())
    {
        state.ratings.insert(my_pubkey.to_hex(), r);
    }
    if let Some(cur) = store.setting(LADDER_FOLDED_KEY)? {
        state.folded = cur
            .split(',')
            .filter(|b| !b.is_empty())
            .map(str::to_string)
            .collect();
    }
    // There is no legacy damping state to migrate (it never persisted —
    // the very bug this rework fixes); it starts empty exactly once.
    Ok(state)
}

/// Settle the ladder from a batch of kind-9265 attestation events (the
/// V12-1 ladder, docs/LADDER.md): fold the stream with
/// [`crusade_bridge::fold_attestations`] over the full persisted rolling
/// state (every rated player's rating, the settlement cursor, and the
/// per-pair damping counters), then write the result back ATOMICALLY as
/// one `ladder.state` JSON value. Returns the local player's folded
/// rating, or `None` when the fold did not rate them (unsigned,
/// replayed, stale, disagreeing, equivocated, or self-signed
/// attestations are all inert by construction — the store then keeps
/// its prior state).
pub fn settle_ladder(
    store: &mut RosterStore,
    my_pubkey: &PublicKey,
    events: &[Event],
) -> Result<Option<u64>, StoreError> {
    let state = load_ladder_state(store, my_pubkey)?;
    let mut base = Vec::new();
    for (hex, r) in &state.ratings {
        match PublicKey::from_hex(hex) {
            Ok(pk) => base.push((pk, *r)),
            Err(_) => continue, // a corrupt row drops out; never fatal
        }
    }
    let mut damping = std::collections::BTreeMap::new();
    for (a, b, epoch, n) in &state.damping {
        match (PublicKey::from_hex(a), PublicKey::from_hex(b)) {
            (Ok(pa), Ok(pb)) => {
                damping.insert((pa, pb, *epoch), *n);
            }
            _ => continue,
        }
    }
    let table = crusade_bridge::fold_attestations(events.iter(), &base, &state.folded, &damping);
    if table.rating(my_pubkey).is_none() {
        // The fold did not rate us (we are no author here, or every seal
        // was inert): never write another party's table into our store.
        return Ok(None);
    }
    if table.newly_settled().is_empty() {
        // Nothing settled: inert batch. Return the already-folded rating
        // (base passthrough) without touching the document.
        return Ok(table.rating(my_pubkey));
    }
    let mut next = LadderState::default();
    for (pk, r) in table.ratings_all() {
        next.ratings.insert(pk.to_hex(), r);
    }
    next.folded = state.folded;
    next.folded.extend(table.newly_settled());
    next.damping = table
        .damping_counts()
        .into_iter()
        .map(|(a, b, e, n)| (a.to_hex(), b.to_hex(), e, n))
        .collect();
    let json = serde_json::to_string(&next)
        .map_err(|e| StoreError::Json(format!("ladder state: {e}")))?;
    store.set_setting(LADDER_STATE_KEY, &json)?;
    Ok(table.rating(my_pubkey))
}

/// One live network duel from a single side's point of view.
///
/// Owns the [`DuelSession`] + transport pair; every driving method is async
/// because each blocks on the peer at least once. UI contract per turn:
/// poll `snapshot()` -> feed `my_turn(input)` -> while
/// `pending_forced_switch()` pick from `legal_switch_targets()` and call
/// `my_switch(idx)` -> repeat until `battle_result()` is `Some` -> call
/// `finalize(store)` once.
pub struct NetDuelDriver {
    session: DuelSession,
    transport: Box<dyn Transport>,
    /// Which engine side this driver drives (host = 0, join = 1).
    side: u8,
    /// Attestation battle id (the challenge flow supplies it from above).
    battle_id: [u8; 32],
    /// The duel host's pubkey (engine side 0), named in the kind-9265
    /// content so the ladder fold can map `winner` to a player.
    host_pubkey: PublicKey,
    /// The COUNTERPARTY's pubkey, named in the kind-9265 content so the
    /// ladder's mutual-peer gate can bind our seal to exactly this duel.
    peer_pubkey: PublicKey,
    /// Cloned at construction; used once at finalize to sign the transcript.
    keys: Option<Keys>,
    /// Set once finalize has signed; handed out via `attestation_event()`.
    attestation: Option<Event>,
    /// The FinalDigest exchange is a protocol step: it runs exactly once.
    finalized: bool,
    /// Human-readable log lines (same rendering as practice mode), capped.
    log: Vec<String>,
    /// Raw events since the last snapshot/take. `RefCell` so `snapshot(&self)`
    /// can drain them (take-drain semantics, mirroring ClientGame).
    fresh_events: RefCell<Vec<BattleEvent>>,
    /// How many engine log events have been mirrored into log/fresh_events.
    seen_events: usize,
    /// Per-side active party index cursor for event rendering.
    actives: [usize; 2],
}

impl std::fmt::Debug for NetDuelDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetDuelDriver")
            .field("side", &self.side)
            .field("battle_id", &hex::encode(self.battle_id))
            .field("finalized", &self.finalized)
            .field("battle_result", &self.battle_result())
            .finish_non_exhaustive()
    }
}

impl NetDuelDriver {
    /// Host a duel: we are engine side 0, so our party is `party_a`.
    ///
    /// `seed` and `battle_id` arrive from the challenge flow above this
    /// layer (the seed_R0 for M6 wiring is simply the `seed` parameter).
    /// `host_pubkey` is OUR pubkey (we host); the joiner passes ours in
    /// from the lobby. `peer_pubkey` is the joiner's pubkey (the lobby's
    /// accept outcome carries it). `attestation_keys`, when supplied, sign the
    /// transcript attestation at finalize; without them the duel still
    /// runs but no kind-9265 seal is produced (and therefore no ladder
    /// settlement can ever fold — the ladder moves ONLY on folds of
    /// mutually signed attestations; see [`settle_ladder`]).
    pub fn host(
        my_party: Vec<ChampionInstance>,
        peer_party: Vec<ChampionInstance>,
        transport: impl Transport + 'static,
        seed: [u8; 32],
        battle_id: [u8; 32],
        host_pubkey: PublicKey,
        peer_pubkey: PublicKey,
        attestation_keys: Option<&Keys>,
    ) -> Result<Self, AppError> {
        Self::build(
            0,
            my_party,
            peer_party,
            transport,
            seed,
            battle_id,
            host_pubkey,
            peer_pubkey,
            attestation_keys,
        )
    }

    /// Join a duel: we are engine side 1, so the HOST's party is `party_a`.
    /// `peer_party` is the host's party, `my_party` our own; `host_pubkey`
    /// is the host's (the peer's) nostr pubkey as sealed by the lobby and
    /// `peer_pubkey` the same key again (the host IS our counterparty).
    /// Same seed + same parties as the host => identical battle replicas.
    pub fn join(
        my_party: Vec<ChampionInstance>,
        peer_party: Vec<ChampionInstance>,
        transport: impl Transport + 'static,
        seed: [u8; 32],
        battle_id: [u8; 32],
        host_pubkey: PublicKey,
        peer_pubkey: PublicKey,
        attestation_keys: Option<&Keys>,
    ) -> Result<Self, AppError> {
        Self::build(
            1,
            my_party,
            peer_party,
            transport,
            seed,
            battle_id,
            host_pubkey,
            peer_pubkey,
            attestation_keys,
        )
    }

    fn build(
        side: u8,
        my_party: Vec<ChampionInstance>,
        peer_party: Vec<ChampionInstance>,
        transport: impl Transport + 'static,
        seed: [u8; 32],
        battle_id: [u8; 32],
        host_pubkey: PublicKey,
        peer_pubkey: PublicKey,
        attestation_keys: Option<&Keys>,
    ) -> Result<Self, AppError> {
        // Both replicas must build the identical Battle: party_a is always
        // the host's party, party_b the joiner's.
        let (party_a, party_b) = if side == 0 {
            (my_party, peer_party)
        } else {
            (peer_party, my_party)
        };
        let battle = Battle::new(party_a, party_b, seed)?;
        Ok(Self {
            session: DuelSession::new(battle, side),
            transport: Box::new(transport),
            side,
            battle_id,
            host_pubkey,
            peer_pubkey,
            keys: attestation_keys.cloned(),
            attestation: None,
            finalized: false,
            log: vec!["The duel begins - may your resolve hold.".to_string()],
            fresh_events: RefCell::new(Vec::new()),
            seen_events: 0,
            actives: [0, 0],
        })
    }

    // ---------- read-only accessors ----------

    /// Which engine side this driver drives (host = 0, join = 1).
    pub fn side(&self) -> u8 {
        self.side
    }

    /// The shared engine replica (read-only; UI policy/legality queries).
    pub fn battle(&self) -> &Battle {
        self.session.battle()
    }

    pub fn turn(&self) -> u32 {
        self.session.battle().turn
    }

    /// Rolling transcript digest; equals the peer's after every verified
    /// turn/switch digest exchange.
    pub fn transcript_hash(&self) -> [u8; 32] {
        self.session.transcript_hash()
    }

    /// Engine winner (0 = host, 1 = joiner) once the duel has ended.
    pub fn battle_result(&self) -> Option<u8> {
        self.session.battle().winner
    }

    /// Our win/loss once the duel has ended.
    pub fn we_won(&self) -> Option<bool> {
        self.battle_result().map(|w| w == self.side)
    }

    /// True after [`NetDuelDriver::finalize`] consumed the FinalDigest round.
    pub fn is_finalized(&self) -> bool {
        self.finalized
    }

    /// Our signed transcript attestation; `Some` only when keys were
    /// supplied at construction AND finalize has run.
    pub fn attestation_event(&self) -> Option<Event> {
        self.attestation.clone()
    }

    /// Raw battle events since the last snapshot/take (UI animation queue).
    pub fn take_fresh_events(&self) -> Vec<BattleEvent> {
        self.fresh_events.take()
    }

    /// True while OUR side owes the engine a forced switch. The UI picks an
    /// index from [`NetDuelDriver::legal_switch_targets`] and calls
    /// [`NetDuelDriver::my_switch`]; `my_turn` refuses to run meanwhile.
    pub fn pending_forced_switch(&self) -> bool {
        self.owes_switch(self.side)
    }

    /// The party indices we'd bring in on a switch right now (alive,
    /// non-active). Nonempty exactly when a switch is pending or voluntary.
    pub fn legal_switch_targets(&self) -> Vec<u8> {
        self.session
            .battle()
            .legal_actions(self.side)
            .iter()
            .filter_map(|a| match a {
                Action::Switch { party_idx } => Some(*party_idx),
                _ => None,
            })
            .collect()
    }

    // ---------- turn driving ----------

    /// Play one full turn against the peer. Accepts the battle inputs
    /// (`PickRite`, a voluntary `PickSwitch`, `Concede`); anything else is
    /// an illegal UI transition. Runs the commit-reveal round, then walks
    /// the pending forced switches in canonical side order (side 0's round
    /// completes fully before side 1's). If OUR side owes a switch the turn
    /// cannot fully resolve without a UI pick, so this returns with
    /// `pending_forced_switch()` true and the caller must follow with
    /// `my_switch`; otherwise the returned state is fully turn-resolved.
    pub async fn my_turn(&mut self, input: ClientInput) -> Result<(), AppError> {
        let action = match input {
            ClientInput::PickRite { slot } => Action::Rite { slot },
            ClientInput::PickSwitch { party_idx } => Action::Switch { party_idx },
            ClientInput::Concede => Action::Yield,
            other => {
                return Err(AppError::IllegalTransition(format!(
                    "{other:?} is not a net-duel turn input"
                )))
            }
        };
        if self.battle_result().is_some() {
            return Err(AppError::IllegalTransition("the duel is over".into()));
        }
        if self.pending_forced_switch() {
            return Err(AppError::IllegalTransition(
                "we owe a forced switch — call my_switch with a chosen index".into(),
            ));
        }
        if !self
            .session
            .battle()
            .legal_actions(self.side)
            .contains(&action)
        {
            return Err(AppError::IllegalTransition(format!(
                "{action:?} is not a legal action right now"
            )));
        }
        self.session.run_turn(&*self.transport, action).await?;
        self.sync_events();
        self.resolve_switch_chain().await
    }

    /// Announce our CHOSEN forced-switch index to the peer, then finish the
    /// canonical switch chain (when we are side 0 and side 1 owes next we
    /// await its announcement; when we are side 1 nothing can follow ours).
    pub async fn my_switch(&mut self, party_idx: u8) -> Result<(), AppError> {
        if !self.pending_forced_switch() {
            return Err(AppError::IllegalTransition(
                "no forced switch is pending for our side".into(),
            ));
        }
        if !self.legal_switch_targets().contains(&party_idx) {
            return Err(AppError::IllegalTransition(format!(
                "switch target {party_idx} is not available"
            )));
        }
        let applied = self
            .session
            .announce_forced_switch(&*self.transport, party_idx)
            .await?;
        debug_assert!(applied, "pending_forced_switch checked above");
        self.sync_events();
        self.resolve_switch_chain().await
    }

    /// Walk the canonical side-0-first forced-switch chain after a state
    /// change. Stops when no side owes a switch OR when OUR side owes one
    /// (that pick belongs to the UI via `my_switch`, which resumes here).
    async fn resolve_switch_chain(&mut self) -> Result<(), AppError> {
        for owed in 0..2u8 {
            while self.battle_result().is_none() && self.owes_switch(owed) {
                if owed == self.side {
                    return Ok(());
                }
                let applied = self.session.await_forced_switch(&*self.transport).await?;
                self.sync_events();
                if !applied {
                    break; // defensive: owes_switch said the peer owed
                }
            }
        }
        Ok(())
    }

    /// Exchange FinalDigest with the peer, then — on verified agreement —
    /// sign the transcript attestation (if keys were supplied). Runs its
    /// protocol step exactly once: the FinalDigest exchange is single-shot,
    /// a mid-exchange failure desynchronizes the pair and ENDS the session
    /// (never retry after an error), and calls after success are no-ops.
    ///
    /// V12-1 (docs/LADDER.md): the ladder no longer moves here. The signed
    /// kind-9265 attestation is published to the relays by the shell; the
    /// store's rating changes only when [`settle_ladder`] folds OUR seal
    /// together with the peer's COUNTER-seal off the relay set. A peer who
    /// never countersigns leaves both ratings unmoved — but also keeps no
    /// untainted claim of their own.
    pub async fn finalize(&mut self) -> Result<Option<Event>, AppError> {
        if self.finalized {
            return Ok(self.attestation.clone());
        }
        if self.battle_result().is_none() {
            return Err(AppError::IllegalTransition(
                "the duel is still running".into(),
            ));
        }
        let winner = self.battle_result().expect("checked above");
        self.session.finalize(&*self.transport).await?;
        // Single-shot: the FinalDigest exchange was just consumed, so the
        // error return above ends the session rather than allowing a retry
        // (the peer may already have read our digest). On success the flag
        // below makes every later call a no-op.
        self.finalized = true;
        if let Some(keys) = &self.keys {
            let att = Attestation {
                digest: hex::encode(self.session.transcript_hash()),
                battle: hex::encode(self.battle_id),
                winner,
                host: Some(self.host_pubkey.to_hex()),
                peer: Some(self.peer_pubkey.to_hex()),
            };
            self.attestation = Some(sign_attestation(keys, &att)?);
        }
        Ok(self.attestation.clone())
    }

    // ---------- view adapter ----------

    /// Render the net battle with the exact in-battle shapes ClientGame
    /// produces for practice/campaign, so one UI renderer serves every
    /// mode. `fresh_events` keeps ClientGame's take-drain semantics;
    /// `roster_count` reads 0 and `campaign` `None` — roster/menus remain
    /// ClientGame's job.
    pub fn snapshot(&self) -> ViewSnapshot {
        let b = self.session.battle();
        let peer_side = 1 - self.side;
        let phase = if b.winner.is_some() {
            Phase::BattleOver
        } else if self.pending_forced_switch() {
            Phase::ForcedSwitchSelect
        } else {
            Phase::InBattle
        };
        let tail_start = self.log.len().saturating_sub(LOG_TAIL);
        ViewSnapshot {
            phase,
            roster_count: 0,
            me: Some(champion_view(b, self.side, self.side)),
            foe: Some(champion_view(b, peer_side, self.side)),
            turn: b.turn,
            my_rites: rite_cards(b.sides[usize::from(self.side)].active()),
            can_switch_to: b
                .legal_actions(self.side)
                .iter()
                .filter_map(|a| match a {
                    Action::Switch { party_idx } => b.sides[usize::from(self.side)]
                        .party
                        .get(usize::from(*party_idx))
                        .map(|c| format!("{party_idx}:{}", species_name(c))),
                    _ => None,
                })
                .collect(),
            log_tail: self.log[tail_start..].to_vec(),
            battle_result: b.winner,
            campaign: None,
            // Seam required by view.rs: the driver renders ONLY the duel;
            // lobby view-state belongs to ClientGame, never to a net duel.
            lobby: None,
            // Seam required by view.rs: the witness roll is ClientGame's
            // read-only memorial screen; a duel never carries it.
            witnesses: Vec::new(),
            // Seam required by view.rs: the Manuscript of Sins belongs to
            // ClientGame's Hagiography screen; a duel never carries it.
            sins: Vec::new(),
            // Same seam: the Roll, the Testimonies, and the Call belong to
            // ClientGame — PvP battles never convert (CONVERSION 3) and a
            // driver's snapshot never enters the codex phases.
            foes: Vec::new(),
            testimonies: Vec::new(),
            conversion: None,
            // Same seam: the Pilgrim Road belongs to ClientGame's solo
            // lane; a net duel's snapshot never carries it.
            trail: None,
            fresh_events: self.fresh_events.take(),
        }
    }

    // ---------- internals ----------

    /// True when `side` owes the engine a forced switch (legal_actions
    /// narrows to all-Switch exactly then; empty on a finished battle).
    fn owes_switch(&self, side: u8) -> bool {
        let legal = self.session.battle().legal_actions(side);
        !legal.is_empty() && legal.iter().all(|a| matches!(a, Action::Switch { .. }))
    }

    /// Mirror newly emitted engine events into the log (rendered like
    /// practice mode) and the fresh-events buffer.
    fn sync_events(&mut self) {
        let total = self.session.battle().events().len();
        if total <= self.seen_events {
            return;
        }
        let delta = self.session.battle().events()[self.seen_events..].to_vec();
        let lines = render_events(self.session.battle(), &delta, &mut self.actives, self.side);
        self.seen_events = total;
        self.log.extend(lines);
        if self.log.len() > LOG_CAP {
            let excess = self.log.len() - LOG_CAP;
            self.log.drain(..excess);
        }
        self.fresh_events.borrow_mut().extend(delta);
    }
}

// ---------- Battle-over alms (card B3b; docs/ZAP-MONETIZATION.md surface #2) ----------
//
// INTEGRITY FIREWALL (docs/ZAP-MONETIZATION.md §2): these functions only
// READ a settlement that is already final — the kind-9265 attestation is
// fully signed inside `finalize` BEFORE any zap code can run, and nothing
// here feeds back into the battle, its seed, matchmaking, or the ladder.
// Soft money may touch what a player SEES; never what a player IS.

/// The ONE construction site for the kind-9734 zap request through which a
/// player may offer alms to the victor of a settled net duel: the
/// `attestation` event id becomes the zap's `e` tag and `victor_pubkey`
/// its `p` tag (plus the `amount` and the `relays` the receipt should land
/// on). Deliberately tiny, with both ids visibly threaded, so review can
/// confirm the two mandatory tags at a glance. Unsigned — the shell signs
/// with the player's own seal at the call site.
pub fn victor_alms_request(
    attestation: &Event,
    victor_pubkey: PublicKey,
    amount_msat: u64,
    relays: &[String],
) -> ZapRequest {
    ZapRequest::new(victor_pubkey, attestation.id, amount_msat)
        .relays(relays.iter().map(String::as_str))
        .message("Alms for the victor — well fought.")
}

/// The victor's lightning route out of a kind-0 profile's content JSON:
/// the `lud16` lightning address preferred, else the `lud06` bech32 lnurl;
/// trimmed, whitespace-only treated as absent. Pure parsing — validating
/// and fetching the endpoint belongs to `crusade_net::zaps`.
pub fn victor_lightning_address(kind0_content: &str) -> Option<String> {
    let metadata = nostr::types::Metadata::from_json(kind0_content).ok()?;
    [metadata.lud16, metadata.lud06]
        .into_iter()
        .flatten()
        .map(|s| s.trim().to_string())
        .find(|s| !s.is_empty())
}

#[cfg(test)]
mod alms_tests {
    //! The B3b acceptance line: the signed kind-9734 visibly carries the
    //! attestation's event id as its `e` tag and the victor's pubkey as its
    //! `p` tag, exactly as a NIP-57 wallet server reads them back.

    use super::*;
    use nostr::{Kind, TagStandard};

    fn signed_attestation(keys: &Keys) -> Event {
        sign_attestation(
            keys,
            &Attestation {
                digest: hex::encode([0xD1; 32]),
                battle: hex::encode([0xB1; 32]),
                winner: 1,
                host: None,
                peer: None,
            },
        )
        .expect("attestation signs")
    }

    #[test]
    fn alms_request_tags_the_attestation_id_and_victor_pubkey() {
        let loser = Keys::generate();
        let victor = Keys::generate();
        // The attestation exists, fully signed, before any zap code runs.
        let attestation = signed_attestation(&loser);
        let relays: Vec<String> = ["wss://relay.one", "wss://relay.two"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let signed = victor_alms_request(&attestation, victor.public_key(), 21_000, &relays)
            .sign(&loser)
            .expect("the zap request signs with the player's seal");

        assert_eq!(signed.kind, Kind::ZapRequest);
        assert_eq!(signed.kind.as_u16(), 9734);
        let e_tags: Vec<_> = signed
            .tags
            .iter()
            .filter_map(|t| match t.as_standardized() {
                Some(TagStandard::Event { event_id, .. }) => Some(*event_id),
                _ => None,
            })
            .collect();
        assert_eq!(
            e_tags,
            vec![attestation.id],
            "the zap request's e tag is exactly the attestation event id"
        );
        let p_tags: Vec<_> = signed
            .tags
            .iter()
            .filter_map(|t| match t.as_standardized() {
                Some(TagStandard::PublicKey { public_key, .. }) => Some(*public_key),
                _ => None,
            })
            .collect();
        assert_eq!(
            p_tags,
            vec![victor.public_key()],
            "the zap request's p tag is exactly the victor's pubkey"
        );
        let amounts: Vec<u64> = signed
            .tags
            .iter()
            .filter_map(|t| match t.as_standardized() {
                Some(TagStandard::Amount { millisats, .. }) => Some(*millisats),
                _ => None,
            })
            .collect();
        assert_eq!(amounts, vec![21_000]);
        let relay_tags: Vec<String> = signed
            .tags
            .iter()
            .filter_map(|t| match t.as_standardized() {
                Some(TagStandard::Relays(urls)) => Some(urls),
                _ => None,
            })
            .flatten()
            .map(|u| u.as_str().to_string())
            .collect();
        assert_eq!(relay_tags, relays, "receipt relays = the duel's relay set");
        assert_eq!(signed.content, "Alms for the victor — well fought.");
    }

    #[test]
    fn victor_address_prefers_lud16_then_lud06_and_skips_empty() {
        assert_eq!(
            victor_lightning_address(r#"{"name":"Victor","lud16":"victor@wallet.example"}"#)
                .as_deref(),
            Some("victor@wallet.example")
        );
        // lud16 wins when both fields are populated.
        assert_eq!(
            victor_lightning_address(r#"{"lud16":"a@b.example","lud06":"lnurl1abc"}"#).as_deref(),
            Some("a@b.example")
        );
        // A whitespace-only lud16 falls through to lud06.
        assert_eq!(
            victor_lightning_address(r#"{"lud16":"  ","lud06":"lnurl1abc"}"#).as_deref(),
            Some("lnurl1abc")
        );
        assert_eq!(victor_lightning_address(r#"{"name":"No purse"}"#), None);
        assert_eq!(victor_lightning_address("not json at all"), None);
        assert_eq!(victor_lightning_address(r#"{"lud16":""}"#), None);
    }
}

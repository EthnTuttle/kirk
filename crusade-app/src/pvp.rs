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
//! nostr keys were supplied at construction) and applies the Elo ladder
//! result to the store.

use std::cell::RefCell;

use crusade_bridge::{sign_attestation, Attestation};
use crusade_core::engine::{Action, Battle, BattleEvent};
use crusade_core::types::ChampionInstance;
use crusade_data::{RosterStore, StoreError};
use crusade_net::{DuelSession, Transport};
use nostr::{Event, Keys};

use crate::client::{champion_view, render_events, rite_cards, species_name, LOG_CAP, LOG_TAIL};
use crate::view::{ClientInput, Phase, ViewSnapshot};
use crate::AppError;

/// Settings key the ladder Elo rating lives under (u64 as decimal string).
pub const LADDER_RATING_KEY: &str = "ladder.rating";
/// A fresh store's implicit ladder rating.
pub const DEFAULT_LADDER_RATING: u64 = 1000;
/// Elo K factor; a win/loss against an equal-rated opponent moves K/2 = 16.
const ELO_K: f64 = 32.0;

/// Record one finished ladder duel into the store's settings and return the
/// new rating. Elo: `new = old + K * (score - 1/(1 + 10^((opp-old)/400)))`,
/// rounded to the nearest integer and floored at 0; score is 1.0 on a win,
/// 0.0 on a loss. A missing or unparseable stored rating starts from
/// [`DEFAULT_LADDER_RATING`], as does an unknown `opponent_rating`.
pub fn record_ladder_result(
    store: &mut RosterStore,
    my_win: bool,
    opponent_rating: Option<u64>,
) -> Result<u64, StoreError> {
    let mine = store
        .setting(LADDER_RATING_KEY)?
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(DEFAULT_LADDER_RATING);
    let theirs = opponent_rating.unwrap_or(DEFAULT_LADDER_RATING);
    let expected = 1.0 / (1.0 + 10f64.powf((theirs as f64 - mine as f64) / 400.0));
    let score = if my_win { 1.0 } else { 0.0 };
    let next = (mine as f64 + ELO_K * (score - expected)).round().max(0.0) as u64;
    store.set_setting(LADDER_RATING_KEY, &next.to_string())?;
    Ok(next)
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
    /// Ladder rating after finalize's update, if that step ran.
    ladder_rating: Option<u64>,
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
    /// `attestation_keys`, when supplied, sign the transcript attestation
    /// at finalize; without them the duel still runs and settles unrated by
    /// attestation (the ladder still moves on a verified FinalDigest).
    pub fn host(
        my_party: Vec<ChampionInstance>,
        peer_party: Vec<ChampionInstance>,
        transport: impl Transport + 'static,
        seed: [u8; 32],
        battle_id: [u8; 32],
        attestation_keys: Option<&Keys>,
    ) -> Result<Self, AppError> {
        Self::build(
            0,
            my_party,
            peer_party,
            transport,
            seed,
            battle_id,
            attestation_keys,
        )
    }

    /// Join a duel: we are engine side 1, so the HOST's party is `party_a`.
    /// `peer_party` is the host's party, `my_party` our own. Same seed +
    /// same parties as the host => identical battle replicas.
    pub fn join(
        my_party: Vec<ChampionInstance>,
        peer_party: Vec<ChampionInstance>,
        transport: impl Transport + 'static,
        seed: [u8; 32],
        battle_id: [u8; 32],
        attestation_keys: Option<&Keys>,
    ) -> Result<Self, AppError> {
        Self::build(
            1,
            my_party,
            peer_party,
            transport,
            seed,
            battle_id,
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
            keys: attestation_keys.cloned(),
            attestation: None,
            finalized: false,
            log: vec!["The duel begins — may your resolve hold.".to_string()],
            fresh_events: RefCell::new(Vec::new()),
            seen_events: 0,
            actives: [0, 0],
            ladder_rating: None,
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

    /// The ladder rating written by finalize, if that step ran.
    pub fn ladder_rating(&self) -> Option<u64> {
        self.ladder_rating
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
    /// sign the transcript attestation (if keys were supplied) and apply the
    /// Elo ladder result to `store`. Runs its protocol step exactly once:
    /// the FinalDigest exchange is single-shot, a mid-exchange failure
    /// desynchronizes the pair and ENDS the session (never retry after an
    /// error), and calls after success are no-ops.
    ///
    /// M6-wire simplifications, to unwind when the kirk challenge flow lands:
    /// ratings move on a verified FinalDigest alone (mutual attestation
    /// publication/checks live above this layer), and the peer's rating
    /// isn't carried in-band yet, so it defaults to 1000.
    pub async fn finalize(&mut self, store: &mut RosterStore) -> Result<(), AppError> {
        if self.finalized {
            return Ok(());
        }
        let Some(winner) = self.battle_result() else {
            return Err(AppError::IllegalTransition(
                "the duel is still running".into(),
            ));
        };
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
            };
            self.attestation = Some(sign_attestation(keys, &att)?);
        }
        self.ladder_rating = Some(record_ladder_result(store, winner == self.side, None)?);
        Ok(())
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

//! DuelSession: per-turn commit-reveal around a crusade-core Battle.
//! Contract orchestrator-owned; worker implements bodies + tests.
//!
//! Per turn (both sides symmetric — send before recv so duplex links pipeline):
//!   send Commit{turn, sha256(action)} -> recv peer Commit -> send Reveal ->
//!   recv peer Reveal (verify vs its commit, then legality-check) ->
//!   battle.apply_turn -> digest -> send TurnDigest{turn, digest} ->
//!   recv + compare peer digest.
//! If the turn emits ForcedSwitchNeeded for side S: S runs a one-way
//! `announce_forced_switch` round (send ForcedSwitch, applied by both
//! replicas via apply_forced_switch) and BOTH sides then fold the newly
//! emitted events into the rolling transcript digest and exchange one more
//! TurnDigest. When both sides owe a switch, callers MUST resolve them in
//! canonical side order: side 0's switch round completes fully
//! (announce/await + digest) before side 1's begins. Ends with FinalDigest
//! exchange.
//!
//! Out-of-order tolerance: lawful in-duel messages for the current or a
//! future turn that arrive in the wrong phase are stashed (bounded) instead
//! of killing the session; Abort always surfaces immediately.

use std::collections::VecDeque;

use crusade_core::engine::{Action, Battle, BattleEvent};

use crate::msg::{commit_hash, digest_update, NetMsg};
use crate::transport::Transport;
use crate::NetError;

/// Max out-of-order messages stashed while awaiting the expected one;
/// beyond this the peer is treated as flooding and the protocol aborts.
const MAX_STASHED: usize = 32;

#[derive(Debug)]
pub struct TurnResult {
    pub events: Vec<BattleEvent>,
    pub transcript_hash: [u8; 32],
    pub battle_over: bool,
}

#[derive(Debug)]
pub struct DuelSession {
    battle: Battle,
    /// 0 or 1 — which side of the engine this instance drives.
    my_side: u8,
    transcript_hash: [u8; 32],
    /// Lawful in-duel messages received out of phase (reordered wires).
    stash: VecDeque<NetMsg>,
    /// How many engine log events have been folded into `transcript_hash`.
    digested_len: usize,
}

impl DuelSession {
    /// `battle` is the fresh, identical battle on both replicas.
    /// my_side==0 drives `party_a`'s choices.
    pub fn new(battle: Battle, my_side: u8) -> Self {
        Self {
            battle,
            my_side,
            transcript_hash: [0u8; 32],
            stash: VecDeque::new(),
            digested_len: 0,
        }
    }

    pub fn battle(&self) -> &Battle {
        &self.battle
    }

    pub fn transcript_hash(&self) -> [u8; 32] {
        self.transcript_hash
    }

    /// True when `side` owes the engine a forced switch (detected via
    /// legal_actions being all-Switch; the battle log's ForcedSwitchNeeded
    /// is the event-level mirror of this state).
    fn switch_pending(&self, side: u8) -> bool {
        let legal = self.battle.legal_actions(side);
        !legal.is_empty() && legal.iter().all(|a| matches!(a, Action::Switch { .. }))
    }

    /// Turn tag of a message; FinalDigest and Abort carry no turn.
    fn msg_turn(m: &NetMsg) -> Option<u32> {
        match m {
            NetMsg::Commit { turn, .. }
            | NetMsg::Reveal { turn, .. }
            | NetMsg::TurnDigest { turn, .. }
            | NetMsg::ForcedSwitch { turn, .. } => Some(*turn),
            NetMsg::FinalDigest { .. } | NetMsg::Abort { .. } => None,
        }
    }

    /// Lawful in-duel message for the CURRENT-or-FUTURE turn: stashable.
    fn stashable(&self, m: &NetMsg) -> bool {
        !matches!(m, NetMsg::Abort { .. })
            && Self::msg_turn(m).is_none_or(|t| t >= self.battle.turn)
    }

    /// Recv until `want` matches, draining the stash first. Out-of-phase
    /// but lawful current/future messages are stashed for later phases;
    /// stale past-turn messages are dropped; Abort surfaces immediately.
    async fn recv_until<T>(
        &mut self,
        t: &dyn Transport,
        want: impl Fn(&NetMsg) -> Option<T>,
    ) -> Result<T, NetError> {
        loop {
            if let Some(pos) = self.stash.iter().position(|m| want(m).is_some()) {
                let m = self.stash.remove(pos).expect("position found above");
                return Ok(want(&m).expect("matched at position"));
            }
            let m = t.recv().await?;
            if let NetMsg::Abort { reason } = m {
                return Err(NetError::Protocol(reason));
            }
            if let Some(v) = want(&m) {
                return Ok(v);
            }
            if self.stashable(&m) {
                if self.stash.len() >= MAX_STASHED {
                    let _ = t
                        .send(&NetMsg::Abort {
                            reason: "stash overflow".into(),
                        })
                        .await;
                    return Err(NetError::Protocol("stash overflow".into()));
                }
                self.stash.push_back(m);
            }
        }
    }

    /// Fold every not-yet-digested engine event into the rolling transcript
    /// hash. Both replicas observe identical event deltas, so both fold the
    /// identical bytes.
    fn fold_new_events(&mut self) -> Result<(), NetError> {
        let delta = &self.battle.events()[self.digested_len..];
        let events_json = serde_json::to_vec(delta).map_err(|e| NetError::Serde(e.to_string()))?;
        self.transcript_hash = digest_update(&self.transcript_hash, &events_json);
        self.digested_len = self.battle.events().len();
        Ok(())
    }

    /// Send our rolling digest for turn `turn` and verify the peer's
    /// matches. Used after apply_turn (turn = the applied turn, before the
    /// engine increments) and after each forced-switch round.
    async fn exchange_turn_digest(&mut self, t: &dyn Transport, turn: u32) -> Result<(), NetError> {
        let digest = self.transcript_hash;
        t.send(&NetMsg::TurnDigest { turn, hash: digest }).await?;
        let (got, hash) = self
            .recv_until(t, |m| match m {
                NetMsg::TurnDigest { turn, hash } => Some((*turn, *hash)),
                _ => None,
            })
            .await?;
        if got != turn {
            return Err(NetError::TurnOrder {
                expected: turn,
                got,
            });
        }
        if hash != digest {
            return Err(NetError::TranscriptDiverged(turn));
        }
        Ok(())
    }

    /// Full synchronous round for one turn. Blocks on the peer multiple
    /// times. Sends before recving at each phase so duplex links pipeline.
    pub async fn run_turn(
        &mut self,
        t: &dyn Transport,
        my_action: Action,
    ) -> Result<TurnResult, NetError> {
        let turn = self.battle.turn;

        // Phase 1: commit.
        t.send(&NetMsg::Commit {
            turn,
            hash: commit_hash(&my_action),
        })
        .await?;
        let (got, peer_commit) = self
            .recv_until(t, |m| match m {
                NetMsg::Commit { turn, hash } => Some((*turn, *hash)),
                _ => None,
            })
            .await?;
        if got != turn {
            return Err(NetError::TurnOrder {
                expected: turn,
                got,
            });
        }

        // Phase 2: reveal; verify the peer's reveal against its stored
        // commitment, then its legality, BEFORE any state is applied.
        t.send(&NetMsg::Reveal {
            turn,
            action: my_action,
        })
        .await?;
        let (got, peer_action) = self
            .recv_until(t, |m| match m {
                NetMsg::Reveal { turn, action } => Some((*turn, *action)),
                _ => None,
            })
            .await?;
        if got != turn {
            return Err(NetError::TurnOrder {
                expected: turn,
                got,
            });
        }
        if commit_hash(&peer_action) != peer_commit {
            return Err(NetError::CommitmentMismatch);
        }
        let peer_side = 1 - self.my_side;
        if !self.battle.legal_actions(peer_side).contains(&peer_action) {
            let _ = t
                .send(&NetMsg::Abort {
                    reason: "illegal action".into(),
                })
                .await;
            return Err(NetError::Protocol("illegal action".into()));
        }

        // Phase 3: apply. apply_turn takes (action_for_side0,
        // action_for_side1) regardless of which side this session drives;
        // both replicas hold identical Battle state, so they converge.
        let (a, b) = if self.my_side == 0 {
            (my_action, peer_action)
        } else {
            (peer_action, my_action)
        };
        let events = self.battle.apply_turn(a, b)?;

        // Phase 4: rolling transcript digest; divergence is caught the turn
        // it happens, not at settlement.
        self.fold_new_events()?;
        let digest = self.transcript_hash;
        self.exchange_turn_digest(t, turn).await?;

        Ok(TurnResult {
            events,
            transcript_hash: digest,
            battle_over: self.battle.winner.is_some(),
        })
    }

    /// If the last turn raised ForcedSwitchNeeded for MY side, announce the
    /// choice to the peer and apply locally. After applying, BOTH replicas
    /// fold the emitted events into the transcript digest and exchange one
    /// TurnDigest round. Returns true when it applied.
    ///
    /// Callers must resolve pending switches in canonical side order: side
    /// 0's round (announce/await + digest) completes fully before side 1's.
    pub async fn announce_forced_switch(
        &mut self,
        t: &dyn Transport,
        party_idx: u8,
    ) -> Result<bool, NetError> {
        if !self.switch_pending(self.my_side) {
            return Ok(false);
        }
        let turn = self.battle.turn;
        t.send(&NetMsg::ForcedSwitch { turn, party_idx }).await?;
        self.battle.apply_forced_switch(self.my_side, party_idx)?;
        self.fold_new_events()?;
        self.exchange_turn_digest(t, turn).await?;
        Ok(true)
    }

    /// If the last turn raised ForcedSwitchNeeded for the PEER's side, await
    /// their announcement and apply locally, then fold + digest-exchange
    /// exactly as the announcing side did. Returns true when it applied.
    pub async fn await_forced_switch(&mut self, t: &dyn Transport) -> Result<bool, NetError> {
        let peer = 1 - self.my_side;
        if !self.switch_pending(peer) {
            return Ok(false);
        }
        let turn = self.battle.turn;
        let (got, party_idx) = self
            .recv_until(t, |m| match m {
                NetMsg::ForcedSwitch { turn, party_idx } => Some((*turn, *party_idx)),
                _ => None,
            })
            .await?;
        if got != turn {
            return Err(NetError::TurnOrder {
                expected: turn,
                got,
            });
        }
        self.battle.apply_forced_switch(peer, party_idx)?;
        self.fold_new_events()?;
        self.exchange_turn_digest(t, turn).await?;
        Ok(true)
    }

    /// Exchange FinalDigest after battle end; verify equality. Single-shot:
    /// the exchange is consumed by the first call, so a mid-exchange failure
    /// desynchronizes the pair (the peer may already have read our digest)
    /// and ends the session — callers must NOT retry after an error.
    pub async fn finalize(&mut self, t: &dyn Transport) -> Result<(), NetError> {
        let hash = self.transcript_hash;
        t.send(&NetMsg::FinalDigest { hash }).await?;
        let peer = self
            .recv_until(t, |m| match m {
                NetMsg::FinalDigest { hash } => Some(*hash),
                _ => None,
            })
            .await?;
        if peer == hash {
            Ok(())
        } else {
            Err(NetError::TranscriptDiverged(self.battle.turn))
        }
    }
}

//! DuelSession: per-turn commit-reveal around a crusade-core Battle.
//! Contract orchestrator-owned; worker implements bodies + tests.
//!
//! Per turn (both sides symmetric — send before recv so duplex links pipeline):
//!   send Commit{turn, sha256(action)} -> recv peer Commit -> send Reveal ->
//!   recv peer Reveal (verify vs its commit) -> battle.apply_turn -> digest
//!   -> send TurnDigest{turn, digest} -> recv + compare peer digest.
//! If the turn emits ForcedSwitchNeeded for side S: S runs a one-way
//! `announce_forced_switch` round (send ForcedSwitch, applied by both
//! replicas via apply_forced_switch). Ends with FinalDigest exchange.

use crusade_core::engine::{Action, Battle, BattleEvent};

use crate::msg::{commit_hash, digest_update, NetMsg};
use crate::transport::Transport;
use crate::NetError;

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
}

impl DuelSession {
    /// `battle` is the fresh, identical battle on both replicas.
    /// my_side==0 drives `party_a`'s choices.
    pub fn new(battle: Battle, my_side: u8) -> Self {
        Self {
            battle,
            my_side,
            transcript_hash: [0u8; 32],
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
        let peer_commit = match t.recv().await? {
            NetMsg::Commit { turn: got, hash } => {
                if got != turn {
                    return Err(NetError::TurnOrder {
                        expected: turn,
                        got,
                    });
                }
                hash
            }
            NetMsg::Abort { reason } => return Err(NetError::Protocol(reason)),
            _ => return Err(NetError::Protocol("unexpected message".into())),
        };

        // Phase 2: reveal; verify the peer's reveal against its stored
        // commitment BEFORE any state is applied.
        t.send(&NetMsg::Reveal {
            turn,
            action: my_action,
        })
        .await?;
        let peer_action = match t.recv().await? {
            NetMsg::Reveal { turn: got, action } => {
                if got != turn {
                    return Err(NetError::TurnOrder {
                        expected: turn,
                        got,
                    });
                }
                if commit_hash(&action) != peer_commit {
                    return Err(NetError::CommitmentMismatch);
                }
                action
            }
            NetMsg::Abort { reason } => return Err(NetError::Protocol(reason)),
            _ => return Err(NetError::Protocol("unexpected message".into())),
        };

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
        let events_json =
            serde_json::to_vec(&events).map_err(|e| NetError::Serde(e.to_string()))?;
        self.transcript_hash = digest_update(&self.transcript_hash, &events_json);
        let digest = self.transcript_hash;
        t.send(&NetMsg::TurnDigest { turn, hash: digest }).await?;
        match t.recv().await? {
            NetMsg::TurnDigest { turn: got, hash } => {
                if got != turn {
                    return Err(NetError::TurnOrder {
                        expected: turn,
                        got,
                    });
                }
                if hash != digest {
                    return Err(NetError::TranscriptDiverged(turn));
                }
            }
            NetMsg::Abort { reason } => return Err(NetError::Protocol(reason)),
            _ => return Err(NetError::Protocol("unexpected message".into())),
        }

        Ok(TurnResult {
            events,
            transcript_hash: digest,
            battle_over: self.battle.winner.is_some(),
        })
    }

    /// If the last turn raised ForcedSwitchNeeded for MY side, announce the
    /// choice to the peer and apply locally. Returns true when it applied.
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
        Ok(true)
    }

    /// If the last turn raised ForcedSwitchNeeded for the PEER's side, await
    /// their announcement and apply locally. Returns true when it applied.
    pub async fn await_forced_switch(&mut self, t: &dyn Transport) -> Result<bool, NetError> {
        let peer = 1 - self.my_side;
        if !self.switch_pending(peer) {
            return Ok(false);
        }
        let turn = self.battle.turn;
        match t.recv().await? {
            NetMsg::ForcedSwitch {
                turn: got,
                party_idx,
            } => {
                if got != turn {
                    return Err(NetError::TurnOrder {
                        expected: turn,
                        got,
                    });
                }
                self.battle.apply_forced_switch(peer, party_idx)?;
                Ok(true)
            }
            NetMsg::Abort { reason } => Err(NetError::Protocol(reason)),
            _ => Err(NetError::Protocol("unexpected message".into())),
        }
    }

    /// Exchange FinalDigest after battle end; verify equality.
    pub async fn finalize(&mut self, t: &dyn Transport) -> Result<(), NetError> {
        let hash = self.transcript_hash;
        t.send(&NetMsg::FinalDigest { hash }).await?;
        match t.recv().await? {
            NetMsg::FinalDigest { hash: peer } => {
                if peer == hash {
                    Ok(())
                } else {
                    Err(NetError::TranscriptDiverged(self.battle.turn))
                }
            }
            NetMsg::Abort { reason } => Err(NetError::Protocol(reason)),
            _ => Err(NetError::Protocol("unexpected message".into())),
        }
    }
}

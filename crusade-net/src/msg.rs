//! Wire messages. serde_json bytes on the wire (upgrade path: bincode).

use crusade_core::engine::Action;
use serde::{Deserialize, Serialize};

/// One duel message. `turn` mirrors the engine turn number (starts at 1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetMsg {
    /// sha256(serde_json(action)) for this turn.
    Commit { turn: u32, hash: [u8; 32] },
    /// The revealed action; receiver verifies against the stored commit.
    Reveal { turn: u32, action: Action },
    /// After both reveals are applied: rolling transcript hash.
    /// `hash = sha256(prev_hash || serde_json(events_this_turn))`.
    TurnDigest { turn: u32, hash: [u8; 32] },
    /// The side owing a forced switch announces its (state-affecting) choice;
    /// the peer applies it to its replica via apply_forced_switch.
    ForcedSwitch { turn: u32, party_idx: u8 },
    /// Polite close at battle end: final transcript hash.
    FinalDigest { hash: [u8; 32] },
    /// Protocol-level abort with a human-readable reason.
    Abort { reason: String },
}

pub fn netmsg_from_json(bytes: &[u8]) -> Result<NetMsg, crate::NetError> {
    serde_json::from_slice(bytes).map_err(|e| crate::NetError::Serde(e.to_string()))
}

pub fn netmsg_to_json(m: &NetMsg) -> Result<Vec<u8>, crate::NetError> {
    serde_json::to_vec(m).map_err(|e| crate::NetError::Serde(e.to_string()))
}

pub fn commit_hash(action: &Action) -> [u8; 32] {
    use sha2::Digest;
    let ser = serde_json::to_vec(action).expect("Action serializes");
    let mut h = sha2::Sha256::new();
    h.update(ser);
    h.finalize().into()
}

pub fn digest_update(prev: &[u8; 32], events_json: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    let mut h = sha2::Sha256::new();
    h.update(prev);
    h.update(events_json);
    h.finalize().into()
}

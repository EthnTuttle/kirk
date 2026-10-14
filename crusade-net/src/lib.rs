//! crusade-net: transport abstraction + synchronous commit-reveal duel
//! sessions over P2P links.
//!
//! Two duelists each run a [`DuelSession`] over a [`Transport`]. Per turn both
//! sides commit `sha256(action)`, exchange reveals, verify, then apply the
//! pair to their local `crusade-core` engine instance. Identical seeds +
//! identical action pairs => identical event transcripts; a rolling
//! transcript hash is exchanged per turn so divergence is detected the turn
//! it happens, not at settlement.
//!
//! The battle seed arrives from OUTSIDE (kirk commit-reveal at M4); this
//! crate treats it as an opaque [u8; 32].

pub mod duel;
pub mod memory;
pub mod msg;
pub mod transport;

pub use duel::*;
pub use msg::*;
pub use transport::*;

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum NetError {
    #[error("transport closed")]
    Closed,
    #[error("peer reveal does not match its commitment")]
    CommitmentMismatch,
    #[error("turn ordering violation: expected turn {expected}, got {got}")]
    TurnOrder { expected: u32, got: u32 },
    #[error("peer transcript diverged at turn {0}")]
    TranscriptDiverged(u32),
    #[error("engine error: {0}")]
    Engine(#[from] crusade_core::engine::BattleError),
    #[error("serialization: {0}")]
    Serde(String),
    #[error("protocol: {0}")]
    Protocol(String),
}

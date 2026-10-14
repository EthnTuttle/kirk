//! crusade-bridge: binds the kirk trust protocol (cashu commit-reveal +
//! nostr) to crusade-core battles.
//!
//! - [`derive_battle_seed`]: after kirk's challenge flow has both players
//!   reveal their game tokens, the revealed C values + battle id hash to the
//!   deterministic battle seed consumed by `crusade_core::engine::Battle`.
//!   Neither player can bias the seed alone (commit-reveal at the kirk layer).
//! - [`attest`]: end-of-duel transcript attestation: both players sign the
//!   final transcript digest with their nostr keys; either can present the
//!   pair of signatures + digest to a kirk mint to claim the wager escrow.
//!
//! NOT (yet): making Battle implement kirk's Game trait for mint-side replay
//! validation. That adapter is a future milestone; the unit here is enough
//! for honest players + dispute by transcript publication.

pub mod attest;
pub mod seed;

pub use attest::*;
pub use seed::*;

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BridgeError {
    #[error("nostr: {0}")]
    Nostr(String),
    #[error("json: {0}")]
    Json(String),
    #[error("signature invalid or digest mismatch")]
    BadAttestation,
    #[error("seed input error: {0}")]
    Seed(String),
}

//! crusade-data: the player's offline-first collection store.
//!
//! One automerge document per player, schema:
//!   root["champions"] : map id -> JSON string of ChampionInstance
//!   root["inventory"] : map item name -> u64 counter
//!   root["settings"]  : map key -> string
//!   root["battles"]   : map battle-hexid -> JSON summary string (append-mostly)
//!
//! Merge model: automerge maps give last-writer-wins per key, multi-value
//! preserved deterministically. Different champions merge cleanly; same
//! champion on two devices resolves deterministically (documented LWW).
//! Battle truth never lives here — only post-game summaries.

mod roster;

pub use roster::RosterStore;

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StoreError {
    #[error("automerge: {0}")]
    Automerge(String),
    #[error("json: {0}")]
    Json(String),
    #[error("unknown champion id {0}")]
    UnknownChampion(String),
    #[error("invalid id")]
    BadId,
}

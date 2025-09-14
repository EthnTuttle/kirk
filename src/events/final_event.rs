//! Final event types for game completion

use serde::{Deserialize, Serialize};
use nostr::{Event, EventBuilder, EventId, Keys};
use crate::error::GameProtocolError;
use super::FINAL_KIND;

/// Method used for multi-token commitments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommitmentMethod {
    /// Simple concatenation of token hashes in ascending order
    Concatenation,
    /// Merkle tree with radix 4, tokens ordered ascending by token hash
    MerkleTreeRadix4,
}

/// Content structure for Final events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalContent {
    pub game_sequence_root: EventId,
    pub commitment_method: Option<CommitmentMethod>,
    pub final_state: serde_json::Value,
}

impl FinalContent {
    /// Create Final event using rust-nostr's EventBuilder
    pub fn to_event(&self, keys: &Keys) -> Result<Event, GameProtocolError> {
        let content = serde_json::to_string(self)?;
        EventBuilder::new(FINAL_KIND, content, Vec::<nostr::Tag>::new())
            .to_event(keys)
            .map_err(|e| GameProtocolError::Nostr(e.to_string()))
    }
}
//! Move event types for game actions

use serde::{Deserialize, Serialize};
use nostr::{Event, EventBuilder, EventId, Keys};
use cdk::nuts::Token as CashuToken;
use crate::error::GameProtocolError;
use super::MOVE_KIND;

/// Type of move being made
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MoveType {
    Move,
    Commit,
    Reveal,
}

/// Content structure for Move events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveContent {
    pub previous_event_id: EventId,
    pub move_type: MoveType,
    pub move_data: serde_json::Value,
    pub revealed_tokens: Option<Vec<CashuToken>>,
}

impl MoveContent {
    /// Create Move event using rust-nostr's EventBuilder
    pub fn to_event(&self, keys: &Keys) -> Result<Event, GameProtocolError> {
        let content = serde_json::to_string(self)?;
        EventBuilder::new(MOVE_KIND, content, Vec::<nostr::Tag>::new())
            .to_event(keys)
            .map_err(|e| GameProtocolError::Nostr(e.to_string()))
    }
}
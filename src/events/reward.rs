//! Reward and validation failure event types

use serde::{Deserialize, Serialize};
use nostr::{Event, EventBuilder, EventId, Keys, PublicKey};
use crate::cashu::GameToken;
use crate::error::GameProtocolError;
use super::REWARD_KIND;

/// Content structure for Reward events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardContent {
    pub game_sequence_root: EventId,
    pub winner_pubkey: PublicKey,
    pub reward_tokens: Vec<GameToken>,
    pub unlock_instructions: Option<String>,
}

impl RewardContent {
    /// Create Reward event using rust-nostr's EventBuilder
    pub fn to_event(&self, keys: &Keys) -> Result<Event, GameProtocolError> {
        let content = serde_json::to_string(self)?;
        EventBuilder::new(REWARD_KIND, content, Vec::<nostr::Tag>::new())
            .to_event(keys)
            .map_err(|e| GameProtocolError::Nostr(e.to_string()))
    }
}

/// Content structure for ValidationFailure events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationFailureContent {
    pub game_sequence_root: EventId,
    pub failure_reason: String,
    pub failed_event_id: Option<EventId>,
}

impl ValidationFailureContent {
    /// Create ValidationFailure event using rust-nostr's EventBuilder
    pub fn to_event(&self, keys: &Keys) -> Result<Event, GameProtocolError> {
        let content = serde_json::to_string(self)?;
        EventBuilder::new(REWARD_KIND, content, Vec::<nostr::Tag>::new())
            .to_event(keys)
            .map_err(|e| GameProtocolError::Nostr(e.to_string()))
    }
}
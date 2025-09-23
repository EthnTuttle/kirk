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
    pub game_sequence: Vec<EventId>,
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
            .map_err(GameProtocolError::from)
    }
    
    /// Validate the reward content
    pub fn validate(&self) -> Result<(), GameProtocolError> {
        if self.reward_tokens.is_empty() {
            return Err(GameProtocolError::GameValidation(
                "Reward events must contain at least one reward token".to_string()
            ));
        }
        
        // Validate that all reward tokens are actually reward type
        for token in &self.reward_tokens {
            if !matches!(token.game_type, crate::cashu::GameTokenType::Reward { .. }) {
                return Err(GameProtocolError::InvalidToken(
                    "All tokens in reward event must be Reward type".to_string()
                ));
            }
        }
        
        Ok(())
    }
}

/// Content structure for ValidationFailure events
// TODO: move out to another file
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
            .map_err(GameProtocolError::from)
    }
    
    /// Validate the validation failure content
    pub fn validate(&self) -> Result<(), GameProtocolError> {
        if self.failure_reason.is_empty() {
            return Err(GameProtocolError::GameValidation(
                "Validation failure reason cannot be empty".to_string()
            ));
        }
        
        Ok(())
    }
}
//! Move event types for game actions

use serde::{Deserialize, Serialize};
use nostr::{Event, EventBuilder, EventId, Keys};
use cdk::nuts::Token as CashuToken;
use crate::error::GameProtocolError;
use super::MOVE_KIND;

/// Type of move being made
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MoveType {
    Move,
    Commit,
    Reveal,
}

impl std::fmt::Display for MoveType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MoveType::Move => write!(f, "Move"),
            MoveType::Commit => write!(f, "Commit"),
            MoveType::Reveal => write!(f, "Reveal"),
        }
    }
}

/// Content structure for Move events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveContent {
    pub previous_event_id: EventId,
    pub move_type: MoveType,
    pub move_data: serde_json::Value,
    pub revealed_tokens: Option<Vec<CashuToken>>,
    /// Optional deadline for this move (Unix timestamp)
    pub deadline: Option<u64>,
}

impl MoveContent {
    /// Create Move event using rust-nostr's EventBuilder
    pub fn to_event(&self, keys: &Keys) -> Result<Event, GameProtocolError> {
        let content = serde_json::to_string(self)?;
        EventBuilder::new(MOVE_KIND, content, Vec::<nostr::Tag>::new())
            .to_event(keys)
            .map_err(GameProtocolError::from)
    }
    
    /// Validate the move content
    pub fn validate(&self) -> Result<(), GameProtocolError> {
        // Validate move type consistency
        match self.move_type {
            MoveType::Reveal => {
                if self.revealed_tokens.is_none() {
                    return Err(GameProtocolError::InvalidMove(
                        "Reveal moves must include revealed tokens".to_string()
                    ));
                }
                // TODO: reference move_data and handle it
            },
            MoveType::Commit => {
                if self.revealed_tokens.is_some() {
                    return Err(GameProtocolError::InvalidMove(
                        "Commit moves should not include revealed tokens".to_string()
                    ));
                }
                // TODO: reference move_data and handle it
            },
            MoveType::Move => {
                // Regular moves may or may not have revealed tokens
                // TODO: need a generic trait for Move validation, probably referencing move_data
            }
        }
        
        // Validate that revealed tokens, if present, are not empty
        if let Some(ref tokens) = self.revealed_tokens {
            if tokens.is_empty() {
                return Err(GameProtocolError::InvalidMove(
                    "If revealed_tokens is present, it cannot be empty".to_string()
                ));
            }
        }
        
        // Validate deadline if present
        if let Some(deadline) = self.deadline {
            let now = chrono::Utc::now().timestamp() as u64;
            if deadline <= now {
                return Err(GameProtocolError::Timeout {
                    message: "Move deadline has already passed".to_string(),
                    duration_ms: 0, // Deadline already passed
                    operation: "move_deadline_validation".to_string(),
                });
            }
        }
        
        Ok(())
    }
}
//! Nostr event types and handling for Kirk gaming protocol

pub mod challenge;
pub mod move_event;
pub mod final_event;
pub mod reward;

#[cfg(test)]
mod tests;

// Re-export all event content types
pub use challenge::{ChallengeContent, ChallengeAcceptContent};
pub use move_event::{MoveContent, MoveType};
pub use final_event::{FinalContent, CommitmentMethod};
pub use reward::{RewardContent, ValidationFailureContent};

// Event kind constants - using contiguous unused kind numbers
use nostr::{Kind, Event};
use crate::error::GameProtocolError;

pub const CHALLENGE_KIND: Kind = Kind::Custom(9259);
pub const CHALLENGE_ACCEPT_KIND: Kind = Kind::Custom(9260);
pub const MOVE_KIND: Kind = Kind::Custom(9261);
pub const FINAL_KIND: Kind = Kind::Custom(9262);
pub const REWARD_KIND: Kind = Kind::Custom(9263);

/// Event validation and parsing utilities
pub struct EventParser;

impl EventParser {
    /// Parse a Challenge event from a nostr Event
    pub fn parse_challenge(event: &Event) -> Result<ChallengeContent, GameProtocolError> {
        if event.kind != CHALLENGE_KIND {
            return Err(GameProtocolError::GameValidation(
                format!("Expected Challenge event kind {}, got {}", CHALLENGE_KIND, event.kind)
            ));
        }
        
        serde_json::from_str(&event.content)
            .map_err(|e| GameProtocolError::Serialization(e))
    }
    
    /// Parse a ChallengeAccept event from a nostr Event
    pub fn parse_challenge_accept(event: &Event) -> Result<ChallengeAcceptContent, GameProtocolError> {
        if event.kind != CHALLENGE_ACCEPT_KIND {
            return Err(GameProtocolError::GameValidation(
                format!("Expected ChallengeAccept event kind {}, got {}", CHALLENGE_ACCEPT_KIND, event.kind)
            ));
        }
        
        serde_json::from_str(&event.content)
            .map_err(|e| GameProtocolError::Serialization(e))
    }
    
    /// Parse a Move event from a nostr Event
    pub fn parse_move(event: &Event) -> Result<MoveContent, GameProtocolError> {
        if event.kind != MOVE_KIND {
            return Err(GameProtocolError::GameValidation(
                format!("Expected Move event kind {}, got {}", MOVE_KIND, event.kind)
            ));
        }
        
        serde_json::from_str(&event.content)
            .map_err(|e| GameProtocolError::Serialization(e))
    }
    
    /// Parse a Final event from a nostr Event
    pub fn parse_final(event: &Event) -> Result<FinalContent, GameProtocolError> {
        if event.kind != FINAL_KIND {
            return Err(GameProtocolError::GameValidation(
                format!("Expected Final event kind {}, got {}", FINAL_KIND, event.kind)
            ));
        }
        
        serde_json::from_str(&event.content)
            .map_err(|e| GameProtocolError::Serialization(e))
    }
    
    /// Parse a Reward event from a nostr Event
    pub fn parse_reward(event: &Event) -> Result<RewardContent, GameProtocolError> {
        if event.kind != REWARD_KIND {
            return Err(GameProtocolError::GameValidation(
                format!("Expected Reward event kind {}, got {}", REWARD_KIND, event.kind)
            ));
        }
        
        serde_json::from_str(&event.content)
            .map_err(|e| GameProtocolError::Serialization(e))
    }
    
    /// Parse a ValidationFailure event from a nostr Event
    pub fn parse_validation_failure(event: &Event) -> Result<ValidationFailureContent, GameProtocolError> {
        if event.kind != REWARD_KIND {
            return Err(GameProtocolError::GameValidation(
                format!("Expected ValidationFailure event kind {}, got {}", REWARD_KIND, event.kind)
            ));
        }
        
        serde_json::from_str(&event.content)
            .map_err(|e| GameProtocolError::Serialization(e))
    }
    
    /// Validate that an event is a game-related event
    pub fn is_game_event(event: &Event) -> bool {
        matches!(event.kind, 
            k if k == CHALLENGE_KIND || 
                 k == CHALLENGE_ACCEPT_KIND || 
                 k == MOVE_KIND || 
                 k == FINAL_KIND || 
                 k == REWARD_KIND
        )
    }
    
    /// Get the game event type name for an event
    pub fn get_event_type_name(event: &Event) -> Option<&'static str> {
        match event.kind {
            k if k == CHALLENGE_KIND => Some("Challenge"),
            k if k == CHALLENGE_ACCEPT_KIND => Some("ChallengeAccept"),
            k if k == MOVE_KIND => Some("Move"),
            k if k == FINAL_KIND => Some("Final"),
            k if k == REWARD_KIND => Some("Reward"),
            _ => None,
        }
    }
}

/// Validate event content structure without parsing
pub fn validate_event_structure(event: &Event) -> Result<(), GameProtocolError> {
    // Check if it's a game event
    if !EventParser::is_game_event(event) {
        return Err(GameProtocolError::GameValidation(
            format!("Event kind {} is not a game event", event.kind)
        ));
    }
    
    // Validate JSON structure
    let _: serde_json::Value = serde_json::from_str(&event.content)
        .map_err(|e| GameProtocolError::Serialization(e))?;
    
    // Validate event signature
    if !event.verify().is_ok() {
        return Err(GameProtocolError::GameValidation(
            "Event signature verification failed".to_string()
        ));
    }
    
    Ok(())
}
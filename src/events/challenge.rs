//! Challenge and ChallengeAccept event types

use serde::{Deserialize, Serialize};
use nostr::{Event, EventBuilder, EventId, Keys};
use crate::error::GameProtocolError;
use super::{CHALLENGE_KIND, CHALLENGE_ACCEPT_KIND};

/// Content structure for Challenge events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeContent {
    pub game_type: String,
    pub commitment_hashes: Vec<String>,
    pub game_parameters: serde_json::Value,
    pub expiry: Option<u64>,
}

impl ChallengeContent {
    /// Create Challenge event using rust-nostr's EventBuilder
    pub fn to_event(&self, keys: &Keys) -> Result<Event, GameProtocolError> {
        let content = serde_json::to_string(self)?;
        EventBuilder::new(CHALLENGE_KIND, content, Vec::<nostr::Tag>::new())
            .to_event(keys)
            .map_err(GameProtocolError::from)
    }
    
    /// Validate the challenge content
    pub fn validate(&self) -> Result<(), GameProtocolError> {
        if self.game_type.is_empty() {
            return Err(GameProtocolError::GameValidation(
                "Game type cannot be empty".to_string()
            ));
        }
        
        if self.commitment_hashes.is_empty() {
            return Err(GameProtocolError::GameValidation(
                "At least one commitment hash is required".to_string()
            ));
        }
        
        // Validate commitment hash format (should be hex strings)
        for hash in &self.commitment_hashes {
            if hash.len() != 64 {
                return Err(GameProtocolError::InvalidCommitment(
                    format!("Commitment hash must be 64 characters (32 bytes hex), got {}", hash.len())
                ));
            }
            
            hex::decode(hash).map_err(|_| {
                GameProtocolError::InvalidCommitment(
                    format!("Invalid hex format in commitment hash: {}", hash)
                )
            })?;
        }
        
        // Validate expiry if present
        if let Some(expiry) = self.expiry {
            let now = chrono::Utc::now().timestamp() as u64;
            if expiry <= now {
                return Err(GameProtocolError::InvalidExpiry);
            }
        }
        
        Ok(())
    }
}

/// Content structure for ChallengeAccept events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeAcceptContent {
    pub challenge_id: EventId,
    pub commitment_hashes: Vec<String>,
}

impl ChallengeAcceptContent {
    /// Create ChallengeAccept event using rust-nostr's EventBuilder
    pub fn to_event(&self, keys: &Keys) -> Result<Event, GameProtocolError> {
        let content = serde_json::to_string(self)?;
        EventBuilder::new(CHALLENGE_ACCEPT_KIND, content, Vec::<nostr::Tag>::new())
            .to_event(keys)
            .map_err(GameProtocolError::from)
    }
    
    /// Validate the challenge accept content
    pub fn validate(&self) -> Result<(), GameProtocolError> {
        if self.commitment_hashes.is_empty() {
            return Err(GameProtocolError::GameValidation(
                "At least one commitment hash is required".to_string()
            ));
        }
        
        // Validate commitment hash format (should be hex strings)
        for hash in &self.commitment_hashes {
            if hash.len() != 64 {
                return Err(GameProtocolError::InvalidCommitment(
                    format!("Commitment hash must be 64 characters (32 bytes hex), got {}", hash.len())
                ));
            }
            
            hex::decode(hash).map_err(|_| {
                GameProtocolError::InvalidCommitment(
                    format!("Invalid hex format in commitment hash: {}", hash)
                )
            })?;
        }
        
        Ok(())
    }
}
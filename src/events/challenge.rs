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
    /// Optional timeout configuration for the game
    pub timeout_config: Option<TimeoutConfig>,
}

/// Timeout configuration for game sequences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutConfig {
    /// Maximum time to wait for challenge acceptance (seconds)
    pub accept_timeout: Option<u64>,
    /// Maximum time between moves during gameplay (seconds)
    pub move_timeout: Option<u64>,
    /// Maximum time to wait for commit/reveal sequences (seconds)
    pub commit_reveal_timeout: Option<u64>,
    /// Maximum time to wait for final events after game completion (seconds)
    pub final_event_timeout: Option<u64>,
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
        
        // Validate timeout configuration if present
        if let Some(ref timeout_config) = self.timeout_config {
            timeout_config.validate()?;
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

impl TimeoutConfig {
    /// Create a new timeout configuration with default values
    pub fn new() -> Self {
        Self {
            accept_timeout: Some(3600),      // 1 hour
            move_timeout: Some(1800),        // 30 minutes
            commit_reveal_timeout: Some(600), // 10 minutes
            final_event_timeout: Some(3600), // 1 hour
        }
    }
    
    /// Create a timeout configuration with custom values
    pub fn custom(
        accept_timeout: Option<u64>,
        move_timeout: Option<u64>,
        commit_reveal_timeout: Option<u64>,
        final_event_timeout: Option<u64>,
    ) -> Self {
        Self {
            accept_timeout,
            move_timeout,
            commit_reveal_timeout,
            final_event_timeout,
        }
    }
    
    /// Validate timeout configuration
    pub fn validate(&self) -> Result<(), GameProtocolError> {
        // Validate that timeouts are reasonable (not too short or too long)
        let min_timeout = 60; // 1 minute minimum
        let max_timeout = 86400; // 24 hours maximum
        
        if let Some(timeout) = self.accept_timeout {
            if timeout < min_timeout || timeout > max_timeout {
                return Err(GameProtocolError::Timeout {
                    message: format!("Accept timeout must be between {} and {} seconds", min_timeout, max_timeout),
                    duration_ms: timeout * 1000,
                    operation: "accept_timeout_validation".to_string(),
                });
            }
        }
        
        if let Some(timeout) = self.move_timeout {
            if timeout < min_timeout || timeout > max_timeout {
                return Err(GameProtocolError::Timeout {
                    message: format!("Move timeout must be between {} and {} seconds", min_timeout, max_timeout),
                    duration_ms: timeout * 1000,
                    operation: "move_timeout_validation".to_string(),
                });
            }
        }
        
        if let Some(timeout) = self.commit_reveal_timeout {
            if timeout < min_timeout || timeout > max_timeout {
                return Err(GameProtocolError::Timeout {
                    message: format!("Commit/reveal timeout must be between {} and {} seconds", min_timeout, max_timeout),
                    duration_ms: timeout * 1000,
                    operation: "commit_reveal_timeout_validation".to_string(),
                });
            }
        }
        
        if let Some(timeout) = self.final_event_timeout {
            if timeout < min_timeout || timeout > max_timeout {
                return Err(GameProtocolError::Timeout {
                    message: format!("Final event timeout must be between {} and {} seconds", min_timeout, max_timeout),
                    duration_ms: timeout * 1000,
                    operation: "final_event_timeout_validation".to_string(),
                });
            }
        }
        
        Ok(())
    }
    
    /// Get the timeout for a specific phase
    pub fn get_timeout_for_phase(&self, phase: TimeoutPhase) -> Option<u64> {
        match phase {
            TimeoutPhase::Accept => self.accept_timeout,
            TimeoutPhase::Move => self.move_timeout,
            TimeoutPhase::CommitReveal => self.commit_reveal_timeout,
            TimeoutPhase::FinalEvent => self.final_event_timeout,
        }
    }
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Different phases of gameplay that can have timeouts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimeoutPhase {
    Accept,
    Move,
    CommitReveal,
    FinalEvent,
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
//! Core game traits for flexible game implementations

use std::fmt::Debug;
use serde::{Serialize, de::DeserializeOwned};
use nostr::{Event as NostrEvent, PublicKey};
use cdk::nuts::Token as CashuToken;
use crate::error::{GameProtocolError, ValidationResult, ValidationError};
use crate::events::{CommitmentMethod, TimeoutConfig, TimeoutPhase};

/// Core trait that all games must implement
pub trait Game: Send + Sync {
    type GamePiece: Clone + Debug;
    type GameState: Clone + Debug;
    type MoveData: Serialize + DeserializeOwned;
    
    /// Decode C value (32 bytes) into game pieces
    fn decode_c_value(&self, c_value: &[u8; 32]) -> Result<Vec<Self::GamePiece>, GameProtocolError>;
    
    /// Validate a sequence of moves
    fn validate_sequence(&self, events: &[NostrEvent]) -> Result<ValidationResult, GameProtocolError>;
    
    /// Determine if game sequence is complete
    fn is_sequence_complete(&self, events: &[NostrEvent]) -> Result<bool, GameProtocolError>;
    
    /// Determine winner from completed sequence
    fn determine_winner(&self, events: &[NostrEvent]) -> Result<Option<PublicKey>, GameProtocolError>;
    
    /// Get required Final event count (1 or 2 players)
    fn required_final_events(&self) -> usize;
    
    /// Check if a timeout should result in forfeiture for this game
    fn should_timeout_forfeit(&self, _phase: TimeoutPhase, overdue_duration: u64) -> bool {
        // Default implementation: forfeit after 5 minutes grace period
        overdue_duration > 300
    }
    
    /// Get default timeout configuration for this game type
    fn default_timeout_config(&self) -> Option<TimeoutConfig> {
        Some(TimeoutConfig::default())
    }
    
    /// Validate that a move deadline is reasonable for this game
    fn validate_move_deadline(&self, deadline: u64, _move_type: &str) -> Result<(), GameProtocolError> {
        let now = chrono::Utc::now().timestamp() as u64;
        let max_future = now + 86400; // 24 hours maximum
        let min_future = now + 60;    // 1 minute minimum
        
        if deadline < min_future {
            return Err(GameProtocolError::Timeout {
                message: "Move deadline too soon: must be at least 1 minute in the future".to_string(),
                duration_ms: (min_future - deadline) * 1000,
                operation: "move_deadline_validation".to_string(),
            });
        }
        
        if deadline > max_future {
            return Err(GameProtocolError::Timeout {
                message: "Move deadline too far: must be within 24 hours".to_string(),
                duration_ms: (deadline - max_future) * 1000,
                operation: "move_deadline_validation".to_string(),
            });
        }
        
        Ok(())
    }
}

/// Trait for validating hash commitments against revealed tokens
pub trait CommitmentValidator {
    /// Validate single token commitment
    fn validate_single_commitment(
        &self, 
        commitment_hash: &str, 
        revealed_token: &CashuToken
    ) -> Result<bool, ValidationError>;
    
    /// Validate multi-token commitment using provided method
    fn validate_multi_commitment(
        &self,
        commitment_hash: &str,
        revealed_tokens: &[CashuToken],
        method: &CommitmentMethod
    ) -> Result<bool, ValidationError>;
}
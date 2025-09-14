//! Core game traits for flexible game implementations

use std::fmt::Debug;
use serde::{Serialize, de::DeserializeOwned};
use nostr::{Event as NostrEvent, PublicKey};
use cdk::nuts::Token as CashuToken;
use crate::error::{GameProtocolError, ValidationResult, ValidationError};
use crate::events::CommitmentMethod;

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
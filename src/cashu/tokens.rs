//! Game token wrapper and utilities

use serde::{Deserialize, Serialize};
use nostr::PublicKey;
use cdk::nuts::Token as CashuToken;

/// Type of game token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameTokenType {
    Game,
    Reward { p2pk_locked: Option<PublicKey> }, // Uses NUT-11 P2PK locking
}

/// Thin wrapper around CDK's Token to add game context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameToken {
    pub inner: CashuToken, // Reuse CDK's Token directly
    pub game_type: GameTokenType,
}

impl GameToken {
    /// Create from existing CDK token
    pub fn from_cdk_token(token: CashuToken, game_type: GameTokenType) -> Self {
        Self {
            inner: token,
            game_type,
        }
    }
    
    /// Get underlying CDK token for operations
    pub fn as_cdk_token(&self) -> &CashuToken {
        &self.inner
    }
    
    /// Extract C values from token proofs for game piece generation
    /// This is a placeholder - actual CDK integration will be implemented in later tasks
    pub fn extract_c_values(&self) -> Vec<[u8; 32]> {
        // Placeholder implementation - actual CDK API integration will be in task 7
        vec![[0u8; 32]] // Return dummy C value for now
    }
    
    /// Check if this is a P2PK locked reward token
    pub fn is_p2pk_locked(&self) -> bool {
        matches!(self.game_type, GameTokenType::Reward { p2pk_locked: Some(_) })
    }
}

/// State of reward tokens using NUT-11 P2PK
#[derive(Debug, Clone)]
pub enum RewardTokenState {
    P2PKLocked { to_pubkey: PublicKey }, // NUT-11 Pay-to-Public-Key
    Unlocked, // Standard Cashu tokens
}

impl RewardTokenState {
    /// Check if P2PK token can be spent by specific pubkey
    pub fn can_spend(&self, pubkey: &PublicKey) -> bool {
        match self {
            RewardTokenState::P2PKLocked { to_pubkey } => to_pubkey == pubkey,
            RewardTokenState::Unlocked => true,
        }
    }
    
    /// Create P2PK locked token using NUT-11
    pub fn create_p2pk_locked(pubkey: PublicKey) -> Self {
        RewardTokenState::P2PKLocked { to_pubkey: pubkey }
    }
}
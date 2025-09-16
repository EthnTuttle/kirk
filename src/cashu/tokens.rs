//! Game token wrapper and utilities

use serde::{Deserialize, Serialize};
use nostr::PublicKey;
use cashu::nuts::Token as CashuToken;
use cashu::{KeySetInfo, Amount};
use crate::error::{GameProtocolError, GameResult};

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
    /// Uses the actual unblinded signatures (C values) from CDK proofs
    pub fn extract_c_values(&self, mint_keysets: &[KeySetInfo]) -> GameResult<Vec<[u8; 33]>> {
        self.inner.extract_c_values(mint_keysets)
            .map_err(|e| GameProtocolError::Cdk(e.to_string()))
    }
    
    /// Get the actual amount for this token
    pub fn amount(&self) -> GameResult<Amount> {
        self.inner.value()
            .map_err(|e| GameProtocolError::Cdk(e.to_string()))
    }
    
    /// Get the number of proofs in this token
    pub fn proof_count(&self, mint_keysets: &[KeySetInfo]) -> GameResult<usize> {
        let proofs = self.inner.proofs(mint_keysets)
            .map_err(|e| GameProtocolError::Cdk(e.to_string()))?;
        Ok(proofs.len())
    }
    
    /// Check if this token has P2PK witnesses using actual CDK inspection
    pub fn has_p2pk_witness(&self, mint_keysets: &[KeySetInfo]) -> GameResult<bool> {
        self.inner.has_p2pk_witnesses(mint_keysets)
            .map_err(|e| GameProtocolError::Cdk(e.to_string()))
    }
    
    /// Extract P2PK public keys from actual CDK proofs
    pub fn extract_p2pk_pubkeys(&self, mint_keysets: &[KeySetInfo]) -> GameResult<Vec<Vec<u8>>> {
        self.inner.extract_p2pk_pubkeys(mint_keysets)
            .map_err(|e| GameProtocolError::Cdk(e.to_string()))
    }
    
    /// Get the P2PK public key from the token type if it's P2PK locked
    pub fn declared_p2pk_pubkey(&self) -> Option<PublicKey> {
        match &self.game_type {
            GameTokenType::Reward { p2pk_locked } => *p2pk_locked,
            GameTokenType::Game => None,
        }
    }
    
    /// Validate that this token's structure is consistent with its declared type
    pub fn validate_token_type(&self, mint_keysets: &[KeySetInfo]) -> GameResult<bool> {
        match &self.game_type {
            GameTokenType::Game => {
                // Game tokens should not have P2PK witnesses
                let has_p2pk = self.has_p2pk_witness(mint_keysets)?;
                if has_p2pk {
                    return Err(GameProtocolError::InvalidToken(
                        "Game token should not have P2PK witness".to_string()
                    ));
                }
                Ok(true)
            }
            GameTokenType::Reward { p2pk_locked } => {
                let has_p2pk = self.has_p2pk_witness(mint_keysets)?;
                match p2pk_locked {
                    Some(_expected_pubkey) => {
                        // P2PK locked reward token should have P2PK witness
                        if !has_p2pk {
                            return Err(GameProtocolError::InvalidToken(
                                "P2PK locked reward token missing P2PK witness".to_string()
                            ));
                        }
                        // TODO: Validate that the actual P2PK witness matches expected pubkey
                        // This would require parsing the P2PK witness structure
                        Ok(true)
                    }
                    None => {
                        // Unlocked reward token should not have P2PK witness
                        if has_p2pk {
                            return Err(GameProtocolError::InvalidToken(
                                "Unlocked reward token should not have P2PK witness".to_string()
                            ));
                        }
                        Ok(true)
                    }
                }
            }
        }
    }
    
    /// Generate a deterministic hash for this token for commitment purposes
    pub fn token_hash(&self) -> GameResult<[u8; 32]> {
        // Use the token's string representation for consistent hashing
        let token_str = self.inner.to_string();
        
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(token_str.as_bytes());
        hasher.update(b"TOKEN_HASH"); // Domain separator
        Ok(hasher.finalize().into())
    }
    
    /// Check if this is a P2PK locked reward token
    pub fn is_p2pk_locked(&self) -> bool {
        matches!(self.game_type, GameTokenType::Reward { p2pk_locked: Some(_) })
    }
}

/// State of reward tokens using NUT-11 P2PK
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    
    /// Create unlocked token state
    pub fn create_unlocked() -> Self {
        RewardTokenState::Unlocked
    }
    
    /// Get the locked pubkey if this is a P2PK locked token
    pub fn locked_pubkey(&self) -> Option<&PublicKey> {
        match self {
            RewardTokenState::P2PKLocked { to_pubkey } => Some(to_pubkey),
            RewardTokenState::Unlocked => None,
        }
    }
    
    /// Check if this token state is locked
    pub fn is_locked(&self) -> bool {
        matches!(self, RewardTokenState::P2PKLocked { .. })
    }
}

/// Utility functions for token operations
impl GameToken {
    /// Create a new Game token
    pub fn new_game_token(token: CashuToken) -> Self {
        Self::from_cdk_token(token, GameTokenType::Game)
    }
    
    /// Create a new unlocked Reward token
    pub fn new_reward_token(token: CashuToken) -> Self {
        Self::from_cdk_token(token, GameTokenType::Reward { p2pk_locked: None })
    }
    
    /// Create a new P2PK locked Reward token
    pub fn new_p2pk_reward_token(token: CashuToken, pubkey: PublicKey) -> Self {
        Self::from_cdk_token(token, GameTokenType::Reward { p2pk_locked: Some(pubkey) })
    }
    
    /// Check if this is a Game token
    pub fn is_game_token(&self) -> bool {
        matches!(self.game_type, GameTokenType::Game)
    }
    
    /// Check if this is a Reward token (locked or unlocked)
    pub fn is_reward_token(&self) -> bool {
        matches!(self.game_type, GameTokenType::Reward { .. })
    }
    
    /// Get the reward token state if this is a reward token
    pub fn reward_state(&self) -> Option<RewardTokenState> {
        match &self.game_type {
            GameTokenType::Reward { p2pk_locked } => {
                Some(match p2pk_locked {
                    Some(pubkey) => RewardTokenState::P2PKLocked { to_pubkey: *pubkey },
                    None => RewardTokenState::Unlocked,
                })
            }
            GameTokenType::Game => None,
        }
    }
    
    /// Convert this token to a standard CDK token (consuming self)
    pub fn into_cdk_token(self) -> CashuToken {
        self.inner
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    /// Create a test public key for testing
    fn create_test_pubkey(seed: u8) -> PublicKey {
        // Create a deterministic public key for testing
        let mut key_bytes = [0u8; 32];
        // Fill with a pattern based on seed to ensure uniqueness
        for i in 0..32 {
            key_bytes[i] = seed.wrapping_add(i as u8);
        }
        // Ensure it's a valid key by setting the first byte to 1 if it's 0
        if key_bytes[0] == 0 {
            key_bytes[0] = 1;
        }
        
        PublicKey::from_slice(&key_bytes)
            .unwrap_or_else(|_| {
                // If that doesn't work, use a known valid key format with seed
                let hex_str = format!("{:02x}{:062x}", seed, seed as u64);
                PublicKey::from_str(&hex_str)
                    .unwrap_or_else(|_| {
                        PublicKey::from_str("0000000000000000000000000000000000000000000000000000000000000001")
                            .expect("Failed to create test pubkey")
                    })
            })
    }

    #[test]
    fn test_game_token_type_serialization() {
        let pubkey = create_test_pubkey(10);
        
        // Test Game type serialization
        let game_type = GameTokenType::Game;
        let serialized = serde_json::to_string(&game_type).expect("Should serialize Game type");
        let deserialized: GameTokenType = serde_json::from_str(&serialized).expect("Should deserialize Game type");
        assert!(matches!(deserialized, GameTokenType::Game));
        
        // Test unlocked Reward type serialization
        let reward_type = GameTokenType::Reward { p2pk_locked: None };
        let serialized = serde_json::to_string(&reward_type).expect("Should serialize unlocked Reward type");
        let deserialized: GameTokenType = serde_json::from_str(&serialized).expect("Should deserialize unlocked Reward type");
        assert!(matches!(deserialized, GameTokenType::Reward { p2pk_locked: None }));
        
        // Test P2PK locked Reward type serialization
        let locked_reward_type = GameTokenType::Reward { p2pk_locked: Some(pubkey) };
        let serialized = serde_json::to_string(&locked_reward_type).expect("Should serialize locked Reward type");
        let deserialized: GameTokenType = serde_json::from_str(&serialized).expect("Should deserialize locked Reward type");
        assert!(matches!(deserialized, GameTokenType::Reward { p2pk_locked: Some(pk) } if pk == pubkey));
    }

    #[test]
    fn test_reward_token_state_creation() {
        let pubkey = create_test_pubkey(5);
        
        // Test unlocked state creation
        let unlocked = RewardTokenState::create_unlocked();
        assert!(matches!(unlocked, RewardTokenState::Unlocked));
        assert!(!unlocked.is_locked());
        assert!(unlocked.can_spend(&pubkey));
        assert!(unlocked.locked_pubkey().is_none());
        
        // Test P2PK locked state creation
        let locked = RewardTokenState::create_p2pk_locked(pubkey);
        assert!(matches!(locked, RewardTokenState::P2PKLocked { .. }));
        assert!(locked.is_locked());
        assert!(locked.can_spend(&pubkey));
        assert_eq!(locked.locked_pubkey(), Some(&pubkey));
        
        // Test that locked state can't be spent by different pubkey
        let other_pubkey = create_test_pubkey(6);
        if other_pubkey != pubkey {
            assert!(!locked.can_spend(&other_pubkey));
        }
    }

    #[test]
    fn test_reward_token_state_serialization() {
        let pubkey = create_test_pubkey(11);
        
        // Test unlocked state serialization
        let unlocked = RewardTokenState::Unlocked;
        let serialized = serde_json::to_string(&unlocked).expect("Should serialize unlocked state");
        let deserialized: RewardTokenState = serde_json::from_str(&serialized).expect("Should deserialize unlocked state");
        assert!(matches!(deserialized, RewardTokenState::Unlocked));
        
        // Test P2PK locked state serialization
        let locked = RewardTokenState::P2PKLocked { to_pubkey: pubkey };
        let serialized = serde_json::to_string(&locked).expect("Should serialize locked state");
        let deserialized: RewardTokenState = serde_json::from_str(&serialized).expect("Should deserialize locked state");
        assert!(matches!(deserialized, RewardTokenState::P2PKLocked { to_pubkey } if to_pubkey == pubkey));
    }
}
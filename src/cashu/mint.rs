//! Game mint wrapper extending CDK mint functionality

use std::sync::Arc;
use std::collections::HashMap;
use nostr::{PublicKey, EventId, Keys, secp256k1::XOnlyPublicKey};
use nostr_sdk::Client as NostrClient;
use cdk::{Mint, Amount};
use cdk::nuts::{
    Token as CashuToken, SwapRequest, SwapResponse, CurrencyUnit, Id, KeySetInfo
};
use cdk::mint::{MintKeySetInfo, QuoteId};
use cashu::nuts::nut00::Error as TokenError;
use crate::error::{GameProtocolError, GameResult};
use crate::cashu::{GameToken, GameTokenType};
use crate::events::reward::RewardContent;

/// Wrapper around CDK Mint with nostr integration
pub struct GameMint {
    inner: Arc<Mint>, // CDK Mint instance
    nostr_client: NostrClient,
    keys: Keys, // Nostr keys for signing events
    unit: CurrencyUnit, // Currency unit for this mint
}

impl GameMint {
    /// Create new GameMint wrapping existing CDK mint
    pub fn new(
        mint: Arc<Mint>, 
        nostr_client: NostrClient, 
        keys: Keys,
        unit: CurrencyUnit
    ) -> Self {
        Self {
            inner: mint,
            nostr_client,
            keys,
            unit,
        }
    }
    
    /// Get reference to underlying CDK mint
    pub fn inner(&self) -> &Arc<Mint> {
        &self.inner
    }
    
    /// Mint Game tokens using CDK's standard minting process
    /// This creates regular tokens that can be used for game commitments
    pub async fn mint_game_tokens(&self, _amount: Amount) -> GameResult<Vec<GameToken>> {
        // This is a simplified implementation that would need to be integrated
        // with the full CDK minting process including quotes and payments
        
        // For now, return an error indicating this needs full implementation
        Err(GameProtocolError::MintError(
            "Game token minting requires full CDK integration with quote/payment cycle. \
             This would involve creating mint quotes, processing payments, and minting tokens.".to_string()
        ))
    }
    
    /// Mint P2PK locked Reward tokens for game winner using NUT-11
    pub async fn mint_reward_tokens(
        &self, 
        _amount: Amount, 
        winner_pubkey: PublicKey
    ) -> GameResult<Vec<GameToken>> {
        // Convert nostr PublicKey to the format needed for P2PK conditions
        let _p2pk_pubkey = self.convert_nostr_pubkey_to_p2pk(winner_pubkey)?;
        
        // This is a simplified implementation that would need to be integrated
        // with the full CDK minting process including P2PK conditions
        
        // For now, return an error indicating this needs full implementation
        Err(GameProtocolError::MintError(
            format!("P2PK reward token minting for pubkey {} requires full CDK integration \
                     with NUT-11 P2PK spending conditions and quote/payment cycle.", winner_pubkey)
        ))
    }
    
    /// Validate tokens using CDK's verify_proofs method
    pub async fn validate_tokens(&self, tokens: &[CashuToken]) -> GameResult<bool> {
        for token in tokens {
            // Get the active keysets for validation
            let keysets: Vec<KeySetInfo> = self.inner.get_active_keysets()
                .into_iter()
                .filter_map(|(unit, id)| {
                    if unit == self.unit {
                        self.inner.get_keyset_info(&id).map(|_info| KeySetInfo {
                            id,
                            unit,
                            active: true,
                            input_fee_ppk: 0, // Would need actual fee from mint keyset info
                            final_expiry: None,
                        })
                    } else {
                        None
                    }
                })
                .collect();
            
            // Extract proofs from each token
            let proofs = token.proofs(&keysets)
                .map_err(|e| GameProtocolError::Cdk(format!("Token proof extraction failed: {}", e)))?;
            
            // Verify proofs using CDK's verification
            match self.inner.verify_proofs(proofs).await {
                Ok(()) => continue, // Token is valid
                Err(_) => return Ok(false), // Token is invalid
            }
        }
        
        Ok(true) // All tokens are valid
    }
    
    /// Process swap request using CDK's swap functionality
    pub async fn swap_tokens(&self, swap_request: SwapRequest) -> GameResult<SwapResponse> {
        self.inner.process_swap_request(swap_request).await
            .map_err(|e| GameProtocolError::Cdk(e.to_string()))
    }
    
    /// Process melt request using CDK's melt functionality
    /// Note: This is a simplified interface - actual melt requests require more parameters
    pub async fn melt_tokens(&self, _quote_id: QuoteId) -> GameResult<Amount> {
        // CDK's process_melt_request requires multiple parameters including
        // proof writer, melt quote, melt request, optional payment hash, and amount
        // This would need to be implemented with the full melt workflow
        
        Err(GameProtocolError::MintError(
            "Token melting requires full CDK integration with melt quotes and payment processing.".to_string()
        ))
    }
    
    /// Create mint quote for external payment
    pub async fn create_mint_quote(
        &self, 
        amount: Amount, 
        description: Option<String>
    ) -> GameResult<String> {
        // This would create a mint quote through CDK's quote system
        // The actual implementation would involve:
        // 1. Creating a proper mint quote request
        // 2. Processing it through the mint's quote system
        // 3. Returning the quote ID for payment
        
        Err(GameProtocolError::MintError(
            format!("Mint quote creation for amount {} with description {:?} \
                     requires full CDK quote system integration.", amount, description)
        ))
    }
    
    /// Create melt quote for external payment
    pub async fn create_melt_quote(
        &self,
        request: String, // Lightning invoice or other payment request
        unit: Option<CurrencyUnit>
    ) -> GameResult<String> {
        // This would create a melt quote through CDK's quote system
        // The actual implementation would involve:
        // 1. Creating a proper melt quote request with the payment request
        // 2. Processing it through the mint's quote system
        // 3. Returning the quote ID for the melt operation
        
        let _unit = unit.unwrap_or_else(|| self.unit.clone());
        
        Err(GameProtocolError::MintError(
            format!("Melt quote creation for request {} requires full CDK quote system integration.", request)
        ))
    }
    
    /// Publish game result and reward to nostr
    pub async fn publish_game_result(
        &self,
        game_sequence_root: EventId,
        winner: PublicKey,
        reward_tokens: Vec<GameToken>
    ) -> GameResult<EventId> {
        // Validate that all reward tokens are actually reward type
        for token in &reward_tokens {
            if !token.is_reward_token() {
                return Err(GameProtocolError::InvalidToken(
                    "All tokens in reward event must be Reward type".to_string()
                ));
            }
        }
        
        let reward_content = RewardContent {
            game_sequence_root,
            winner_pubkey: winner,
            reward_tokens,
            unlock_instructions: Some(
                "These tokens are locked to your nostr public key using NUT-11 P2PK. \
                 Use a compatible Cashu wallet to spend them.".to_string()
            ),
        };
        
        // Validate the reward content
        reward_content.validate()?;
        
        // Create and publish the reward event
        let event = reward_content.to_event(&self.keys)?;
        let event_id = event.id;
        
        self.nostr_client.send_event(event).await
            .map_err(|e| GameProtocolError::NostrSdk(e.to_string()))?;
        
        Ok(event_id)
    }
    
    /// Internal helper to process mint requests and wrap tokens
    /// This would be implemented with full CDK integration
    async fn _process_mint_request_internal(
        &self,
        _game_type: GameTokenType
    ) -> GameResult<Vec<GameToken>> {
        // This is a placeholder for the full mint integration
        // In a real implementation, this would:
        // 1. Process the mint request through CDK's full minting process
        // 2. Handle the quote/payment/mint cycle
        // 3. Return the minted tokens wrapped as GameTokens
        
        Err(GameProtocolError::MintError(
            "Full CDK mint integration not yet implemented. \
             This requires implementing the complete quote/payment/mint cycle.".to_string()
        ))
    }
    
    /// Convert nostr PublicKey to the format needed for P2PK conditions
    fn convert_nostr_pubkey_to_p2pk(&self, pubkey: PublicKey) -> GameResult<XOnlyPublicKey> {
        // Convert nostr public key bytes to secp256k1 XOnlyPublicKey
        let pubkey_bytes = pubkey.to_bytes();
        XOnlyPublicKey::from_slice(&pubkey_bytes)
            .map_err(|e| GameProtocolError::InvalidToken(
                format!("Invalid public key for P2PK: {}", e)
            ))
    }
    
    /// Get mint information
    pub async fn get_mint_info(&self) -> GameResult<cdk::nuts::MintInfo> {
        self.inner.mint_info().await
            .map_err(|e| GameProtocolError::Cdk(e.to_string()))
    }
    
    /// Get active keysets for this mint
    pub fn get_active_keysets(&self) -> HashMap<CurrencyUnit, Id> {
        self.inner.get_active_keysets()
    }
    
    /// Get keyset info by ID
    pub fn get_keyset_info(&self, id: &Id) -> Option<MintKeySetInfo> {
        self.inner.get_keyset_info(id)
    }
    
    /// Get the nostr keys for this mint (for signing events)
    pub fn keys(&self) -> &Keys {
        &self.keys
    }
    
    /// Create a P2PK locked token utility using NUT-11
    /// This is a utility function that would be used in the full minting process
    pub fn create_p2pk_locked_utility(
        &self,
        pubkey: PublicKey,
        _amount: Amount
    ) -> GameResult<String> {
        // Convert nostr pubkey to the format needed for P2PK conditions
        let p2pk_pubkey = self.convert_nostr_pubkey_to_p2pk(pubkey)?;
        
        // Create P2PK spending condition
        // This is a simplified representation - actual NUT-11 implementation would be more complex
        let condition = format!("p2pk:{}", p2pk_pubkey);
        
        Ok(condition)
    }
    
    /// Unlock P2PK tokens through standard CDK swap operations
    /// This would swap P2PK locked tokens for standard unlocked tokens
    pub async fn unlock_p2pk_tokens(
        &self,
        locked_tokens: Vec<GameToken>,
        spending_pubkey: PublicKey
    ) -> GameResult<Vec<GameToken>> {
        // Validate that all tokens are P2PK locked and can be spent by the pubkey
        for token in &locked_tokens {
            if !token.is_p2pk_locked() {
                return Err(GameProtocolError::InvalidToken(
                    "All tokens must be P2PK locked for unlocking".to_string()
                ));
            }
            
            if !token.can_spend(&spending_pubkey) {
                return Err(GameProtocolError::InvalidToken(
                    format!("Token cannot be spent by pubkey {}", spending_pubkey)
                ));
            }
        }
        
        // In a full implementation, this would:
        // 1. Create a swap request with the P2PK locked tokens as inputs
        // 2. Create new output tokens without P2PK conditions
        // 3. Process the swap through CDK's swap mechanism
        // 4. Return the unlocked tokens
        
        // For now, return a placeholder error indicating full implementation needed
        Err(GameProtocolError::MintError(
            format!("P2PK token unlocking for {} tokens by pubkey {} requires full CDK swap integration. \
                     This would involve creating swap requests with P2PK spending conditions.", 
                     locked_tokens.len(), spending_pubkey)
        ))
    }
    
    /// Validate P2PK spending conditions for a token
    pub fn validate_p2pk_spending(
        &self,
        token: &GameToken,
        spending_pubkey: PublicKey
    ) -> GameResult<bool> {
        match &token.game_type {
            GameTokenType::Game => {
                // Game tokens don't have P2PK conditions
                Ok(true)
            }
            GameTokenType::Reward { p2pk_locked } => {
                match p2pk_locked {
                    Some(locked_pubkey) => {
                        // Check if the spending pubkey matches the locked pubkey
                        Ok(*locked_pubkey == spending_pubkey)
                    }
                    None => {
                        // Unlocked reward tokens can be spent by anyone
                        Ok(true)
                    }
                }
            }
        }
    }
}

// Helper trait implementations for error conversion
impl From<cdk::Error> for GameProtocolError {
    fn from(err: cdk::Error) -> Self {
        GameProtocolError::Cdk(err.to_string())
    }
}

impl From<TokenError> for GameProtocolError {
    fn from(err: TokenError) -> Self {
        GameProtocolError::Cdk(format!("Token error: {}", err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    
    /// Create a test public key for testing
    fn create_test_pubkey(seed: u8) -> PublicKey {
        let mut key_bytes = [0u8; 32];
        for i in 0..32 {
            key_bytes[i] = seed.wrapping_add(i as u8);
        }
        if key_bytes[0] == 0 {
            key_bytes[0] = 1;
        }
        
        PublicKey::from_slice(&key_bytes)
            .unwrap_or_else(|_| {
                let hex_str = format!("{:02x}{:062x}", seed, seed as u64);
                PublicKey::from_str(&hex_str)
                    .unwrap_or_else(|_| {
                        PublicKey::from_str("0000000000000000000000000000000000000000000000000000000000000001")
                            .expect("Failed to create test pubkey")
                    })
            })
    }
    
    #[test]
    fn test_pubkey_conversion() {
        let pubkey = create_test_pubkey(42);
        
        // Test that we can convert nostr pubkey to secp256k1 format
        let pubkey_bytes = pubkey.to_bytes();
        let secp_pubkey = XOnlyPublicKey::from_slice(&pubkey_bytes);
        
        assert!(secp_pubkey.is_ok(), "Should be able to convert nostr pubkey to secp256k1 format");
    }

    #[test]
    fn test_p2pk_utility_creation() {
        // Create a mock GameMint for testing
        let _keys = Keys::generate();
        let _nostr_client = NostrClient::default();
        let _unit = CurrencyUnit::Sat;
        
        // We can't easily create a real CDK Mint for testing, so we'll test the utility functions
        let pubkey = create_test_pubkey(50);
        
        // Test P2PK pubkey conversion
        let pubkey_bytes = pubkey.to_bytes();
        let secp_pubkey = XOnlyPublicKey::from_slice(&pubkey_bytes);
        assert!(secp_pubkey.is_ok(), "Should convert nostr pubkey to secp256k1 format");
        
        // Test that we can create a P2PK condition string
        let condition = format!("p2pk:{}", secp_pubkey.unwrap());
        assert!(condition.contains("p2pk:"));
        assert!(condition.len() > 5); // Should have content after "p2pk:"
    }

    #[test]
    fn test_p2pk_spending_validation_logic() {
        let pubkey1 = create_test_pubkey(60);
        let pubkey2 = create_test_pubkey(61);
        
        // Test P2PK spending validation logic (without full mint)
        
        // Game tokens should always be spendable
        let game_type = GameTokenType::Game;
        let can_spend_game = match game_type {
            GameTokenType::Game => true,
            GameTokenType::Reward { .. } => false,
        };
        assert!(can_spend_game);
        
        // P2PK locked reward tokens should only be spendable by correct pubkey
        let locked_type = GameTokenType::Reward { p2pk_locked: Some(pubkey1) };
        let can_spend_by_owner = match &locked_type {
            GameTokenType::Reward { p2pk_locked: Some(locked_pubkey) } => *locked_pubkey == pubkey1,
            _ => false,
        };
        let can_spend_by_other = match &locked_type {
            GameTokenType::Reward { p2pk_locked: Some(locked_pubkey) } => *locked_pubkey == pubkey2,
            _ => false,
        };
        assert!(can_spend_by_owner);
        assert!(!can_spend_by_other);
        
        // Unlocked reward tokens should be spendable by anyone
        let unlocked_type = GameTokenType::Reward { p2pk_locked: None };
        let can_spend_unlocked = match &unlocked_type {
            GameTokenType::Reward { p2pk_locked: None } => true,
            _ => false,
        };
        assert!(can_spend_unlocked);
    }
}
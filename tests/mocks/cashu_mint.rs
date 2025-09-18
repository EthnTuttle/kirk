//! Mock Cashu mint for testing using CDK

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use cdk::nuts::{Token, Proof, MintInfo, KeySet, Id, CurrencyUnit, MeltQuoteState, MintQuoteState, Secret, PublicKey as CashuPublicKey};
use cdk::Amount;
use nostr::PublicKey;
use kirk::{GameProtocolError, GameToken, GameTokenType};

/// Mock Cashu mint that simulates CDK mint operations
#[derive(Debug, Clone)]
pub struct MockCashuMint {
    /// Stored tokens by their token ID
    tokens: Arc<Mutex<HashMap<String, Token>>>,
    /// Mint info
    info: MintInfo,
    /// Active keysets
    keysets: Arc<Mutex<Vec<KeySet>>>,
    /// Mint quote states
    mint_quotes: Arc<Mutex<HashMap<String, MintQuoteState>>>,
    /// Melt quote states  
    melt_quotes: Arc<Mutex<HashMap<String, MeltQuoteState>>>,
}

impl MockCashuMint {
    /// Create a new mock mint
    pub fn new() -> Self {
        let info = MintInfo {
            name: Some("Mock Mint".to_string()),
            pubkey: None,
            version: None, // Simplified for testing
            description: Some("Mock mint for testing".to_string()),
            description_long: None,
            contact: None,
            motd: None,
            nuts: Default::default(),
            icon_url: None,
            time: None,
            tos_url: None,
            privacy_policy_url: None,
        };

        Self {
            tokens: Arc::new(Mutex::new(HashMap::new())),
            info,
            keysets: Arc::new(Mutex::new(Vec::new())),
            mint_quotes: Arc::new(Mutex::new(HashMap::new())),
            melt_quotes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get mint info
    pub fn get_mint_info(&self) -> MintInfo {
        self.info.clone()
    }

    /// Add a keyset to the mint
    pub fn add_keyset(&self, keyset: KeySet) {
        let mut keysets = self.keysets.lock().unwrap();
        keysets.push(keyset);
    }

    /// Get active keysets
    pub fn get_keysets(&self) -> Vec<KeySet> {
        let keysets = self.keysets.lock().unwrap();
        keysets.clone()
    }

    /// Mint new tokens (simplified mock implementation)
    pub async fn mint_tokens(&self, amount: Amount, keyset_id: Id) -> Result<Vec<GameToken>, GameProtocolError> {
        // Create mock proofs for the requested amount
        let mut proofs = Vec::new();
        let mut remaining = amount;

        // Break down amount into standard denominations
        let denominations = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024];
        
        for &denom in denominations.iter().rev() {
            while remaining >= Amount::from(denom) {
                let proof = self.create_mock_proof(Amount::from(denom), keyset_id)?;
                proofs.push(proof);
                remaining = remaining - Amount::from(denom);
            }
        }

        // Create token from proofs
        let token = Token::new(
            "https://mock-mint.example.com".parse().unwrap(),
            proofs,
            None,
            CurrencyUnit::Sat,
        );

        // Store the token
        let token_id = format!("token_{}", uuid::Uuid::new_v4());
        {
            let mut tokens = self.tokens.lock().unwrap();
            tokens.insert(token_id, token.clone());
        }

        // Wrap as GameToken
        let game_token = GameToken::from_cdk_token(token, GameTokenType::Game);
        Ok(vec![game_token])
    }

    /// Mint P2PK locked reward tokens
    pub async fn mint_reward_tokens(
        &self, 
        amount: Amount, 
        winner_pubkey: PublicKey,
        keyset_id: Id
    ) -> Result<Vec<GameToken>, GameProtocolError> {
        // Create P2PK locked proofs
        let mut proofs = Vec::new();
        let mut remaining = amount;

        let denominations = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024];
        
        for &denom in denominations.iter().rev() {
            while remaining >= Amount::from(denom) {
                let mut proof = self.create_mock_proof(Amount::from(denom), keyset_id)?;
                
                // Add P2PK witness (simplified)
                proof.witness = Some(format!("p2pk:{}", winner_pubkey));
                
                proofs.push(proof);
                remaining = remaining - Amount::from(denom);
            }
        }

        let token = Token::new(
            "https://mock-mint.example.com".parse().unwrap(),
            proofs,
            None,
            CurrencyUnit::Sat,
        );

        let token_id = format!("reward_token_{}", uuid::Uuid::new_v4());
        {
            let mut tokens = self.tokens.lock().unwrap();
            tokens.insert(token_id, token.clone());
        }

        let game_token = GameToken::from_cdk_token(
            token, 
            GameTokenType::Reward { p2pk_locked: Some(winner_pubkey) }
        );
        Ok(vec![game_token])
    }

    /// Validate a token (check if proofs are valid)
    pub async fn validate_token(&self, token: &Token) -> Result<bool, GameProtocolError> {
        // In a real implementation, this would verify the cryptographic proofs
        // For testing, we'll just check if the token structure is valid
        
        if token.proofs().is_empty() {
            return Ok(false);
        }

        // Check that all proofs have valid amounts
        // Note: In newer CDK versions, proofs() might require keysets parameter
        // For mock purposes, we'll assume the token structure is valid if it has proofs
        if token.token.is_empty() {
            return Ok(false);
        }

        Ok(true)
    }

    /// Check if a token has been spent (simplified)
    pub async fn is_token_spent(&self, token: &Token) -> Result<bool, GameProtocolError> {
        // For testing purposes, we'll consider tokens as unspent by default
        // In a real implementation, this would check the nullifier database
        Ok(false)
    }

    /// Get stored token by ID (for testing)
    pub fn get_stored_token(&self, token_id: &str) -> Option<Token> {
        let tokens = self.tokens.lock().unwrap();
        tokens.get(token_id).cloned()
    }

    /// Clear all stored tokens
    pub fn clear_tokens(&self) {
        let mut tokens = self.tokens.lock().unwrap();
        tokens.clear();
    }

    /// Get count of stored tokens
    pub fn token_count(&self) -> usize {
        let tokens = self.tokens.lock().unwrap();
        tokens.len()
    }

    /// Create a mock proof with realistic structure
    fn create_mock_proof(&self, amount: Amount, keyset_id: Id) -> Result<Proof, GameProtocolError> {
        use sha2::{Sha256, Digest};
        use hex;

        // Generate a mock secret
        let secret_str = format!("mock_secret_{}", uuid::Uuid::new_v4());
        let secret = Secret::new(secret_str);
        
        // Generate mock C value (this would be the unblinded signature in real Cashu)
        let mut hasher = Sha256::new();
        hasher.update(secret.as_bytes());
        hasher.update(amount.to_string().as_bytes());
        let c_bytes = hasher.finalize();
        let c_hex = hex::encode(c_bytes);
        let c = CashuPublicKey::from_hex(&c_hex).map_err(|e| GameProtocolError::GameValidation(format!("Invalid C value: {}", e)))?;

        Ok(Proof {
            amount,
            secret,
            c,
            keyset_id,
            witness: None,
            dleq: None,
        })
    }
    
    /// Helper to create a test keyset ID
    fn create_test_keyset_id() -> Id {
        Id::from_bytes(&[0u8; 8]).unwrap()
    }
}

impl Default for MockCashuMint {
    fn default() -> Self {
        Self::new()
    }
}

// Add uuid dependency for generating unique IDs
use uuid;

#[cfg(test)]
mod tests {
    use super::*;
    use cdk::nuts::Id;

    #[tokio::test]
    async fn test_mock_mint_basic_operations() {
        let mint = MockCashuMint::new();
        let keyset_id = Id::from_bytes(&[0u8; 8]).unwrap();

        // Test minting tokens
        let tokens = mint.mint_tokens(Amount::from(100), keyset_id).await.unwrap();
        assert!(!tokens.is_empty());

        // Test token validation
        let is_valid = mint.validate_token(tokens[0].as_cdk_token()).await.unwrap();
        assert!(is_valid);

        // Test token count
        assert_eq!(mint.token_count(), 1);
    }

    #[tokio::test]
    async fn test_mint_reward_tokens() {
        let mint = MockCashuMint::new();
        let keyset_id = Id::from_bytes(&[1u8; 8]).unwrap();
        let winner_pubkey = PublicKey::from_slice(&[2u8; 32]).unwrap();

        let reward_tokens = mint.mint_reward_tokens(
            Amount::from(50), 
            winner_pubkey, 
            keyset_id
        ).await.unwrap();

        assert!(!reward_tokens.is_empty());
        assert!(reward_tokens[0].is_p2pk_locked());
    }

    #[tokio::test]
    async fn test_token_validation() {
        let mint = MockCashuMint::new();
        let keyset_id = Id::from_bytes(&[3u8; 8]).unwrap();

        // Create valid token
        let tokens = mint.mint_tokens(Amount::from(25), keyset_id).await.unwrap();
        let is_valid = mint.validate_token(tokens[0].as_cdk_token()).await.unwrap();
        assert!(is_valid);

        // Test with empty token
        let empty_token = Token::new(
            "https://mock-mint.example.com".parse().unwrap(),
            vec![],
            None,
            Some(CurrencyUnit::Sat),
        ).unwrap();
        
        let is_empty_valid = mint.validate_token(&empty_token).await.unwrap();
        assert!(!is_empty_valid);
    }
}
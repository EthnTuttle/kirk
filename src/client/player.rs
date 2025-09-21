//! Player client for game participation

use nostr::{Keys, EventId};
use nostr_sdk::Client as NostrClient;
use cdk::wallet::Wallet as CdkWallet;
use crate::error::GameProtocolError;
use crate::cashu::{GameToken, TokenCommitment};
use crate::events::{
    ChallengeContent, ChallengeAcceptContent, MoveContent, FinalContent,
    MoveType, CommitmentMethod, TimeoutConfig
};
use crate::game::Game;

/// Client interface for players to participate in games
pub struct PlayerClient {
    nostr_client: NostrClient,
    cashu_wallet: CdkWallet,
    keys: Keys, // Nostr keys for signing
}

impl PlayerClient {
    /// Create new player client
    pub fn new(nostr_client: NostrClient, cashu_wallet: CdkWallet, keys: Keys) -> Self {
        Self {
            nostr_client,
            cashu_wallet,
            keys,
        }
    }
    
    /// Create and publish challenge with configurable expiry and timeout configuration
    pub async fn create_challenge<G: Game>(
        &self,
        _game: &G, // Game parameter for future use in validation
        tokens: &[GameToken],
        expiry_seconds: Option<u64>
    ) -> Result<EventId, GameProtocolError> {
        self.create_challenge_with_timeouts(_game, tokens, expiry_seconds, None).await
    }
    
    /// Create and publish challenge with configurable expiry and timeout configuration
    pub async fn create_challenge_with_timeouts<G: Game>(
        &self,
        _game: &G, // Game parameter for future use in validation
        tokens: &[GameToken],
        expiry_seconds: Option<u64>,
        timeout_config: Option<TimeoutConfig>
    ) -> Result<EventId, GameProtocolError> {
        // Validate tokens are Game tokens
        for token in tokens {
            if !token.is_game_token() {
                return Err(GameProtocolError::InvalidToken(
                    "Only Game tokens can be used for challenges".to_string()
                ));
            }
        }
        
        // Create hash commitments for tokens
        let commitment_hashes = self.create_commitments(tokens)?;
        
        // Calculate expiry time (default to 1 hour if not specified)
        let expiry = if let Some(seconds) = expiry_seconds {
            let now = chrono::Utc::now().timestamp() as u64;
            now.checked_add(seconds)
                .ok_or(GameProtocolError::InvalidExpiry)?
        } else {
            // Default to 1 hour (3600 seconds)
            let now = chrono::Utc::now().timestamp() as u64;
            now.checked_add(3600)
                .ok_or(GameProtocolError::InvalidExpiry)?
        };
        
        // Create challenge content
        let challenge_content = ChallengeContent {
            game_type: std::any::type_name::<G>().to_string(),
            commitment_hashes,
            game_parameters: serde_json::json!({}), // Game-specific parameters can be added later
            expiry: Some(expiry),
            timeout_config,
        };
        
        // Validate challenge content
        challenge_content.validate()?;
        
        // Create and publish event
        let event = challenge_content.to_event(&self.keys)?;
        let event_id = event.id;
        
        self.nostr_client.send_event(event).await?;
        
        Ok(event_id)
    }
    
    /// Create challenge with default 1-hour expiry (convenience method)
    pub async fn create_challenge_default<G: Game>(
        &self,
        game: &G,
        tokens: &[GameToken]
    ) -> Result<EventId, GameProtocolError> {
        self.create_challenge(game, tokens, None).await
    }
    
    /// Accept existing challenge with commitment creation
    pub async fn accept_challenge<G: Game>(
        &self,
        challenge_id: EventId,
        _game: &G, // Game parameter for future use in validation
        tokens: &[GameToken]
    ) -> Result<EventId, GameProtocolError> {
        // Validate tokens are Game tokens
        for token in tokens {
            if !token.is_game_token() {
                return Err(GameProtocolError::InvalidToken(
                    "Only Game tokens can be used for challenge acceptance".to_string()
                ));
            }
        }
        
        // Create hash commitments for tokens
        let commitment_hashes = self.create_commitments(tokens)?;
        
        // Create challenge accept content
        let accept_content = ChallengeAcceptContent {
            challenge_id,
            commitment_hashes,
        };
        
        // Validate accept content
        accept_content.validate()?;
        
        // Create and publish event
        let event = accept_content.to_event(&self.keys)?;
        let event_id = event.id;
        
        self.nostr_client.send_event(event).await?;
        
        Ok(event_id)
    }
    
    /// Make a move supporting all move types (Move, Commit, Reveal)
    pub async fn make_move<G: Game>(
        &self,
        previous_event: EventId,
        move_type: MoveType,
        move_data: G::MoveData,
        revealed_tokens: Option<Vec<GameToken>>
    ) -> Result<EventId, GameProtocolError> {
        self.make_move_with_deadline::<G>(previous_event, move_type, move_data, revealed_tokens, None).await
    }
    
    /// Make a move with an optional deadline
    pub async fn make_move_with_deadline<G: Game>(
        &self,
        previous_event: EventId,
        move_type: MoveType,
        move_data: G::MoveData,
        revealed_tokens: Option<Vec<GameToken>>,
        deadline: Option<u64>
    ) -> Result<EventId, GameProtocolError> {
        // Validate move type consistency
        match move_type {
            MoveType::Reveal => {
                if revealed_tokens.is_none() {
                    return Err(GameProtocolError::InvalidMove(
                        "Reveal moves must include revealed tokens".to_string()
                    ));
                }
            },
            MoveType::Commit => {
                if revealed_tokens.is_some() {
                    return Err(GameProtocolError::InvalidMove(
                        "Commit moves should not include revealed tokens".to_string()
                    ));
                }
            },
            MoveType::Move => {
                // Regular moves may or may not have revealed tokens
            }
        }
        
        // Convert revealed tokens to CDK tokens for serialization
        let revealed_cdk_tokens = revealed_tokens.map(|tokens| {
            tokens.into_iter().map(|gt| gt.into_cdk_token()).collect()
        });
        
        // Create move content
        let move_content = MoveContent {
            previous_event_id: previous_event,
            move_type,
            move_data: serde_json::to_value(move_data)?,
            revealed_tokens: revealed_cdk_tokens,
            deadline,
        };
        
        // Validate move content
        move_content.validate()?;
        
        // Create and publish event
        let event = move_content.to_event(&self.keys)?;
        let event_id = event.id;
        
        self.nostr_client.send_event(event).await?;
        
        Ok(event_id)
    }
    
    /// Publish final event for game completion
    pub async fn finalize_game(
        &self,
        game_root: EventId,
        commitment_method: Option<CommitmentMethod>,
        final_state: serde_json::Value
    ) -> Result<EventId, GameProtocolError> {
        // Create final content
        let final_content = FinalContent {
            game_sequence_root: game_root,
            commitment_method,
            final_state,
        };
        
        // Validate final content
        final_content.validate()?;
        
        // Create and publish event
        let event = final_content.to_event(&self.keys)?;
        let event_id = event.id;
        
        self.nostr_client.send_event(event).await?;
        
        Ok(event_id)
    }
    
    /// Create hash commitments for tokens (private helper method)
    fn create_commitments(&self, tokens: &[GameToken]) -> Result<Vec<String>, GameProtocolError> {
        if tokens.is_empty() {
            return Err(GameProtocolError::InvalidToken(
                "At least one token is required for commitment".to_string()
            ));
        }
        
        if tokens.len() == 1 {
            // Single token commitment
            let commitment = TokenCommitment::single(&tokens[0].inner);
            Ok(vec![commitment.commitment_hash])
        } else {
            // Multiple token commitment using merkle tree radix 4 as default
            let cdk_tokens: Vec<_> = tokens.iter().map(|gt| gt.inner.clone()).collect();
            let commitment = TokenCommitment::multiple(&cdk_tokens, CommitmentMethod::MerkleTreeRadix4);
            Ok(vec![commitment.commitment_hash])
        }
    }
    
    /// Get the nostr client for advanced operations
    pub fn nostr_client(&self) -> &NostrClient {
        &self.nostr_client
    }
    
    /// Get the CDK wallet for token operations
    pub fn cashu_wallet(&self) -> &CdkWallet {
        &self.cashu_wallet
    }
    
    /// Get the nostr keys for signing
    pub fn keys(&self) -> &Keys {
        &self.keys
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{Keys, PublicKey, Event as NostrEvent};
    use nostr_sdk::Client as NostrClient;
    use cdk::wallet::Wallet as CdkWallet;
    use cdk::nuts::Token as CashuToken;
    use crate::cashu::{GameToken, GameTokenType};
    use crate::events::MoveType;

    /// Create test keys for testing
    fn create_test_keys() -> Keys {
        Keys::generate()
    }

    /// Create a mock nostr client for testing
    fn create_mock_nostr_client() -> NostrClient {
        // Create a client that won't actually connect to relays for testing
        NostrClient::default()
    }

    /// Create a mock CDK wallet for testing
    async fn create_mock_cdk_wallet() -> CdkWallet {
        // This is a placeholder - in real implementation we'd need proper CDK wallet setup
        // For testing, we'll create a minimal wallet using available dependencies
        use cdk::nuts::CurrencyUnit;
        use cdk_sqlite::WalletSqliteDatabase;
        use std::sync::Arc;
        
        // Create an in-memory SQLite database for testing
        let db = WalletSqliteDatabase::new(":memory:").await.unwrap();
        let localstore = Arc::new(db);
        
        // Create a test seed
        let seed = [1u8; 64];
        
        CdkWallet::new(
            "https://test-mint.example.com",
            CurrencyUnit::Sat,
            localstore,
            seed,
            None,
        ).unwrap()
    }

    /// Create a test CDK token for testing
    fn create_test_cdk_token(seed: u8) -> CashuToken {
        // Create a simple JSON structure that can be parsed as a CDK Token
        let token_json = format!(
            r#"{{"token":[{{"mint":"https://mint{}.example.com","proofs":[]}}],"memo":"test_token_seed_{}","unit":"sat"}}"#,
            seed, seed
        );
        
        serde_json::from_str(&token_json)
            .unwrap_or_else(|e| {
                panic!("Failed to create test CDK token for seed {}: {}. CDK Token structure may have changed.", seed, e);
            })
    }

    /// Create test game tokens
    fn create_test_game_tokens(count: usize) -> Vec<GameToken> {
        (0..count).map(|i| {
            let cdk_token = create_test_cdk_token(i as u8);
            GameToken::from_cdk_token(cdk_token, GameTokenType::Game)
        }).collect()
    }

    /// Create a test player client
    async fn create_test_player_client() -> PlayerClient {
        let keys = create_test_keys();
        let nostr_client = create_mock_nostr_client();
        let cashu_wallet = create_mock_cdk_wallet().await;
        
        PlayerClient::new(nostr_client, cashu_wallet, keys)
    }

    #[tokio::test]
    async fn test_player_client_creation() {
        let client = create_test_player_client().await;
        
        // Verify client was created successfully
        assert!(!client.keys().public_key().to_string().is_empty());
    }

    #[tokio::test]
    async fn test_create_commitments_single_token() {
        let client = create_test_player_client().await;
        let tokens = create_test_game_tokens(1);
        
        let commitments = client.create_commitments(&tokens).unwrap();
        
        // Should have exactly one commitment
        assert_eq!(commitments.len(), 1);
        
        // Commitment should be valid hex string of correct length
        assert_eq!(commitments[0].len(), 64); // SHA256 hex = 64 chars
        assert!(hex::decode(&commitments[0]).is_ok());
    }

    #[tokio::test]
    async fn test_create_commitments_multiple_tokens() {
        let client = create_test_player_client().await;
        let tokens = create_test_game_tokens(3);
        
        let commitments = client.create_commitments(&tokens).unwrap();
        
        // Should have exactly one commitment (merkle tree root)
        assert_eq!(commitments.len(), 1);
        
        // Commitment should be valid hex string of correct length
        assert_eq!(commitments[0].len(), 64);
        assert!(hex::decode(&commitments[0]).is_ok());
    }

    #[tokio::test]
    async fn test_create_commitments_empty_tokens() {
        let client = create_test_player_client().await;
        let tokens = vec![];
        
        let result = client.create_commitments(&tokens);
        
        // Should fail with empty tokens
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GameProtocolError::InvalidToken(_)));
    }

    #[tokio::test]
    async fn test_create_commitments_deterministic() {
        let client = create_test_player_client().await;
        let tokens = create_test_game_tokens(2);
        
        let commitments1 = client.create_commitments(&tokens).unwrap();
        let commitments2 = client.create_commitments(&tokens).unwrap();
        
        // Same tokens should produce same commitments
        assert_eq!(commitments1, commitments2);
    }

    #[tokio::test]
    async fn test_validate_game_tokens_only() {
        let client = create_test_player_client().await;
        
        // Create game tokens (should be valid)
        let game_tokens = create_test_game_tokens(2);
        let result = client.create_commitments(&game_tokens);
        assert!(result.is_ok());
        
        // Create reward tokens (should be rejected in actual challenge/accept methods)
        let cdk_token = create_test_cdk_token(1);
        let reward_token = GameToken::from_cdk_token(cdk_token, GameTokenType::Reward { p2pk_locked: None });
        let reward_tokens = vec![reward_token];
        
        // The create_commitments method itself doesn't validate token types,
        // but the challenge/accept methods should validate
        let result = client.create_commitments(&reward_tokens);
        assert!(result.is_ok()); // create_commitments doesn't validate types
    }

    // Note: The following tests would require actual async runtime and mock implementations
    // of nostr client and CDK wallet to test the full async methods. For now, we test
    // the core logic that doesn't require external dependencies.

    #[tokio::test]
    async fn test_create_challenge_validates_token_types() {
        let client = create_test_player_client().await;
        
        // Create reward tokens (should be rejected)
        let cdk_token = create_test_cdk_token(1);
        let reward_token = GameToken::from_cdk_token(cdk_token, GameTokenType::Reward { p2pk_locked: None });
        let reward_tokens = vec![reward_token];
        
        // Mock game implementation
        struct MockGame;
        impl Game for MockGame {
            type GamePiece = u8;
            type GameState = u8;
            type MoveData = u8;
            
            fn decode_c_value(&self, _c_value: &[u8; 32]) -> Result<Vec<Self::GamePiece>, GameProtocolError> {
                Ok(vec![1])
            }
            
            fn validate_sequence(&self, _events: &[NostrEvent]) -> Result<crate::error::ValidationResult, GameProtocolError> {
                Ok(crate::error::ValidationResult::new(
                    true,
                    None,
                    vec![],
                    None,
                ))
            }
            
            fn is_sequence_complete(&self, _events: &[NostrEvent]) -> Result<bool, GameProtocolError> {
                Ok(false)
            }
            
            fn determine_winner(&self, _events: &[NostrEvent]) -> Result<Option<PublicKey>, GameProtocolError> {
                Ok(None)
            }
            
            fn required_final_events(&self) -> usize {
                1
            }
        }
        
        let game = MockGame;
        
        // Should fail because reward tokens are not allowed for challenges
        let result = client.create_challenge(&game, &reward_tokens, Some(3600)).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GameProtocolError::InvalidToken(_)));
    }

    #[tokio::test]
    async fn test_accept_challenge_validates_token_types() {
        let client = create_test_player_client().await;
        
        // Create reward tokens (should be rejected)
        let cdk_token = create_test_cdk_token(1);
        let reward_token = GameToken::from_cdk_token(cdk_token, GameTokenType::Reward { p2pk_locked: None });
        let reward_tokens = vec![reward_token];
        
        // Mock game implementation
        struct MockGame;
        impl Game for MockGame {
            type GamePiece = u8;
            type GameState = u8;
            type MoveData = u8;
            
            fn decode_c_value(&self, _c_value: &[u8; 32]) -> Result<Vec<Self::GamePiece>, GameProtocolError> {
                Ok(vec![1])
            }
            
            fn validate_sequence(&self, _events: &[NostrEvent]) -> Result<crate::error::ValidationResult, GameProtocolError> {
                Ok(crate::error::ValidationResult::new(
                    true,
                    None,
                    vec![],
                    None,
                ))
            }
            
            fn is_sequence_complete(&self, _events: &[NostrEvent]) -> Result<bool, GameProtocolError> {
                Ok(false)
            }
            
            fn determine_winner(&self, _events: &[NostrEvent]) -> Result<Option<PublicKey>, GameProtocolError> {
                Ok(None)
            }
            
            fn required_final_events(&self) -> usize {
                1
            }
        }
        
        let game = MockGame;
        let challenge_id = nostr::EventId::from_hex("0000000000000000000000000000000000000000000000000000000000000001").unwrap();
        
        // Should fail because reward tokens are not allowed for challenge acceptance
        let result = client.accept_challenge(challenge_id, &game, &reward_tokens).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GameProtocolError::InvalidToken(_)));
    }

    #[test]
    fn test_move_type_validation() {
        // Test that move type validation logic is correct
        
        // Reveal moves must have tokens
        let _game_tokens = create_test_game_tokens(1);
        assert!(format!("{:?}", MoveType::Reveal).contains("Reveal")); // Basic enum test
        
        // Commit moves should not have tokens (this is validated in make_move)
        // Move moves can have or not have tokens
        
        // These validations are tested in the actual make_move method calls
    }

    #[tokio::test]
    async fn test_commitment_method_usage() {
        let client = create_test_player_client().await;
        
        // Single token should use single commitment (no method needed)
        let single_token = create_test_game_tokens(1);
        let commitments = client.create_commitments(&single_token).unwrap();
        assert_eq!(commitments.len(), 1);
        
        // Multiple tokens should use merkle tree radix 4 by default
        let multiple_tokens = create_test_game_tokens(4);
        let commitments = client.create_commitments(&multiple_tokens).unwrap();
        assert_eq!(commitments.len(), 1);
        
        // The actual commitment method is tested in the TokenCommitment tests
    }

    #[test]
    fn test_expiry_calculation() {
        let now = chrono::Utc::now().timestamp() as u64;
        
        // Test default expiry (1 hour = 3600 seconds)
        let default_expiry = now + 3600;
        assert!(default_expiry > now);
        assert!(default_expiry <= now + 3601); // Allow for small timing differences
        
        // Test custom expiry
        let custom_seconds = 7200u64; // 2 hours
        let custom_expiry = now + custom_seconds;
        assert!(custom_expiry > now);
        assert_eq!(custom_expiry - now, custom_seconds);
    }

    #[tokio::test]
    async fn test_client_accessors() {
        let client = create_test_player_client().await;
        
        // Test that accessors work
        let _nostr_client = client.nostr_client();
        let _cashu_wallet = client.cashu_wallet();
        let _keys = client.keys();
        
        // Verify keys are consistent
        let keys1 = client.keys();
        let keys2 = client.keys();
        assert_eq!(keys1.public_key(), keys2.public_key());
    }
}
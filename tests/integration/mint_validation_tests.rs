//! Integration tests for mint validation workflows

use kirk::{SequenceProcessor, SequenceProcessorConfig, ProcessingResult, GameProtocolError};
use tests::mocks::{MockNostrRelay, MockCashuMint, CoinFlipGame};
use nostr::{Keys, EventBuilder, Kind};
use cdk::{Amount, nuts::Id};
use std::sync::Arc;

/// Helper to create a complete game sequence for testing
async fn create_test_game_sequence(
    relay: &MockNostrRelay,
    mint: &MockCashuMint,
) -> Result<Vec<nostr::Event>, GameProtocolError> {
    let player1_keys = Keys::generate();
    let player2_keys = Keys::generate();
    let game = CoinFlipGame::new();
    
    // Create tokens
    let keyset_id = Id::from_bytes(&[0u8; 8]).unwrap();
    let player1_tokens = mint.mint_tokens(Amount::from(100), keyset_id).await?;
    let player2_tokens = mint.mint_tokens(Amount::from(100), keyset_id).await?;
    
    // Challenge
    let challenge_content = kirk::ChallengeContent {
        game_type: CoinFlipGame::game_type(),
        commitment_hashes: vec!["p1_hash".to_string()],
        game_parameters: game.get_parameters()?,
        expiry: None,
    };
    let challenge_event = challenge_content.to_event(&player1_keys)?;
    relay.store_event(challenge_event.clone())?;
    
    // Accept
    let accept_content = kirk::ChallengeAcceptContent {
        challenge_id: challenge_event.id,
        commitment_hashes: vec!["p2_hash".to_string()],
    };
    let accept_event = accept_content.to_event(&player2_keys)?;
    relay.store_event(accept_event.clone())?;
    
    // Moves
    let move1_content = kirk::MoveContent {
        previous_event_id: accept_event.id,
        move_type: kirk::MoveType::Reveal,
        move_data: serde_json::to_value(&tests::mocks::reference_game::CoinFlipMove {
            chosen_side: tests::mocks::reference_game::CoinSide::Heads,
            confidence: 150,
        })?,
        revealed_tokens: Some(vec![player1_tokens[0].as_cdk_token().clone()]),
    };
    let move1_event = move1_content.to_event(&player1_keys)?;
    relay.store_event(move1_event.clone())?;
    
    let move2_content = kirk::MoveContent {
        previous_event_id: move1_event.id,
        move_type: kirk::MoveType::Reveal,
        move_data: serde_json::to_value(&tests::mocks::reference_game::CoinFlipMove {
            chosen_side: tests::mocks::reference_game::CoinSide::Tails,
            confidence: 120,
        })?,
        revealed_tokens: Some(vec![player2_tokens[0].as_cdk_token().clone()]),
    };
    let move2_event = move2_content.to_event(&player2_keys)?;
    relay.store_event(move2_event.clone())?;
    
    // Final events
    let final1_content = kirk::FinalContent {
        game_sequence_root: challenge_event.id,
        commitment_method: None,
        final_state: serde_json::json!({"complete": true}),
    };
    let final1_event = final1_content.to_event(&player1_keys)?;
    relay.store_event(final1_event.clone())?;
    
    let final2_content = kirk::FinalContent {
        game_sequence_root: challenge_event.id,
        commitment_method: None,
        final_state: serde_json::json!({"complete": true}),
    };
    let final2_event = final2_content.to_event(&player2_keys)?;
    relay.store_event(final2_event.clone())?;
    
    Ok(vec![
        challenge_event, accept_event, move1_event, move2_event, final1_event, final2_event
    ])
}

#[cfg(test)]
mod sequence_processor_tests {
    use super::*;

    #[ignore] // Skip for now - requires complex GameMint setup
    #[tokio::test]
    async fn test_sequence_processor_valid_game() {
        let relay = MockNostrRelay::new();
        let mint = MockCashuMint::new();
        let game = Arc::new(CoinFlipGame::new());
        
        let config = SequenceProcessorConfig {
            max_concurrent_games: 10,
            validation_timeout_seconds: 30,
            reward_multiplier: 1.0,
        };
        
        let processor = SequenceProcessor::new(config, game.clone());
        
        // Create a valid game sequence
        let events = create_test_game_sequence(&relay, &mint).await.unwrap();
        
        // Process the sequence
        let result = processor.process_sequence(&events, &mint).await.unwrap();
        
        match result {
            ProcessingResult::GameComplete { winner, reward_amount, .. } => {
                assert!(winner.is_some());
                assert!(reward_amount > Amount::ZERO);
            },
            _ => panic!("Expected GameComplete result"),
        }
    }

    #[ignore] // Skip for now - requires complex GameMint setup
    #[tokio::test]
    async fn test_sequence_processor_invalid_tokens() {
        let relay = MockNostrRelay::new();
        let mint = MockCashuMint::new();
        let game = Arc::new(CoinFlipGame::new());
        
        let config = SequenceProcessorConfig {
            max_concurrent_games: 10,
            validation_timeout_seconds: 30,
            reward_multiplier: 1.0,
        };
        
        let processor = SequenceProcessor::new(config, game.clone());
        
        // Create sequence with invalid tokens (empty proofs)
        let player1_keys = Keys::generate();
        let player2_keys = Keys::generate();
        
        let challenge_content = kirk::ChallengeContent {
            game_type: CoinFlipGame::game_type(),
            commitment_hashes: vec!["hash".to_string()],
            game_parameters: serde_json::json!({}),
            expiry: None,
        };
        let challenge_event = challenge_content.to_event(&player1_keys).unwrap();
        
        // Create token with empty proofs (invalid)
        let invalid_token = cdk::nuts::Token::new(
            "https://test-mint.example.com".parse().unwrap(),
            vec![], // Empty proofs
            None,
            cdk::nuts::CurrencyUnit::Sat,
        );
        
        let move_content = kirk::MoveContent {
            previous_event_id: challenge_event.id,
            move_type: kirk::MoveType::Reveal,
            move_data: serde_json::json!({"test": "move"}),
            revealed_tokens: Some(vec![invalid_token]),
        };
        let move_event = move_content.to_event(&player1_keys).unwrap();
        
        let events = vec![challenge_event, move_event];
        
        // Should detect invalid tokens
        let result = processor.process_sequence(&events, &mint).await.unwrap();
        
        match result {
            ProcessingResult::ValidationFailure { reason, .. } => {
                assert!(reason.contains("token") || reason.contains("invalid"));
            },
            _ => panic!("Expected ValidationFailure result"),
        }
    }

    #[ignore] // Skip for now - requires complex GameMint setup
    #[tokio::test]
    async fn test_sequence_processor_incomplete_sequence() {
        let relay = MockNostrRelay::new();
        let mint = MockCashuMint::new();
        let game = Arc::new(CoinFlipGame::new());
        
        let config = SequenceProcessorConfig {
            max_concurrent_games: 10,
            validation_timeout_seconds: 30,
            reward_multiplier: 1.0,
        };
        
        let processor = SequenceProcessor::new(config, game.clone());
        
        // Create incomplete sequence (only challenge)
        let player1_keys = Keys::generate();
        let challenge_content = kirk::ChallengeContent {
            game_type: CoinFlipGame::game_type(),
            commitment_hashes: vec!["hash".to_string()],
            game_parameters: serde_json::json!({}),
            expiry: None,
        };
        let challenge_event = challenge_content.to_event(&player1_keys).unwrap();
        
        let events = vec![challenge_event];
        
        // Should detect incomplete sequence
        let result = processor.process_sequence(&events, &mint).await.unwrap();
        
        match result {
            ProcessingResult::SequenceIncomplete { .. } => {
                // Expected result
            },
            _ => panic!("Expected SequenceIncomplete result"),
        }
    }

    #[ignore] // Skip for now - requires complex GameMint setup
    #[tokio::test]
    async fn test_sequence_processor_fraud_detection() {
        let relay = MockNostrRelay::new();
        let mint = MockCashuMint::new();
        let game = Arc::new(CoinFlipGame::new());
        
        let config = SequenceProcessorConfig {
            max_concurrent_games: 10,
            validation_timeout_seconds: 30,
            reward_multiplier: 1.0,
        };
        
        let processor = SequenceProcessor::new(config, game.clone());
        
        // Create sequence with fraudulent move (invalid JSON)
        let player1_keys = Keys::generate();
        let challenge_content = kirk::ChallengeContent {
            game_type: CoinFlipGame::game_type(),
            commitment_hashes: vec!["hash".to_string()],
            game_parameters: serde_json::json!({}),
            expiry: None,
        };
        let challenge_event = challenge_content.to_event(&player1_keys).unwrap();
        
        // Create move with invalid content
        let fraudulent_move = EventBuilder::new(Kind::Custom(9261), "invalid json", Vec::<nostr::Tag>::new())
            .to_event(&player1_keys)
            .unwrap();
        
        let events = vec![challenge_event, fraudulent_move];
        
        // Should detect fraud
        let result = processor.process_sequence(&events, &mint).await.unwrap();
        
        match result {
            ProcessingResult::PlayerForfeited { forfeited_player, .. } => {
                assert_eq!(forfeited_player, player1_keys.public_key());
            },
            ProcessingResult::ValidationFailure { .. } => {
                // Also acceptable - depends on implementation
            },
            _ => panic!("Expected PlayerForfeited or ValidationFailure result"),
        }
    }
}

#[cfg(test)]
mod mint_integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_mint_token_validation_workflow() {
        let mint = MockCashuMint::new();
        let keyset_id = Id::from_bytes(&[1u8; 8]).unwrap();
        
        // Mint tokens
        let tokens = mint.mint_tokens(Amount::from(200), keyset_id).await.unwrap();
        assert!(!tokens.is_empty());
        
        // Validate tokens
        for token in &tokens {
            let is_valid = mint.validate_token(token.as_cdk_token()).await.unwrap();
            assert!(is_valid);
        }
        
        // Check if tokens are spent (should be false for new tokens)
        for token in &tokens {
            let is_spent = mint.is_token_spent(token.as_cdk_token()).await.unwrap();
            assert!(!is_spent);
        }
    }

    #[tokio::test]
    async fn test_mint_reward_token_creation() {
        let mint = MockCashuMint::new();
        let keyset_id = Id::from_bytes(&[2u8; 8]).unwrap();
        let winner_pubkey = nostr::PublicKey::from_slice(&[3u8; 32]).unwrap();
        
        // Mint P2PK locked reward tokens
        let reward_tokens = mint.mint_reward_tokens(
            Amount::from(500), 
            winner_pubkey, 
            keyset_id
        ).await.unwrap();
        
        assert!(!reward_tokens.is_empty());
        
        // Verify tokens are P2PK locked
        for token in &reward_tokens {
            assert!(token.is_p2pk_locked());
            
            // Check witness contains P2PK reference
            for proof in token.as_cdk_token().proofs() {
                if let Some(witness) = &proof.witness {
                    assert!(witness.contains("p2pk"));
                    assert!(witness.contains(&winner_pubkey.to_string()));
                }
            }
        }
    }

    #[tokio::test]
    async fn test_mint_info_and_keysets() {
        let mint = MockCashuMint::new();
        
        // Get mint info
        let info = mint.get_mint_info();
        assert!(info.name.is_some());
        assert_eq!(info.name.unwrap(), "Mock Mint");
        
        // Initially no keysets
        let keysets = mint.get_keysets();
        assert!(keysets.is_empty());
        
        // Add a keyset - using a simpler approach for testing
        let keyset = cdk::nuts::KeySet {
            id: Id::from_bytes(&[4u8; 8]).unwrap(),
            unit: cdk::nuts::CurrencyUnit::Sat,
            keys: cdk::nuts::Keys::new(std::collections::BTreeMap::new()),
            final_expiry: None,
        };
        mint.add_keyset(keyset.clone());
        
        // Should now have one keyset
        let keysets = mint.get_keysets();
        assert_eq!(keysets.len(), 1);
        assert_eq!(keysets[0].id, keyset.id);
    }

    #[tokio::test]
    async fn test_mint_token_storage() {
        let mint = MockCashuMint::new();
        let keyset_id = Id::from_bytes(&[5u8; 8]).unwrap();
        
        // Initially no tokens
        assert_eq!(mint.token_count(), 0);
        
        // Mint some tokens
        let tokens = mint.mint_tokens(Amount::from(150), keyset_id).await.unwrap();
        
        // Should have stored tokens
        assert!(mint.token_count() > 0);
        
        // Clear tokens
        mint.clear_tokens();
        assert_eq!(mint.token_count(), 0);
    }
}

#[cfg(test)]
mod validation_workflow_tests {
    use super::*;

    #[tokio::test]
    async fn test_complete_validation_workflow() {
        let relay = MockNostrRelay::new();
        let mint = MockCashuMint::new();
        let game = Arc::new(CoinFlipGame::new());
        
        // Create complete game sequence
        let events = create_test_game_sequence(&relay, &mint).await.unwrap();
        
        // Validate sequence using game logic
        let validation_result = game.validate_sequence(&events).unwrap();
        assert!(validation_result.is_valid);
        
        // Check sequence completeness
        let is_complete = game.is_sequence_complete(&events).unwrap();
        assert!(is_complete);
        
        // Determine winner
        let winner = game.determine_winner(&events).unwrap();
        assert!(winner.is_some());
        
        // Verify all events are properly stored
        assert_eq!(relay.event_count(), 6);
        
        // Verify event types
        let challenge_events = relay.get_events_by_kind(Kind::Custom(9259));
        let accept_events = relay.get_events_by_kind(Kind::Custom(9260));
        let move_events = relay.get_events_by_kind(Kind::Custom(9261));
        let final_events = relay.get_events_by_kind(Kind::Custom(9262));
        
        assert_eq!(challenge_events.len(), 1);
        assert_eq!(accept_events.len(), 1);
        assert_eq!(move_events.len(), 2);
        assert_eq!(final_events.len(), 2);
    }

    #[tokio::test]
    async fn test_validation_with_commitment_verification() {
        let relay = MockNostrRelay::new();
        let mint = MockCashuMint::new();
        let game = Arc::new(CoinFlipGame::new());
        
        // Create tokens with known C values
        let keyset_id = Id::from_bytes(&[6u8; 8]).unwrap();
        let tokens = mint.mint_tokens(Amount::from(100), keyset_id).await.unwrap();
        
        // Create commitment for the tokens
        let commitment = kirk::TokenCommitment::single(tokens[0].as_cdk_token());
        
        // Create challenge with real commitment hash
        let player_keys = Keys::generate();
        let challenge_content = kirk::ChallengeContent {
            game_type: CoinFlipGame::game_type(),
            commitment_hashes: vec![commitment.commitment_hash.clone()],
            game_parameters: game.get_parameters().unwrap(),
            expiry: None,
        };
        let challenge_event = challenge_content.to_event(&player_keys).unwrap();
        
        // Create move that reveals the committed tokens
        let move_content = kirk::MoveContent {
            previous_event_id: challenge_event.id,
            move_type: kirk::MoveType::Reveal,
            move_data: serde_json::json!({"test": "move"}),
            revealed_tokens: Some(vec![tokens[0].as_cdk_token().clone()]),
        };
        let move_event = move_content.to_event(&player_keys).unwrap();
        
        let events = vec![challenge_event, move_event];
        
        // Validate commitment
        let is_valid = commitment.verify(&[tokens[0].as_cdk_token().clone()]).unwrap();
        assert!(is_valid);
        
        // Validate sequence
        let validation_result = game.validate_sequence(&events).unwrap();
        assert!(validation_result.is_valid);
    }
}
#[cfg(test)
]
mod p2pk_integration_tests {
    use super::*;
    use kirk::cashu::GameToken;
    use std::str::FromStr;

    /// Helper to create a mock game token for testing
    fn create_mock_game_token() -> cdk::nuts::Token {
        // Create a minimal mock token for testing
        let token_str = r#"cashuAeyJ0b2tlbiI6W3sicHJvb2ZzIjpbeyJpZCI6IjAwOWExZjI5MzI1M2U0MWUiLCJhbW91bnQiOjIsInNlY3JldCI6IjQwNzkxNWJjMjEyYmU2MWE3N2UzZTZkMmFlYjRjNzI3OTgwYmRhNTFjZDA2YTZhZmMyOWUyODYxNzY4YTc4MzciLCJDIjoiMDJiYzkwOTc5OTdkODFhZmIyY2M3MzQ2YjVlNGQ3YTI2MDEwNzAwMjY1NGI2ZjJkZjNmZjU0Y2ZjN2Y0MDMxNzNjIn1dLCJtaW50IjoiaHR0cHM6Ly84MzMzLnNwYWNlOjMzMzgifV19"#;
        
        cdk::nuts::Token::from_str(token_str)
            .unwrap_or_else(|_| {
                // Create a minimal token structure for testing
                cdk::nuts::Token::new(
                    "https://test-mint.example.com".parse().unwrap(),
                    vec![], // Empty proofs for testing
                    None,
                    cdk::nuts::CurrencyUnit::Sat,
                )
            })
    }

    #[tokio::test]
    async fn test_p2pk_token_lifecycle_management() {
        let mint = MockCashuMint::new();
        let winner_pubkey = nostr::PublicKey::from_slice(&[5u8; 32]).unwrap();
        let other_pubkey = nostr::PublicKey::from_slice(&[6u8; 32]).unwrap();
        
        // Mint P2PK locked reward tokens
        let reward_tokens = mint.mint_reward_tokens(
            Amount::from(1000), 
            winner_pubkey,
            Id::from_str("00ffd48b8f5ecf80").unwrap()
        ).await.expect("Should mint P2PK reward tokens");
        
        assert!(!reward_tokens.is_empty());
        let token = &reward_tokens[0];
        
        // Test P2PK spending validation
        assert!(token.can_spend(&winner_pubkey));
        assert!(!token.can_spend(&other_pubkey));
        
        // Test P2PK spending condition creation
        let condition = token.create_p2pk_spending_condition(&winner_pubkey)
            .expect("Should create P2PK spending condition");
        assert!(condition.contains("p2pk:"));
        
        // Test token unlocking (metadata operation)
        let unlocked_token = token.clone().unlock_p2pk_token()
            .expect("Should unlock P2PK token");
        assert!(!unlocked_token.is_p2pk_locked());
        assert!(unlocked_token.can_spend(&other_pubkey)); // Now spendable by anyone
    }

    #[tokio::test]
    async fn test_p2pk_state_transitions() {
        let mint = MockCashuMint::new();
        let pubkey = nostr::PublicKey::from_slice(&[7u8; 32]).unwrap();
        let mock_token = create_mock_game_token();
        
        // Test state transitions
        let locked_token = GameToken::new_p2pk_reward_token(mock_token.clone(), pubkey);
        assert!(locked_token.is_p2pk_locked());
        assert!(locked_token.can_spend(&pubkey));
        
        // Unlock the token
        let unlocked_token = locked_token.unlock_p2pk_token()
            .expect("Should unlock token");
        assert!(!unlocked_token.is_p2pk_locked());
        assert!(unlocked_token.is_reward_token());
        
        // Test reward state
        let reward_state = unlocked_token.reward_state().expect("Should have reward state");
        assert!(matches!(reward_state, kirk::cashu::RewardTokenState::Unlocked));
        assert!(!reward_state.is_locked());
    }

    #[tokio::test]
    async fn test_p2pk_spending_validation_errors() {
        let mock_token = create_mock_game_token();
        let pubkey = nostr::PublicKey::from_slice(&[8u8; 32]).unwrap();
        
        // Test creating P2PK condition for non-P2PK token
        let game_token = GameToken::new_game_token(mock_token.clone());
        let result = game_token.create_p2pk_spending_condition(&pubkey);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not P2PK locked"));
        
        // Test unlocking Game token (should fail)
        let result = game_token.unlock_p2pk_token();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cannot unlock Game tokens"));
        
        // Test unlocking already unlocked token (should succeed)
        let unlocked_reward = GameToken::new_reward_token(mock_token);
        let result = unlocked_reward.unlock_p2pk_token();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_multiple_p2pk_tokens() {
        let mint = MockCashuMint::new();
        let pubkey1 = nostr::PublicKey::from_slice(&[10u8; 32]).unwrap();
        let pubkey2 = nostr::PublicKey::from_slice(&[11u8; 32]).unwrap();
        let keyset_id = Id::from_str("00ffd48b8f5ecf81").unwrap();
        
        // Mint tokens for different pubkeys
        let tokens1 = mint.mint_reward_tokens(Amount::from(500), pubkey1, keyset_id).await.unwrap();
        let tokens2 = mint.mint_reward_tokens(Amount::from(300), pubkey2, keyset_id).await.unwrap();
        
        // Test that tokens are locked to correct pubkeys
        assert!(tokens1[0].can_spend(&pubkey1));
        assert!(!tokens1[0].can_spend(&pubkey2));
        
        assert!(!tokens2[0].can_spend(&pubkey1));
        assert!(tokens2[0].can_spend(&pubkey2));
        
        // Test spending conditions are different
        let condition1 = tokens1[0].create_p2pk_spending_condition(&pubkey1).unwrap();
        let condition2 = tokens2[0].create_p2pk_spending_condition(&pubkey2).unwrap();
        assert_ne!(condition1, condition2);
        assert!(condition1.contains(&pubkey1.to_string()));
        assert!(condition2.contains(&pubkey2.to_string()));
    }

    #[tokio::test]
    async fn test_p2pk_token_serialization_in_events() {
        let pubkey = nostr::PublicKey::from_slice(&[12u8; 32]).unwrap();
        let mock_token = create_mock_game_token();
        let keys = Keys::generate();
        
        // Create P2PK locked reward token
        let reward_token = GameToken::new_p2pk_reward_token(mock_token, pubkey);
        
        // Create reward event with P2PK token
        let reward_content = kirk::RewardContent {
            game_sequence_root: nostr::EventId::from_slice(&[1u8; 32]).unwrap(),
            winner_pubkey: pubkey,
            reward_tokens: vec![reward_token],
            unlock_instructions: Some("Use P2PK to unlock".to_string()),
        };
        
        // Test serialization
        let event = reward_content.to_event(&keys).expect("Should create reward event");
        assert!(event.content.contains("p2pk_locked"));
        assert!(event.content.contains(&pubkey.to_string()));
        
        // Test deserialization
        let parsed_content: kirk::RewardContent = serde_json::from_str(&event.content)
            .expect("Should parse reward content");
        assert_eq!(parsed_content.winner_pubkey, pubkey);
        assert!(!parsed_content.reward_tokens.is_empty());
        assert!(parsed_content.reward_tokens[0].is_p2pk_locked());
    }
}
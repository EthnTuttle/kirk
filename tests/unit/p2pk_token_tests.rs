//! Tests for P2PK token lifecycle management

use kirk::cashu::{GameToken, GameTokenType, RewardTokenState};
use kirk::error::GameProtocolError;
use nostr::PublicKey;
use std::str::FromStr;
use cashu::nuts::Token as CashuToken;

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

/// Create a mock Cashu token for testing
fn create_mock_token() -> CashuToken {
    // Create a minimal mock token
    // In a real implementation, this would be a proper CDK token
    let token_str = r#"cashuAeyJ0b2tlbiI6W3sicHJvb2ZzIjpbeyJpZCI6IjAwOWExZjI5MzI1M2U0MWUiLCJhbW91bnQiOjIsInNlY3JldCI6IjQwNzkxNWJjMjEyYmU2MWE3N2UzZTZkMmFlYjRjNzI3OTgwYmRhNTFjZDA2YTZhZmMyOWUyODYxNzY4YTc4MzciLCJDIjoiMDJiYzkwOTc5OTdkODFhZmIyY2M3MzQ2YjVlNGQ3YTI2MDEwNzAwMjY1NGI2ZjJkZjNmZjU0Y2ZjN2Y0MDMxNzNjIn1dLCJtaW50IjoiaHR0cHM6Ly84MzMzLnNwYWNlOjMzMzgifV19"#;
    
    CashuToken::from_str(token_str)
        .unwrap_or_else(|_| {
            // If parsing fails, create a minimal token structure
            // This is a fallback for testing purposes
            panic!("Failed to create mock token - would need proper CDK token construction")
        })
}

#[test]
fn test_reward_token_state_creation_and_properties() {
    let pubkey1 = create_test_pubkey(1);
    let pubkey2 = create_test_pubkey(2);
    
    // Test unlocked state
    let unlocked = RewardTokenState::create_unlocked();
    assert!(matches!(unlocked, RewardTokenState::Unlocked));
    assert!(!unlocked.is_locked());
    assert!(unlocked.can_spend(&pubkey1));
    assert!(unlocked.can_spend(&pubkey2));
    assert!(unlocked.locked_pubkey().is_none());
    
    // Test P2PK locked state
    let locked = RewardTokenState::create_p2pk_locked(pubkey1);
    assert!(matches!(locked, RewardTokenState::P2PKLocked { .. }));
    assert!(locked.is_locked());
    assert!(locked.can_spend(&pubkey1));
    assert!(!locked.can_spend(&pubkey2));
    assert_eq!(locked.locked_pubkey(), Some(&pubkey1));
}

#[test]
fn test_game_token_p2pk_spending_validation() {
    let pubkey1 = create_test_pubkey(10);
    let pubkey2 = create_test_pubkey(11);
    let mock_token = create_mock_token();
    
    // Test Game token (always spendable)
    let game_token = GameToken::new_game_token(mock_token.clone());
    assert!(game_token.can_spend(&pubkey1));
    assert!(game_token.can_spend(&pubkey2));
    
    // Test unlocked Reward token (spendable by anyone)
    let unlocked_reward = GameToken::new_reward_token(mock_token.clone());
    assert!(unlocked_reward.can_spend(&pubkey1));
    assert!(unlocked_reward.can_spend(&pubkey2));
    
    // Test P2PK locked Reward token (only spendable by locked pubkey)
    let locked_reward = GameToken::new_p2pk_reward_token(mock_token.clone(), pubkey1);
    assert!(locked_reward.can_spend(&pubkey1));
    assert!(!locked_reward.can_spend(&pubkey2));
}

#[test]
fn test_p2pk_token_unlocking() {
    let pubkey = create_test_pubkey(20);
    let mock_token = create_mock_token();
    
    // Test unlocking P2PK locked token
    let locked_token = GameToken::new_p2pk_reward_token(mock_token.clone(), pubkey);
    assert!(locked_token.is_p2pk_locked());
    
    let unlocked_token = locked_token.unlock_p2pk_token().expect("Should unlock P2PK token");
    assert!(!unlocked_token.is_p2pk_locked());
    assert!(unlocked_token.is_reward_token());
    
    // Test unlocking already unlocked token (should succeed)
    let already_unlocked = GameToken::new_reward_token(mock_token.clone());
    let still_unlocked = already_unlocked.unlock_p2pk_token().expect("Should handle already unlocked token");
    assert!(!still_unlocked.is_p2pk_locked());
    
    // Test unlocking Game token (should fail)
    let game_token = GameToken::new_game_token(mock_token);
    let result = game_token.unlock_p2pk_token();
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), GameProtocolError::InvalidToken(_)));
}

#[test]
fn test_p2pk_spending_condition_creation() {
    let pubkey = create_test_pubkey(30);
    let mock_token = create_mock_token();
    
    // Test creating spending condition for P2PK locked token
    let locked_token = GameToken::new_p2pk_reward_token(mock_token.clone(), pubkey);
    let condition = locked_token.create_p2pk_spending_condition(&pubkey)
        .expect("Should create P2PK spending condition");
    assert!(condition.contains("p2pk:"));
    assert!(condition.contains(&pubkey.to_string()));
    
    // Test creating spending condition for non-P2PK token (should fail)
    let unlocked_token = GameToken::new_reward_token(mock_token);
    let result = unlocked_token.create_p2pk_spending_condition(&pubkey);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), GameProtocolError::InvalidToken(_)));
}

#[test]
fn test_reward_token_state_transitions() {
    let pubkey = create_test_pubkey(40);
    
    // Test state transition from locked to unlocked
    let locked_state = RewardTokenState::create_p2pk_locked(pubkey);
    assert!(locked_state.is_locked());
    assert_eq!(locked_state.locked_pubkey(), Some(&pubkey));
    
    // Simulate unlocking by creating new unlocked state
    let unlocked_state = RewardTokenState::create_unlocked();
    assert!(!unlocked_state.is_locked());
    assert!(unlocked_state.locked_pubkey().is_none());
    
    // Test that states have correct spending permissions
    assert!(locked_state.can_spend(&pubkey));
    assert!(unlocked_state.can_spend(&pubkey));
    
    let other_pubkey = create_test_pubkey(41);
    assert!(!locked_state.can_spend(&other_pubkey));
    assert!(unlocked_state.can_spend(&other_pubkey));
}

#[test]
fn test_game_token_type_consistency() {
    let pubkey = create_test_pubkey(50);
    let mock_token = create_mock_token();
    
    // Test Game token properties
    let game_token = GameToken::new_game_token(mock_token.clone());
    assert!(game_token.is_game_token());
    assert!(!game_token.is_reward_token());
    assert!(!game_token.is_p2pk_locked());
    assert!(game_token.reward_state().is_none());
    
    // Test unlocked Reward token properties
    let unlocked_reward = GameToken::new_reward_token(mock_token.clone());
    assert!(!unlocked_reward.is_game_token());
    assert!(unlocked_reward.is_reward_token());
    assert!(!unlocked_reward.is_p2pk_locked());
    let reward_state = unlocked_reward.reward_state().expect("Should have reward state");
    assert!(matches!(reward_state, RewardTokenState::Unlocked));
    
    // Test P2PK locked Reward token properties
    let locked_reward = GameToken::new_p2pk_reward_token(mock_token, pubkey);
    assert!(!locked_reward.is_game_token());
    assert!(locked_reward.is_reward_token());
    assert!(locked_reward.is_p2pk_locked());
    let locked_state = locked_reward.reward_state().expect("Should have reward state");
    assert!(matches!(locked_state, RewardTokenState::P2PKLocked { .. }));
    assert_eq!(locked_state.locked_pubkey(), Some(&pubkey));
}

#[test]
fn test_p2pk_token_serialization_roundtrip() {
    let pubkey = create_test_pubkey(60);
    
    // Test RewardTokenState serialization
    let locked_state = RewardTokenState::create_p2pk_locked(pubkey);
    let serialized = serde_json::to_string(&locked_state).expect("Should serialize");
    let deserialized: RewardTokenState = serde_json::from_str(&serialized).expect("Should deserialize");
    
    assert!(deserialized.is_locked());
    assert_eq!(deserialized.locked_pubkey(), Some(&pubkey));
    assert!(deserialized.can_spend(&pubkey));
    
    // Test GameTokenType serialization
    let token_type = GameTokenType::Reward { p2pk_locked: Some(pubkey) };
    let serialized = serde_json::to_string(&token_type).expect("Should serialize");
    let deserialized: GameTokenType = serde_json::from_str(&serialized).expect("Should deserialize");
    
    assert!(matches!(deserialized, GameTokenType::Reward { p2pk_locked: Some(pk) } if pk == pubkey));
}

#[test]
fn test_multiple_pubkey_scenarios() {
    let pubkey1 = create_test_pubkey(70);
    let pubkey2 = create_test_pubkey(71);
    let pubkey3 = create_test_pubkey(72);
    let mock_token = create_mock_token();
    
    // Create tokens locked to different pubkeys
    let token1 = GameToken::new_p2pk_reward_token(mock_token.clone(), pubkey1);
    let token2 = GameToken::new_p2pk_reward_token(mock_token.clone(), pubkey2);
    let token3 = GameToken::new_p2pk_reward_token(mock_token, pubkey3);
    
    // Test that each token can only be spent by its respective pubkey
    assert!(token1.can_spend(&pubkey1));
    assert!(!token1.can_spend(&pubkey2));
    assert!(!token1.can_spend(&pubkey3));
    
    assert!(!token2.can_spend(&pubkey1));
    assert!(token2.can_spend(&pubkey2));
    assert!(!token2.can_spend(&pubkey3));
    
    assert!(!token3.can_spend(&pubkey1));
    assert!(!token3.can_spend(&pubkey2));
    assert!(token3.can_spend(&pubkey3));
    
    // Test spending condition creation
    let condition1 = token1.create_p2pk_spending_condition(&pubkey1).expect("Should create condition");
    let condition2 = token2.create_p2pk_spending_condition(&pubkey2).expect("Should create condition");
    
    assert!(condition1.contains(&pubkey1.to_string()));
    assert!(condition2.contains(&pubkey2.to_string()));
    assert_ne!(condition1, condition2);
}
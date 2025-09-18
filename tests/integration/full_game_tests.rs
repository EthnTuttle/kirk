//! End-to-end integration tests for complete game sequences

use kirk::{
    PlayerClient, GameToken, GameTokenType, ChallengeContent, ChallengeAcceptContent, 
    MoveContent, FinalContent, MoveType, CommitmentMethod, Game
};
use tests::mocks::{MockNostrRelay, MockCashuMint, CoinFlipGame, reference_game::{CoinFlipMove, CoinSide}};
use nostr::{Keys, EventId, Kind};
use nostr_sdk::Client as NostrClient;
use cdk::{Amount, nuts::Id};
use std::sync::Arc;
use tokio;

/// Helper to set up test environment
async fn setup_test_environment() -> (MockNostrRelay, MockCashuMint, Keys, Keys) {
    let relay = MockNostrRelay::new();
    let mint = MockCashuMint::new();
    let player1_keys = Keys::generate();
    let player2_keys = Keys::generate();
    
    (relay, mint, player1_keys, player2_keys)
}

/// Helper to create test tokens
async fn create_test_tokens(mint: &MockCashuMint, amount: u64) -> Vec<GameToken> {
    let keyset_id = Id::from_bytes(&[0u8; 8]).unwrap();
    mint.mint_tokens(Amount::from(amount), keyset_id).await.unwrap()
}

#[cfg(test)]
mod end_to_end_game_tests {
    use super::*;

    #[tokio::test]
    async fn test_complete_coin_flip_game_sequence() {
        let (relay, mint, player1_keys, player2_keys) = setup_test_environment().await;
        let game = CoinFlipGame::new();
        
        // Step 1: Players mint tokens
        let player1_tokens = create_test_tokens(&mint, 100).await;
        let player2_tokens = create_test_tokens(&mint, 100).await;
        
        assert_eq!(player1_tokens.len(), 1);
        assert_eq!(player2_tokens.len(), 1);
        
        // Step 2: Player 1 creates challenge
        let challenge_content = ChallengeContent {
            game_type: CoinFlipGame::game_type(),
            commitment_hashes: vec!["player1_commitment_hash".to_string()],
            game_parameters: game.get_parameters().unwrap(),
            expiry: Some(chrono::Utc::now().timestamp() as u64 + 3600),
        };
        
        let challenge_event = challenge_content.to_event(&player1_keys).unwrap();
        relay.store_event(challenge_event.clone()).unwrap();
        
        // Step 3: Player 2 accepts challenge
        let accept_content = ChallengeAcceptContent {
            challenge_id: challenge_event.id,
            commitment_hashes: vec!["player2_commitment_hash".to_string()],
        };
        
        let accept_event = accept_content.to_event(&player2_keys).unwrap();
        relay.store_event(accept_event.clone()).unwrap();
        
        // Step 4: Players make moves
        let player1_move = CoinFlipMove {
            chosen_side: CoinSide::Heads,
            confidence: 150,
        };
        
        let move1_content = MoveContent {
            previous_event_id: accept_event.id,
            move_type: MoveType::Reveal,
            move_data: serde_json::to_value(&player1_move).unwrap(),
            revealed_tokens: Some(vec![player1_tokens[0].as_cdk_token().clone()]),
        };
        
        let move1_event = move1_content.to_event(&player1_keys).unwrap();
        relay.store_event(move1_event.clone()).unwrap();
        
        let player2_move = CoinFlipMove {
            chosen_side: CoinSide::Tails,
            confidence: 120,
        };
        
        let move2_content = MoveContent {
            previous_event_id: move1_event.id,
            move_type: MoveType::Reveal,
            move_data: serde_json::to_value(&player2_move).unwrap(),
            revealed_tokens: Some(vec![player2_tokens[0].as_cdk_token().clone()]),
        };
        
        let move2_event = move2_content.to_event(&player2_keys).unwrap();
        relay.store_event(move2_event.clone()).unwrap();
        
        // Step 5: Players publish final events
        let final1_content = FinalContent {
            game_sequence_root: challenge_event.id,
            commitment_method: None, // Single token, no method needed
            final_state: serde_json::json!({"player": "player1", "complete": true}),
        };
        
        let final1_event = final1_content.to_event(&player1_keys).unwrap();
        relay.store_event(final1_event.clone()).unwrap();
        
        let final2_content = FinalContent {
            game_sequence_root: challenge_event.id,
            commitment_method: None,
            final_state: serde_json::json!({"player": "player2", "complete": true}),
        };
        
        let final2_event = final2_content.to_event(&player2_keys).unwrap();
        relay.store_event(final2_event.clone()).unwrap();
        
        // Step 6: Validate complete sequence
        let all_events = vec![
            challenge_event, accept_event, move1_event, move2_event, final1_event, final2_event
        ];
        
        // Check sequence completeness
        let is_complete = game.is_sequence_complete(&all_events).unwrap();
        assert!(is_complete);
        
        // Validate sequence
        let validation_result = game.validate_sequence(&all_events).unwrap();
        assert!(validation_result.is_valid);
        
        // Verify all events are stored in relay
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
    async fn test_game_sequence_with_commit_reveal() {
        let (relay, mint, player1_keys, player2_keys) = setup_test_environment().await;
        let game = CoinFlipGame::new();
        
        // Create tokens
        let player1_tokens = create_test_tokens(&mint, 50).await;
        let player2_tokens = create_test_tokens(&mint, 50).await;
        
        // Challenge and accept (abbreviated)
        let challenge_content = ChallengeContent {
            game_type: CoinFlipGame::game_type(),
            commitment_hashes: vec!["p1_hash".to_string()],
            game_parameters: game.get_parameters().unwrap(),
            expiry: None,
        };
        let challenge_event = challenge_content.to_event(&player1_keys).unwrap();
        relay.store_event(challenge_event.clone()).unwrap();
        
        let accept_content = ChallengeAcceptContent {
            challenge_id: challenge_event.id,
            commitment_hashes: vec!["p2_hash".to_string()],
        };
        let accept_event = accept_content.to_event(&player2_keys).unwrap();
        relay.store_event(accept_event.clone()).unwrap();
        
        // Commit phase - players commit to moves without revealing
        let commit1_content = MoveContent {
            previous_event_id: accept_event.id,
            move_type: MoveType::Commit,
            move_data: serde_json::json!({"commitment": "hidden_move_1"}),
            revealed_tokens: None,
        };
        let commit1_event = commit1_content.to_event(&player1_keys).unwrap();
        relay.store_event(commit1_event.clone()).unwrap();
        
        let commit2_content = MoveContent {
            previous_event_id: commit1_event.id,
            move_type: MoveType::Commit,
            move_data: serde_json::json!({"commitment": "hidden_move_2"}),
            revealed_tokens: None,
        };
        let commit2_event = commit2_content.to_event(&player2_keys).unwrap();
        relay.store_event(commit2_event.clone()).unwrap();
        
        // Reveal phase - players reveal their actual moves and tokens
        let reveal1_move = CoinFlipMove {
            chosen_side: CoinSide::Heads,
            confidence: 180,
        };
        let reveal1_content = MoveContent {
            previous_event_id: commit2_event.id,
            move_type: MoveType::Reveal,
            move_data: serde_json::to_value(&reveal1_move).unwrap(),
            revealed_tokens: Some(vec![player1_tokens[0].as_cdk_token().clone()]),
        };
        let reveal1_event = reveal1_content.to_event(&player1_keys).unwrap();
        relay.store_event(reveal1_event.clone()).unwrap();
        
        let reveal2_move = CoinFlipMove {
            chosen_side: CoinSide::Tails,
            confidence: 160,
        };
        let reveal2_content = MoveContent {
            previous_event_id: reveal1_event.id,
            move_type: MoveType::Reveal,
            move_data: serde_json::to_value(&reveal2_move).unwrap(),
            revealed_tokens: Some(vec![player2_tokens[0].as_cdk_token().clone()]),
        };
        let reveal2_event = reveal2_content.to_event(&player2_keys).unwrap();
        relay.store_event(reveal2_event.clone()).unwrap();
        
        // Final events
        let final1_content = FinalContent {
            game_sequence_root: challenge_event.id,
            commitment_method: None,
            final_state: serde_json::json!({"phase": "reveal_complete"}),
        };
        let final1_event = final1_content.to_event(&player1_keys).unwrap();
        relay.store_event(final1_event.clone()).unwrap();
        
        let final2_content = FinalContent {
            game_sequence_root: challenge_event.id,
            commitment_method: None,
            final_state: serde_json::json!({"phase": "reveal_complete"}),
        };
        let final2_event = final2_content.to_event(&player2_keys).unwrap();
        relay.store_event(final2_event.clone()).unwrap();
        
        // Validate the commit-reveal sequence
        let all_events = vec![
            challenge_event, accept_event, 
            commit1_event, commit2_event,
            reveal1_event, reveal2_event,
            final1_event, final2_event
        ];
        
        let is_complete = game.is_sequence_complete(&all_events).unwrap();
        assert!(is_complete);
        
        let validation_result = game.validate_sequence(&all_events).unwrap();
        assert!(validation_result.is_valid);
        
        // Verify we have both commit and reveal moves
        let move_events = relay.get_events_by_kind(Kind::Custom(9261));
        assert_eq!(move_events.len(), 4); // 2 commits + 2 reveals
    }

    #[tokio::test]
    async fn test_game_sequence_validation_failure() {
        let (relay, mint, player1_keys, player2_keys) = setup_test_environment().await;
        let game = CoinFlipGame::new();
        
        // Create a sequence with invalid move content
        let challenge_content = ChallengeContent {
            game_type: CoinFlipGame::game_type(),
            commitment_hashes: vec!["hash".to_string()],
            game_parameters: game.get_parameters().unwrap(),
            expiry: None,
        };
        let challenge_event = challenge_content.to_event(&player1_keys).unwrap();
        relay.store_event(challenge_event.clone()).unwrap();
        
        // Create move with invalid JSON content
        let invalid_move_event = nostr::EventBuilder::new(
            Kind::Custom(9261), 
            "invalid json content", 
            Vec::<nostr::Tag>::new()
        ).to_event(&player1_keys).unwrap();
        relay.store_event(invalid_move_event.clone()).unwrap();
        
        let events = vec![challenge_event, invalid_move_event];
        
        // Validation should fail
        let validation_result = game.validate_sequence(&events).unwrap();
        assert!(!validation_result.is_valid);
        assert!(!validation_result.errors.is_empty());
        
        // Should not be complete
        let is_complete = game.is_sequence_complete(&events).unwrap();
        assert!(!is_complete);
    }

    #[tokio::test]
    async fn test_multi_token_commitment_game() {
        let (relay, mint, player1_keys, player2_keys) = setup_test_environment().await;
        let game = CoinFlipGame::new();
        
        // Create multiple tokens per player
        let player1_tokens = vec![
            create_test_tokens(&mint, 25).await[0].clone(),
            create_test_tokens(&mint, 25).await[0].clone(),
        ];
        let player2_tokens = vec![
            create_test_tokens(&mint, 30).await[0].clone(),
            create_test_tokens(&mint, 20).await[0].clone(),
        ];
        
        // Challenge with multiple token commitment
        let challenge_content = ChallengeContent {
            game_type: CoinFlipGame::game_type(),
            commitment_hashes: vec!["multi_token_hash_p1".to_string()],
            game_parameters: game.get_parameters().unwrap(),
            expiry: None,
        };
        let challenge_event = challenge_content.to_event(&player1_keys).unwrap();
        relay.store_event(challenge_event.clone()).unwrap();
        
        let accept_content = ChallengeAcceptContent {
            challenge_id: challenge_event.id,
            commitment_hashes: vec!["multi_token_hash_p2".to_string()],
        };
        let accept_event = accept_content.to_event(&player2_keys).unwrap();
        relay.store_event(accept_event.clone()).unwrap();
        
        // Moves revealing multiple tokens
        let move1_content = MoveContent {
            previous_event_id: accept_event.id,
            move_type: MoveType::Reveal,
            move_data: serde_json::to_value(&CoinFlipMove {
                chosen_side: CoinSide::Heads,
                confidence: 200,
            }).unwrap(),
            revealed_tokens: Some(player1_tokens.iter().map(|t| t.as_cdk_token().clone()).collect()),
        };
        let move1_event = move1_content.to_event(&player1_keys).unwrap();
        relay.store_event(move1_event.clone()).unwrap();
        
        let move2_content = MoveContent {
            previous_event_id: move1_event.id,
            move_type: MoveType::Reveal,
            move_data: serde_json::to_value(&CoinFlipMove {
                chosen_side: CoinSide::Tails,
                confidence: 180,
            }).unwrap(),
            revealed_tokens: Some(player2_tokens.iter().map(|t| t.as_cdk_token().clone()).collect()),
        };
        let move2_event = move2_content.to_event(&player2_keys).unwrap();
        relay.store_event(move2_event.clone()).unwrap();
        
        // Final events with commitment method specification
        let final1_content = FinalContent {
            game_sequence_root: challenge_event.id,
            commitment_method: Some(CommitmentMethod::MerkleTreeRadix4),
            final_state: serde_json::json!({"tokens_revealed": 2}),
        };
        let final1_event = final1_content.to_event(&player1_keys).unwrap();
        relay.store_event(final1_event.clone()).unwrap();
        
        let final2_content = FinalContent {
            game_sequence_root: challenge_event.id,
            commitment_method: Some(CommitmentMethod::MerkleTreeRadix4),
            final_state: serde_json::json!({"tokens_revealed": 2}),
        };
        let final2_event = final2_content.to_event(&player2_keys).unwrap();
        relay.store_event(final2_event.clone()).unwrap();
        
        let all_events = vec![
            challenge_event, accept_event, move1_event, move2_event, final1_event, final2_event
        ];
        
        // Should be complete and valid
        let is_complete = game.is_sequence_complete(&all_events).unwrap();
        assert!(is_complete);
        
        let validation_result = game.validate_sequence(&all_events).unwrap();
        assert!(validation_result.is_valid);
    }
}

#[cfg(test)]
mod game_flow_edge_cases {
    use super::*;

    #[tokio::test]
    async fn test_incomplete_sequence_missing_final() {
        let (relay, mint, player1_keys, player2_keys) = setup_test_environment().await;
        let game = CoinFlipGame::new();
        
        let tokens1 = create_test_tokens(&mint, 100).await;
        let tokens2 = create_test_tokens(&mint, 100).await;
        
        // Complete sequence except missing one final event
        let challenge_content = ChallengeContent {
            game_type: CoinFlipGame::game_type(),
            commitment_hashes: vec!["hash1".to_string()],
            game_parameters: game.get_parameters().unwrap(),
            expiry: None,
        };
        let challenge_event = challenge_content.to_event(&player1_keys).unwrap();
        
        let accept_content = ChallengeAcceptContent {
            challenge_id: challenge_event.id,
            commitment_hashes: vec!["hash2".to_string()],
        };
        let accept_event = accept_content.to_event(&player2_keys).unwrap();
        
        let move1_content = MoveContent {
            previous_event_id: accept_event.id,
            move_type: MoveType::Reveal,
            move_data: serde_json::to_value(&CoinFlipMove {
                chosen_side: CoinSide::Heads,
                confidence: 100,
            }).unwrap(),
            revealed_tokens: Some(vec![tokens1[0].as_cdk_token().clone()]),
        };
        let move1_event = move1_content.to_event(&player1_keys).unwrap();
        
        let move2_content = MoveContent {
            previous_event_id: move1_event.id,
            move_type: MoveType::Reveal,
            move_data: serde_json::to_value(&CoinFlipMove {
                chosen_side: CoinSide::Tails,
                confidence: 100,
            }).unwrap(),
            revealed_tokens: Some(vec![tokens2[0].as_cdk_token().clone()]),
        };
        let move2_event = move2_content.to_event(&player2_keys).unwrap();
        
        // Only one final event (need 2 for coin flip game)
        let final1_content = FinalContent {
            game_sequence_root: challenge_event.id,
            commitment_method: None,
            final_state: serde_json::json!({"complete": true}),
        };
        let final1_event = final1_content.to_event(&player1_keys).unwrap();
        
        let incomplete_events = vec![
            challenge_event, accept_event, move1_event, move2_event, final1_event
        ];
        
        // Should not be complete (missing second final event)
        let is_complete = game.is_sequence_complete(&incomplete_events).unwrap();
        assert!(!is_complete);
        
        // But should still be valid so far
        let validation_result = game.validate_sequence(&incomplete_events).unwrap();
        assert!(validation_result.is_valid);
    }

    #[tokio::test]
    async fn test_sequence_with_wrong_game_type() {
        let (relay, mint, player1_keys, _player2_keys) = setup_test_environment().await;
        let game = CoinFlipGame::new();
        
        // Challenge with wrong game type
        let challenge_content = ChallengeContent {
            game_type: "wrong_game_type".to_string(),
            commitment_hashes: vec!["hash".to_string()],
            game_parameters: serde_json::json!({}),
            expiry: None,
        };
        let challenge_event = challenge_content.to_event(&player1_keys).unwrap();
        
        let events = vec![challenge_event];
        
        // Game should still validate (it doesn't check game type in validation)
        let validation_result = game.validate_sequence(&events).unwrap();
        assert!(validation_result.is_valid);
        
        // But sequence is not complete
        let is_complete = game.is_sequence_complete(&events).unwrap();
        assert!(!is_complete);
    }

    #[tokio::test]
    async fn test_sequence_with_expired_challenge() {
        let (relay, mint, player1_keys, player2_keys) = setup_test_environment().await;
        let game = CoinFlipGame::new();
        
        // Challenge with past expiry
        let past_expiry = chrono::Utc::now().timestamp() as u64 - 3600; // 1 hour ago
        let challenge_content = ChallengeContent {
            game_type: CoinFlipGame::game_type(),
            commitment_hashes: vec!["hash".to_string()],
            game_parameters: game.get_parameters().unwrap(),
            expiry: Some(past_expiry),
        };
        let challenge_event = challenge_content.to_event(&player1_keys).unwrap();
        
        let accept_content = ChallengeAcceptContent {
            challenge_id: challenge_event.id,
            commitment_hashes: vec!["hash2".to_string()],
        };
        let accept_event = accept_content.to_event(&player2_keys).unwrap();
        
        let events = vec![challenge_event, accept_event];
        
        // Current implementation doesn't check expiry in validation
        // This would be handled by the client or mint logic
        let validation_result = game.validate_sequence(&events).unwrap();
        assert!(validation_result.is_valid);
    }
}
//! Unit tests for event types and builders

#[cfg(test)]
mod tests {

    use crate::events::{
        ChallengeContent, ChallengeAcceptContent, MoveContent, MoveType,
        FinalContent, CommitmentMethod, RewardContent, ValidationFailureContent,
        EventParser, validate_event_structure,
        CHALLENGE_KIND, CHALLENGE_ACCEPT_KIND, MOVE_KIND, FINAL_KIND, REWARD_KIND
    };
    use nostr::{Keys, EventId};
    use serde_json::json;
    use crate::cashu::{GameToken, GameTokenType};
    use cdk::nuts::Token as CashuToken;
    use std::str::FromStr;

    fn create_test_keys() -> Keys {
        Keys::generate()
    }

    fn create_dummy_cashu_token() -> CashuToken {
        // This is a placeholder - actual CDK integration will be in later tasks
        // For testing purposes, we'll create a token from a known valid token string
        // This is just for testing event serialization, not actual CDK functionality
        let token_str = r#"cashuAeyJ0b2tlbiI6W3sibWludCI6Imh0dHBzOi8vODMzMy5zcGFjZTozMzM4IiwicHJvb2ZzIjpbeyJhbW91bnQiOjIsImlkIjoiMDA5YTFmMjkzMjUzZTQxZSIsInNlY3JldCI6IjQwNzkxNWJjMjEyYmU2MWE3N2UzZTZkMmFlYjRjNzI3OTgwYmRhNTFjZDA2YTZhZmMyOWUyODYxNzY4YTc4MzciLCJDIjoiMDJiYzkwOTc5OTdkODFhZmIyY2M3MzQ2YjVlNGQ3YTcyMmQ1YTc4MzNhNGI3YzM4NWRlNGZiMGI2MzU5NjM0ZTJhIn1dfV0sInVuaXQiOiJzYXQiLCJtZW1vIjoiVGhhbmsgeW91LiJ9"#;
        
        // Try to parse the token, if it fails, create a minimal structure
        match CashuToken::from_str(token_str) {
            Ok(token) => token,
            Err(_) => {
                // If parsing fails, we'll create a very simple mock for testing
                // This should not happen in real usage, but for testing event serialization it's fine
                serde_json::from_str(r#"{"token":[{"mint":"https://mint.example.com","proofs":[]}]}"#)
                    .unwrap_or_else(|_| {
                        // Last resort: create an empty token structure
                        // This is just for testing event builders, not actual token functionality
                        panic!("Cannot create test token - CDK integration will be implemented in later tasks")
                    })
            }
        }
    }

    fn create_test_game_token() -> GameToken {
        GameToken::from_cdk_token(
            create_dummy_cashu_token(),
            GameTokenType::Reward { p2pk_locked: Some(create_test_keys().public_key()) }
        )
    }

    #[test]
    fn test_challenge_content_creation() {
        let keys = create_test_keys();
        let challenge = ChallengeContent {
            game_type: "test_game".to_string(),
            commitment_hashes: vec!["a".repeat(64)],
            game_parameters: json!({"param1": "value1"}),
            expiry: Some(chrono::Utc::now().timestamp() as u64 + 3600),
        };

        let event = challenge.to_event(&keys).unwrap();
        assert_eq!(event.kind, CHALLENGE_KIND);
        assert!(event.verify().is_ok());
    }

    #[test]
    fn test_challenge_content_validation() {
        let mut challenge = ChallengeContent {
            game_type: "test_game".to_string(),
            commitment_hashes: vec!["a".repeat(64)],
            game_parameters: json!({}),
            expiry: Some(chrono::Utc::now().timestamp() as u64 + 3600),
        };

        // Valid challenge should pass
        assert!(challenge.validate().is_ok());

        // Empty game type should fail
        challenge.game_type = "".to_string();
        assert!(challenge.validate().is_err());
        challenge.game_type = "test_game".to_string();

        // Empty commitment hashes should fail
        challenge.commitment_hashes = vec![];
        assert!(challenge.validate().is_err());
        challenge.commitment_hashes = vec!["a".repeat(64)];

        // Invalid hex format should fail
        challenge.commitment_hashes = vec!["invalid_hex".to_string()];
        assert!(challenge.validate().is_err());
        challenge.commitment_hashes = vec!["a".repeat(64)];

        // Wrong length should fail
        challenge.commitment_hashes = vec!["a".repeat(32)];
        assert!(challenge.validate().is_err());
        challenge.commitment_hashes = vec!["a".repeat(64)];

        // Past expiry should fail
        challenge.expiry = Some(chrono::Utc::now().timestamp() as u64 - 3600);
        assert!(challenge.validate().is_err());
    }

    #[test]
    fn test_challenge_accept_content_creation() {
        let keys = create_test_keys();
        let challenge_id = EventId::from_hex("a".repeat(64)).unwrap();
        let accept = ChallengeAcceptContent {
            challenge_id,
            commitment_hashes: vec!["b".repeat(64)],
        };

        let event = accept.to_event(&keys).unwrap();
        assert_eq!(event.kind, CHALLENGE_ACCEPT_KIND);
        assert!(event.verify().is_ok());
    }

    #[test]
    fn test_challenge_accept_validation() {
        let challenge_id = EventId::from_hex("a".repeat(64)).unwrap();
        let mut accept = ChallengeAcceptContent {
            challenge_id,
            commitment_hashes: vec!["b".repeat(64)],
        };

        // Valid accept should pass
        assert!(accept.validate().is_ok());

        // Empty commitment hashes should fail
        accept.commitment_hashes = vec![];
        assert!(accept.validate().is_err());

        // Invalid hex format should fail
        accept.commitment_hashes = vec!["invalid_hex".to_string()];
        assert!(accept.validate().is_err());
    }

    #[test]
    fn test_move_content_creation() {
        let keys = create_test_keys();
        let previous_event_id = EventId::from_hex("a".repeat(64)).unwrap();
        let move_content = MoveContent {
            previous_event_id,
            move_type: MoveType::Move,
            move_data: json!({"action": "test"}),
            revealed_tokens: None,
        };

        let event = move_content.to_event(&keys).unwrap();
        assert_eq!(event.kind, MOVE_KIND);
        assert!(event.verify().is_ok());
    }

    #[test]
    fn test_move_content_validation() {
        let previous_event_id = EventId::from_hex("a".repeat(64)).unwrap();
        
        // Reveal move without tokens should fail
        let reveal_move = MoveContent {
            previous_event_id,
            move_type: MoveType::Reveal,
            move_data: json!({}),
            revealed_tokens: None,
        };
        assert!(reveal_move.validate().is_err());

        // Reveal move with tokens should pass
        let reveal_move = MoveContent {
            previous_event_id,
            move_type: MoveType::Reveal,
            move_data: json!({}),
            revealed_tokens: Some(vec![create_dummy_cashu_token()]),
        };
        assert!(reveal_move.validate().is_ok());

        // Commit move with tokens should fail
        let commit_move = MoveContent {
            previous_event_id,
            move_type: MoveType::Commit,
            move_data: json!({}),
            revealed_tokens: Some(vec![create_dummy_cashu_token()]),
        };
        assert!(commit_move.validate().is_err());

        // Commit move without tokens should pass
        let commit_move = MoveContent {
            previous_event_id,
            move_type: MoveType::Commit,
            move_data: json!({}),
            revealed_tokens: None,
        };
        assert!(commit_move.validate().is_ok());

        // Move with empty tokens should fail
        let move_with_empty_tokens = MoveContent {
            previous_event_id,
            move_type: MoveType::Move,
            move_data: json!({}),
            revealed_tokens: Some(vec![]),
        };
        assert!(move_with_empty_tokens.validate().is_err());
    }

    #[test]
    fn test_final_content_creation() {
        let keys = create_test_keys();
        let game_sequence_root = EventId::from_hex("a".repeat(64)).unwrap();
        let final_content = FinalContent {
            game_sequence_root,
            commitment_method: Some(CommitmentMethod::Concatenation),
            final_state: json!({"winner": "player1"}),
        };

        let event = final_content.to_event(&keys).unwrap();
        assert_eq!(event.kind, FINAL_KIND);
        assert!(event.verify().is_ok());
    }

    #[test]
    fn test_final_content_validation() {
        let game_sequence_root = EventId::from_hex("a".repeat(64)).unwrap();
        
        let final_content = FinalContent {
            game_sequence_root,
            commitment_method: Some(CommitmentMethod::MerkleTreeRadix4),
            final_state: json!({}),
        };

        // Should always pass validation for now
        assert!(final_content.validate().is_ok());
    }

    #[test]
    fn test_reward_content_creation() {
        let keys = create_test_keys();
        let game_sequence_root = EventId::from_hex("a".repeat(64)).unwrap();
        let winner_pubkey = create_test_keys().public_key();
        let reward_content = RewardContent {
            game_sequence_root,
            winner_pubkey,
            reward_tokens: vec![create_test_game_token()],
            unlock_instructions: Some("Use NUT-11 P2PK".to_string()),
        };

        let event = reward_content.to_event(&keys).unwrap();
        assert_eq!(event.kind, REWARD_KIND);
        assert!(event.verify().is_ok());
    }

    #[test]
    fn test_reward_content_validation() {
        let game_sequence_root = EventId::from_hex("a".repeat(64)).unwrap();
        let winner_pubkey = create_test_keys().public_key();
        
        // Empty reward tokens should fail
        let reward_content = RewardContent {
            game_sequence_root,
            winner_pubkey,
            reward_tokens: vec![],
            unlock_instructions: None,
        };
        assert!(reward_content.validate().is_err());

        // Game token in reward should fail
        let game_token = GameToken::from_cdk_token(
            create_dummy_cashu_token(),
            GameTokenType::Game
        );
        let reward_content = RewardContent {
            game_sequence_root,
            winner_pubkey,
            reward_tokens: vec![game_token],
            unlock_instructions: None,
        };
        assert!(reward_content.validate().is_err());

        // Reward token should pass
        let reward_content = RewardContent {
            game_sequence_root,
            winner_pubkey,
            reward_tokens: vec![create_test_game_token()],
            unlock_instructions: None,
        };
        assert!(reward_content.validate().is_ok());
    }

    #[test]
    fn test_validation_failure_content_creation() {
        let keys = create_test_keys();
        let game_sequence_root = EventId::from_hex("a".repeat(64)).unwrap();
        let failure_content = ValidationFailureContent {
            game_sequence_root,
            failure_reason: "Invalid token".to_string(),
            failed_event_id: Some(EventId::from_hex("b".repeat(64)).unwrap()),
        };

        let event = failure_content.to_event(&keys).unwrap();
        assert_eq!(event.kind, REWARD_KIND);
        assert!(event.verify().is_ok());
    }

    #[test]
    fn test_validation_failure_validation() {
        let game_sequence_root = EventId::from_hex("a".repeat(64)).unwrap();
        
        // Empty failure reason should fail
        let failure_content = ValidationFailureContent {
            game_sequence_root,
            failure_reason: "".to_string(),
            failed_event_id: None,
        };
        assert!(failure_content.validate().is_err());

        // Valid failure reason should pass
        let failure_content = ValidationFailureContent {
            game_sequence_root,
            failure_reason: "Test failure".to_string(),
            failed_event_id: None,
        };
        assert!(failure_content.validate().is_ok());
    }

    #[test]
    fn test_event_parser_challenge() {
        let keys = create_test_keys();
        let challenge = ChallengeContent {
            game_type: "test_game".to_string(),
            commitment_hashes: vec!["a".repeat(64)],
            game_parameters: json!({"param1": "value1"}),
            expiry: None,
        };

        let event = challenge.to_event(&keys).unwrap();
        let parsed = EventParser::parse_challenge(&event).unwrap();
        
        assert_eq!(parsed.game_type, challenge.game_type);
        assert_eq!(parsed.commitment_hashes, challenge.commitment_hashes);
    }

    #[test]
    fn test_event_parser_wrong_kind() {
        let keys = create_test_keys();
        let challenge = ChallengeContent {
            game_type: "test_game".to_string(),
            commitment_hashes: vec!["a".repeat(64)],
            game_parameters: json!({}),
            expiry: None,
        };

        let event = challenge.to_event(&keys).unwrap();
        
        // Try to parse as wrong type
        assert!(EventParser::parse_move(&event).is_err());
        assert!(EventParser::parse_final(&event).is_err());
        assert!(EventParser::parse_reward(&event).is_err());
    }

    #[test]
    fn test_is_game_event() {
        let keys = create_test_keys();
        
        // Test game events
        let challenge = ChallengeContent {
            game_type: "test".to_string(),
            commitment_hashes: vec!["a".repeat(64)],
            game_parameters: json!({}),
            expiry: None,
        };
        let event = challenge.to_event(&keys).unwrap();
        assert!(EventParser::is_game_event(&event));

        // Test non-game event
        let non_game_event = nostr::EventBuilder::new(nostr::Kind::TextNote, "test", Vec::<nostr::Tag>::new())
            .to_event(&keys).unwrap();
        assert!(!EventParser::is_game_event(&non_game_event));
    }

    #[test]
    fn test_get_event_type_name() {
        let keys = create_test_keys();
        
        let challenge = ChallengeContent {
            game_type: "test".to_string(),
            commitment_hashes: vec!["a".repeat(64)],
            game_parameters: json!({}),
            expiry: None,
        };
        let event = challenge.to_event(&keys).unwrap();
        assert_eq!(EventParser::get_event_type_name(&event), Some("Challenge"));

        let non_game_event = nostr::EventBuilder::new(nostr::Kind::TextNote, "test", Vec::<nostr::Tag>::new())
            .to_event(&keys).unwrap();
        assert_eq!(EventParser::get_event_type_name(&non_game_event), None);
    }

    #[test]
    fn test_validate_event_structure() {
        let keys = create_test_keys();
        
        // Valid game event
        let challenge = ChallengeContent {
            game_type: "test".to_string(),
            commitment_hashes: vec!["a".repeat(64)],
            game_parameters: json!({}),
            expiry: None,
        };
        let event = challenge.to_event(&keys).unwrap();
        assert!(validate_event_structure(&event).is_ok());

        // Non-game event should fail
        let non_game_event = nostr::EventBuilder::new(nostr::Kind::TextNote, "test", Vec::<nostr::Tag>::new())
            .to_event(&keys).unwrap();
        assert!(validate_event_structure(&non_game_event).is_err());
    }

    #[test]
    fn test_event_serialization_roundtrip() {
        let keys = create_test_keys();
        
        // Test Challenge roundtrip
        let challenge = ChallengeContent {
            game_type: "test_game".to_string(),
            commitment_hashes: vec!["a".repeat(64), "b".repeat(64)],
            game_parameters: json!({"rounds": 3, "timeout": 300}),
            expiry: Some(1234567890),
        };
        
        let event = challenge.to_event(&keys).unwrap();
        let parsed = EventParser::parse_challenge(&event).unwrap();
        
        assert_eq!(challenge.game_type, parsed.game_type);
        assert_eq!(challenge.commitment_hashes, parsed.commitment_hashes);
        assert_eq!(challenge.game_parameters, parsed.game_parameters);
        assert_eq!(challenge.expiry, parsed.expiry);
    }

    #[test]
    fn test_move_type_serialization() {
        // Test that MoveType serializes correctly
        let move_type = MoveType::Commit;
        let serialized = serde_json::to_string(&move_type).unwrap();
        let deserialized: MoveType = serde_json::from_str(&serialized).unwrap();
        
        match deserialized {
            MoveType::Commit => {},
            _ => panic!("MoveType serialization failed"),
        }
    }

    #[test]
    fn test_commitment_method_serialization() {
        // Test that CommitmentMethod serializes correctly
        let method = CommitmentMethod::MerkleTreeRadix4;
        let serialized = serde_json::to_string(&method).unwrap();
        let deserialized: CommitmentMethod = serde_json::from_str(&serialized).unwrap();
        
        match deserialized {
            CommitmentMethod::MerkleTreeRadix4 => {},
            _ => panic!("CommitmentMethod serialization failed"),
        }
    }
}
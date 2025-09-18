//! Integration tests for the sequence processor

use kirk::{
    SequenceProcessorConfig, ProcessingResult,
    ChallengeContent, ChallengeAcceptContent, MoveContent, FinalContent,
    MoveType, GameProtocolError
};
use kirk::cashu::SequenceStatistics;
use kirk::game::SequenceState;
use nostr::{Keys, Kind};

/// Mock test to verify sequence processor structure
#[tokio::test]
async fn test_sequence_processor_config() {
    let config = SequenceProcessorConfig::default();
    
    // Verify default configuration values
    assert_eq!(config.final_event_timeout, 3600); // 1 hour
    assert_eq!(config.move_timeout, 1800);        // 30 minutes
    assert!(config.auto_process);
    assert_eq!(config.max_batch_size, 100);
}

#[tokio::test]
async fn test_sequence_processor_custom_config() {
    let config = SequenceProcessorConfig {
        final_event_timeout: 7200, // 2 hours
        move_timeout: 3600,        // 1 hour
        auto_process: false,
        max_batch_size: 50,
    };
    
    assert_eq!(config.final_event_timeout, 7200);
    assert_eq!(config.move_timeout, 3600);
    assert!(!config.auto_process);
    assert_eq!(config.max_batch_size, 50);
}

/// Test event creation for sequence processing
#[test]
fn test_create_challenge_event() {
    let keys = Keys::generate();
    
    let content = ChallengeContent {
        game_type: "test_game".to_string(),
        commitment_hashes: vec!["abc123".to_string(), "def456".to_string()],
        game_parameters: serde_json::json!({
            "max_moves": 10,
            "timeout": 3600
        }),
        expiry: Some(chrono::Utc::now().timestamp() as u64 + 3600),
    };
    
    let event_result = content.to_event(&keys);
    assert!(event_result.is_ok());
    
    let event = event_result.unwrap();
    assert_eq!(event.kind, Kind::Custom(9259)); // CHALLENGE_KIND
    assert_eq!(event.pubkey, keys.public_key());
    
    // Verify content can be parsed back
    let parsed_content: ChallengeContent = serde_json::from_str(&event.content).unwrap();
    assert_eq!(parsed_content.game_type, "test_game");
    assert_eq!(parsed_content.commitment_hashes.len(), 2);
}

#[test]
fn test_create_challenge_accept_event() {
    let keys = Keys::generate();
    let challenge_id = nostr::EventId::all_zeros();
    
    let content = ChallengeAcceptContent {
        challenge_id,
        commitment_hashes: vec!["ghi789".to_string()],
    };
    
    let event_result = content.to_event(&keys);
    assert!(event_result.is_ok());
    
    let event = event_result.unwrap();
    assert_eq!(event.kind, Kind::Custom(9260)); // CHALLENGE_ACCEPT_KIND
    
    // Verify content can be parsed back
    let parsed_content: ChallengeAcceptContent = serde_json::from_str(&event.content).unwrap();
    assert_eq!(parsed_content.challenge_id, challenge_id);
    assert_eq!(parsed_content.commitment_hashes.len(), 1);
}

#[test]
fn test_create_move_event() {
    let keys = Keys::generate();
    let previous_event_id = nostr::EventId::all_zeros();
    
    let content = MoveContent {
        previous_event_id,
        move_type: MoveType::Move,
        move_data: serde_json::json!({
            "action": "place_piece",
            "position": [1, 2]
        }),
        revealed_tokens: None,
    };
    
    let event_result = content.to_event(&keys);
    assert!(event_result.is_ok());
    
    let event = event_result.unwrap();
    assert_eq!(event.kind, Kind::Custom(9261)); // MOVE_KIND
    
    // Verify content can be parsed back
    let parsed_content: MoveContent = serde_json::from_str(&event.content).unwrap();
    assert_eq!(parsed_content.previous_event_id, previous_event_id);
    assert!(matches!(parsed_content.move_type, MoveType::Move));
}

#[test]
fn test_create_final_event() {
    let keys = Keys::generate();
    let game_sequence_root = nostr::EventId::all_zeros();
    
    let content = FinalContent {
        game_sequence_root,
        commitment_method: Some(kirk::CommitmentMethod::Concatenation),
        final_state: serde_json::json!({
            "winner": "player1",
            "final_score": [3, 1]
        }),
    };
    
    let event_result = content.to_event(&keys);
    assert!(event_result.is_ok());
    
    let event = event_result.unwrap();
    assert_eq!(event.kind, Kind::Custom(9262)); // FINAL_KIND
    
    // Verify content can be parsed back
    let parsed_content: FinalContent = serde_json::from_str(&event.content).unwrap();
    assert_eq!(parsed_content.game_sequence_root, game_sequence_root);
    assert!(matches!(parsed_content.commitment_method, Some(kirk::CommitmentMethod::Concatenation)));
}

/// Test processing result variants
#[test]
fn test_processing_result_variants() {
    let keys = Keys::generate();
    let challenge_id = nostr::EventId::all_zeros();
    let event_id = nostr::EventId::all_zeros();
    
    // Test SequenceCreated
    let result = ProcessingResult::SequenceCreated {
        challenge_id,
        challenger: keys.public_key(),
    };
    
    match result {
        ProcessingResult::SequenceCreated { challenge_id: id, challenger } => {
            assert_eq!(id, challenge_id);
            assert_eq!(challenger, keys.public_key());
        },
        _ => panic!("Expected SequenceCreated variant"),
    }
    
    // Test SequenceUpdated
    let result = ProcessingResult::SequenceUpdated {
        challenge_id,
        event_id,
        new_state: SequenceState::InProgress,
    };
    
    match result {
        ProcessingResult::SequenceUpdated { new_state, .. } => {
            assert!(matches!(new_state, SequenceState::InProgress));
        },
        _ => panic!("Expected SequenceUpdated variant"),
    }
    
    // Test ValidationFailure
    let result = ProcessingResult::ValidationFailure {
        event_id,
        reason: "Test validation failure".to_string(),
    };
    
    match result {
        ProcessingResult::ValidationFailure { reason, .. } => {
            assert_eq!(reason, "Test validation failure");
        },
        _ => panic!("Expected ValidationFailure variant"),
    }
}

/// Test error handling
#[test]
fn test_error_types() {
    let error = GameProtocolError::SequenceError("Test sequence error".to_string());
    assert!(matches!(error, GameProtocolError::SequenceError(_)));
    
    let error = GameProtocolError::MintError("Test mint error".to_string());
    assert!(matches!(error, GameProtocolError::MintError(_)));
    
    let error = GameProtocolError::InvalidCommitment("Test commitment error".to_string());
    assert!(matches!(error, GameProtocolError::InvalidCommitment(_)));
}

/// Test sequence statistics
#[test]
fn test_sequence_statistics() {
    let stats = SequenceStatistics::default();
    assert_eq!(stats.waiting_for_accept, 0);
    assert_eq!(stats.in_progress, 0);
    assert_eq!(stats.waiting_for_final, 0);
    assert_eq!(stats.completed, 0);
    assert_eq!(stats.forfeited, 0);
    assert_eq!(stats.total_completed, 0);
}

/// Test commitment method serialization
#[test]
fn test_commitment_method_serialization() {
    use kirk::CommitmentMethod;
    
    let method = CommitmentMethod::Concatenation;
    let serialized = serde_json::to_string(&method).unwrap();
    let deserialized: CommitmentMethod = serde_json::from_str(&serialized).unwrap();
    assert!(matches!(deserialized, CommitmentMethod::Concatenation));
    
    let method = CommitmentMethod::MerkleTreeRadix4;
    let serialized = serde_json::to_string(&method).unwrap();
    let deserialized: CommitmentMethod = serde_json::from_str(&serialized).unwrap();
    assert!(matches!(deserialized, CommitmentMethod::MerkleTreeRadix4));
}

/// Test move type serialization
#[test]
fn test_move_type_serialization() {
    let move_type = MoveType::Move;
    let serialized = serde_json::to_string(&move_type).unwrap();
    let deserialized: MoveType = serde_json::from_str(&serialized).unwrap();
    assert!(matches!(deserialized, MoveType::Move));
    
    let move_type = MoveType::Commit;
    let serialized = serde_json::to_string(&move_type).unwrap();
    let deserialized: MoveType = serde_json::from_str(&serialized).unwrap();
    assert!(matches!(deserialized, MoveType::Commit));
    
    let move_type = MoveType::Reveal;
    let serialized = serde_json::to_string(&move_type).unwrap();
    let deserialized: MoveType = serde_json::from_str(&serialized).unwrap();
    assert!(matches!(deserialized, MoveType::Reveal));
}
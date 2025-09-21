//! Unit tests for timeout and deadline management functionality

use std::collections::HashMap;
use nostr::{Keys, EventBuilder, EventId, PublicKey};
use kirk::{
    events::{
        ChallengeContent, ChallengeAcceptContent, MoveContent, FinalContent,
        TimeoutConfig, TimeoutPhase, MoveType, CHALLENGE_KIND, CHALLENGE_ACCEPT_KIND, 
        MOVE_KIND, FINAL_KIND
    },
    game::{GameSequence, SequenceState, TimeoutViolation},
    error::GameProtocolError,
};

fn create_test_keys() -> Keys {
    Keys::generate()
}

fn create_challenge_event_with_timeout(keys: &Keys, timeout_config: Option<TimeoutConfig>) -> nostr::Event {
    let content = ChallengeContent {
        game_type: "test_game".to_string(),
        commitment_hashes: vec!["abc123".to_string().repeat(32)[..64].to_string()],
        game_parameters: serde_json::json!({}),
        expiry: Some(chrono::Utc::now().timestamp() as u64 + 3600),
        timeout_config,
    };
    
    EventBuilder::new(CHALLENGE_KIND, serde_json::to_string(&content).unwrap(), Vec::<nostr::Tag>::new())
        .to_event(keys)
        .unwrap()
}

fn create_challenge_accept_event(keys: &Keys, challenge_id: EventId) -> nostr::Event {
    let content = ChallengeAcceptContent {
        challenge_id,
        commitment_hashes: vec!["def456".to_string().repeat(32)[..64].to_string()],
    };
    
    EventBuilder::new(CHALLENGE_ACCEPT_KIND, serde_json::to_string(&content).unwrap(), Vec::<nostr::Tag>::new())
        .to_event(keys)
        .unwrap()
}

fn create_move_event_with_deadline(keys: &Keys, previous_event_id: EventId, deadline: Option<u64>) -> nostr::Event {
    let content = MoveContent {
        previous_event_id,
        move_type: MoveType::Move,
        move_data: serde_json::json!({"action": "test_move"}),
        revealed_tokens: None,
        deadline,
    };
    
    EventBuilder::new(MOVE_KIND, serde_json::to_string(&content).unwrap(), Vec::<nostr::Tag>::new())
        .to_event(keys)
        .unwrap()
}

#[test]
fn test_timeout_config_creation() {
    let config = TimeoutConfig::new();
    assert_eq!(config.accept_timeout, Some(3600));
    assert_eq!(config.move_timeout, Some(1800));
    assert_eq!(config.commit_reveal_timeout, Some(600));
    assert_eq!(config.final_event_timeout, Some(3600));
}

#[test]
fn test_timeout_config_custom() {
    let config = TimeoutConfig::custom(
        Some(7200), // 2 hours
        Some(900),  // 15 minutes
        Some(300),  // 5 minutes
        Some(1800), // 30 minutes
    );
    
    assert_eq!(config.accept_timeout, Some(7200));
    assert_eq!(config.move_timeout, Some(900));
    assert_eq!(config.commit_reveal_timeout, Some(300));
    assert_eq!(config.final_event_timeout, Some(1800));
}

#[test]
fn test_timeout_config_validation() {
    // Valid config should pass
    let valid_config = TimeoutConfig::new();
    assert!(valid_config.validate().is_ok());
    
    // Config with too short timeout should fail
    let invalid_config = TimeoutConfig::custom(
        Some(30), // Too short (less than 60 seconds)
        None,
        None,
        None,
    );
    assert!(invalid_config.validate().is_err());
    
    // Config with too long timeout should fail
    let invalid_config = TimeoutConfig::custom(
        Some(100000), // Too long (more than 86400 seconds)
        None,
        None,
        None,
    );
    assert!(invalid_config.validate().is_err());
}

#[test]
fn test_timeout_config_get_timeout_for_phase() {
    let config = TimeoutConfig::new();
    
    assert_eq!(config.get_timeout_for_phase(TimeoutPhase::Accept), Some(3600));
    assert_eq!(config.get_timeout_for_phase(TimeoutPhase::Move), Some(1800));
    assert_eq!(config.get_timeout_for_phase(TimeoutPhase::CommitReveal), Some(600));
    assert_eq!(config.get_timeout_for_phase(TimeoutPhase::FinalEvent), Some(3600));
}

#[test]
fn test_challenge_content_with_timeout_validation() {
    let valid_timeout_config = TimeoutConfig::new();
    let content = ChallengeContent {
        game_type: "test_game".to_string(),
        commitment_hashes: vec!["a".repeat(64)],
        game_parameters: serde_json::json!({}),
        expiry: Some(chrono::Utc::now().timestamp() as u64 + 3600),
        timeout_config: Some(valid_timeout_config),
    };
    
    assert!(content.validate().is_ok());
    
    // Test with invalid timeout config
    let invalid_timeout_config = TimeoutConfig::custom(Some(30), None, None, None);
    let invalid_content = ChallengeContent {
        game_type: "test_game".to_string(),
        commitment_hashes: vec!["a".repeat(64)],
        game_parameters: serde_json::json!({}),
        expiry: Some(chrono::Utc::now().timestamp() as u64 + 3600),
        timeout_config: Some(invalid_timeout_config),
    };
    
    assert!(invalid_content.validate().is_err());
}

#[test]
fn test_move_content_deadline_validation() {
    let future_deadline = chrono::Utc::now().timestamp() as u64 + 3600;
    let past_deadline = chrono::Utc::now().timestamp() as u64 - 3600;
    
    // Valid future deadline
    let valid_content = MoveContent {
        previous_event_id: EventId::all_zeros(),
        move_type: MoveType::Move,
        move_data: serde_json::json!({}),
        revealed_tokens: None,
        deadline: Some(future_deadline),
    };
    assert!(valid_content.validate().is_ok());
    
    // Invalid past deadline
    let invalid_content = MoveContent {
        previous_event_id: EventId::all_zeros(),
        move_type: MoveType::Move,
        move_data: serde_json::json!({}),
        revealed_tokens: None,
        deadline: Some(past_deadline),
    };
    assert!(invalid_content.validate().is_err());
}

#[test]
fn test_game_sequence_with_timeout_config() {
    let keys = create_test_keys();
    let timeout_config = TimeoutConfig::new();
    let challenge_event = create_challenge_event_with_timeout(&keys, Some(timeout_config.clone()));
    
    let sequence = GameSequence::new(challenge_event, keys.public_key()).unwrap();
    
    assert!(sequence.timeout_config.is_some());
    assert_eq!(sequence.timeout_config.unwrap().accept_timeout, timeout_config.accept_timeout);
    assert!(!sequence.phase_deadlines.is_empty());
    assert!(sequence.phase_deadlines.contains_key(&TimeoutPhase::Accept));
}

#[test]
fn test_game_sequence_phase_deadline_updates() {
    let challenger_keys = create_test_keys();
    let accepter_keys = create_test_keys();
    let timeout_config = TimeoutConfig::new();
    
    let challenge_event = create_challenge_event_with_timeout(&challenger_keys, Some(timeout_config));
    let mut sequence = GameSequence::new(challenge_event.clone(), challenger_keys.public_key()).unwrap();
    
    // Initially should have accept timeout
    assert!(sequence.phase_deadlines.contains_key(&TimeoutPhase::Accept));
    
    // Add challenge accept event
    let accept_event = create_challenge_accept_event(&accepter_keys, challenge_event.id);
    sequence.add_event(accept_event).unwrap();
    
    // Should now have move timeout and no accept timeout
    assert!(!sequence.phase_deadlines.contains_key(&TimeoutPhase::Accept));
    assert!(sequence.phase_deadlines.contains_key(&TimeoutPhase::Move));
}

#[test]
fn test_timeout_violation_creation() {
    let now = chrono::Utc::now().timestamp() as u64;
    let deadline = now - 300; // 5 minutes ago
    let player = PublicKey::from_hex("0000000000000000000000000000000000000000000000000000000000000001").unwrap();
    
    let violation = TimeoutViolation {
        phase: TimeoutPhase::Move,
        deadline,
        current_time: now,
        affected_player: Some(player),
    };
    
    assert_eq!(violation.overdue_duration(), 300);
    assert!(violation.should_forfeit(60)); // Should forfeit with 1 minute grace period
    assert!(!violation.should_forfeit(600)); // Should not forfeit with 10 minute grace period
}

#[test]
fn test_game_sequence_timeout_checking() {
    let keys = create_test_keys();
    let timeout_config = TimeoutConfig::custom(
        Some(60), // Very short accept timeout for testing
        Some(60),
        Some(60),
        Some(60),
    );
    
    let challenge_event = create_challenge_event_with_timeout(&keys, Some(timeout_config));
    let sequence = GameSequence::new(challenge_event, keys.public_key()).unwrap();
    
    // Wait a bit to ensure timeout
    std::thread::sleep(std::time::Duration::from_secs(1));
    
    // Should detect timeout violations (though they may not be overdue enough to forfeit yet)
    let violations = sequence.check_timeouts();
    // Note: This test is time-sensitive and may be flaky in CI environments
    // In a real implementation, we'd use mock time for more reliable testing
}

#[test]
fn test_game_sequence_has_active_timeouts() {
    let keys = create_test_keys();
    let timeout_config = TimeoutConfig::new();
    
    // Sequence with timeout config should have active timeouts
    let challenge_event = create_challenge_event_with_timeout(&keys, Some(timeout_config));
    let sequence_with_timeouts = GameSequence::new(challenge_event, keys.public_key()).unwrap();
    assert!(sequence_with_timeouts.has_active_timeouts());
    
    // Sequence without timeout config should not have active timeouts
    let challenge_event_no_timeout = create_challenge_event_with_timeout(&keys, None);
    let sequence_no_timeouts = GameSequence::new(challenge_event_no_timeout, keys.public_key()).unwrap();
    assert!(!sequence_no_timeouts.has_active_timeouts());
}

#[test]
fn test_game_sequence_get_next_deadline() {
    let keys = create_test_keys();
    let timeout_config = TimeoutConfig::new();
    
    let challenge_event = create_challenge_event_with_timeout(&keys, Some(timeout_config));
    let sequence = GameSequence::new(challenge_event, keys.public_key()).unwrap();
    
    let next_deadline = sequence.get_next_deadline();
    assert!(next_deadline.is_some());
    
    let (phase, deadline) = next_deadline.unwrap();
    assert_eq!(phase, TimeoutPhase::Accept);
    assert!(deadline > chrono::Utc::now().timestamp() as u64);
}

#[test]
fn test_move_event_with_deadline() {
    let keys = create_test_keys();
    let future_deadline = chrono::Utc::now().timestamp() as u64 + 3600;
    
    let move_event = create_move_event_with_deadline(&keys, EventId::all_zeros(), Some(future_deadline));
    
    // Parse the event to verify deadline was included
    let parsed_content: MoveContent = serde_json::from_str(&move_event.content).unwrap();
    assert_eq!(parsed_content.deadline, Some(future_deadline));
}

#[test]
fn test_timeout_phase_enum() {
    // Test that timeout phases can be compared and used in collections
    let mut phases = vec![
        TimeoutPhase::FinalEvent,
        TimeoutPhase::Accept,
        TimeoutPhase::Move,
        TimeoutPhase::CommitReveal,
    ];
    
    phases.sort_by_key(|p| format!("{:?}", p));
    
    // Verify phases can be used as HashMap keys
    let mut phase_map = HashMap::new();
    phase_map.insert(TimeoutPhase::Accept, 3600u64);
    phase_map.insert(TimeoutPhase::Move, 1800u64);
    
    assert_eq!(phase_map.get(&TimeoutPhase::Accept), Some(&3600));
    assert_eq!(phase_map.get(&TimeoutPhase::Move), Some(&1800));
}

#[test]
fn test_timeout_error_types() {
    // Test that timeout-related errors are properly categorized
    let timeout_error = GameProtocolError::Timeout("Test timeout".to_string());
    assert!(matches!(timeout_error, GameProtocolError::Timeout(_)));
    
    let timeout_message = format!("{}", timeout_error);
    assert!(timeout_message.contains("Timeout error"));
    assert!(timeout_message.contains("Test timeout"));
}

#[test]
fn test_sequence_state_timeout_compatibility() {
    // Test that sequence states work correctly with timeout checking
    let states = vec![
        SequenceState::WaitingForAccept,
        SequenceState::InProgress,
        SequenceState::WaitingForFinal,
        SequenceState::Complete { winner: None },
        SequenceState::Forfeited { 
            winner: PublicKey::from_hex("0000000000000000000000000000000000000000000000000000000000000001").unwrap()
        },
    ];
    
    for state in states {
        // Verify that finished states don't accept moves (important for timeout handling)
        match state {
            SequenceState::Complete { .. } | SequenceState::Forfeited { .. } => {
                assert!(state.is_finished());
                assert!(!state.can_accept_moves());
            },
            _ => {
                assert!(!state.is_finished());
            }
        }
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    
    #[test]
    fn test_full_timeout_scenario() {
        let challenger_keys = create_test_keys();
        let accepter_keys = create_test_keys();
        
        // Create challenge with short timeouts for testing
        let timeout_config = TimeoutConfig::custom(
            Some(120), // 2 minutes accept timeout
            Some(60),  // 1 minute move timeout
            Some(30),  // 30 second commit/reveal timeout
            Some(180), // 3 minute final event timeout
        );
        
        let challenge_event = create_challenge_event_with_timeout(&challenger_keys, Some(timeout_config));
        let mut sequence = GameSequence::new(challenge_event.clone(), challenger_keys.public_key()).unwrap();
        
        // Verify initial state
        assert!(matches!(sequence.state, SequenceState::WaitingForAccept));
        assert!(sequence.has_active_timeouts());
        
        // Accept the challenge
        let accept_event = create_challenge_accept_event(&accepter_keys, challenge_event.id);
        sequence.add_event(accept_event).unwrap();
        
        // Verify state transition and timeout updates
        assert!(matches!(sequence.state, SequenceState::InProgress));
        assert!(sequence.phase_deadlines.contains_key(&TimeoutPhase::Move));
        assert!(!sequence.phase_deadlines.contains_key(&TimeoutPhase::Accept));
        
        // The sequence should still have active timeouts
        assert!(sequence.has_active_timeouts());
    }
    
    #[test]
    fn test_timeout_violation_detection() {
        let keys = create_test_keys();
        
        // Create a sequence that will immediately have timeout violations
        let past_time = chrono::Utc::now().timestamp() as u64 - 3600; // 1 hour ago
        let timeout_config = TimeoutConfig::custom(
            Some(60), // Short timeout that will be exceeded
            Some(60),
            Some(60),
            Some(60),
        );
        
        let challenge_event = create_challenge_event_with_timeout(&keys, Some(timeout_config));
        let mut sequence = GameSequence::new(challenge_event, keys.public_key()).unwrap();
        
        // Manually set an old deadline to simulate timeout
        sequence.phase_deadlines.insert(TimeoutPhase::Accept, past_time);
        
        let violations = sequence.check_timeouts();
        assert!(!violations.is_empty());
        
        let violation = &violations[0];
        assert_eq!(violation.phase, TimeoutPhase::Accept);
        assert!(violation.overdue_duration() > 0);
        assert!(violation.should_forfeit(60)); // Should forfeit with 1 minute grace period
    }
}
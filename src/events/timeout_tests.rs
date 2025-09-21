//! Tests for timeout and deadline management functionality

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{TimeoutConfig, TimeoutPhase, ChallengeContent, MoveContent, MoveType};
    use crate::error::GameProtocolError;
    use nostr::{Keys, EventId};

    fn create_test_keys() -> Keys {
        Keys::generate()
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
    fn test_timeout_phase_enum() {
        // Test that timeout phases can be compared and used in collections
        use std::collections::HashMap;
        
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
        let timeout_error = GameProtocolError::Timeout {
            message: "Test timeout".to_string(),
            duration_ms: 1000,
            operation: "test_operation".to_string(),
        };
        assert!(matches!(timeout_error, GameProtocolError::Timeout { .. }));
        
        let timeout_message = format!("{}", timeout_error);
        assert!(timeout_message.contains("Timeout error"));
        assert!(timeout_message.contains("Test timeout"));
    }
}
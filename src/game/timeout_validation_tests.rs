//! Tests for timeout validation in game sequences

#[cfg(test)]
mod tests {
    use super::super::validation::{GameSequence, TimeoutViolation};
    use crate::events::{TimeoutConfig, TimeoutPhase, ChallengeContent};
    use crate::error::GameProtocolError;
    use nostr::{Keys, EventBuilder, PublicKey};
    use std::collections::HashMap;

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
        
        EventBuilder::new(crate::events::CHALLENGE_KIND, serde_json::to_string(&content).unwrap(), Vec::<nostr::Tag>::new())
            .to_event(keys)
            .unwrap()
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
    fn test_timeout_violation_analysis() {
        let now = chrono::Utc::now().timestamp() as u64;
        let player = PublicKey::from_hex("0000000000000000000000000000000000000000000000000000000000000001").unwrap();
        
        // Test different timeout scenarios
        let scenarios = vec![
            (TimeoutPhase::Accept, now - 300, 60),   // 5 minutes overdue, 1 minute grace
            (TimeoutPhase::Move, now - 30, 60),     // 30 seconds overdue, 1 minute grace
            (TimeoutPhase::CommitReveal, now - 600, 300), // 10 minutes overdue, 5 minute grace
            (TimeoutPhase::FinalEvent, now - 120, 60),    // 2 minutes overdue, 1 minute grace
        ];
        
        for (phase, deadline, grace_period) in scenarios {
            let violation = TimeoutViolation {
                phase,
                deadline,
                current_time: now,
                affected_player: Some(player),
            };
            
            let overdue = violation.overdue_duration();
            let should_forfeit = violation.should_forfeit(grace_period);
            
            // Verify overdue calculation
            assert_eq!(overdue, now - deadline);
            
            // Verify forfeiture logic
            assert_eq!(should_forfeit, overdue > grace_period);
        }
    }

    #[test]
    fn test_game_sequence_timeout_checking() {
        let keys = create_test_keys();
        let timeout_config = TimeoutConfig::custom(
            Some(1), // Very short accept timeout for testing (1 second)
            Some(1),
            Some(1),
            Some(1),
        );
        
        let challenge_event = create_challenge_event_with_timeout(&keys, Some(timeout_config));
        let mut sequence = GameSequence::new(challenge_event, keys.public_key()).unwrap();
        
        // Manually set an old deadline to simulate timeout
        let past_time = chrono::Utc::now().timestamp() as u64 - 300; // 5 minutes ago
        sequence.phase_deadlines.insert(TimeoutPhase::Accept, past_time);
        
        // Should detect timeout violations
        let violations = sequence.check_timeouts();
        assert!(!violations.is_empty());
        
        let violation = &violations[0];
        assert_eq!(violation.phase, TimeoutPhase::Accept);
        assert!(violation.overdue_duration() > 0);
        assert!(violation.should_forfeit(60)); // Should forfeit with 1 minute grace period
    }
}
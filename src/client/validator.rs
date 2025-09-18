//! Validation client for independent game sequence verification

use std::collections::HashMap;
use nostr::{Event, EventId, Filter, PublicKey};
use nostr_sdk::{Client as NostrClient, EventSource};
use cdk::nuts::Token as CashuToken;
use crate::error::{GameProtocolError, ValidationResult, ValidationError, ValidationErrorType};
use crate::events::{EventParser, CommitmentMethod, CHALLENGE_KIND, CHALLENGE_ACCEPT_KIND, MOVE_KIND, FINAL_KIND, REWARD_KIND};
use crate::game::{GameSequence, SequenceState};
use crate::cashu::commitments::TokenCommitment;

/// Client for third-party validation of game sequences
pub struct ValidationClient {
    nostr_client: NostrClient,
}

/// Validation report containing detailed analysis of a game sequence
#[derive(Debug, Clone)]
pub struct ValidationReport {
    pub challenge_id: EventId,
    pub sequence_valid: bool,
    pub commitment_valid: bool,
    pub events_collected: usize,
    pub validation_result: ValidationResult,
    pub commitment_errors: Vec<CommitmentValidationError>,
    pub sequence_errors: Vec<String>,
}

/// Specific commitment validation error
#[derive(Debug, Clone)]
pub struct CommitmentValidationError {
    pub event_id: EventId,
    pub player: PublicKey,
    pub commitment_hash: String,
    pub error_message: String,
}

impl ValidationClient {
    /// Create new validation client
    pub fn new(nostr_client: NostrClient) -> Self {
        Self {
            nostr_client,
        }
    }
    
    /// Collect and validate complete game sequence
    pub async fn validate_game_sequence(
        &self,
        challenge_id: EventId
    ) -> Result<ValidationReport, GameProtocolError> {
        // Step 1: Collect all events for the game sequence
        let events = self.collect_game_events(challenge_id).await?;
        
        if events.is_empty() {
            return Ok(ValidationReport {
                challenge_id,
                sequence_valid: false,
                commitment_valid: false,
                events_collected: 0,
                validation_result: ValidationResult {
                    is_valid: false,
                    winner: None,
                    errors: vec![ValidationError {
                        event_id: challenge_id,
                        error_type: ValidationErrorType::InvalidSequence,
                        message: "No events found for challenge".to_string(),
                    }],
                    forfeited_player: None,
                },
                commitment_errors: vec![],
                sequence_errors: vec!["No events found for challenge".to_string()],
            });
        }
        
        // Step 2: Build game sequence from events
        let sequence = self.build_game_sequence_from_events(&events)?;
        
        // Step 3: Validate sequence integrity
        let validation_result = sequence.validate_sequence_integrity()?;
        
        // Step 4: Verify commitments against revealed tokens
        let (commitment_valid, commitment_errors) = self.verify_all_commitments(&events)?;
        
        // Step 5: Collect any additional sequence errors
        let sequence_errors = self.collect_sequence_errors(&sequence);
        
        Ok(ValidationReport {
            challenge_id,
            sequence_valid: validation_result.is_valid,
            commitment_valid,
            events_collected: events.len(),
            validation_result,
            commitment_errors,
            sequence_errors,
        })
    }
    
    /// Collect all events for a game sequence by filtering related events
    pub async fn collect_game_events(
        &self,
        challenge_id: EventId
    ) -> Result<Vec<Event>, GameProtocolError> {
        let mut all_events = Vec::new();
        
        // Step 1: Get the challenge event
        let challenge_filter = Filter::new()
            .id(challenge_id)
            .kind(CHALLENGE_KIND);
        
        let challenge_events = self.nostr_client
            .get_events_of(vec![challenge_filter], EventSource::relays(None))
            .await
            .map_err(GameProtocolError::from)?;
        
        if challenge_events.is_empty() {
            return Err(GameProtocolError::SequenceError(
                "Challenge event not found".to_string()
            ));
        }
        
        let challenge_event = challenge_events[0].clone();
        all_events.push(challenge_event.clone());
        
        // Step 2: Get ChallengeAccept events that reference this challenge
        let accept_filter = Filter::new()
            .kind(CHALLENGE_ACCEPT_KIND)
            .since(challenge_event.created_at);
        
        let accept_events = self.nostr_client
            .get_events_of(vec![accept_filter], EventSource::relays(None))
            .await
            .map_err(GameProtocolError::from)?;
        
        // Filter accept events that reference our challenge
        for event in accept_events {
            if let Ok(accept_content) = EventParser::parse_challenge_accept(&event) {
                if accept_content.challenge_id == challenge_id {
                    all_events.push(event);
                }
            }
        }
        
        // Step 3: Get Move events that are part of this sequence
        let move_filter = Filter::new()
            .kind(MOVE_KIND)
            .since(challenge_event.created_at);
        
        let move_events = self.nostr_client
            .get_events_of(vec![move_filter], EventSource::relays(None))
            .await
            .map_err(GameProtocolError::from)?;
        
        // Filter move events that are part of our sequence
        let event_ids: std::collections::HashSet<EventId> = all_events.iter().map(|e| e.id).collect();
        for event in move_events {
            if let Ok(move_content) = EventParser::parse_move(&event) {
                if event_ids.contains(&move_content.previous_event_id) {
                    all_events.push(event);
                }
            }
        }
        
        // Step 4: Get Final events that reference this game sequence
        let final_filter = Filter::new()
            .kind(FINAL_KIND)
            .since(challenge_event.created_at);
        
        let final_events = self.nostr_client
            .get_events_of(vec![final_filter], EventSource::relays(None))
            .await
            .map_err(GameProtocolError::from)?;
        
        // Filter final events that reference our sequence
        for event in final_events {
            if let Ok(final_content) = EventParser::parse_final(&event) {
                if final_content.game_sequence_root == challenge_id {
                    all_events.push(event);
                }
            }
        }
        
        // Step 5: Get Reward events for this sequence (optional)
        let reward_filter = Filter::new()
            .kind(REWARD_KIND)
            .since(challenge_event.created_at);
        
        let reward_events = self.nostr_client
            .get_events_of(vec![reward_filter], EventSource::relays(None))
            .await
            .map_err(GameProtocolError::from)?;
        
        // Filter reward events that reference our sequence
        for event in reward_events {
            if let Ok(reward_content) = EventParser::parse_reward(&event) {
                if reward_content.game_sequence_root == challenge_id {
                    all_events.push(event);
                }
            }
        }
        
        // Sort events by creation time for proper sequence
        all_events.sort_by_key(|e| e.created_at);
        
        Ok(all_events)
    }
    
    /// Verify commitments against revealed tokens for all events in sequence
    pub fn verify_all_commitments(
        &self,
        events: &[Event]
    ) -> Result<(bool, Vec<CommitmentValidationError>), GameProtocolError> {
        let mut commitment_errors = Vec::new();
        let mut all_valid = true;
        
        // Build map of player commitments from Challenge and ChallengeAccept events
        let mut player_commitments: HashMap<PublicKey, Vec<String>> = HashMap::new();
        
        // Collect commitments from Challenge and ChallengeAccept events
        for event in events {
            match event.kind {
                k if k == CHALLENGE_KIND => {
                    let challenge_content = EventParser::parse_challenge(event)?;
                    player_commitments.insert(event.pubkey, challenge_content.commitment_hashes);
                },
                k if k == CHALLENGE_ACCEPT_KIND => {
                    let accept_content = EventParser::parse_challenge_accept(event)?;
                    player_commitments.insert(event.pubkey, accept_content.commitment_hashes);
                },
                _ => {}
            }
        }
        
        // Collect revealed tokens from Move events
        let mut player_revealed_tokens: HashMap<PublicKey, Vec<CashuToken>> = HashMap::new();
        
        for event in events {
            if event.kind == MOVE_KIND {
                let move_content = EventParser::parse_move(event)?;
                if let Some(revealed_tokens) = move_content.revealed_tokens {
                    player_revealed_tokens
                        .entry(event.pubkey)
                        .or_insert_with(Vec::new)
                        .extend(revealed_tokens);
                }
            }
        }
        
        // Get commitment methods from Final events
        let mut commitment_methods: HashMap<PublicKey, CommitmentMethod> = HashMap::new();
        
        for event in events {
            if event.kind == FINAL_KIND {
                let final_content = EventParser::parse_final(event)?;
                if let Some(method) = final_content.commitment_method {
                    commitment_methods.insert(event.pubkey, method);
                }
            }
        }
        
        // Verify each player's commitments against their revealed tokens
        for (player, commitments) in &player_commitments {
            if let Some(revealed_tokens) = player_revealed_tokens.get(player) {
                for commitment_hash in commitments {
                    let verification_result = self.verify_single_commitment(
                        *player,
                        commitment_hash,
                        revealed_tokens,
                        commitment_methods.get(player)
                    );
                    
                    match verification_result {
                        Ok(is_valid) => {
                            if !is_valid {
                                all_valid = false;
                                commitment_errors.push(CommitmentValidationError {
                                    event_id: EventId::from_hex("0".repeat(64)).unwrap(), // Placeholder
                                    player: *player,
                                    commitment_hash: commitment_hash.clone(),
                                    error_message: "Commitment hash does not match revealed tokens".to_string(),
                                });
                            }
                        },
                        Err(e) => {
                            all_valid = false;
                            commitment_errors.push(CommitmentValidationError {
                                event_id: EventId::from_hex("0".repeat(64)).unwrap(), // Placeholder
                                player: *player,
                                commitment_hash: commitment_hash.clone(),
                                error_message: e.to_string(),
                            });
                        }
                    }
                }
            } else {
                // Player made commitments but never revealed tokens
                all_valid = false;
                for commitment_hash in commitments {
                    commitment_errors.push(CommitmentValidationError {
                        event_id: EventId::from_hex("0".repeat(64)).unwrap(), // Placeholder
                        player: *player,
                        commitment_hash: commitment_hash.clone(),
                        error_message: "Player made commitment but never revealed tokens".to_string(),
                    });
                }
            }
        }
        
        Ok((all_valid, commitment_errors))
    }
    
    /// Verify a single commitment against revealed tokens
    fn verify_single_commitment(
        &self,
        _player: PublicKey,
        commitment_hash: &str,
        revealed_tokens: &[CashuToken],
        commitment_method: Option<&CommitmentMethod>
    ) -> Result<bool, GameProtocolError> {
        if revealed_tokens.is_empty() {
            return Ok(false);
        }
        
        // For single token, no method needed
        if revealed_tokens.len() == 1 {
            let expected_commitment = TokenCommitment::single(&revealed_tokens[0]);
            return Ok(expected_commitment.commitment_hash == commitment_hash);
        }
        
        // For multiple tokens, method is required
        let method = commitment_method.ok_or_else(|| {
            GameProtocolError::InvalidCommitment(
                "Multiple tokens require commitment method specification".to_string()
            )
        })?;
        
        let expected_commitment = TokenCommitment::multiple(revealed_tokens, method.clone());
        Ok(expected_commitment.commitment_hash == commitment_hash)
    }
    
    /// Build a GameSequence from collected events
    fn build_game_sequence_from_events(&self, events: &[Event]) -> Result<GameSequence, GameProtocolError> {
        if events.is_empty() {
            return Err(GameProtocolError::SequenceError(
                "Cannot build sequence from empty events".to_string()
            ));
        }
        
        // Find the challenge event
        let challenge_event = events.iter()
            .find(|e| e.kind == CHALLENGE_KIND)
            .ok_or_else(|| GameProtocolError::SequenceError(
                "No challenge event found in events".to_string()
            ))?;
        
        // Create initial sequence
        let mut sequence = GameSequence::new(challenge_event.clone(), challenge_event.pubkey)?;
        
        // Add remaining events in chronological order
        let mut remaining_events: Vec<_> = events.iter()
            .filter(|e| e.id != challenge_event.id)
            .collect();
        remaining_events.sort_by_key(|e| e.created_at);
        
        for event in remaining_events {
            // Skip reward events as they're not part of the core game sequence
            if event.kind == REWARD_KIND {
                continue;
            }
            
            if let Err(e) = sequence.add_event(event.clone()) {
                // Log the error but continue building the sequence for validation
                eprintln!("Warning: Failed to add event to sequence: {}", e);
            }
        }
        
        Ok(sequence)
    }
    
    /// Collect additional sequence-level errors
    fn collect_sequence_errors(&self, sequence: &GameSequence) -> Vec<String> {
        let mut errors = Vec::new();
        
        // Check for common sequence issues
        if sequence.events.is_empty() {
            errors.push("Empty event sequence".to_string());
        }
        
        // Check for proper event ordering
        for i in 1..sequence.events.len() {
            if sequence.events[i].created_at < sequence.events[i-1].created_at {
                errors.push("Events not in chronological order".to_string());
                break;
            }
        }
        
        // Check for duplicate events
        let mut event_ids = std::collections::HashSet::new();
        for event in &sequence.events {
            if !event_ids.insert(event.id) {
                errors.push(format!("Duplicate event found: {}", event.id));
            }
        }
        
        // Check state consistency
        match &sequence.state {
            SequenceState::WaitingForAccept => {
                if sequence.get_events_by_kind(CHALLENGE_ACCEPT_KIND).len() > 0 {
                    errors.push("Sequence in WaitingForAccept state but has ChallengeAccept events".to_string());
                }
            },
            SequenceState::InProgress => {
                if sequence.get_events_by_kind(CHALLENGE_ACCEPT_KIND).is_empty() {
                    errors.push("Sequence in InProgress state but missing ChallengeAccept event".to_string());
                }
            },
            SequenceState::WaitingForFinal => {
                if sequence.get_events_by_kind(FINAL_KIND).is_empty() {
                    errors.push("Sequence in WaitingForFinal state but has no Final events".to_string());
                }
            },
            SequenceState::Complete { .. } => {
                if sequence.get_events_by_kind(FINAL_KIND).is_empty() {
                    errors.push("Sequence marked Complete but has no Final events".to_string());
                }
            },
            SequenceState::Forfeited { .. } => {
                // Forfeited sequences can be in various states
            }
        }
        
        errors
    }
    
    /// Validate a specific game sequence without collecting events (for pre-collected events)
    pub fn validate_collected_events(
        &self,
        challenge_id: EventId,
        events: &[Event]
    ) -> Result<ValidationReport, GameProtocolError> {
        if events.is_empty() {
            return Ok(ValidationReport {
                challenge_id,
                sequence_valid: false,
                commitment_valid: false,
                events_collected: 0,
                validation_result: ValidationResult {
                    is_valid: false,
                    winner: None,
                    errors: vec![ValidationError {
                        event_id: challenge_id,
                        error_type: ValidationErrorType::InvalidSequence,
                        message: "No events provided for validation".to_string(),
                    }],
                    forfeited_player: None,
                },
                commitment_errors: vec![],
                sequence_errors: vec!["No events provided for validation".to_string()],
            });
        }
        
        // Build game sequence from events
        let sequence = self.build_game_sequence_from_events(events)?;
        
        // Validate sequence integrity
        let validation_result = sequence.validate_sequence_integrity()?;
        
        // Verify commitments against revealed tokens
        let (commitment_valid, commitment_errors) = self.verify_all_commitments(events)?;
        
        // Collect any additional sequence errors
        let sequence_errors = self.collect_sequence_errors(&sequence);
        
        Ok(ValidationReport {
            challenge_id,
            sequence_valid: validation_result.is_valid,
            commitment_valid,
            events_collected: events.len(),
            validation_result,
            commitment_errors,
            sequence_errors,
        })
    }
    
    /// Get validation statistics for multiple game sequences
    pub async fn get_validation_statistics(
        &self,
        challenge_ids: &[EventId]
    ) -> Result<ValidationStatistics, GameProtocolError> {
        let mut stats = ValidationStatistics::default();
        
        for &challenge_id in challenge_ids {
            match self.validate_game_sequence(challenge_id).await {
                Ok(report) => {
                    stats.total_sequences += 1;
                    if report.sequence_valid && report.commitment_valid {
                        stats.valid_sequences += 1;
                    } else {
                        stats.invalid_sequences += 1;
                    }
                    stats.total_events += report.events_collected;
                    stats.total_commitment_errors += report.commitment_errors.len();
                },
                Err(_) => {
                    stats.total_sequences += 1;
                    stats.invalid_sequences += 1;
                }
            }
        }
        
        Ok(stats)
    }
}

/// Statistics about validation results across multiple sequences
#[derive(Debug, Clone, Default)]
pub struct ValidationStatistics {
    pub total_sequences: usize,
    pub valid_sequences: usize,
    pub invalid_sequences: usize,
    pub total_events: usize,
    pub total_commitment_errors: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{Keys, EventBuilder};
    use crate::events::{ChallengeContent, ChallengeAcceptContent, MoveContent, FinalContent, MoveType};
    use crate::events::{CHALLENGE_KIND, CHALLENGE_ACCEPT_KIND, MOVE_KIND, FINAL_KIND};
    
    fn create_test_keys() -> Keys {
        Keys::generate()
    }
    
    fn create_mock_nostr_client() -> NostrClient {
        // Create a mock client for testing
        // In a real implementation, this would connect to actual relays
        NostrClient::default()
    }
    
    fn create_challenge_event(keys: &Keys) -> Event {
        let content = ChallengeContent {
            game_type: "test_game".to_string(),
            commitment_hashes: vec!["a".repeat(64)], // Valid 64-char hex
            game_parameters: serde_json::json!({}),
            expiry: Some(chrono::Utc::now().timestamp() as u64 + 3600),
        };
        
        EventBuilder::new(CHALLENGE_KIND, serde_json::to_string(&content).unwrap(), Vec::<nostr::Tag>::new())
            .to_event(keys)
            .unwrap()
    }
    
    fn create_challenge_accept_event(keys: &Keys, challenge_id: EventId) -> Event {
        let content = ChallengeAcceptContent {
            challenge_id,
            commitment_hashes: vec!["b".repeat(64)], // Valid 64-char hex
        };
        
        EventBuilder::new(CHALLENGE_ACCEPT_KIND, serde_json::to_string(&content).unwrap(), Vec::<nostr::Tag>::new())
            .to_event(keys)
            .unwrap()
    }
    
    fn create_move_event(keys: &Keys, previous_event_id: EventId) -> Event {
        let content = MoveContent {
            previous_event_id,
            move_type: MoveType::Move,
            move_data: serde_json::json!({"action": "test_move"}),
            revealed_tokens: None,
        };
        
        EventBuilder::new(MOVE_KIND, serde_json::to_string(&content).unwrap(), Vec::<nostr::Tag>::new())
            .to_event(keys)
            .unwrap()
    }
    
    fn create_final_event(keys: &Keys, game_sequence_root: EventId) -> Event {
        let content = FinalContent {
            game_sequence_root,
            commitment_method: None,
            final_state: serde_json::json!({"result": "test_complete"}),
        };
        
        EventBuilder::new(FINAL_KIND, serde_json::to_string(&content).unwrap(), Vec::<nostr::Tag>::new())
            .to_event(keys)
            .unwrap()
    }
    
    #[test]
    fn test_validation_client_creation() {
        let client = create_mock_nostr_client();
        let validator = ValidationClient::new(client);
        
        // Just test that we can create the validator
        assert!(std::ptr::addr_of!(validator).is_aligned());
    }
    
    #[test]
    fn test_build_game_sequence_from_events() {
        let challenger_keys = create_test_keys();
        let accepter_keys = create_test_keys();
        
        let challenge_event = create_challenge_event(&challenger_keys);
        let accept_event = create_challenge_accept_event(&accepter_keys, challenge_event.id);
        let final_event = create_final_event(&challenger_keys, challenge_event.id);
        
        let events = vec![challenge_event.clone(), accept_event, final_event];
        
        let client = create_mock_nostr_client();
        let validator = ValidationClient::new(client);
        
        let sequence = validator.build_game_sequence_from_events(&events).unwrap();
        
        assert_eq!(sequence.challenge_id, challenge_event.id);
        assert_eq!(sequence.players[0], challenger_keys.public_key());
        assert_eq!(sequence.events.len(), 3); // Challenge + Accept + Final
    }
    
    #[test]
    fn test_build_game_sequence_empty_events() {
        let client = create_mock_nostr_client();
        let validator = ValidationClient::new(client);
        
        let result = validator.build_game_sequence_from_events(&[]);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GameProtocolError::SequenceError(_)));
    }
    
    #[test]
    fn test_build_game_sequence_no_challenge() {
        let keys = create_test_keys();
        let move_event = create_move_event(&keys, EventId::from_hex("0".repeat(64)).unwrap());
        
        let events = vec![move_event];
        
        let client = create_mock_nostr_client();
        let validator = ValidationClient::new(client);
        
        let result = validator.build_game_sequence_from_events(&events);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GameProtocolError::SequenceError(_)));
    }
    
    #[test]
    fn test_collect_sequence_errors() {
        let challenger_keys = create_test_keys();
        let accepter_keys = create_test_keys();
        
        let challenge_event = create_challenge_event(&challenger_keys);
        let accept_event = create_challenge_accept_event(&accepter_keys, challenge_event.id);
        
        let events = vec![challenge_event.clone(), accept_event];
        
        let client = create_mock_nostr_client();
        let validator = ValidationClient::new(client);
        
        let sequence = validator.build_game_sequence_from_events(&events).unwrap();
        let errors = validator.collect_sequence_errors(&sequence);
        
        // Should have no errors for a valid sequence
        assert!(errors.is_empty());
    }
    
    #[test]
    fn test_collect_sequence_errors_empty_sequence() {
        let keys = create_test_keys();
        let challenge_event = create_challenge_event(&keys);
        
        // Create a sequence with just the challenge event
        let mut sequence = GameSequence::new(challenge_event, keys.public_key()).unwrap();
        
        // Clear events to simulate empty sequence
        sequence.events.clear();
        
        let client = create_mock_nostr_client();
        let validator = ValidationClient::new(client);
        
        let errors = validator.collect_sequence_errors(&sequence);
        
        // Should detect empty sequence
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.contains("Empty event sequence")));
    }
    
    #[test]
    fn test_verify_single_commitment_single_token() {
        let client = create_mock_nostr_client();
        let validator = ValidationClient::new(client);
        
        // Create a test token
        let token_json = r#"{"token":[{"mint":"https://mint.example.com","proofs":[]}],"memo":"test","unit":"sat"}"#;
        let token: CashuToken = serde_json::from_str(token_json).unwrap();
        
        // Create commitment for this token
        let commitment = TokenCommitment::single(&token);
        
        let player = PublicKey::from_hex("0000000000000000000000000000000000000000000000000000000000000001").unwrap();
        
        // Verify commitment
        let result = validator.verify_single_commitment(
            player,
            &commitment.commitment_hash,
            &[token],
            None
        ).unwrap();
        
        assert!(result);
    }
    
    #[test]
    fn test_verify_single_commitment_wrong_token() {
        let client = create_mock_nostr_client();
        let validator = ValidationClient::new(client);
        
        // Create two different test tokens
        let token1_json = r#"{"token":[{"mint":"https://mint1.example.com","proofs":[]}],"memo":"test1","unit":"sat"}"#;
        let token1: CashuToken = serde_json::from_str(token1_json).unwrap();
        
        let token2_json = r#"{"token":[{"mint":"https://mint2.example.com","proofs":[]}],"memo":"test2","unit":"sat"}"#;
        let token2: CashuToken = serde_json::from_str(token2_json).unwrap();
        
        // Create commitment for token1
        let commitment = TokenCommitment::single(&token1);
        
        let player = PublicKey::from_hex("0000000000000000000000000000000000000000000000000000000000000001").unwrap();
        
        // Try to verify with token2 (should fail)
        let result = validator.verify_single_commitment(
            player,
            &commitment.commitment_hash,
            &[token2],
            None
        ).unwrap();
        
        assert!(!result);
    }
    
    #[test]
    fn test_verify_single_commitment_multiple_tokens_no_method() {
        let client = create_mock_nostr_client();
        let validator = ValidationClient::new(client);
        
        // Create test tokens
        let token1_json = r#"{"token":[{"mint":"https://mint1.example.com","proofs":[]}],"memo":"test1","unit":"sat"}"#;
        let token1: CashuToken = serde_json::from_str(token1_json).unwrap();
        
        let token2_json = r#"{"token":[{"mint":"https://mint2.example.com","proofs":[]}],"memo":"test2","unit":"sat"}"#;
        let token2: CashuToken = serde_json::from_str(token2_json).unwrap();
        
        let player = PublicKey::from_hex("0000000000000000000000000000000000000000000000000000000000000001").unwrap();
        
        // Try to verify multiple tokens without method (should fail)
        let result = validator.verify_single_commitment(
            player,
            "dummy_hash",
            &[token1, token2],
            None
        );
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GameProtocolError::InvalidCommitment(_)));
    }
    
    #[test]
    fn test_validate_collected_events_empty() {
        let client = create_mock_nostr_client();
        let validator = ValidationClient::new(client);
        
        let challenge_id = EventId::from_hex("0".repeat(64)).unwrap();
        let report = validator.validate_collected_events(challenge_id, &[]).unwrap();
        
        assert!(!report.sequence_valid);
        assert!(!report.commitment_valid);
        assert_eq!(report.events_collected, 0);
        assert!(!report.validation_result.is_valid);
    }
    
    #[test]
    fn test_validate_collected_events_valid_sequence() {
        let challenger_keys = create_test_keys();
        let accepter_keys = create_test_keys();
        
        let challenge_event = create_challenge_event(&challenger_keys);
        let accept_event = create_challenge_accept_event(&accepter_keys, challenge_event.id);
        let final_event = create_final_event(&challenger_keys, challenge_event.id);
        
        let events = vec![challenge_event.clone(), accept_event, final_event];
        
        let client = create_mock_nostr_client();
        let validator = ValidationClient::new(client);
        
        let report = validator.validate_collected_events(challenge_event.id, &events).unwrap();
        
        assert_eq!(report.challenge_id, challenge_event.id);
        assert_eq!(report.events_collected, 3);
        // Note: sequence_valid might be true, but commitment_valid will likely be false
        // since we're using dummy commitment hashes that don't match any real tokens
    }
    
    #[test]
    fn test_validation_statistics_default() {
        let stats = ValidationStatistics::default();
        
        assert_eq!(stats.total_sequences, 0);
        assert_eq!(stats.valid_sequences, 0);
        assert_eq!(stats.invalid_sequences, 0);
        assert_eq!(stats.total_events, 0);
        assert_eq!(stats.total_commitment_errors, 0);
    }
    
    #[test]
    fn test_validation_report_structure() {
        let challenge_id = EventId::from_hex("0".repeat(64)).unwrap();
        let validation_result = ValidationResult {
            is_valid: true,
            winner: None,
            errors: vec![],
            forfeited_player: None,
        };
        
        let report = ValidationReport {
            challenge_id,
            sequence_valid: true,
            commitment_valid: true,
            events_collected: 5,
            validation_result,
            commitment_errors: vec![],
            sequence_errors: vec![],
        };
        
        assert_eq!(report.challenge_id, challenge_id);
        assert!(report.sequence_valid);
        assert!(report.commitment_valid);
        assert_eq!(report.events_collected, 5);
        assert!(report.validation_result.is_valid);
        assert!(report.commitment_errors.is_empty());
        assert!(report.sequence_errors.is_empty());
    }
    
    #[test]
    fn test_commitment_validation_error_structure() {
        let event_id = EventId::from_hex("0".repeat(64)).unwrap();
        let player = PublicKey::from_hex("0000000000000000000000000000000000000000000000000000000000000001").unwrap();
        
        let error = CommitmentValidationError {
            event_id,
            player,
            commitment_hash: "test_hash".to_string(),
            error_message: "Test error message".to_string(),
        };
        
        assert_eq!(error.event_id, event_id);
        assert_eq!(error.player, player);
        assert_eq!(error.commitment_hash, "test_hash");
        assert_eq!(error.error_message, "Test error message");
    }
}
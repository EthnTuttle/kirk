//! Game sequence processor for mint operations
//!
//! This module provides the core sequence processing logic for mints to validate
//! game sequences, detect fraud, determine winners, and distribute rewards.

use std::collections::HashMap;
use std::sync::Arc;
use nostr::{Event, EventId, PublicKey, Filter, Timestamp};
use nostr_sdk::Client as NostrClient;
use crate::error::{GameProtocolError, GameResult};
use crate::game::{GameSequence, SequenceState};
use crate::events::{EventParser, ValidationFailureContent, CHALLENGE_KIND};
use crate::cashu::GameMint;


/// Configuration for sequence processing
#[derive(Debug, Clone)]
pub struct SequenceProcessorConfig {
    /// Maximum time to wait for final events after game completion (seconds)
    pub final_event_timeout: u64,
    /// Maximum time to wait for moves during gameplay (seconds)
    pub move_timeout: u64,
    /// Whether to automatically process sequences or wait for manual triggers
    pub auto_process: bool,
    /// Maximum number of events to process in a single batch
    pub max_batch_size: usize,
}

impl Default for SequenceProcessorConfig {
    fn default() -> Self {
        Self {
            final_event_timeout: 3600, // 1 hour
            move_timeout: 1800,        // 30 minutes
            auto_process: true,
            max_batch_size: 100,
        }
    }
}

/// Game sequence processor for mint operations
pub struct SequenceProcessor {
    mint: Arc<GameMint>,
    nostr_client: NostrClient,
    config: SequenceProcessorConfig,
    /// Active game sequences being tracked
    active_sequences: HashMap<EventId, GameSequence>,
    /// Completed sequences (for audit trail)
    completed_sequences: HashMap<EventId, GameSequence>,
}

impl SequenceProcessor {
    /// Create new sequence processor
    pub fn new(
        mint: Arc<GameMint>,
        nostr_client: NostrClient,
        config: Option<SequenceProcessorConfig>,
    ) -> Self {
        Self {
            mint,
            nostr_client,
            config: config.unwrap_or_default(),
            active_sequences: HashMap::new(),
            completed_sequences: HashMap::new(),
        }
    }

    /// Start processing sequences from nostr events
    pub async fn start_processing(&mut self) -> GameResult<()> {
        if !self.config.auto_process {
            return Ok(());
        }

        // Subscribe to game events
        let filter = Filter::new()
            .kinds(vec![CHALLENGE_KIND, 
                       crate::events::CHALLENGE_ACCEPT_KIND,
                       crate::events::MOVE_KIND,
                       crate::events::FINAL_KIND])
            .since(Timestamp::now());

        self.nostr_client.subscribe(vec![filter], None).await
            .map_err(|e| GameProtocolError::NostrSdk(e.to_string()))?;

        Ok(())
    }

    /// Process a batch of events
    pub async fn process_events(&mut self, events: Vec<Event>) -> GameResult<Vec<ProcessingResult>> {
        let mut results = Vec::new();
        
        for event in events.into_iter().take(self.config.max_batch_size) {
            match self.process_single_event(event).await {
                Ok(result) => results.push(result),
                Err(e) => {
                    // Log error but continue processing other events
                    eprintln!("Error processing event: {}", e);
                    results.push(ProcessingResult::Error { 
                        event_id: EventId::all_zeros(), // Placeholder
                        error: e.to_string() 
                    });
                }
            }
        }

        Ok(results)
    }

    /// Process a single event and update sequence state
    async fn process_single_event(&mut self, event: Event) -> GameResult<ProcessingResult> {
        // Validate event structure first
        crate::events::validate_event_structure(&event)?;

        match event.kind {
            k if k == CHALLENGE_KIND => {
                self.handle_challenge_event(event).await
            },
            k if k == crate::events::CHALLENGE_ACCEPT_KIND => {
                self.handle_challenge_accept_event(event).await
            },
            k if k == crate::events::MOVE_KIND => {
                self.handle_move_event(event).await
            },
            k if k == crate::events::FINAL_KIND => {
                self.handle_final_event(event).await
            },
            _ => {
                Err(GameProtocolError::SequenceError(
                    format!("Unsupported event kind: {}", event.kind)
                ))
            }
        }
    }

    /// Handle Challenge event - create new sequence
    async fn handle_challenge_event(&mut self, event: Event) -> GameResult<ProcessingResult> {
        let challenge_content = EventParser::parse_challenge(&event)?;
        
        // Validate challenge content
        if challenge_content.commitment_hashes.is_empty() {
            return Ok(ProcessingResult::ValidationFailure {
                event_id: event.id,
                reason: "Challenge must contain at least one commitment hash".to_string(),
            });
        }

        // Check for expiry
        if let Some(expiry) = challenge_content.expiry {
            let now = chrono::Utc::now().timestamp() as u64;
            if now > expiry {
                return Ok(ProcessingResult::ValidationFailure {
                    event_id: event.id,
                    reason: "Challenge has expired".to_string(),
                });
            }
        }

        // Create new game sequence
        let sequence = GameSequence::new(event.clone(), event.pubkey)?;
        let challenge_id = event.id;
        
        self.active_sequences.insert(challenge_id, sequence);

        Ok(ProcessingResult::SequenceCreated { 
            challenge_id,
            challenger: event.pubkey,
        })
    }

    /// Handle ChallengeAccept event - update sequence
    async fn handle_challenge_accept_event(&mut self, event: Event) -> GameResult<ProcessingResult> {
        let accept_content = EventParser::parse_challenge_accept(&event)?;
        
        // Find the corresponding challenge sequence
        let sequence = self.active_sequences.get_mut(&accept_content.challenge_id)
            .ok_or_else(|| GameProtocolError::SequenceError(
                "Challenge not found for ChallengeAccept".to_string()
            ))?;

        // Validate that we're in the correct state
        if !matches!(sequence.state, SequenceState::WaitingForAccept) {
            return Ok(ProcessingResult::ValidationFailure {
                event_id: event.id,
                reason: "Challenge is not accepting new players".to_string(),
            });
        }

        // Validate commitment hashes
        if accept_content.commitment_hashes.is_empty() {
            return Ok(ProcessingResult::ValidationFailure {
                event_id: event.id,
                reason: "ChallengeAccept must contain at least one commitment hash".to_string(),
            });
        }

        // Add event to sequence
        sequence.add_event(event.clone())?;

        Ok(ProcessingResult::SequenceUpdated {
            challenge_id: accept_content.challenge_id,
            event_id: event.id,
            new_state: sequence.state.clone(),
        })
    }

    /// Handle Move event - validate and update sequence
    async fn handle_move_event(&mut self, event: Event) -> GameResult<ProcessingResult> {
        let move_content = EventParser::parse_move(&event)?;
        
        // Find the sequence containing the previous event
        let challenge_id = self.find_sequence_for_event(&move_content.previous_event_id)?;
        
        // First, get the data we need without mutable borrow
        let (can_accept_moves, player_commitments) = {
            let sequence = self.active_sequences.get(&challenge_id)
                .ok_or_else(|| GameProtocolError::SequenceError(
                    "Sequence not found for Move event".to_string()
                ))?;

            let can_accept = sequence.state.can_accept_moves();
            let commitments = if move_content.revealed_tokens.is_some() {
                Some(self.get_player_commitments(&event.pubkey, sequence)?)
            } else {
                None
            };
            
            (can_accept, commitments)
        };

        // Validate that we can accept moves
        if !can_accept_moves {
            return Ok(ProcessingResult::ValidationFailure {
                event_id: event.id,
                reason: "Sequence is not accepting moves in current state".to_string(),
            });
        }

        // Validate revealed tokens if present
        if let Some(ref revealed_tokens) = move_content.revealed_tokens {
            let player_commitments = player_commitments.unwrap();
            
            match self.validate_revealed_tokens_with_commitments(&event.pubkey, revealed_tokens, &player_commitments).await {
                Ok(true) => {}, // Tokens are valid
                Ok(false) => {
                    return self.handle_fraud_detection(
                        challenge_id,
                        event.pubkey,
                        "Invalid token commitment revealed".to_string(),
                        Some(event.id)
                    ).await;
                },
                Err(e) => {
                    return Ok(ProcessingResult::ValidationFailure {
                        event_id: event.id,
                        reason: format!("Token validation error: {}", e),
                    });
                }
            }
        }

        // Now get mutable access to add the event
        let sequence = self.active_sequences.get_mut(&challenge_id)
            .ok_or_else(|| GameProtocolError::SequenceError(
                "Sequence not found for Move event update".to_string()
            ))?;

        // Add event to sequence
        sequence.add_event(event.clone())?;
        let new_state = sequence.state.clone();

        // Check if this move completes the game (game-specific logic would go here)
        // For now, we'll assume moves continue until Final events are received

        Ok(ProcessingResult::SequenceUpdated {
            challenge_id,
            event_id: event.id,
            new_state,
        })
    }

    /// Handle Final event - validate and potentially complete sequence
    async fn handle_final_event(&mut self, event: Event) -> GameResult<ProcessingResult> {
        let final_content = EventParser::parse_final(&event)?;
        
        let sequence = self.active_sequences.get_mut(&final_content.game_sequence_root)
            .ok_or_else(|| GameProtocolError::SequenceError(
                "Sequence not found for Final event".to_string()
            ))?;

        // Validate that we can accept final events
        if !sequence.state.can_accept_moves() && !sequence.state.needs_final_events() {
            return Ok(ProcessingResult::ValidationFailure {
                event_id: event.id,
                reason: "Sequence is not accepting Final events in current state".to_string(),
            });
        }

        // Add event to sequence
        sequence.add_event(event.clone())?;

        // Check if we have enough final events to complete the sequence
        // This would be game-specific, but for now we'll assume 2 final events are needed
        if sequence.count_final_events() >= 2 {
            return self.complete_sequence(final_content.game_sequence_root).await;
        }

        Ok(ProcessingResult::SequenceUpdated {
            challenge_id: final_content.game_sequence_root,
            event_id: event.id,
            new_state: sequence.state.clone(),
        })
    }

    /// Complete a game sequence and distribute rewards
    async fn complete_sequence(&mut self, challenge_id: EventId) -> GameResult<ProcessingResult> {
        let sequence = self.active_sequences.remove(&challenge_id)
            .ok_or_else(|| GameProtocolError::SequenceError(
                "Sequence not found for completion".to_string()
            ))?;

        // Validate the complete sequence
        let validation_result = sequence.validate_sequence_integrity()?;
        
        if !validation_result.is_valid {
            // Handle validation failure
            return self.publish_validation_failure(
                challenge_id,
                "Sequence validation failed".to_string(),
                None
            ).await;
        }

        // Determine winner (this would be game-specific)
        let winner = self.determine_winner(&sequence).await?;

        // Calculate reward amount
        let reward_amount = self.calculate_reward_amount(&sequence)?;
        
        // Calculate and distribute rewards
        match winner {
            Some(winner_pubkey) => {
                match self.calculate_and_distribute_rewards_with_amount(winner_pubkey, reward_amount, challenge_id).await {
                    Ok(reward_event_id) => {
                        // Move to completed sequences
                        self.completed_sequences.insert(challenge_id, sequence);
                        
                        Ok(ProcessingResult::SequenceCompleted {
                            challenge_id,
                            winner,
                            reward_event_id: Some(reward_event_id),
                        })
                    },
                    Err(e) => {
                        // Handle reward distribution failure
                        self.publish_validation_failure(
                            challenge_id,
                            format!("Reward distribution failed: {}", e),
                            None
                        ).await
                    }
                }
            },
            None => {
                // No winner (tie), complete without rewards
                self.completed_sequences.insert(challenge_id, sequence);
                Ok(ProcessingResult::SequenceCompleted {
                    challenge_id,
                    winner: None,
                    reward_event_id: None,
                })
            }
        }
    }

    /// Handle fraud detection and forfeit the cheating player
    async fn handle_fraud_detection(
        &mut self,
        challenge_id: EventId,
        cheating_player: PublicKey,
        reason: String,
        failed_event_id: Option<EventId>
    ) -> GameResult<ProcessingResult> {
        // First, get the data we need and calculate reward amount
        let (winner, reward_amount) = {
            let sequence = self.active_sequences.get_mut(&challenge_id)
                .ok_or_else(|| GameProtocolError::SequenceError(
                    "Sequence not found for fraud handling".to_string()
                ))?;

            // Forfeit the cheating player
            sequence.forfeit_player(cheating_player)?;

            // Determine the winner (the other player)
            let winner = if cheating_player == sequence.players[0] {
                sequence.players[1]
            } else {
                sequence.players[0]
            };

            // Calculate reward amount (using static method to avoid borrowing issues)
            let reward_amount = Self::calculate_reward_amount_static(sequence)?;
            
            (winner, reward_amount)
        };
        
        // Distribute rewards to the honest player
        match self.calculate_and_distribute_rewards_with_amount(winner, reward_amount, challenge_id).await {
            Ok(reward_event_id) => {
                // Move to completed sequences
                let completed_sequence = self.active_sequences.remove(&challenge_id).unwrap();
                self.completed_sequences.insert(challenge_id, completed_sequence);
                
                Ok(ProcessingResult::FraudDetected {
                    challenge_id,
                    cheating_player,
                    winner,
                    reason,
                    reward_event_id: Some(reward_event_id),
                })
            },
            Err(e) => {
                self.publish_validation_failure(
                    challenge_id,
                    format!("Fraud detected but reward distribution failed: {}. Original reason: {}", e, reason),
                    failed_event_id
                ).await
            }
        }
    }

    /// Validate revealed tokens against commitments (with pre-fetched commitments)
    async fn validate_revealed_tokens_with_commitments(
        &self,
        _player: &PublicKey,
        revealed_tokens: &[cdk::nuts::Token],
        commitment_hashes: &[String],
    ) -> GameResult<bool> {
        // Validate tokens using CDK
        if !self.mint.validate_tokens(revealed_tokens).await? {
            return Ok(false);
        }

        // Validate commitment hashes
        for (i, token) in revealed_tokens.iter().enumerate() {
            if i >= commitment_hashes.len() {
                return Ok(false); // More tokens than commitments
            }
            
            let commitment = crate::cashu::TokenCommitment::single(token);
            if commitment.commitment_hash != commitment_hashes[i] {
                return Ok(false); // Commitment doesn't match
            }
        }

        Ok(true)
    }

    /// Get player's commitment hashes from the sequence
    fn get_player_commitments(&self, player: &PublicKey, sequence: &GameSequence) -> GameResult<Vec<String>> {
        // Find the player's commitment event (Challenge or ChallengeAccept)
        for event in &sequence.events {
            if event.pubkey == *player {
                match event.kind {
                    k if k == CHALLENGE_KIND => {
                        let content = EventParser::parse_challenge(event)?;
                        return Ok(content.commitment_hashes);
                    },
                    k if k == crate::events::CHALLENGE_ACCEPT_KIND => {
                        let content = EventParser::parse_challenge_accept(event)?;
                        return Ok(content.commitment_hashes);
                    },
                    _ => continue,
                }
            }
        }
        
        Err(GameProtocolError::SequenceError(
            "Player commitment not found in sequence".to_string()
        ))
    }

    /// Find which sequence contains a specific event
    fn find_sequence_for_event(&self, event_id: &EventId) -> GameResult<EventId> {
        for (challenge_id, sequence) in &self.active_sequences {
            if sequence.events.iter().any(|e| e.id == *event_id) {
                return Ok(*challenge_id);
            }
        }
        
        Err(GameProtocolError::SequenceError(
            "Event not found in any active sequence".to_string()
        ))
    }

    /// Determine the winner of a completed sequence
    async fn determine_winner(&self, sequence: &GameSequence) -> GameResult<Option<PublicKey>> {
        // This is a placeholder implementation
        // In a real implementation, this would use game-specific logic
        // through the Game trait to determine the winner
        
        match &sequence.state {
            SequenceState::Complete { winner } => Ok(*winner),
            SequenceState::Forfeited { winner } => Ok(Some(*winner)),
            _ => {
                // For now, we'll return None (tie/no winner)
                // Real game implementations would analyze the moves and determine winner
                Ok(None)
            }
        }
    }

    /// Calculate rewards and distribute to winner (with pre-calculated amount)
    async fn calculate_and_distribute_rewards_with_amount(
        &self,
        winner: PublicKey,
        reward_amount: cdk::Amount,
        challenge_id: EventId,
    ) -> GameResult<EventId> {
        // Mint reward tokens locked to winner
        let reward_tokens = self.mint.mint_reward_tokens(reward_amount, winner).await?;

        // Publish reward event
        self.mint.publish_game_result(
            challenge_id,
            winner,
            reward_tokens
        ).await
    }

    /// Calculate rewards and distribute to winner (original method for complete sequences)
    async fn calculate_and_distribute_rewards(
        &self,
        sequence: &GameSequence,
        winner: Option<PublicKey>
    ) -> GameResult<EventId> {
        let winner = winner.ok_or_else(|| GameProtocolError::MintError(
            "Cannot distribute rewards without a winner".to_string()
        ))?;

        // Calculate reward amount (simplified - would be game-specific)
        let reward_amount = self.calculate_reward_amount(sequence)?;

        // Use the helper method
        self.calculate_and_distribute_rewards_with_amount(winner, reward_amount, sequence.challenge_id).await
    }

    /// Calculate the reward amount for a sequence
    fn calculate_reward_amount(&self, sequence: &GameSequence) -> GameResult<cdk::Amount> {
        Self::calculate_reward_amount_static(sequence)
    }
    
    /// Calculate the reward amount for a sequence (static version to avoid borrowing issues)
    fn calculate_reward_amount_static(_sequence: &GameSequence) -> GameResult<cdk::Amount> {
        // This is a placeholder implementation
        // In a real implementation, this would:
        // 1. Sum up all the Game tokens that were burned during the sequence
        // 2. Apply any game-specific reward multipliers
        // 3. Subtract any mint fees
        
        // For now, return a fixed amount
        Ok(cdk::Amount::from(1000)) // 1000 sats
    }

    /// Publish a validation failure event
    async fn publish_validation_failure(
        &self,
        challenge_id: EventId,
        reason: String,
        failed_event_id: Option<EventId>
    ) -> GameResult<ProcessingResult> {
        let failure_content = ValidationFailureContent {
            game_sequence_root: challenge_id,
            failure_reason: reason.clone(),
            failed_event_id,
        };

        let event = failure_content.to_event(self.mint.keys())?;
        let event_id = event.id;

        self.nostr_client.send_event(event).await
            .map_err(|e| GameProtocolError::NostrSdk(e.to_string()))?;

        Ok(ProcessingResult::ValidationFailure {
            event_id,
            reason,
        })
    }

    /// Check for timeouts and handle them
    pub async fn check_timeouts(&mut self) -> GameResult<Vec<ProcessingResult>> {
        let mut results = Vec::new();
        let now = chrono::Utc::now().timestamp() as u64;
        let mut timed_out_sequences = Vec::new();

        for (challenge_id, sequence) in &self.active_sequences {
            let timeout_occurred = match &sequence.state {
                SequenceState::WaitingForAccept => {
                    // Check if challenge has expired
                    now > sequence.created_at + self.config.move_timeout
                },
                SequenceState::InProgress => {
                    // Check if moves have timed out
                    now > sequence.last_activity + self.config.move_timeout
                },
                SequenceState::WaitingForFinal => {
                    // Check if final events have timed out
                    now > sequence.last_activity + self.config.final_event_timeout
                },
                _ => false, // Complete or forfeited sequences don't timeout
            };

            if timeout_occurred {
                timed_out_sequences.push(*challenge_id);
            }
        }

        // Handle timeouts
        for challenge_id in timed_out_sequences {
            match self.handle_timeout(challenge_id).await {
                Ok(result) => results.push(result),
                Err(e) => {
                    eprintln!("Error handling timeout for sequence {}: {}", challenge_id, e);
                }
            }
        }

        Ok(results)
    }

    /// Handle a timeout for a specific sequence
    async fn handle_timeout(&mut self, challenge_id: EventId) -> GameResult<ProcessingResult> {
        let sequence = self.active_sequences.get(&challenge_id)
            .ok_or_else(|| GameProtocolError::SequenceError(
                "Sequence not found for timeout handling".to_string()
            ))?;

        match &sequence.state {
            SequenceState::WaitingForAccept => {
                // Challenge expired, remove it
                self.active_sequences.remove(&challenge_id);
                Ok(ProcessingResult::SequenceExpired { challenge_id })
            },
            SequenceState::InProgress => {
                // Determine which player should be forfeited based on last activity
                // This is simplified - real implementation would track per-player timeouts
                let last_event = sequence.events.last().unwrap();
                let other_player = if last_event.pubkey == sequence.players[0] {
                    sequence.players[1]
                } else {
                    sequence.players[0]
                };
                
                self.handle_fraud_detection(
                    challenge_id,
                    other_player,
                    "Player timed out during gameplay".to_string(),
                    None
                ).await
            },
            SequenceState::WaitingForFinal => {
                // Determine which players haven't submitted final events
                let final_events = sequence.get_events_by_kind(crate::events::FINAL_KIND);
                let submitted_players: std::collections::HashSet<_> = 
                    final_events.iter().map(|e| e.pubkey).collect();
                
                // Forfeit the first player who hasn't submitted (simplified)
                let forfeited_player = sequence.players.iter()
                    .find(|p| !submitted_players.contains(p))
                    .copied()
                    .unwrap_or(sequence.players[0]); // Fallback
                
                self.handle_fraud_detection(
                    challenge_id,
                    forfeited_player,
                    "Player timed out submitting final event".to_string(),
                    None
                ).await
            },
            _ => {
                Err(GameProtocolError::SequenceError(
                    "Cannot timeout completed or forfeited sequence".to_string()
                ))
            }
        }
    }

    /// Get statistics about active sequences
    pub fn get_statistics(&self) -> SequenceStatistics {
        let mut stats = SequenceStatistics::default();
        
        for sequence in self.active_sequences.values() {
            match &sequence.state {
                SequenceState::WaitingForAccept => stats.waiting_for_accept += 1,
                SequenceState::InProgress => stats.in_progress += 1,
                SequenceState::WaitingForFinal => stats.waiting_for_final += 1,
                SequenceState::Complete { .. } => stats.completed += 1,
                SequenceState::Forfeited { .. } => stats.forfeited += 1,
            }
        }
        
        stats.total_completed = self.completed_sequences.len();
        stats
    }
}

/// Result of processing a single event
#[derive(Debug, Clone)]
pub enum ProcessingResult {
    /// New sequence created from Challenge event
    SequenceCreated {
        challenge_id: EventId,
        challenger: PublicKey,
    },
    /// Existing sequence updated with new event
    SequenceUpdated {
        challenge_id: EventId,
        event_id: EventId,
        new_state: SequenceState,
    },
    /// Sequence completed successfully
    SequenceCompleted {
        challenge_id: EventId,
        winner: Option<PublicKey>,
        reward_event_id: Option<EventId>,
    },
    /// Fraud detected and handled
    FraudDetected {
        challenge_id: EventId,
        cheating_player: PublicKey,
        winner: PublicKey,
        reason: String,
        reward_event_id: Option<EventId>,
    },
    /// Validation failure occurred
    ValidationFailure {
        event_id: EventId,
        reason: String,
    },
    /// Sequence expired due to timeout
    SequenceExpired {
        challenge_id: EventId,
    },
    /// Error occurred during processing
    Error {
        event_id: EventId,
        error: String,
    },
}

/// Statistics about sequence processing
#[derive(Debug, Default)]
pub struct SequenceStatistics {
    pub waiting_for_accept: usize,
    pub in_progress: usize,
    pub waiting_for_final: usize,
    pub completed: usize,
    pub forfeited: usize,
    pub total_completed: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{Keys, EventBuilder};
    use crate::events::ChallengeContent;
    
    fn create_test_keys() -> Keys {
        Keys::generate()
    }
    
    fn create_challenge_event(keys: &Keys) -> Event {
        let content = ChallengeContent {
            game_type: "test_game".to_string(),
            commitment_hashes: vec!["abc123".to_string()],
            game_parameters: serde_json::json!({}),
            expiry: Some(chrono::Utc::now().timestamp() as u64 + 3600),
        };
        
        EventBuilder::new(CHALLENGE_KIND, serde_json::to_string(&content).unwrap(), Vec::<nostr::Tag>::new())
            .to_event(keys)
            .unwrap()
    }
    
    #[tokio::test]
    async fn test_sequence_processor_creation() {
        // This test would require setting up a mock GameMint and NostrClient
        // For now, we'll just test that the structure compiles
        let config = SequenceProcessorConfig::default();
        assert_eq!(config.final_event_timeout, 3600);
        assert_eq!(config.move_timeout, 1800);
        assert!(config.auto_process);
    }
    
    #[test]
    fn test_processing_result_variants() {
        let keys = create_test_keys();
        let challenge_id = EventId::all_zeros();
        
        let result = ProcessingResult::SequenceCreated {
            challenge_id,
            challenger: keys.public_key(),
        };
        
        match result {
            ProcessingResult::SequenceCreated { .. } => {},
            _ => panic!("Expected SequenceCreated variant"),
        }
    }
    
    #[test]
    fn test_sequence_statistics() {
        let stats = SequenceStatistics::default();
        assert_eq!(stats.waiting_for_accept, 0);
        assert_eq!(stats.in_progress, 0);
        assert_eq!(stats.completed, 0);
    }
}
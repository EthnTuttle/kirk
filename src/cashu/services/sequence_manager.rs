//! Sequence management service for tracking game state

use std::collections::HashMap;
use nostr::{Event, EventId, PublicKey};
use tracing::{debug, info, warn};
use crate::error::{GameResult, GameProtocolError};
use crate::game::GameSequence;
use super::{ServiceContext, SequenceUpdate, event_processor::ParsedEvent};

/// Service responsible for managing game sequences and their state
#[derive(Debug)]
pub struct SequenceManager {
    context: ServiceContext,
    /// Active game sequences being tracked
    active_sequences: HashMap<EventId, GameSequence>,
    /// Completed sequences (for audit trail)
    completed_sequences: HashMap<EventId, GameSequence>,
    /// Mapping from event IDs to their sequence IDs
    event_to_sequence: HashMap<EventId, EventId>,
}

impl SequenceManager {
    /// Create a new sequence manager
    pub fn new(context: ServiceContext) -> Self {
        Self {
            context,
            active_sequences: HashMap::new(),
            completed_sequences: HashMap::new(),
            event_to_sequence: HashMap::new(),
        }
    }

    /// Extract winner from sequence state
    fn get_winner_from_state(state: &crate::game::SequenceState) -> Option<nostr::PublicKey> {
        match state {
            crate::game::SequenceState::Complete { winner } => *winner,
            crate::game::SequenceState::Forfeited { winner } => Some(*winner),
            _ => None,
        }
    }

    /// Handle a new event and update sequence state
    pub async fn handle_event(&mut self, event: &Event, parsed_event: &ParsedEvent) -> GameResult<SequenceUpdate> {
        debug!(
            event_id = %event.id,
            event_type = parsed_event.event_type(),
            author = %parsed_event.author(),
            "Handling event for sequence update"
        );

        match parsed_event {
            ParsedEvent::Challenge { content, author } => {
                self.handle_challenge_event(event, content, *author).await
            },
            ParsedEvent::ChallengeAccept { content, author } => {
                self.handle_challenge_accept_event(event, content, *author).await
            },
            ParsedEvent::Move { content, author } => {
                self.handle_move_event(event, content, *author).await
            },
            ParsedEvent::Final { content, author } => {
                self.handle_final_event(event, content, *author).await
            },
            ParsedEvent::Unknown { .. } => {
                Ok(SequenceUpdate {
                    sequence_id: event.id, // Use event ID as fallback
                    sequence: None,
                    is_complete: false,
                    action_taken: "Ignored unknown event".to_string(),
                })
            },
        }
    }

    /// Handle a challenge event (creates new sequence)
    async fn handle_challenge_event(
        &mut self,
        event: &Event,
        _content: &crate::events::ChallengeContent,
        author: PublicKey,
    ) -> GameResult<SequenceUpdate> {
        let sequence_id = event.id;

        // Check if we already have too many sequences for this player
        let player_sequences = self.active_sequences.values()
            .filter(|seq| seq.players[0] == author)
            .count();

        if player_sequences >= self.context.constants.max_concurrent_sequences {
            return Err(GameProtocolError::Validation {
                message: format!("Player has too many active sequences: {}", player_sequences),
                field: Some("concurrent_sequences".to_string()),
                event_id: Some(event.id),
            });
        }

        // Create new sequence
        let sequence = GameSequence::new(event.clone(), author)?;

        // Store the sequence
        self.active_sequences.insert(sequence_id, sequence.clone());
        self.event_to_sequence.insert(event.id, sequence_id);

        info!(
            sequence_id = %sequence_id,
            challenger = %author,
            "Created new game sequence"
        );

        Ok(SequenceUpdate {
            sequence_id,
            sequence: Some(sequence),
            is_complete: false,
            action_taken: "Created new sequence".to_string(),
        })
    }

    /// Handle a challenge accept event
    async fn handle_challenge_accept_event(
        &mut self,
        event: &Event,
        content: &crate::events::ChallengeAcceptContent,
        author: PublicKey,
    ) -> GameResult<SequenceUpdate> {
        let sequence_id = content.challenge_id;

        // Find the sequence
        let sequence = self.active_sequences.get_mut(&sequence_id)
            .ok_or_else(|| GameProtocolError::Validation {
                message: format!("Sequence {} not found", sequence_id),
                field: Some("challenge_id".to_string()),
                event_id: Some(event.id),
            })?;

        // Add the accept event
        sequence.add_event(event.clone())?;
        self.event_to_sequence.insert(event.id, sequence_id);

        info!(
            sequence_id = %sequence_id,
            accepter = %author,
            "Challenge accepted"
        );

        Ok(SequenceUpdate {
            sequence_id,
            sequence: Some(sequence.clone()),
            is_complete: false,
            action_taken: "Challenge accepted".to_string(),
        })
    }

    /// Handle a move event
    async fn handle_move_event(
        &mut self,
        event: &Event,
        content: &crate::events::MoveContent,
        author: PublicKey,
    ) -> GameResult<SequenceUpdate> {
        let sequence_id = content.previous_event_id;

        // Find the sequence
        let sequence = self.active_sequences.get_mut(&sequence_id)
            .ok_or_else(|| GameProtocolError::Validation {
                message: format!("Sequence {} not found", sequence_id),
                field: Some("challenge_id".to_string()),
                event_id: Some(event.id),
            })?;

        // Validate player is part of this game
        let challenger = sequence.players[0];
        let accepter = sequence.players[1];

        if challenger != author && accepter != author {
            return Err(GameProtocolError::Validation {
                message: "Player not part of this game".to_string(),
                field: Some("author".to_string()),
                event_id: Some(event.id),
            });
        }

        // Add the move event
        sequence.add_event(event.clone())?;
        self.event_to_sequence.insert(event.id, sequence_id);

        info!(
            sequence_id = %sequence_id,
            player = %author,
            move_type = ?content.move_type,
            "Move added to sequence"
        );

        Ok(SequenceUpdate {
            sequence_id,
            sequence: Some(sequence.clone()),
            is_complete: false,
            action_taken: format!("Added {} move", content.move_type),
        })
    }

    /// Handle a final event
    async fn handle_final_event(
        &mut self,
        event: &Event,
        content: &crate::events::FinalContent,
        author: PublicKey,
    ) -> GameResult<SequenceUpdate> {
        let sequence_id = content.game_sequence_root;

        // Find the sequence
        let sequence = self.active_sequences.get_mut(&sequence_id)
            .ok_or_else(|| GameProtocolError::Validation {
                message: format!("Sequence {} not found", sequence_id),
                field: Some("challenge_id".to_string()),
                event_id: Some(event.id),
            })?;

        // Validate player is part of this game
        let challenger = sequence.players[0];
        let accepter = sequence.players[1];

        if challenger != author && accepter != author {
            return Err(GameProtocolError::Validation {
                message: "Player not part of this game".to_string(),
                field: Some("author".to_string()),
                event_id: Some(event.id),
            });
        }

        // Add the final event
        sequence.add_event(event.clone())?;
        self.event_to_sequence.insert(event.id, sequence_id);

        // Check if sequence is complete
        let is_complete = sequence.state.is_finished();

        if is_complete {
            // Move to completed sequences
            let completed_sequence = self.active_sequences.remove(&sequence_id)
                .expect("Sequence should exist");
            self.completed_sequences.insert(sequence_id, completed_sequence.clone());

            info!(
                sequence_id = %sequence_id,
                winner = ?Self::get_winner_from_state(&completed_sequence.state),
                "Sequence completed"
            );

            Ok(SequenceUpdate {
                sequence_id,
                sequence: Some(completed_sequence),
                is_complete: true,
                action_taken: "Sequence completed".to_string(),
            })
        } else {
            info!(
                sequence_id = %sequence_id,
                "Final event added, awaiting completion"
            );

            Ok(SequenceUpdate {
                sequence_id,
                sequence: Some(sequence.clone()),
                is_complete: false,
                action_taken: "Final event added".to_string(),
            })
        }
    }

    /// Get an active sequence by ID
    pub fn get_active_sequence(&self, sequence_id: &EventId) -> Option<&GameSequence> {
        self.active_sequences.get(sequence_id)
    }

    /// Get a completed sequence by ID
    pub fn get_completed_sequence(&self, sequence_id: &EventId) -> Option<&GameSequence> {
        self.completed_sequences.get(sequence_id)
    }

    /// Get all active sequences
    pub fn get_active_sequences(&self) -> &HashMap<EventId, GameSequence> {
        &self.active_sequences
    }

    /// Get all completed sequences
    pub fn get_completed_sequences(&self) -> &HashMap<EventId, GameSequence> {
        &self.completed_sequences
    }

    /// Find which sequence an event belongs to
    pub fn find_sequence_for_event(&self, event_id: &EventId) -> Option<EventId> {
        self.event_to_sequence.get(event_id).copied()
    }

    /// Get statistics about managed sequences
    pub fn get_statistics(&self) -> SequenceManagerStats {
        SequenceManagerStats {
            active_sequences: self.active_sequences.len(),
            completed_sequences: self.completed_sequences.len(),
            total_events_tracked: self.event_to_sequence.len(),
        }
    }

    /// Clean up old completed sequences to prevent memory leaks
    pub async fn cleanup_old_sequences(&mut self) -> GameResult<usize> {
        let cutoff_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .saturating_sub(self.context.constants.cleanup_interval);

        let mut removed_count = 0;

        // Find sequences to remove (older than cutoff)
        let sequences_to_remove: Vec<EventId> = self.completed_sequences
            .iter()
            .filter_map(|(id, sequence)| {
                // Get the earliest event timestamp in the sequence
                let earliest_time = sequence.events.first()
                    .map(|event| event.created_at.as_u64())
                    .unwrap_or(0);

                if earliest_time < cutoff_time {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();

        // Remove old sequences
        for sequence_id in sequences_to_remove {
            if let Some(sequence) = self.completed_sequences.remove(&sequence_id) {
                removed_count += 1;

                // Also remove event mappings
                for event in &sequence.events {
                    self.event_to_sequence.remove(&event.id);
                }

                debug!(
                    sequence_id = %sequence_id,
                    "Cleaned up old sequence"
                );
            }
        }

        if removed_count > 0 {
            info!(
                removed_count = removed_count,
                remaining_active = self.active_sequences.len(),
                remaining_completed = self.completed_sequences.len(),
                "Cleaned up old sequences"
            );
        }

        Ok(removed_count)
    }

    /// Force complete a sequence (for timeout handling)
    pub async fn force_complete_sequence(&mut self, sequence_id: &EventId, reason: &str) -> GameResult<Option<GameSequence>> {
        if let Some(sequence) = self.active_sequences.remove(sequence_id) {
            // sequence.forfeit_player(sequence.players[0])?; // Would need to determine the forfeited player based on reason

            let completed_sequence = sequence.clone();
            self.completed_sequences.insert(*sequence_id, completed_sequence.clone());

            info!(
                sequence_id = %sequence_id,
                reason = reason,
                "Force completed sequence"
            );

            Ok(Some(completed_sequence))
        } else {
            warn!(
                sequence_id = %sequence_id,
                reason = reason,
                "Attempted to force complete non-existent sequence"
            );

            Ok(None)
        }
    }
}

/// Statistics about the sequence manager
#[derive(Debug, Clone)]
pub struct SequenceManagerStats {
    pub active_sequences: usize,
    pub completed_sequences: usize,
    pub total_events_tracked: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{Keys, EventBuilder, Kind, Tag};
    use crate::cashu::GameMint;
    use std::sync::Arc;

    fn create_test_context() -> ServiceContext {
        let keys = Keys::generate();
        let mint = Arc::new(GameMint::new_test(keys));
        let nostr_client = nostr_sdk::Client::default();
        let observability = Arc::new(crate::observability::ObservabilityManager::new(
            crate::observability::ObservabilityConfig::default()
        ));
        ServiceContext::new(mint, nostr_client, observability)
    }

    #[tokio::test]
    async fn test_sequence_lifecycle() {
        let mut manager = SequenceManager::new(create_test_context());

        let challenger_keys = Keys::generate();
        let accepter_keys = Keys::generate();

        // Create challenge event
        let challenge_event = EventBuilder::new(
            Kind::from(crate::events::CHALLENGE_KIND),
            r#"{"game_type": "test", "stakes": []}"#,
            []
        ).to_event(&challenger_keys).unwrap();

        let challenge_parsed = crate::cashu::services::event_processor::ParsedEvent::Challenge {
            content: crate::events::ChallengeContent {
                game_type: "test".to_string(),
                commitment_hashes: vec!["test_hash".to_string()],
                game_parameters: serde_json::json!({}),
                expiry: None,
                timeout_config: None,
            },
            author: challenger_keys.public_key(),
        };

        // Handle challenge
        let result = manager.handle_event(&challenge_event, &challenge_parsed).await;
        assert!(result.is_ok());

        let update = result.unwrap();
        assert!(!update.is_complete);
        assert_eq!(update.action_taken, "Created new sequence");

        // Verify sequence was created
        let sequence_id = challenge_event.id;
        assert!(manager.get_active_sequence(&sequence_id).is_some());
        assert_eq!(manager.get_statistics().active_sequences, 1);
    }

    #[tokio::test]
    async fn test_concurrent_sequence_limit() {
        let mut manager = SequenceManager::new(create_test_context());

        let challenger_keys = Keys::generate();

        // Create maximum allowed sequences
        for i in 0..manager.context.constants.max_concurrent_sequences {
            let challenge_event = EventBuilder::new(
                Kind::from(crate::events::CHALLENGE_KIND),
                format!(r#"{{"game_type": "test_{}", "stakes": []}}"#, i),
                []
            ).to_event(&challenger_keys).unwrap();

            let challenge_parsed = crate::cashu::services::event_processor::ParsedEvent::Challenge {
                content: crate::events::ChallengeContent {
                    game_type: format!("test_{}", i),
                    commitment_hashes: vec!["test_hash".to_string()],
                    game_parameters: serde_json::json!({}),
                    expiry: None,
                    timeout_config: None,
                },
                author: challenger_keys.public_key(),
            };

            let result = manager.handle_event(&challenge_event, &challenge_parsed).await;
            assert!(result.is_ok());
        }

        // Try to create one more - should fail
        let extra_challenge = EventBuilder::new(
            Kind::from(crate::events::CHALLENGE_KIND),
            r#"{"game_type": "extra", "stakes": []}"#,
            []
        ).to_event(&challenger_keys).unwrap();

        let extra_parsed = crate::cashu::services::event_processor::ParsedEvent::Challenge {
            content: crate::events::ChallengeContent {
                game_type: "extra".to_string(),
                commitment_hashes: vec!["test_hash".to_string()],
                game_parameters: serde_json::json!({}),
                expiry: None,
                timeout_config: None,
            },
            author: challenger_keys.public_key(),
        };

        let result = manager.handle_event(&extra_challenge, &extra_parsed).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_cleanup_old_sequences() {
        let mut manager = SequenceManager::new(create_test_context());

        // Add a completed sequence (simulate by directly inserting)
        let sequence_id = EventId::from_hex("1234567890abcdef1234567890abcdef12345678").unwrap();
        let keys = Keys::generate();
        let challenge_event = EventBuilder::new(
            Kind::from(crate::events::CHALLENGE_KIND),
            r#"{"game_type": "test", "commitment_hashes": ["test"], "stakes": []}"#,
            []
        ).to_event(&keys).unwrap();
        let sequence = crate::game::GameSequence::new(challenge_event, keys.public_key()).unwrap();
        manager.completed_sequences.insert(sequence_id, sequence);

        let stats_before = manager.get_statistics();
        assert_eq!(stats_before.completed_sequences, 1);

        // Clean up (with very recent cutoff, so nothing should be removed)
        let removed = manager.cleanup_old_sequences().await.unwrap();
        assert_eq!(removed, 0);

        let stats_after = manager.get_statistics();
        assert_eq!(stats_after.completed_sequences, 1);
    }
}
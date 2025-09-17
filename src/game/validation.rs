//! Game sequence validation and state management

use nostr::{Event, EventId, PublicKey};
use std::collections::HashMap;
use crate::error::{GameProtocolError, ValidationError, ValidationErrorType, ValidationResult, GameResult};
use crate::events::{EventParser, CHALLENGE_KIND, CHALLENGE_ACCEPT_KIND, MOVE_KIND, FINAL_KIND};
use crate::game::traits::Game;

/// Represents a complete game sequence with state tracking
#[derive(Debug, Clone)]
pub struct GameSequence {
    pub challenge_id: EventId,
    pub players: [PublicKey; 2],
    pub events: Vec<Event>,
    pub state: SequenceState,
    pub created_at: u64,
    pub last_activity: u64,
}

/// State transitions for game sequences
#[derive(Debug, Clone)]
pub enum SequenceState {
    /// Challenge published, waiting for ChallengeAccept
    WaitingForAccept,
    /// Both players committed, game moves are happening
    InProgress,
    /// All moves complete, waiting for Final events from players
    WaitingForFinal,
    /// All Final events received, game validated and complete
    Complete { winner: Option<PublicKey> },
    /// Player forfeited (timeout, invalid move, etc.)
    Forfeited { winner: PublicKey },
}

impl SequenceState {
    /// Check if the game sequence can accept new moves
    pub fn can_accept_moves(&self) -> bool {
        matches!(self, SequenceState::InProgress)
    }
    
    /// Check if the game is waiting for Final events
    pub fn needs_final_events(&self) -> bool {
        matches!(self, SequenceState::WaitingForFinal)
    }
    
    /// Check if the game is complete (finished or forfeited)
    pub fn is_finished(&self) -> bool {
        matches!(self, SequenceState::Complete { .. } | SequenceState::Forfeited { .. })
    }
    
    /// Get the next valid state transitions from current state
    pub fn valid_transitions(&self) -> Vec<SequenceState> {
        let placeholder_key = PublicKey::from_hex("0000000000000000000000000000000000000000000000000000000000000001").unwrap();
        match self {
            SequenceState::WaitingForAccept => vec![
                SequenceState::InProgress,
                SequenceState::Forfeited { winner: placeholder_key }
            ],
            SequenceState::InProgress => vec![
                SequenceState::WaitingForFinal,
                SequenceState::Forfeited { winner: placeholder_key }
            ],
            SequenceState::WaitingForFinal => vec![
                SequenceState::Complete { winner: None },
                SequenceState::Forfeited { winner: placeholder_key }
            ],
            SequenceState::Complete { .. } | SequenceState::Forfeited { .. } => vec![],
        }
    }
    
    /// Check if a state transition is valid
    pub fn can_transition_to(&self, new_state: &SequenceState) -> bool {
        match (self, new_state) {
            // Allow staying in the same state
            (SequenceState::WaitingForAccept, SequenceState::WaitingForAccept) => true,
            (SequenceState::InProgress, SequenceState::InProgress) => true,
            (SequenceState::WaitingForFinal, SequenceState::WaitingForFinal) => true,
            // Valid forward transitions
            (SequenceState::WaitingForAccept, SequenceState::InProgress) => true,
            (SequenceState::WaitingForAccept, SequenceState::Forfeited { .. }) => true,
            (SequenceState::InProgress, SequenceState::WaitingForFinal) => true,
            (SequenceState::InProgress, SequenceState::Forfeited { .. }) => true,
            (SequenceState::WaitingForFinal, SequenceState::Complete { .. }) => true,
            (SequenceState::WaitingForFinal, SequenceState::Forfeited { .. }) => true,
            // Terminal states cannot transition
            (SequenceState::Complete { .. }, _) => false,
            (SequenceState::Forfeited { .. }, _) => false,
            _ => false,
        }
    }
}

impl GameSequence {
    /// Create new game sequence from challenge event
    pub fn new(challenge_event: Event, challenger: PublicKey) -> GameResult<Self> {
        // Validate that this is actually a challenge event
        if challenge_event.kind != CHALLENGE_KIND {
            return Err(GameProtocolError::SequenceError(
                format!("Expected Challenge event, got kind {}", challenge_event.kind)
            ));
        }
        
        // Validate the challenge event content
        EventParser::parse_challenge(&challenge_event)?;
        
        Ok(Self {
            challenge_id: challenge_event.id,
            players: [challenger, PublicKey::from_hex("0000000000000000000000000000000000000000000000000000000000000001").unwrap()], // Placeholder for second player
            events: vec![challenge_event],
            state: SequenceState::WaitingForAccept,
            created_at: chrono::Utc::now().timestamp() as u64,
            last_activity: chrono::Utc::now().timestamp() as u64,
        })
    }
    
    /// Add event to sequence and update state
    pub fn add_event(&mut self, event: Event) -> GameResult<()> {
        // Validate event structure first
        crate::events::validate_event_structure(&event)?;
        
        // Validate event chain integrity
        self.validate_event_chain_integrity(&event)?;
        
        // Determine new state based on event type and current state
        let new_state = self.determine_new_state(&event)?;
        
        // Validate state transition
        if !self.state.can_transition_to(&new_state) {
            return Err(GameProtocolError::SequenceError(
                format!("Invalid state transition from {:?} to {:?}", self.state, new_state)
            ));
        }
        
        // Update second player if this is a ChallengeAccept event
        if event.kind == CHALLENGE_ACCEPT_KIND {
            self.players[1] = event.pubkey;
        }
        
        // Add event and update state
        self.events.push(event);
        self.state = new_state;
        self.last_activity = chrono::Utc::now().timestamp() as u64;
        
        Ok(())
    }
    
    /// Validate the complete sequence integrity
    pub fn validate_sequence_integrity(&self) -> GameResult<ValidationResult> {
        let mut errors = Vec::new();
        
        // Validate event chain
        if let Err(e) = self.validate_complete_event_chain() {
            errors.push(ValidationError {
                event_id: self.challenge_id,
                error_type: ValidationErrorType::InvalidSequence,
                message: e.to_string(),
            });
        }
        
        // Validate state consistency
        if let Err(e) = self.validate_state_consistency() {
            errors.push(ValidationError {
                event_id: self.challenge_id,
                error_type: ValidationErrorType::InvalidSequence,
                message: e.to_string(),
            });
        }
        
        // Validate player consistency
        if let Err(e) = self.validate_player_consistency() {
            errors.push(ValidationError {
                event_id: self.challenge_id,
                error_type: ValidationErrorType::InvalidSequence,
                message: e.to_string(),
            });
        }
        
        let is_valid = errors.is_empty();
        let winner = match &self.state {
            SequenceState::Complete { winner } => *winner,
            SequenceState::Forfeited { winner } => Some(*winner),
            _ => None,
        };
        
        let forfeited_player = match &self.state {
            SequenceState::Forfeited { winner } => {
                // The forfeited player is the other player
                if *winner == self.players[0] {
                    Some(self.players[1])
                } else {
                    Some(self.players[0])
                }
            },
            _ => None,
        };
        
        Ok(ValidationResult {
            is_valid,
            winner,
            errors,
            forfeited_player,
        })
    }
    
    /// Get events by type
    pub fn get_events_by_kind(&self, kind: nostr::Kind) -> Vec<&Event> {
        self.events.iter().filter(|e| e.kind == kind).collect()
    }
    
    /// Get events by player
    pub fn get_events_by_player(&self, player: &PublicKey) -> Vec<&Event> {
        self.events.iter().filter(|e| e.pubkey == *player).collect()
    }
    
    /// Count final events received
    pub fn count_final_events(&self) -> usize {
        self.get_events_by_kind(FINAL_KIND).len()
    }
    
    /// Check if sequence has required final events for completion
    pub fn has_required_final_events<G: Game>(&self, game: &G) -> bool {
        self.count_final_events() >= game.required_final_events()
    }
    
    /// Validate event chain integrity for a new event
    fn validate_event_chain_integrity(&self, new_event: &Event) -> GameResult<()> {
        // For the first event (Challenge), no previous event reference needed
        if self.events.is_empty() {
            return Ok(());
        }
        
        // For ChallengeAccept, it should reference the Challenge
        if new_event.kind == CHALLENGE_ACCEPT_KIND {
            let accept_content = EventParser::parse_challenge_accept(new_event)?;
            if accept_content.challenge_id != self.challenge_id {
                return Err(GameProtocolError::SequenceError(
                    "ChallengeAccept does not reference the correct Challenge".to_string()
                ));
            }
        }
        
        // For Move events, validate previous event reference
        if new_event.kind == MOVE_KIND {
            let move_content = EventParser::parse_move(new_event)?;
            
            // Check if the referenced previous event exists in our sequence
            let previous_exists = self.events.iter().any(|e| e.id == move_content.previous_event_id);
            if !previous_exists {
                return Err(GameProtocolError::SequenceError(
                    "Move event references non-existent previous event".to_string()
                ));
            }
        }
        
        // For Final events, validate game sequence root reference
        if new_event.kind == FINAL_KIND {
            let final_content = EventParser::parse_final(new_event)?;
            if final_content.game_sequence_root != self.challenge_id {
                return Err(GameProtocolError::SequenceError(
                    "Final event does not reference the correct game sequence root".to_string()
                ));
            }
        }
        
        Ok(())
    }
    
    /// Determine new state based on incoming event
    fn determine_new_state(&self, event: &Event) -> GameResult<SequenceState> {
        match (&self.state, event.kind) {
            (SequenceState::WaitingForAccept, k) if k == CHALLENGE_ACCEPT_KIND => {
                Ok(SequenceState::InProgress)
            },
            (SequenceState::InProgress, k) if k == MOVE_KIND => {
                // Stay in progress unless this is a game-ending move
                // For now, we'll stay in InProgress - game-specific logic will determine
                // when to transition to WaitingForFinal
                Ok(SequenceState::InProgress)
            },
            (SequenceState::InProgress, k) if k == FINAL_KIND => {
                Ok(SequenceState::WaitingForFinal)
            },
            (SequenceState::WaitingForFinal, k) if k == FINAL_KIND => {
                // Check if we have enough final events to complete
                let final_count = self.count_final_events() + 1; // +1 for the new event
                if final_count >= 2 { // Assuming 2 players need to submit final events
                    Ok(SequenceState::Complete { winner: None })
                } else {
                    Ok(SequenceState::WaitingForFinal)
                }
            },
            _ => {
                // Invalid event for current state
                Err(GameProtocolError::SequenceError(
                    format!("Event kind {:?} not valid for state {:?}", event.kind, self.state)
                ))
            }
        }
    }
    
    /// Validate the complete event chain
    fn validate_complete_event_chain(&self) -> GameResult<()> {
        if self.events.is_empty() {
            return Err(GameProtocolError::SequenceError(
                "Empty event sequence".to_string()
            ));
        }
        
        // First event must be Challenge
        if self.events[0].kind != CHALLENGE_KIND {
            return Err(GameProtocolError::SequenceError(
                "First event must be Challenge".to_string()
            ));
        }
        
        // Build event reference map
        let mut event_map: HashMap<EventId, &Event> = HashMap::new();
        for event in &self.events {
            event_map.insert(event.id, event);
        }
        
        // Validate each event's references
        for event in &self.events[1..] { // Skip first event (Challenge)
            match event.kind {
                k if k == CHALLENGE_ACCEPT_KIND => {
                    let content = EventParser::parse_challenge_accept(event)?;
                    if !event_map.contains_key(&content.challenge_id) {
                        return Err(GameProtocolError::SequenceError(
                            "ChallengeAccept references unknown Challenge".to_string()
                        ));
                    }
                },
                k if k == MOVE_KIND => {
                    let content = EventParser::parse_move(event)?;
                    if !event_map.contains_key(&content.previous_event_id) {
                        return Err(GameProtocolError::SequenceError(
                            "Move references unknown previous event".to_string()
                        ));
                    }
                },
                k if k == FINAL_KIND => {
                    let content = EventParser::parse_final(event)?;
                    if !event_map.contains_key(&content.game_sequence_root) {
                        return Err(GameProtocolError::SequenceError(
                            "Final event references unknown game sequence root".to_string()
                        ));
                    }
                },
                _ => {
                    return Err(GameProtocolError::SequenceError(
                        format!("Unknown event kind in sequence: {}", event.kind)
                    ));
                }
            }
        }
        
        Ok(())
    }
    
    /// Validate state consistency with events
    fn validate_state_consistency(&self) -> GameResult<()> {
        let challenge_count = self.get_events_by_kind(CHALLENGE_KIND).len();
        let accept_count = self.get_events_by_kind(CHALLENGE_ACCEPT_KIND).len();
        let _move_count = self.get_events_by_kind(MOVE_KIND).len();
        let final_count = self.get_events_by_kind(FINAL_KIND).len();
        
        match &self.state {
            SequenceState::WaitingForAccept => {
                if challenge_count != 1 || accept_count != 0 {
                    return Err(GameProtocolError::SequenceError(
                        "WaitingForAccept state inconsistent with events".to_string()
                    ));
                }
            },
            SequenceState::InProgress => {
                if challenge_count != 1 || accept_count != 1 {
                    return Err(GameProtocolError::SequenceError(
                        "InProgress state inconsistent with events".to_string()
                    ));
                }
            },
            SequenceState::WaitingForFinal => {
                if challenge_count != 1 || accept_count != 1 || final_count == 0 {
                    return Err(GameProtocolError::SequenceError(
                        "WaitingForFinal state inconsistent with events".to_string()
                    ));
                }
            },
            SequenceState::Complete { .. } => {
                if challenge_count != 1 || accept_count != 1 || final_count < 1 {
                    return Err(GameProtocolError::SequenceError(
                        "Complete state inconsistent with events".to_string()
                    ));
                }
            },
            SequenceState::Forfeited { .. } => {
                // Forfeited can happen at any point, so less strict validation
                if challenge_count != 1 {
                    return Err(GameProtocolError::SequenceError(
                        "Forfeited state inconsistent with events".to_string()
                    ));
                }
            },
        }
        
        Ok(())
    }
    
    /// Validate player consistency across events
    fn validate_player_consistency(&self) -> GameResult<()> {
        // Check that all events are from one of the two players
        for event in &self.events {
            if event.pubkey != self.players[0] && event.pubkey != self.players[1] {
                // Allow placeholder for second player until ChallengeAccept
                let placeholder_key = PublicKey::from_hex("0000000000000000000000000000000000000000000000000000000000000001").unwrap();
                if self.players[1] == placeholder_key && 
                   event.kind == CHALLENGE_ACCEPT_KIND {
                    continue; // This will set the second player
                }
                
                return Err(GameProtocolError::SequenceError(
                    "Event from unknown player".to_string()
                ));
            }
        }
        
        Ok(())
    }
    
    /// Force forfeit a player (for timeout or rule violations)
    pub fn forfeit_player(&mut self, forfeited_player: PublicKey) -> GameResult<()> {
        // Determine winner (the other player)
        let winner = if forfeited_player == self.players[0] {
            self.players[1]
        } else if forfeited_player == self.players[1] {
            self.players[0]
        } else {
            return Err(GameProtocolError::SequenceError(
                "Cannot forfeit unknown player".to_string()
            ));
        };
        
        // Update state to forfeited
        self.state = SequenceState::Forfeited { winner };
        self.last_activity = chrono::Utc::now().timestamp() as u64;
        
        Ok(())
    }
    
    /// Complete the game with a winner
    pub fn complete_game(&mut self, winner: Option<PublicKey>) -> GameResult<()> {
        // Validate that we're in a state that can be completed
        if !matches!(self.state, SequenceState::WaitingForFinal) {
            return Err(GameProtocolError::SequenceError(
                "Cannot complete game from current state".to_string()
            ));
        }
        
        // Validate winner is one of the players (if specified)
        if let Some(winner_key) = winner {
            if winner_key != self.players[0] && winner_key != self.players[1] {
                return Err(GameProtocolError::SequenceError(
                    "Winner must be one of the game players".to_string()
                ));
            }
        }
        
        self.state = SequenceState::Complete { winner };
        self.last_activity = chrono::Utc::now().timestamp() as u64;
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{Keys, EventBuilder};
    use crate::events::{ChallengeContent, ChallengeAcceptContent, MoveContent, FinalContent, MoveType};
    
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
    
    fn create_challenge_accept_event(keys: &Keys, challenge_id: EventId) -> Event {
        let content = ChallengeAcceptContent {
            challenge_id,
            commitment_hashes: vec!["def456".to_string()],
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
    fn test_sequence_state_transitions() {
        let state = SequenceState::WaitingForAccept;
        assert!(state.can_transition_to(&SequenceState::InProgress));
        assert!(!state.can_transition_to(&SequenceState::Complete { winner: None }));
        
        let state = SequenceState::InProgress;
        assert!(state.can_accept_moves());
        assert!(!state.is_finished());
        assert!(state.can_transition_to(&SequenceState::WaitingForFinal));
        
        let state = SequenceState::WaitingForFinal;
        assert!(state.needs_final_events());
        assert!(state.can_transition_to(&SequenceState::Complete { winner: None }));
        
        let state = SequenceState::Complete { winner: None };
        assert!(state.is_finished());
        assert!(!state.can_accept_moves());
    }
    
    #[test]
    fn test_game_sequence_creation() {
        let keys = create_test_keys();
        let challenge_event = create_challenge_event(&keys);
        let challenger = keys.public_key();
        
        let sequence = GameSequence::new(challenge_event.clone(), challenger).unwrap();
        
        assert_eq!(sequence.challenge_id, challenge_event.id);
        assert_eq!(sequence.players[0], challenger);
        assert_eq!(sequence.events.len(), 1);
        assert!(matches!(sequence.state, SequenceState::WaitingForAccept));
    }
    
    #[test]
    fn test_add_challenge_accept_event() {
        let challenger_keys = create_test_keys();
        let accepter_keys = create_test_keys();
        
        let challenge_event = create_challenge_event(&challenger_keys);
        let mut sequence = GameSequence::new(challenge_event.clone(), challenger_keys.public_key()).unwrap();
        
        let accept_event = create_challenge_accept_event(&accepter_keys, challenge_event.id);
        sequence.add_event(accept_event.clone()).unwrap();
        
        assert_eq!(sequence.events.len(), 2);
        assert_eq!(sequence.players[1], accepter_keys.public_key());
        assert!(matches!(sequence.state, SequenceState::InProgress));
    }
    
    #[test]
    fn test_add_move_event() {
        let challenger_keys = create_test_keys();
        let accepter_keys = create_test_keys();
        
        let challenge_event = create_challenge_event(&challenger_keys);
        let mut sequence = GameSequence::new(challenge_event.clone(), challenger_keys.public_key()).unwrap();
        
        let accept_event = create_challenge_accept_event(&accepter_keys, challenge_event.id);
        sequence.add_event(accept_event.clone()).unwrap();
        
        let move_event = create_move_event(&challenger_keys, accept_event.id);
        sequence.add_event(move_event).unwrap();
        
        assert_eq!(sequence.events.len(), 3);
        assert!(matches!(sequence.state, SequenceState::InProgress));
    }
    
    #[test]
    fn test_add_final_event() {
        let challenger_keys = create_test_keys();
        let accepter_keys = create_test_keys();
        
        let challenge_event = create_challenge_event(&challenger_keys);
        let mut sequence = GameSequence::new(challenge_event.clone(), challenger_keys.public_key()).unwrap();
        
        let accept_event = create_challenge_accept_event(&accepter_keys, challenge_event.id);
        sequence.add_event(accept_event.clone()).unwrap();
        
        let final_event = create_final_event(&challenger_keys, challenge_event.id);
        sequence.add_event(final_event).unwrap();
        
        assert_eq!(sequence.events.len(), 3);
        assert!(matches!(sequence.state, SequenceState::WaitingForFinal));
    }
    
    #[test]
    fn test_complete_sequence_with_two_final_events() {
        let challenger_keys = create_test_keys();
        let accepter_keys = create_test_keys();
        
        let challenge_event = create_challenge_event(&challenger_keys);
        let mut sequence = GameSequence::new(challenge_event.clone(), challenger_keys.public_key()).unwrap();
        
        let accept_event = create_challenge_accept_event(&accepter_keys, challenge_event.id);
        sequence.add_event(accept_event).unwrap();
        
        let final_event1 = create_final_event(&challenger_keys, challenge_event.id);
        sequence.add_event(final_event1).unwrap();
        
        let final_event2 = create_final_event(&accepter_keys, challenge_event.id);
        sequence.add_event(final_event2).unwrap();
        
        assert_eq!(sequence.events.len(), 4);
        assert!(matches!(sequence.state, SequenceState::Complete { .. }));
    }
    
    #[test]
    fn test_invalid_state_transition() {
        let keys = create_test_keys();
        let challenge_event = create_challenge_event(&keys);
        let mut sequence = GameSequence::new(challenge_event.clone(), keys.public_key()).unwrap();
        
        // Try to add a Move event while in WaitingForAccept state
        let move_event = create_move_event(&keys, challenge_event.id);
        let result = sequence.add_event(move_event);
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GameProtocolError::SequenceError(_)));
    }
    
    #[test]
    fn test_forfeit_player() {
        let challenger_keys = create_test_keys();
        let accepter_keys = create_test_keys();
        
        let challenge_event = create_challenge_event(&challenger_keys);
        let mut sequence = GameSequence::new(challenge_event.clone(), challenger_keys.public_key()).unwrap();
        
        let accept_event = create_challenge_accept_event(&accepter_keys, challenge_event.id);
        sequence.add_event(accept_event).unwrap();
        
        // Forfeit the challenger
        sequence.forfeit_player(challenger_keys.public_key()).unwrap();
        
        assert!(matches!(sequence.state, SequenceState::Forfeited { winner } if winner == accepter_keys.public_key()));
    }
    
    #[test]
    fn test_complete_game() {
        let challenger_keys = create_test_keys();
        let accepter_keys = create_test_keys();
        
        let challenge_event = create_challenge_event(&challenger_keys);
        let mut sequence = GameSequence::new(challenge_event.clone(), challenger_keys.public_key()).unwrap();
        
        let accept_event = create_challenge_accept_event(&accepter_keys, challenge_event.id);
        sequence.add_event(accept_event).unwrap();
        
        let final_event = create_final_event(&challenger_keys, challenge_event.id);
        sequence.add_event(final_event).unwrap();
        
        // Complete the game with challenger as winner
        sequence.complete_game(Some(challenger_keys.public_key())).unwrap();
        
        assert!(matches!(sequence.state, SequenceState::Complete { winner: Some(w) } if w == challenger_keys.public_key()));
    }
    
    #[test]
    fn test_sequence_integrity_validation() {
        let challenger_keys = create_test_keys();
        let accepter_keys = create_test_keys();
        
        let challenge_event = create_challenge_event(&challenger_keys);
        let mut sequence = GameSequence::new(challenge_event.clone(), challenger_keys.public_key()).unwrap();
        
        let accept_event = create_challenge_accept_event(&accepter_keys, challenge_event.id);
        sequence.add_event(accept_event).unwrap();
        
        let final_event = create_final_event(&challenger_keys, challenge_event.id);
        sequence.add_event(final_event).unwrap();
        
        let validation_result = sequence.validate_sequence_integrity().unwrap();
        assert!(validation_result.is_valid);
        assert!(validation_result.errors.is_empty());
    }
    
    #[test]
    fn test_get_events_by_kind() {
        let challenger_keys = create_test_keys();
        let accepter_keys = create_test_keys();
        
        let challenge_event = create_challenge_event(&challenger_keys);
        let mut sequence = GameSequence::new(challenge_event.clone(), challenger_keys.public_key()).unwrap();
        
        let accept_event = create_challenge_accept_event(&accepter_keys, challenge_event.id);
        sequence.add_event(accept_event).unwrap();
        
        let challenge_events = sequence.get_events_by_kind(CHALLENGE_KIND);
        assert_eq!(challenge_events.len(), 1);
        
        let accept_events = sequence.get_events_by_kind(CHALLENGE_ACCEPT_KIND);
        assert_eq!(accept_events.len(), 1);
        
        let move_events = sequence.get_events_by_kind(MOVE_KIND);
        assert_eq!(move_events.len(), 0);
    }
    
    #[test]
    fn test_get_events_by_player() {
        let challenger_keys = create_test_keys();
        let accepter_keys = create_test_keys();
        
        let challenge_event = create_challenge_event(&challenger_keys);
        let mut sequence = GameSequence::new(challenge_event.clone(), challenger_keys.public_key()).unwrap();
        
        let accept_event = create_challenge_accept_event(&accepter_keys, challenge_event.id);
        sequence.add_event(accept_event).unwrap();
        
        let challenger_events = sequence.get_events_by_player(&challenger_keys.public_key());
        assert_eq!(challenger_events.len(), 1);
        
        let accepter_events = sequence.get_events_by_player(&accepter_keys.public_key());
        assert_eq!(accepter_events.len(), 1);
    }
}
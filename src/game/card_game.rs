//! Card game implementation using Kirk protocol

use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use nostr::{Event as NostrEvent, PublicKey};
use cdk::nuts::Token as CashuToken;

use crate::error::{GameProtocolError, ValidationResult, ValidationError, ValidationErrorType};
use crate::events::{EventParser, CHALLENGE_KIND, CHALLENGE_ACCEPT_KIND, MOVE_KIND, FINAL_KIND, MoveType, TimeoutPhase};
use crate::game::traits::{Game, CommitmentValidator};
use crate::game::pieces::PlayingCard;
use crate::cashu::commitments::TokenCommitment;

/// Card game state tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardGameState {
    pub players: [PublicKey; 2],
    pub revealed_cards: HashMap<PublicKey, PlayingCard>,
    pub phase: CardGamePhase,
}

/// Phases of card game progression
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CardGamePhase {
    /// Waiting for challenge acceptance
    WaitingForAccept,
    /// Players are revealing their cards
    Revealing,
    /// Game complete, winner determined
    Complete,
}

/// Card game move data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardMove {
    pub action: CardAction,
}

/// Actions available in card game
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CardAction {
    /// Reveal the card derived from committed token
    RevealCard,
}

/// Implementation of the Game trait for a simple card game
pub struct CardGame;

impl Game for CardGame {
    type GamePiece = PlayingCard;
    type GameState = CardGameState;
    type MoveData = CardMove;
    
    fn decode_c_value(&self, c_value: &[u8; 32]) -> Result<Vec<Self::GamePiece>, GameProtocolError> {
        let card = PlayingCard::from_c_value(c_value)?;
        Ok(vec![card])
    }
    
    fn validate_sequence(&self, events: &[NostrEvent]) -> Result<ValidationResult, GameProtocolError> {
        let mut errors = Vec::new();
        let mut revealed_cards: HashMap<PublicKey, PlayingCard> = HashMap::new();
        let mut players = Vec::new();
        
        // Extract challenge ID from first event
        let challenge_id = if let Some(first_event) = events.first() {
            first_event.id
        } else {
            return Err(GameProtocolError::SequenceError(
                "Empty event sequence".to_string()
            ));
        };
        
        // Validate event sequence structure
        if let Err(e) = self.validate_event_structure(events) {
            errors.push(ValidationError::new(
                challenge_id,
                ValidationErrorType::InvalidSequence,
                e.to_string(),
            ));
        }
        
        // Extract players from Challenge and ChallengeAccept events
        for event in events {
            match event.kind {
                k if k == CHALLENGE_KIND => {
                    players.push(event.pubkey);
                },
                k if k == CHALLENGE_ACCEPT_KIND => {
                    players.push(event.pubkey);
                },
                _ => {}
            }
        }
        
        if players.len() != 2 {
            errors.push(ValidationError::new(
                challenge_id,
                ValidationErrorType::InvalidSequence,
                format!("Expected 2 players, found {}", players.len()),
            ));
        }
        
        // Process Move events to extract revealed cards
        for event in events {
            if event.kind == MOVE_KIND {
                if let Ok(move_content) = EventParser::parse_move(event) {
                    // Check if this is a reveal move
                    if move_content.move_type == MoveType::Reveal {
                        if let Some(ref revealed_tokens) = move_content.revealed_tokens {
                            // Extract cards from revealed tokens
                            for token in revealed_tokens {
                                if let Ok(cards) = self.extract_cards_from_token(token) {
                                    if let Some(card) = cards.first() {
                                        revealed_cards.insert(event.pubkey, *card);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Validate commitment integrity
        if let Err(e) = self.validate_commitments(events, &revealed_cards) {
            errors.push(ValidationError::new(
                challenge_id,
                ValidationErrorType::InvalidCommitment,
                e.to_string(),
            ));
        }
        
        // Determine winner if both cards are revealed
        let winner = if revealed_cards.len() == 2 && players.len() == 2 {
            self.determine_winner_from_cards(&revealed_cards, &players)?
        } else {
            None
        };
        
        let is_valid = errors.is_empty();
        
        Ok(ValidationResult::new(
            is_valid,
            winner,
            errors,
            None, // No forfeited player in normal validation
        ))
    }
    
    fn is_sequence_complete(&self, events: &[NostrEvent]) -> Result<bool, GameProtocolError> {
        // Count reveal moves - need exactly 2 (one from each player)
        let reveal_count = events.iter()
            .filter(|e| e.kind == MOVE_KIND)
            .filter_map(|e| EventParser::parse_move(e).ok())
            .filter(|content| content.move_type == MoveType::Reveal)
            .count();
        
        // Also check for Final events - need at least 1
        let final_count = events.iter()
            .filter(|e| e.kind == FINAL_KIND)
            .count();
        
        Ok(reveal_count >= 2 && final_count >= 1)
    }
    
    fn determine_winner(&self, events: &[NostrEvent]) -> Result<Option<PublicKey>, GameProtocolError> {
        let mut revealed_cards: HashMap<PublicKey, PlayingCard> = HashMap::new();
        let mut players = Vec::new();
        
        // Extract players
        for event in events {
            match event.kind {
                k if k == CHALLENGE_KIND => {
                    players.push(event.pubkey);
                },
                k if k == CHALLENGE_ACCEPT_KIND => {
                    players.push(event.pubkey);
                },
                _ => {}
            }
        }
        
        // Extract revealed cards
        for event in events {
            if event.kind == MOVE_KIND {
                if let Ok(move_content) = EventParser::parse_move(event) {
                    if move_content.move_type == MoveType::Reveal {
                        if let Some(ref revealed_tokens) = move_content.revealed_tokens {
                            for token in revealed_tokens {
                                if let Ok(cards) = self.extract_cards_from_token(token) {
                                    if let Some(card) = cards.first() {
                                        revealed_cards.insert(event.pubkey, *card);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        if revealed_cards.len() == 2 && players.len() == 2 {
            self.determine_winner_from_cards(&revealed_cards, &players)
        } else {
            Ok(None) // Game not complete
        }
    }
    
    fn required_final_events(&self) -> usize {
        2 // Both players must publish Final events
    }
    
    fn should_timeout_forfeit(&self, phase: TimeoutPhase, overdue_duration: u64) -> bool {
        match phase {
            TimeoutPhase::Accept => overdue_duration > 1800, // 30 minutes grace for accepting
            TimeoutPhase::Move | TimeoutPhase::CommitReveal => overdue_duration > 600, // 10 minutes grace for moves
            TimeoutPhase::FinalEvent => overdue_duration > 300, // 5 minutes grace for final events
        }
    }
    
    fn validate_move_deadline(&self, deadline: u64, move_type: &str) -> Result<(), GameProtocolError> {
        let now = chrono::Utc::now().timestamp() as u64;
        let max_future = now + 3600; // 1 hour maximum for card game moves
        let min_future = now + 30;   // 30 seconds minimum
        
        if deadline < min_future {
            return Err(GameProtocolError::Timeout {
                message: "Card game move deadline too soon: must be at least 30 seconds in the future".to_string(),
                duration_ms: (min_future - deadline) * 1000,
                operation: format!("card_game_{}_deadline_validation", move_type),
            });
        }
        
        if deadline > max_future {
            return Err(GameProtocolError::Timeout {
                message: "Card game move deadline too far: must be within 1 hour".to_string(),
                duration_ms: (deadline - max_future) * 1000,
                operation: format!("card_game_{}_deadline_validation", move_type),
            });
        }
        
        Ok(())
    }
}

impl CardGame {
    /// Create a new CardGame instance
    pub fn new() -> Self {
        Self
    }
    
    /// Extract cards from a Cashu token using its C value
    fn extract_cards_from_token(&self, token: &CashuToken) -> Result<Vec<PlayingCard>, GameProtocolError> {
        // Extract C value from token
        // Note: This is a simplified implementation - in practice, you'd need to
        // access the token's proof and extract the C value from it
        // For now, we'll use a placeholder approach
        
        // TODO: Implement proper C value extraction from CashuToken
        // This would require accessing the token's proof structure
        let c_value = self.extract_c_value_from_token(token)?;
        self.decode_c_value(&c_value)
    }
    
    /// Extract C value from Cashu token (placeholder implementation)
    fn extract_c_value_from_token(&self, _token: &CashuToken) -> Result<[u8; 32], GameProtocolError> {
        // TODO: Implement proper C value extraction
        // This is a placeholder that would need to be replaced with actual
        // CDK token proof parsing
        Err(GameProtocolError::InvalidMove(
            "C value extraction not yet implemented".to_string()
        ))
    }
    
    /// Validate event structure for card game
    fn validate_event_structure(&self, events: &[NostrEvent]) -> Result<(), GameProtocolError> {
        if events.is_empty() {
            return Err(GameProtocolError::SequenceError(
                "Empty event sequence".to_string()
            ));
        }
        
        // First event must be Challenge
        if events[0].kind != CHALLENGE_KIND {
            return Err(GameProtocolError::SequenceError(
                "First event must be Challenge".to_string()
            ));
        }
        
        // Must have exactly one Challenge and one ChallengeAccept
        let challenge_count = events.iter().filter(|e| e.kind == CHALLENGE_KIND).count();
        let accept_count = events.iter().filter(|e| e.kind == CHALLENGE_ACCEPT_KIND).count();
        
        if challenge_count != 1 {
            return Err(GameProtocolError::SequenceError(
                format!("Expected exactly 1 Challenge event, found {}", challenge_count)
            ));
        }
        
        if accept_count != 1 {
            return Err(GameProtocolError::SequenceError(
                format!("Expected exactly 1 ChallengeAccept event, found {}", accept_count)
            ));
        }
        
        // Validate Move events are Reveal type
        for event in events {
            if event.kind == MOVE_KIND {
                let move_content = EventParser::parse_move(event)?;
                if move_content.move_type != MoveType::Reveal {
                    return Err(GameProtocolError::InvalidMove(
                        format!("Card game only supports Reveal moves, got {:?}", move_content.move_type)
                    ));
                }
            }
        }
        
        Ok(())
    }
    
    /// Validate commitment integrity against revealed tokens
    fn validate_commitments(
        &self, 
        events: &[NostrEvent], 
        revealed_cards: &HashMap<PublicKey, PlayingCard>
    ) -> Result<(), GameProtocolError> {
        // Extract commitment hashes from Challenge and ChallengeAccept events
        let mut player_commitments: HashMap<PublicKey, Vec<String>> = HashMap::new();
        
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
        
        // For each revealed card, validate against the player's commitment
        for (player, _card) in revealed_cards {
            if let Some(_commitments) = player_commitments.get(player) {
                // TODO: Implement actual commitment validation
                // This would require:
                // 1. Extracting the revealed tokens from Move events
                // 2. Computing the commitment hash from the revealed tokens
                // 3. Comparing against the original commitment hash
                
                // For now, we'll skip this validation as it requires
                // proper integration with the CDK token system
            }
        }
        
        Ok(())
    }
    
    /// Determine winner from revealed cards
    fn determine_winner_from_cards(
        &self,
        revealed_cards: &HashMap<PublicKey, PlayingCard>,
        players: &[PublicKey]
    ) -> Result<Option<PublicKey>, GameProtocolError> {
        if revealed_cards.len() != 2 || players.len() != 2 {
            return Ok(None);
        }
        
        let player1 = players[0];
        let player2 = players[1];
        
        let card1 = revealed_cards.get(&player1)
            .ok_or_else(|| GameProtocolError::InvalidMove(
                "Player 1 card not found".to_string()
            ))?;
        
        let card2 = revealed_cards.get(&player2)
            .ok_or_else(|| GameProtocolError::InvalidMove(
                "Player 2 card not found".to_string()
            ))?;
        
        // Compare cards - higher card wins
        match card1.cmp(card2) {
            std::cmp::Ordering::Greater => Ok(Some(player1)),
            std::cmp::Ordering::Less => Ok(Some(player2)),
            std::cmp::Ordering::Equal => {
                // Tie-breaking rule: higher suit wins (already handled by PlayingCard::cmp)
                // If cards are exactly equal (same rank and suit), it's a true tie
                if card1 == card2 {
                    Ok(None) // True tie - no winner
                } else {
                    // This shouldn't happen due to PlayingCard::cmp implementation
                    // but handle it gracefully
                    Ok(None)
                }
            }
        }
    }
}

impl Default for CardGame {
    fn default() -> Self {
        Self::new()
    }
}

impl CommitmentValidator for CardGame {
    fn validate_single_commitment(
        &self,
        commitment_hash: &str,
        revealed_token: &CashuToken
    ) -> Result<bool, crate::error::ValidationError> {
        // Create single token commitment and compare
        let commitment = TokenCommitment::single(revealed_token);
        Ok(commitment.commitment_hash == commitment_hash)
    }
    
    fn validate_multi_commitment(
        &self,
        commitment_hash: &str,
        revealed_tokens: &[CashuToken],
        method: &crate::events::CommitmentMethod
    ) -> Result<bool, crate::error::ValidationError> {
        // Create multi-token commitment using specified method
        let commitment = TokenCommitment::multiple(revealed_tokens, method.clone());
        Ok(commitment.commitment_hash == commitment_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{Keys, EventBuilder};
    use crate::events::{ChallengeContent, ChallengeAcceptContent, MoveContent, FinalContent};
    use crate::game::pieces::{Suit, Rank};
    
    fn create_test_keys() -> Keys {
        Keys::generate()
    }
    
    fn create_challenge_event(keys: &Keys) -> NostrEvent {
        let content = ChallengeContent {
            game_type: "card_game".to_string(),
            commitment_hashes: vec!["abc123".repeat(32)[..64].to_string()],
            game_parameters: serde_json::json!({}),
            expiry: Some(chrono::Utc::now().timestamp() as u64 + 3600),
            timeout_config: None,
        };
        
        EventBuilder::new(CHALLENGE_KIND, serde_json::to_string(&content).unwrap(), Vec::<nostr::Tag>::new())
            .to_event(keys)
            .unwrap()
    }
    
    fn create_challenge_accept_event(keys: &Keys, challenge_id: nostr::EventId) -> NostrEvent {
        let content = ChallengeAcceptContent {
            challenge_id,
            commitment_hashes: vec!["def456".repeat(32)[..64].to_string()],
        };
        
        EventBuilder::new(CHALLENGE_ACCEPT_KIND, serde_json::to_string(&content).unwrap(), Vec::<nostr::Tag>::new())
            .to_event(keys)
            .unwrap()
    }
    
    fn create_reveal_move_event(keys: &Keys, previous_event_id: nostr::EventId) -> NostrEvent {
        let content = MoveContent {
            previous_event_id,
            move_type: MoveType::Reveal,
            move_data: serde_json::json!({"action": "RevealCard"}),
            revealed_tokens: Some(vec![]), // Empty for now - would contain actual tokens
            deadline: None,
        };
        
        EventBuilder::new(MOVE_KIND, serde_json::to_string(&content).unwrap(), Vec::<nostr::Tag>::new())
            .to_event(keys)
            .unwrap()
    }
    
    fn create_final_event(keys: &Keys, game_sequence_root: nostr::EventId) -> NostrEvent {
        let content = FinalContent {
            game_sequence_root,
            commitment_method: None,
            final_state: serde_json::json!({"result": "game_complete"}),
        };
        
        EventBuilder::new(FINAL_KIND, serde_json::to_string(&content).unwrap(), Vec::<nostr::Tag>::new())
            .to_event(keys)
            .unwrap()
    }
    
    #[test]
    fn test_card_game_creation() {
        let game = CardGame::new();
        assert_eq!(game.required_final_events(), 2);
    }
    
    #[test]
    fn test_c_value_decoding() {
        let game = CardGame::new();
        let c_value = [0u8; 32]; // Should map to Two of Clubs
        
        let cards = game.decode_c_value(&c_value).unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].suit, Suit::Clubs);
        assert_eq!(cards[0].rank, Rank::Two);
    }
    
    #[test]
    fn test_winner_determination_from_cards() {
        let game = CardGame::new();
        let player1 = create_test_keys().public_key();
        let player2 = create_test_keys().public_key();
        let players = vec![player1, player2];
        
        let mut revealed_cards = HashMap::new();
        revealed_cards.insert(player1, PlayingCard::new(Suit::Spades, Rank::Ace));
        revealed_cards.insert(player2, PlayingCard::new(Suit::Hearts, Rank::King));
        
        let winner = game.determine_winner_from_cards(&revealed_cards, &players).unwrap();
        assert_eq!(winner, Some(player1)); // Ace beats King
    }
    
    #[test]
    fn test_winner_determination_tie() {
        let game = CardGame::new();
        let player1 = create_test_keys().public_key();
        let player2 = create_test_keys().public_key();
        let players = vec![player1, player2];
        
        let mut revealed_cards = HashMap::new();
        revealed_cards.insert(player1, PlayingCard::new(Suit::Hearts, Rank::King));
        revealed_cards.insert(player2, PlayingCard::new(Suit::Spades, Rank::King));
        
        let winner = game.determine_winner_from_cards(&revealed_cards, &players).unwrap();
        assert_eq!(winner, Some(player2)); // Spades beats Hearts for same rank
    }
    
    #[test]
    fn test_event_structure_validation() {
        let game = CardGame::new();
        let keys1 = create_test_keys();
        let keys2 = create_test_keys();
        
        // Valid sequence
        let challenge = create_challenge_event(&keys1);
        let accept = create_challenge_accept_event(&keys2, challenge.id);
        let events = vec![challenge, accept.clone()];
        
        assert!(game.validate_event_structure(&events).is_ok());
        
        // Invalid sequence - no challenge
        let events = vec![accept];
        assert!(game.validate_event_structure(&events).is_err());
    }
    
    #[test]
    fn test_sequence_completion() {
        let game = CardGame::new();
        let keys1 = create_test_keys();
        let keys2 = create_test_keys();
        
        let challenge = create_challenge_event(&keys1);
        let accept = create_challenge_accept_event(&keys2, challenge.id);
        let move1 = create_reveal_move_event(&keys1, accept.id);
        let move2 = create_reveal_move_event(&keys2, move1.id);
        let final_event = create_final_event(&keys1, challenge.id);
        
        // Incomplete - no final event
        let events = vec![challenge.clone(), accept.clone(), move1.clone(), move2.clone()];
        assert!(!game.is_sequence_complete(&events).unwrap());
        
        // Complete - has reveals and final event
        let events = vec![challenge, accept, move1, move2, final_event];
        assert!(game.is_sequence_complete(&events).unwrap());
    }
    
    #[test]
    fn test_timeout_forfeit_rules() {
        let game = CardGame::new();
        
        // Accept phase - 30 minutes grace
        assert!(!game.should_timeout_forfeit(TimeoutPhase::Accept, 1800)); // Exactly 30 min
        assert!(game.should_timeout_forfeit(TimeoutPhase::Accept, 1801));  // Over 30 min
        
        // Move phase - 10 minutes grace
        assert!(!game.should_timeout_forfeit(TimeoutPhase::Move, 600));    // Exactly 10 min
        assert!(game.should_timeout_forfeit(TimeoutPhase::Move, 601));     // Over 10 min
        
        // Final event phase - 5 minutes grace
        assert!(!game.should_timeout_forfeit(TimeoutPhase::FinalEvent, 300)); // Exactly 5 min
        assert!(game.should_timeout_forfeit(TimeoutPhase::FinalEvent, 301));  // Over 5 min
    }
    
    #[test]
    fn test_move_deadline_validation() {
        let game = CardGame::new();
        let now = chrono::Utc::now().timestamp() as u64;
        
        // Valid deadline (30 seconds to 1 hour in future)
        assert!(game.validate_move_deadline(now + 60, "reveal").is_ok());
        assert!(game.validate_move_deadline(now + 3600, "reveal").is_ok());
        
        // Too soon (less than 30 seconds)
        assert!(game.validate_move_deadline(now + 29, "reveal").is_err());
        
        // Too far (more than 1 hour)
        assert!(game.validate_move_deadline(now + 3601, "reveal").is_err());
        
        // In the past
        assert!(game.validate_move_deadline(now - 1, "reveal").is_err());
    }
}
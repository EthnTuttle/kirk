//! Reference game implementation for testing the framework

use kirk::{Game, GameProtocolError, MoveContent, MoveType};
use kirk::error::{ValidationResult, ValidationError, ValidationErrorType};
use nostr::{Event, PublicKey};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Simple coin flip game for testing
/// Players commit to a coin side (heads/tails) and the winner is determined by XOR of C values
#[derive(Debug, Clone)]
pub struct CoinFlipGame {
    /// Game parameters
    pub config: CoinFlipConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinFlipConfig {
    /// Minimum number of tokens required per player
    pub min_tokens: usize,
    /// Maximum number of tokens allowed per player
    pub max_tokens: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoinSide {
    Heads,
    Tails,
}

#[derive(Debug, Clone)]
pub struct CoinFlipPiece {
    pub side: CoinSide,
    pub strength: u8, // 0-255 strength value from C value
}

#[derive(Debug, Clone)]
pub struct CoinFlipState {
    pub players: HashMap<PublicKey, Vec<CoinFlipPiece>>,
    pub moves_made: HashMap<PublicKey, CoinFlipMove>,
    pub winner: Option<PublicKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinFlipMove {
    pub chosen_side: CoinSide,
    pub confidence: u8, // How confident the player is (affects scoring)
}

impl CoinFlipGame {
    /// Create a new coin flip game with default config
    pub fn new() -> Self {
        Self {
            config: CoinFlipConfig {
                min_tokens: 1,
                max_tokens: 5,
            },
        }
    }

    /// Create with custom config
    pub fn with_config(config: CoinFlipConfig) -> Self {
        Self { config }
    }

    /// Get game type identifier
    pub fn game_type() -> String {
        "coinflip".to_string()
    }

    /// Get game parameters as JSON
    pub fn get_parameters(&self) -> Result<serde_json::Value, GameProtocolError> {
        serde_json::to_value(&self.config)
            .map_err(|e| GameProtocolError::GameValidation(format!("Failed to serialize config: {}", e)))
    }

    /// Determine winner based on XOR of all C values and player moves
    fn calculate_winner(&self, state: &CoinFlipState) -> Option<PublicKey> {
        if state.moves_made.len() != 2 {
            return None; // Need exactly 2 players
        }

        // Get all pieces from all players
        let mut all_pieces = Vec::new();
        for pieces in state.players.values() {
            all_pieces.extend(pieces.iter());
        }

        if all_pieces.is_empty() {
            return None;
        }

        // XOR all strength values to get randomness
        let mut random_value = 0u8;
        for piece in &all_pieces {
            random_value ^= piece.strength;
        }

        // Determine actual coin result (heads = even, tails = odd)
        let actual_result = if random_value % 2 == 0 {
            CoinSide::Heads
        } else {
            CoinSide::Tails
        };

        // Find winner - player who guessed correctly with highest confidence
        let mut best_player = None;
        let mut best_confidence = 0u8;

        for (player, player_move) in &state.moves_made {
            if player_move.chosen_side == actual_result {
                if player_move.confidence > best_confidence {
                    best_confidence = player_move.confidence;
                    best_player = Some(*player);
                }
            }
        }

        best_player
    }
}

impl Default for CoinFlipGame {
    fn default() -> Self {
        Self::new()
    }
}

impl Game for CoinFlipGame {
    type GamePiece = CoinFlipPiece;
    type GameState = CoinFlipState;
    type MoveData = CoinFlipMove;

    /// Decode C value into coin flip pieces
    /// Uses the C value bytes to determine coin side and strength
    fn decode_c_value(&self, c_value: &[u8; 32]) -> Result<Vec<Self::GamePiece>, GameProtocolError> {
        // Use first byte for side determination (even = heads, odd = tails)
        let side = if c_value[0] % 2 == 0 {
            CoinSide::Heads
        } else {
            CoinSide::Tails
        };

        // Use second byte for strength
        let strength = c_value[1];

        Ok(vec![CoinFlipPiece { side, strength }])
    }

    /// Validate a sequence of game events
    fn validate_sequence(&self, events: &[Event]) -> Result<ValidationResult, GameProtocolError> {

        let mut errors = Vec::new();
        let mut state = CoinFlipState {
            players: HashMap::new(),
            moves_made: HashMap::new(),
            winner: None,
        };

        // Parse events and build game state
        for event in events {
            match self.parse_event(event, &mut state) {
                Ok(_) => {},
                Err(e) => {
                    errors.push(ValidationError {
                        event_id: event.id,
                        error_type: ValidationErrorType::InvalidMove,
                        message: e.to_string(),
                    });
                }
            }
        }

        // Determine winner if game is complete
        let winner = if self.is_sequence_complete(events)? {
            self.calculate_winner(&state)
        } else {
            None
        };

        Ok(ValidationResult {
            is_valid: errors.is_empty(),
            winner,
            errors,
            forfeited_player: None,
        })
    }

    /// Check if the game sequence is complete
    fn is_sequence_complete(&self, events: &[Event]) -> Result<bool, GameProtocolError> {
        // Game is complete when we have:
        // 1. Challenge event
        // 2. ChallengeAccept event  
        // 3. Move events from both players
        // 4. Final events from required number of players

        let mut has_challenge = false;
        let mut has_accept = false;
        let mut move_players = std::collections::HashSet::new();
        let mut final_players = std::collections::HashSet::new();

        for event in events {
            match event.kind.as_u16() {
                9259 => has_challenge = true, // Challenge
                9260 => has_accept = true,    // ChallengeAccept
                9261 => { // Move
                    move_players.insert(event.pubkey);
                },
                9262 => { // Final
                    final_players.insert(event.pubkey);
                },
                _ => {} // Ignore other event types
            }
        }

        // Need challenge, accept, moves from 2 players, and final events
        let required_final_events = self.required_final_events();
        Ok(has_challenge && 
           has_accept && 
           move_players.len() >= 2 && 
           final_players.len() >= required_final_events)
    }

    /// Determine winner from completed sequence
    fn determine_winner(&self, events: &[Event]) -> Result<Option<PublicKey>, GameProtocolError> {
        let validation_result = self.validate_sequence(events)?;
        Ok(validation_result.winner)
    }

    /// Get required number of Final events (both players must publish)
    fn required_final_events(&self) -> usize {
        2 // Both players must publish Final events
    }
    
    /// Check if a timeout should result in forfeiture for this game
    fn should_timeout_forfeit(&self, phase: kirk::events::TimeoutPhase, overdue_duration: u64) -> bool {
        use kirk::events::TimeoutPhase;
        
        match phase {
            TimeoutPhase::Accept => overdue_duration > 300,      // 5 minutes grace for accepting
            TimeoutPhase::Move => overdue_duration > 120,       // 2 minutes grace for moves
            TimeoutPhase::CommitReveal => overdue_duration > 60, // 1 minute grace for commit/reveal
            TimeoutPhase::FinalEvent => overdue_duration > 180, // 3 minutes grace for final events
        }
    }
    
    /// Get default timeout configuration for coin flip games
    fn default_timeout_config(&self) -> Option<kirk::events::TimeoutConfig> {
        Some(kirk::events::TimeoutConfig::custom(
            Some(1800), // 30 minutes to accept
            Some(600),  // 10 minutes per move
            Some(300),  // 5 minutes for commit/reveal
            Some(900),  // 15 minutes for final events
        ))
    }
    
    /// Validate that a move deadline is reasonable for coin flip games
    fn validate_move_deadline(&self, deadline: u64, move_type: &str) -> Result<(), GameProtocolError> {
        let now = chrono::Utc::now().timestamp() as u64;
        
        // Coin flip games should have reasonable deadlines
        let min_future = now + 30;    // 30 seconds minimum
        let max_future = now + 3600;  // 1 hour maximum
        
        if deadline < min_future {
            return Err(GameProtocolError::Timeout(
                format!("Coin flip move deadline too soon: must be at least 30 seconds in the future")
            ));
        }
        
        if deadline > max_future {
            return Err(GameProtocolError::Timeout(
                format!("Coin flip move deadline too far: must be within 1 hour")
            ));
        }
        
        // Commit/reveal moves should have shorter deadlines
        if move_type == "Commit" || move_type == "Reveal" {
            let max_commit_reveal = now + 600; // 10 minutes for commit/reveal
            if deadline > max_commit_reveal {
                return Err(GameProtocolError::Timeout(
                    format!("Commit/reveal deadline too far: must be within 10 minutes")
                ));
            }
        }
        
        Ok(())
    }
}

impl CoinFlipGame {
    /// Parse a single event and update game state
    fn parse_event(&self, event: &Event, state: &mut CoinFlipState) -> Result<(), GameProtocolError> {
        match event.kind.as_u16() {
            9261 => { // Move event
                let move_content: kirk::MoveContent = serde_json::from_str(&event.content)
                    .map_err(|e| GameProtocolError::GameValidation(format!("Invalid move content: {}", e)))?;

                // Parse move data
                let move_data: CoinFlipMove = serde_json::from_value(move_content.move_data)
                    .map_err(|e| GameProtocolError::GameValidation(format!("Invalid move data: {}", e)))?;

                // If tokens are revealed, decode game pieces
                if let Some(revealed_tokens) = move_content.revealed_tokens {
                    let mut pieces = Vec::new();
                    for _token in revealed_tokens {
                        // For testing purposes, we'll use a simplified approach
                        // In a real implementation, this would properly extract C values from proofs
                        // For now, just create a mock piece based on the token
                        let mock_c_value = [42u8; 32]; // Mock C value for testing
                        let decoded_pieces = self.decode_c_value(&mock_c_value)?;
                        pieces.extend(decoded_pieces);
                    }
                    state.players.insert(event.pubkey, pieces);
                }

                state.moves_made.insert(event.pubkey, move_data);
            },
            _ => {} // Ignore other event types for now
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_c_value_decoding() {
        let game = CoinFlipGame::new();
        
        // Test with even first byte (should be heads)
        let c_value_heads = [2u8, 100u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8];
        let pieces = game.decode_c_value(&c_value_heads).unwrap();
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].side, CoinSide::Heads);
        assert_eq!(pieces[0].strength, 100);

        // Test with odd first byte (should be tails)
        let c_value_tails = [3u8, 150u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8];
        let pieces = game.decode_c_value(&c_value_tails).unwrap();
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].side, CoinSide::Tails);
        assert_eq!(pieces[0].strength, 150);
    }

    #[test]
    fn test_winner_calculation() {
        let game = CoinFlipGame::new();
        let player1 = PublicKey::from_slice(&[1u8; 32]).unwrap();
        let player2 = PublicKey::from_slice(&[2u8; 32]).unwrap();

        let mut state = CoinFlipState {
            players: HashMap::new(),
            moves_made: HashMap::new(),
            winner: None,
        };

        // Add game pieces (XOR will result in even number = heads)
        state.players.insert(player1, vec![CoinFlipPiece { side: CoinSide::Heads, strength: 4 }]);
        state.players.insert(player2, vec![CoinFlipPiece { side: CoinSide::Tails, strength: 6 }]);

        // Add moves - player1 guesses heads with high confidence
        state.moves_made.insert(player1, CoinFlipMove { chosen_side: CoinSide::Heads, confidence: 200 });
        state.moves_made.insert(player2, CoinFlipMove { chosen_side: CoinSide::Tails, confidence: 100 });

        // XOR: 4 ^ 6 = 2 (even) = Heads, so player1 should win
        let winner = game.calculate_winner(&state);
        assert_eq!(winner, Some(player1));
    }

    #[test]
    fn test_required_final_events() {
        let game = CoinFlipGame::new();
        assert_eq!(game.required_final_events(), 2);
    }

    #[test]
    fn test_game_type() {
        assert_eq!(CoinFlipGame::game_type(), "coinflip");
    }
}
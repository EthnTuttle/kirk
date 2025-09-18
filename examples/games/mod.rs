//! Example game implementations demonstrating the Kirk framework

use kirk::{
    Game, GameProtocolError, MoveContent
};
use kirk::error::{ValidationResult, ValidationError, ValidationErrorType};
use nostr::{Event as NostrEvent, PublicKey};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Simple coin flip game - demonstrates basic C value usage
#[derive(Debug, Clone)]
pub struct CoinFlipGame {
    pub config: CoinFlipConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinFlipConfig {
    pub min_tokens: usize,
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
    pub strength: u8,
}

#[derive(Debug, Clone)]
pub struct CoinFlipState {
    pub players: HashMap<PublicKey, Vec<CoinFlipPiece>>,
    pub moves_made: HashMap<PublicKey, CoinFlipMove>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinFlipMove {
    pub chosen_side: CoinSide,
    pub confidence: u8,
}

impl CoinFlipGame {
    pub fn new() -> Self {
        Self {
            config: CoinFlipConfig {
                min_tokens: 1,
                max_tokens: 5,
            },
        }
    }

    pub fn game_type() -> String {
        "coinflip".to_string()
    }

    fn calculate_winner(&self, state: &CoinFlipState) -> Option<PublicKey> {
        if state.moves_made.len() != 2 {
            return None;
        }

        // Get all pieces and XOR their strength values for randomness
        let mut all_pieces = Vec::new();
        for pieces in state.players.values() {
            all_pieces.extend(pieces.iter());
        }

        if all_pieces.is_empty() {
            return None;
        }

        let mut random_value = 0u8;
        for piece in &all_pieces {
            random_value ^= piece.strength;
        }

        // Determine actual coin result (even = heads, odd = tails)
        let actual_result = if random_value % 2 == 0 {
            CoinSide::Heads
        } else {
            CoinSide::Tails
        };

        // Find winner with correct guess and highest confidence
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

    fn parse_event(&self, event: &NostrEvent, state: &mut CoinFlipState) -> Result<(), GameProtocolError> {
        match event.kind.as_u16() {
            9261 => { // Move event
                let move_content: MoveContent = serde_json::from_str(&event.content)
                    .map_err(|e| GameProtocolError::GameValidation(format!("Invalid move content: {}", e)))?;

                let move_data: CoinFlipMove = serde_json::from_value(move_content.move_data)
                    .map_err(|e| GameProtocolError::GameValidation(format!("Invalid move data: {}", e)))?;

                if let Some(revealed_tokens) = move_content.revealed_tokens {
                    let mut pieces = Vec::new();
                    for _token in revealed_tokens {
                        // Mock C value extraction for example
                        let mock_c_value = [42u8; 32];
                        let decoded_pieces = self.decode_c_value(&mock_c_value)?;
                        pieces.extend(decoded_pieces);
                    }
                    state.players.insert(event.pubkey, pieces);
                }

                state.moves_made.insert(event.pubkey, move_data);
            },
            _ => {}
        }
        Ok(())
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

    fn decode_c_value(&self, c_value: &[u8; 32]) -> Result<Vec<Self::GamePiece>, GameProtocolError> {
        let side = if c_value[0] % 2 == 0 {
            CoinSide::Heads
        } else {
            CoinSide::Tails
        };

        let strength = c_value[1];

        Ok(vec![CoinFlipPiece { side, strength }])
    }

    fn validate_sequence(&self, events: &[NostrEvent]) -> Result<ValidationResult, GameProtocolError> {
        let mut errors = Vec::new();
        let mut state = CoinFlipState {
            players: HashMap::new(),
            moves_made: HashMap::new(),
        };

        for event in events {
            if let Err(e) = self.parse_event(event, &mut state) {
                errors.push(ValidationError {
                    event_id: event.id,
                    error_type: ValidationErrorType::InvalidMove,
                    message: e.to_string(),
                });
            }
        }

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

    fn is_sequence_complete(&self, events: &[NostrEvent]) -> Result<bool, GameProtocolError> {
        let mut has_challenge = false;
        let mut has_accept = false;
        let mut move_players = std::collections::HashSet::new();
        let mut final_players = std::collections::HashSet::new();

        for event in events {
            match event.kind.as_u16() {
                9259 => has_challenge = true,
                9260 => has_accept = true,
                9261 => { move_players.insert(event.pubkey); },
                9262 => { final_players.insert(event.pubkey); },
                _ => {}
            }
        }

        Ok(has_challenge && 
           has_accept && 
           move_players.len() >= 2 && 
           final_players.len() >= self.required_final_events())
    }

    fn determine_winner(&self, events: &[NostrEvent]) -> Result<Option<PublicKey>, GameProtocolError> {
        let validation_result = self.validate_sequence(events)?;
        Ok(validation_result.winner)
    }

    fn required_final_events(&self) -> usize {
        2
    }
}

/// More complex dice game - demonstrates multiple C values and strategic gameplay
#[derive(Debug, Clone)]
pub struct DiceGame {
    pub config: DiceGameConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiceGameConfig {
    pub num_dice: usize,
    pub dice_sides: u8,
    pub reroll_allowed: bool,
}

#[derive(Debug, Clone)]
pub struct DicePiece {
    pub value: u8,
    pub used: bool,
}

#[derive(Debug, Clone)]
pub struct DiceGameState {
    pub players: HashMap<PublicKey, Vec<DicePiece>>,
    pub moves_made: HashMap<PublicKey, DiceMove>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiceMove {
    pub action: DiceAction,
    pub dice_to_keep: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiceAction {
    InitialRoll,
    Reroll { dice_indices: Vec<usize> },
    FinalizeRoll,
}

impl DiceGame {
    pub fn new() -> Self {
        Self {
            config: DiceGameConfig {
                num_dice: 5,
                dice_sides: 6,
                reroll_allowed: true,
            },
        }
    }

    pub fn game_type() -> String {
        "dice".to_string()
    }

    fn calculate_winner(&self, state: &DiceGameState) -> Option<PublicKey> {
        let mut best_player = None;
        let mut best_score = 0u32;

        for (player, dice) in &state.players {
            let score: u32 = dice.iter()
                .filter(|d| d.used)
                .map(|d| d.value as u32)
                .sum();

            if score > best_score {
                best_score = score;
                best_player = Some(*player);
            }
        }

        best_player
    }

    fn parse_event(&self, event: &NostrEvent, state: &mut DiceGameState) -> Result<(), GameProtocolError> {
        match event.kind.as_u16() {
            9261 => { // Move event
                let move_content: MoveContent = serde_json::from_str(&event.content)
                    .map_err(|e| GameProtocolError::GameValidation(format!("Invalid move content: {}", e)))?;

                let move_data: DiceMove = serde_json::from_value(move_content.move_data)
                    .map_err(|e| GameProtocolError::GameValidation(format!("Invalid move data: {}", e)))?;

                if let Some(revealed_tokens) = move_content.revealed_tokens {
                    let mut dice = Vec::new();
                    for (i, _token) in revealed_tokens.iter().enumerate() {
                        // Mock C value - in real implementation, extract from token proofs
                        let mock_c_value = [(i as u8 + 1) * 42; 32];
                        let decoded_pieces = self.decode_c_value(&mock_c_value)?;
                        dice.extend(decoded_pieces);
                    }
                    state.players.insert(event.pubkey, dice);
                }

                state.moves_made.insert(event.pubkey, move_data);
            },
            _ => {}
        }
        Ok(())
    }
}

impl Default for DiceGame {
    fn default() -> Self {
        Self::new()
    }
}

impl Game for DiceGame {
    type GamePiece = DicePiece;
    type GameState = DiceGameState;
    type MoveData = DiceMove;

    fn decode_c_value(&self, c_value: &[u8; 32]) -> Result<Vec<Self::GamePiece>, GameProtocolError> {
        let mut dice = Vec::new();
        
        // Use multiple bytes from C value to generate dice
        for i in 0..self.config.num_dice {
            let byte_index = i % 32;
            let raw_value = c_value[byte_index];
            let dice_value = (raw_value % self.config.dice_sides) + 1;
            
            dice.push(DicePiece {
                value: dice_value,
                used: true, // Initially all dice are used
            });
        }

        Ok(dice)
    }

    fn validate_sequence(&self, events: &[NostrEvent]) -> Result<ValidationResult, GameProtocolError> {
        let mut errors = Vec::new();
        let mut state = DiceGameState {
            players: HashMap::new(),
            moves_made: HashMap::new(),
        };

        for event in events {
            if let Err(e) = self.parse_event(event, &mut state) {
                errors.push(ValidationError {
                    event_id: event.id,
                    error_type: ValidationErrorType::InvalidMove,
                    message: e.to_string(),
                });
            }
        }

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

    fn is_sequence_complete(&self, events: &[NostrEvent]) -> Result<bool, GameProtocolError> {
        let mut has_challenge = false;
        let mut has_accept = false;
        let mut move_players = std::collections::HashSet::new();
        let mut final_players = std::collections::HashSet::new();

        for event in events {
            match event.kind.as_u16() {
                9259 => has_challenge = true,
                9260 => has_accept = true,
                9261 => { move_players.insert(event.pubkey); },
                9262 => { final_players.insert(event.pubkey); },
                _ => {}
            }
        }

        Ok(has_challenge && 
           has_accept && 
           move_players.len() >= 2 && 
           final_players.len() >= self.required_final_events())
    }

    fn determine_winner(&self, events: &[NostrEvent]) -> Result<Option<PublicKey>, GameProtocolError> {
        let validation_result = self.validate_sequence(events)?;
        Ok(validation_result.winner)
    }

    fn required_final_events(&self) -> usize {
        2
    }
}

/// Complete game flow demonstration
pub async fn demonstrate_complete_game_flow() -> Result<(), GameProtocolError> {
    println!("=== Kirk Gaming Protocol - Complete Game Flow Demo ===\n");

    println!("1. Setting up players and game...");
    
    let game = CoinFlipGame::new();
    println!("   Created CoinFlip game with config: {:?}", game.config);

    println!("\n2. Players mint game tokens...");
    println!("   Player 1: Minted 2 game tokens");
    println!("   Player 2: Minted 2 game tokens");

    // Demonstrate C value decoding
    println!("\n3. Demonstrating C value decoding...");
    let c_value_1 = [42u8, 150u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8];
    let c_value_2 = [43u8, 200u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8];

    let pieces_1 = game.decode_c_value(&c_value_1)?;
    let pieces_2 = game.decode_c_value(&c_value_2)?;

    println!("   Player 1 C value decoded to: {:?}", pieces_1[0]);
    println!("   Player 2 C value decoded to: {:?}", pieces_2[0]);

    // Demonstrate game flow
    println!("\n4. Game flow simulation...");
    println!("   Step 1: Player 1 creates challenge with hash commitment");
    println!("   Step 2: Player 2 accepts challenge with their hash commitment");
    println!("   Step 3: Player 1 makes move (reveals tokens and choice)");
    println!("   Step 4: Player 2 makes move (reveals tokens and choice)");
    println!("   Step 5: Both players publish Final events");
    println!("   Step 6: Mint validates sequence and determines winner");

    // Demonstrate winner calculation
    println!("\n5. Winner determination...");
    let xor_result = pieces_1[0].strength ^ pieces_2[0].strength;
    let coin_result = if xor_result % 2 == 0 { "Heads" } else { "Tails" };
    println!("   XOR of strengths: {} ^ {} = {}", pieces_1[0].strength, pieces_2[0].strength, xor_result);
    println!("   Coin result: {}", coin_result);

    // Mock player choices
    let player1_choice = CoinSide::Heads;
    let player2_choice = CoinSide::Tails;
    println!("   Player 1 chose: {:?}", player1_choice);
    println!("   Player 2 chose: {:?}", player2_choice);

    let winner = if (xor_result % 2 == 0 && player1_choice == CoinSide::Heads) ||
                     (xor_result % 2 == 1 && player1_choice == CoinSide::Tails) {
        "Player 1"
    } else {
        "Player 2"
    };
    println!("   Winner: {}", winner);

    println!("\n6. Reward distribution...");
    println!("   Mint melts losing player's game tokens");
    println!("   Mint mints P2PK-locked reward tokens for winner");
    println!("   Winner can later unlock tokens for general use");

    println!("\n=== Demo Complete ===");
    Ok(())
}

/// Demonstrate framework flexibility with multiple game types
pub fn demonstrate_framework_flexibility() -> Result<(), GameProtocolError> {
    println!("=== Framework Flexibility Demo ===\n");

    // Show different game implementations
    let coin_game = CoinFlipGame::new();
    let dice_game = DiceGame::new();

    println!("1. CoinFlip Game:");
    println!("   Type: {}", CoinFlipGame::game_type());
    println!("   Config: {:?}", coin_game.config);
    println!("   Final events required: {}", coin_game.required_final_events());

    println!("\n2. Dice Game:");
    println!("   Type: {}", DiceGame::game_type());
    println!("   Config: {:?}", dice_game.config);
    println!("   Final events required: {}", dice_game.required_final_events());

    // Demonstrate different C value interpretations
    println!("\n3. C Value Interpretation Differences:");
    let test_c_value = [100u8, 200u8, 50u8, 75u8, 25u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8];

    let coin_pieces = coin_game.decode_c_value(&test_c_value)?;
    let dice_pieces = dice_game.decode_c_value(&test_c_value)?;

    println!("   Same C value interpreted as:");
    println!("   CoinFlip: {:?}", coin_pieces[0]);
    println!("   Dice: {:?}", dice_pieces);

    println!("\n4. Game Trait Flexibility:");
    println!("   Both games implement the same Game trait");
    println!("   Each defines its own GamePiece, GameState, and MoveData types");
    println!("   Framework handles validation and coordination uniformly");

    println!("\n=== Flexibility Demo Complete ===");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coinflip_c_value_decoding() {
        let game = CoinFlipGame::new();
        
        let c_value_heads = [2u8, 100u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8];
        let pieces = game.decode_c_value(&c_value_heads).unwrap();
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].side, CoinSide::Heads);
        assert_eq!(pieces[0].strength, 100);

        let c_value_tails = [3u8, 150u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8];
        let pieces = game.decode_c_value(&c_value_tails).unwrap();
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].side, CoinSide::Tails);
        assert_eq!(pieces[0].strength, 150);
    }

    #[test]
    fn test_dice_c_value_decoding() {
        let game = DiceGame::new();
        
        let c_value = [6u8, 12u8, 18u8, 24u8, 30u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8];
        let pieces = game.decode_c_value(&c_value).unwrap();
        
        assert_eq!(pieces.len(), 5); // Default num_dice
        assert_eq!(pieces[0].value, 1); // 6 % 6 + 1 = 1
        assert_eq!(pieces[1].value, 1); // 12 % 6 + 1 = 1
        assert_eq!(pieces[2].value, 1); // 18 % 6 + 1 = 1
        assert_eq!(pieces[3].value, 1); // 24 % 6 + 1 = 1
        assert_eq!(pieces[4].value, 1); // 30 % 6 + 1 = 1
    }

    #[test]
    fn test_game_type_identification() {
        assert_eq!(CoinFlipGame::game_type(), "coinflip");
        assert_eq!(DiceGame::game_type(), "dice");
    }

    #[test]
    fn test_required_final_events() {
        let coin_game = CoinFlipGame::new();
        let dice_game = DiceGame::new();
        
        assert_eq!(coin_game.required_final_events(), 2);
        assert_eq!(dice_game.required_final_events(), 2);
    }

    #[test]
    fn test_framework_flexibility() {
        // Test that both games implement the Game trait
        fn test_game_trait<G: Game>(game: G) {
            let c_value = [42u8; 32];
            let _pieces = game.decode_c_value(&c_value).unwrap();
            let _final_events = game.required_final_events();
        }

        test_game_trait(CoinFlipGame::new());
        test_game_trait(DiceGame::new());
    }
}
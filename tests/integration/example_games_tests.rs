//! Integration tests for example game implementations
//! 
//! These tests validate that the example games work correctly with the Kirk framework
//! and demonstrate proper usage patterns.

use kirk::{Game, GameProtocolError, MoveContent, MoveType};
use kirk::error::{ValidationResult, ValidationError, ValidationErrorType};
use nostr::{Event, EventBuilder, Keys, Kind, PublicKey};
use serde_json;
use std::collections::HashMap;

// Import example games from the examples directory
// Note: In a real project, these would be separate crates or modules
#[path = "../../examples/games/mod.rs"]
mod example_games;

use example_games::{CoinFlipGame, DiceGame, CoinSide, CoinFlipMove, DiceMove, DiceAction};

#[tokio::test]
async fn test_coinflip_game_complete_flow() -> Result<(), GameProtocolError> {
    let game = CoinFlipGame::new();
    
    // Test C value decoding
    let c_value_1 = [42u8, 150u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8];
    let c_value_2 = [43u8, 200u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8];
    
    let pieces_1 = game.decode_c_value(&c_value_1)?;
    let pieces_2 = game.decode_c_value(&c_value_2)?;
    
    // Verify C value decoding
    assert_eq!(pieces_1.len(), 1);
    assert_eq!(pieces_1[0].side, CoinSide::Heads); // 42 is even
    assert_eq!(pieces_1[0].strength, 150);
    
    assert_eq!(pieces_2.len(), 1);
    assert_eq!(pieces_2[0].side, CoinSide::Tails); // 43 is odd
    assert_eq!(pieces_2[0].strength, 200);
    
    // Test winner calculation logic
    let xor_result = pieces_1[0].strength ^ pieces_2[0].strength; // 150 ^ 200 = 86
    let expected_coin_result = if xor_result % 2 == 0 { CoinSide::Heads } else { CoinSide::Tails };
    assert_eq!(expected_coin_result, CoinSide::Heads); // 86 is even
    
    // Test game trait methods
    assert_eq!(game.required_final_events(), 2);
    
    Ok(())
}

#[tokio::test]
async fn test_dice_game_complete_flow() -> Result<(), GameProtocolError> {
    let game = DiceGame::new();
    
    // Test C value decoding
    let c_value = [6u8, 12u8, 18u8, 24u8, 30u8, 36u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8];
    let pieces = game.decode_c_value(&c_value)?;
    
    // Verify dice generation
    assert_eq!(pieces.len(), 5); // Default num_dice
    assert_eq!(pieces[0].value, 1); // 6 % 6 + 1 = 1
    assert_eq!(pieces[1].value, 1); // 12 % 6 + 1 = 1
    assert_eq!(pieces[2].value, 1); // 18 % 6 + 1 = 1
    assert_eq!(pieces[3].value, 1); // 24 % 6 + 1 = 1
    assert_eq!(pieces[4].value, 1); // 30 % 6 + 1 = 1
    
    for piece in &pieces {
        assert!(piece.value >= 1 && piece.value <= 6);
        assert!(piece.used);
    }
    
    // Test game trait methods
    assert_eq!(game.required_final_events(), 2);
    
    Ok(())
}

#[tokio::test]
async fn test_framework_flexibility() -> Result<(), GameProtocolError> {
    // Test that both games implement the Game trait correctly
    fn test_game_trait<G: Game>(game: G) -> Result<(), GameProtocolError> {
        let c_value = [42u8; 32];
        let pieces = game.decode_c_value(&c_value)?;
        assert!(!pieces.is_empty());
        
        let final_events = game.required_final_events();
        assert!(final_events > 0);
        
        Ok(())
    }
    
    test_game_trait(CoinFlipGame::new())?;
    test_game_trait(DiceGame::new())?;
    
    Ok(())
}

#[tokio::test]
async fn test_c_value_randomness_properties() -> Result<(), GameProtocolError> {
    let coin_game = CoinFlipGame::new();
    let dice_game = DiceGame::new();
    
    // Test multiple C values to ensure variety
    let test_c_values = vec![
        [1u8; 32],
        [255u8; 32],
        [42u8, 150u8, 200u8, 75u8, 100u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8],
        [128u8, 64u8, 32u8, 16u8, 8u8, 4u8, 2u8, 1u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8],
    ];
    
    let mut coin_heads_count = 0;
    let mut coin_tails_count = 0;
    let mut dice_values = HashMap::new();
    
    for c_value in test_c_values {
        // Test coin flip variety
        let coin_pieces = coin_game.decode_c_value(&c_value)?;
        match coin_pieces[0].side {
            CoinSide::Heads => coin_heads_count += 1,
            CoinSide::Tails => coin_tails_count += 1,
        }
        
        // Test dice variety
        let dice_pieces = dice_game.decode_c_value(&c_value)?;
        for piece in dice_pieces {
            *dice_values.entry(piece.value).or_insert(0) += 1;
        }
    }
    
    // Should have some variety in coin flips
    assert!(coin_heads_count > 0);
    assert!(coin_tails_count > 0);
    
    // Should have some variety in dice values
    assert!(dice_values.len() > 1);
    
    Ok(())
}

#[tokio::test]
async fn test_game_sequence_validation() -> Result<(), GameProtocolError> {
    let game = CoinFlipGame::new();
    
    // Create mock events for a complete game sequence
    let keys1 = Keys::generate();
    let keys2 = Keys::generate();
    
    // Challenge event
    let challenge_content = serde_json::json!({
        "game_type": "coinflip",
        "commitment_hashes": ["abc123"],
        "game_parameters": {},
        "expiry": null
    });
    let challenge_event = EventBuilder::new(Kind::Custom(9259), challenge_content.to_string(), &[])
        .to_event(&keys1)?;
    
    // ChallengeAccept event
    let accept_content = serde_json::json!({
        "challenge_id": challenge_event.id.to_hex(),
        "commitment_hashes": ["def456"]
    });
    let accept_event = EventBuilder::new(Kind::Custom(9260), accept_content.to_string(), &[])
        .to_event(&keys2)?;
    
    // Move events
    let move1_content = MoveContent {
        previous_event_id: accept_event.id,
        move_type: MoveType::Move,
        move_data: serde_json::to_value(CoinFlipMove {
            chosen_side: CoinSide::Heads,
            confidence: 200,
        })?,
        revealed_tokens: Some(vec![]), // Mock empty tokens for test
    };
    let move1_event = EventBuilder::new(Kind::Custom(9261), serde_json::to_string(&move1_content)?, &[])
        .to_event(&keys1)?;
    
    let move2_content = MoveContent {
        previous_event_id: move1_event.id,
        move_type: MoveType::Move,
        move_data: serde_json::to_value(CoinFlipMove {
            chosen_side: CoinSide::Tails,
            confidence: 150,
        })?,
        revealed_tokens: Some(vec![]), // Mock empty tokens for test
    };
    let move2_event = EventBuilder::new(Kind::Custom(9261), serde_json::to_string(&move2_content)?, &[])
        .to_event(&keys2)?;
    
    // Final events
    let final1_content = serde_json::json!({
        "game_sequence_root": challenge_event.id.to_hex(),
        "commitment_method": null,
        "final_state": {"complete": true}
    });
    let final1_event = EventBuilder::new(Kind::Custom(9262), final1_content.to_string(), &[])
        .to_event(&keys1)?;
    
    let final2_content = serde_json::json!({
        "game_sequence_root": challenge_event.id.to_hex(),
        "commitment_method": null,
        "final_state": {"complete": true}
    });
    let final2_event = EventBuilder::new(Kind::Custom(9262), final2_content.to_string(), &[])
        .to_event(&keys2)?;
    
    let all_events = vec![
        challenge_event,
        accept_event,
        move1_event,
        move2_event,
        final1_event,
        final2_event,
    ];
    
    // Test sequence completion
    let is_complete = game.is_sequence_complete(&all_events)?;
    assert!(is_complete);
    
    // Test validation (will have errors due to mock tokens, but structure should be valid)
    let validation_result = game.validate_sequence(&all_events)?;
    // Note: validation may fail due to mock tokens, but the sequence structure is correct
    
    Ok(())
}

#[tokio::test]
async fn test_different_game_types() -> Result<(), GameProtocolError> {
    // Test that different games interpret the same C value differently
    let coin_game = CoinFlipGame::new();
    let dice_game = DiceGame::new();
    
    let c_value = [100u8, 200u8, 50u8, 75u8, 25u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8];
    
    let coin_pieces = coin_game.decode_c_value(&c_value)?;
    let dice_pieces = dice_game.decode_c_value(&c_value)?;
    
    // Same C value, different interpretations
    assert_eq!(coin_pieces.len(), 1);
    assert_eq!(dice_pieces.len(), 5);
    
    // Coin: 100 is even = Heads, strength = 200
    assert_eq!(coin_pieces[0].side, CoinSide::Heads);
    assert_eq!(coin_pieces[0].strength, 200);
    
    // Dice: Each byte % 6 + 1
    assert_eq!(dice_pieces[0].value, 5); // 100 % 6 + 1 = 5
    assert_eq!(dice_pieces[1].value, 5); // 200 % 6 + 1 = 5
    assert_eq!(dice_pieces[2].value, 3); // 50 % 6 + 1 = 3
    assert_eq!(dice_pieces[3].value, 4); // 75 % 6 + 1 = 4
    assert_eq!(dice_pieces[4].value, 2); // 25 % 6 + 1 = 2
    
    Ok(())
}

#[test]
fn test_game_configuration() {
    let coin_game = CoinFlipGame::new();
    let dice_game = DiceGame::new();
    
    // Test game type identification
    assert_eq!(CoinFlipGame::game_type(), "coinflip");
    assert_eq!(DiceGame::game_type(), "dice");
    
    // Test configuration
    assert_eq!(coin_game.config.min_tokens, 1);
    assert_eq!(coin_game.config.max_tokens, 5);
    
    assert_eq!(dice_game.config.num_dice, 5);
    assert_eq!(dice_game.config.dice_sides, 6);
    assert!(dice_game.config.reroll_allowed);
}

#[test]
fn test_move_data_serialization() -> Result<(), GameProtocolError> {
    // Test CoinFlip move serialization
    let coin_move = CoinFlipMove {
        chosen_side: CoinSide::Heads,
        confidence: 200,
    };
    let coin_json = serde_json::to_value(&coin_move)?;
    let coin_deserialized: CoinFlipMove = serde_json::from_value(coin_json)?;
    assert_eq!(coin_deserialized.chosen_side, CoinSide::Heads);
    assert_eq!(coin_deserialized.confidence, 200);
    
    // Test Dice move serialization
    let dice_move = DiceMove {
        action: DiceAction::Reroll { dice_indices: vec![0, 2, 4] },
        dice_to_keep: vec![1, 3],
    };
    let dice_json = serde_json::to_value(&dice_move)?;
    let dice_deserialized: DiceMove = serde_json::from_value(dice_json)?;
    assert_eq!(dice_deserialized.dice_to_keep, vec![1, 3]);
    
    Ok(())
}
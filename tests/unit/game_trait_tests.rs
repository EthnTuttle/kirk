//! Unit tests for game trait implementations and validation

use kirk::{Game, ValidationResult, ValidationError, ValidationErrorType};
use tests::mocks::{CoinFlipGame, reference_game::{CoinFlipMove, CoinSide}};
use nostr::{Event, EventBuilder, Keys, EventId, Kind};
use cdk::nuts::{Token, Proof, Id, CurrencyUnit, Secret, PublicKey as CashuPublicKey};
use cdk::Amount;

/// Helper to create a mock event for testing
fn create_mock_event(kind: u16, content: &str, keys: &Keys) -> Event {
    EventBuilder::new(Kind::Custom(kind), content, Vec::<nostr::Tag>::new())
        .to_event(keys)
        .unwrap()
}

/// Helper to create a test token with specific C value
fn create_test_token_with_c(c_value: &str) -> Token {
    let proof = Proof {
        amount: Amount::from(100),
        secret: Secret::new("test_secret"),
        c: CashuPublicKey::from_hex(&format!("{:0>64}", c_value)).unwrap(),
        keyset_id: Id::from_bytes(&[0u8; 8]).unwrap(),
        witness: None,
        dleq: None,
    };

    Token::new(
        "https://test-mint.example.com".parse().unwrap(),
        vec![proof],
        None,
        CurrencyUnit::Sat,
    )
}

#[cfg(test)]
mod coin_flip_game_tests {
    use super::*;

    #[test]
    fn test_c_value_decoding() {
        let game = CoinFlipGame::new();
        
        // Test heads (even first byte)
        let c_value_heads = [2u8, 150u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8];
        let pieces = game.decode_c_value(&c_value_heads).unwrap();
        
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].side, CoinSide::Heads);
        assert_eq!(pieces[0].strength, 150);
        
        // Test tails (odd first byte)
        let c_value_tails = [3u8, 75u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8];
        let pieces = game.decode_c_value(&c_value_tails).unwrap();
        
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].side, CoinSide::Tails);
        assert_eq!(pieces[0].strength, 75);
    }

    #[test]
    fn test_game_type() {
        assert_eq!(CoinFlipGame::game_type(), "coinflip");
    }

    #[test]
    fn test_required_final_events() {
        let game = CoinFlipGame::new();
        assert_eq!(game.required_final_events(), 2);
    }

    #[test]
    fn test_get_parameters() {
        let game = CoinFlipGame::new();
        let params = game.get_parameters().unwrap();
        
        assert!(params.get("min_tokens").is_some());
        assert!(params.get("max_tokens").is_some());
        assert_eq!(params["min_tokens"], 1);
        assert_eq!(params["max_tokens"], 5);
    }

    #[test]
    fn test_custom_config() {
        let config = tests::mocks::reference_game::CoinFlipConfig {
            min_tokens: 2,
            max_tokens: 10,
        };
        let game = CoinFlipGame::with_config(config);
        let params = game.get_parameters().unwrap();
        
        assert_eq!(params["min_tokens"], 2);
        assert_eq!(params["max_tokens"], 10);
    }
}

#[cfg(test)]
mod sequence_validation_tests {
    use super::*;

    #[test]
    fn test_empty_sequence_validation() {
        let game = CoinFlipGame::new();
        let events = vec![];
        
        let result = game.validate_sequence(&events).unwrap();
        assert!(result.is_valid);
        assert!(result.winner.is_none());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_sequence_completeness_empty() {
        let game = CoinFlipGame::new();
        let events = vec![];
        
        let is_complete = game.is_sequence_complete(&events).unwrap();
        assert!(!is_complete);
    }

    #[test]
    fn test_sequence_completeness_partial() {
        let game = CoinFlipGame::new();
        let keys1 = Keys::generate();
        let keys2 = Keys::generate();
        
        // Create partial sequence: Challenge + Accept
        let challenge_event = create_mock_event(9259, "challenge", &keys1);
        let accept_event = create_mock_event(9260, "accept", &keys2);
        
        let events = vec![challenge_event, accept_event];
        let is_complete = game.is_sequence_complete(&events).unwrap();
        assert!(!is_complete); // Missing moves and final events
    }

    #[test]
    fn test_sequence_completeness_full() {
        let game = CoinFlipGame::new();
        let keys1 = Keys::generate();
        let keys2 = Keys::generate();
        
        // Create complete sequence
        let challenge_event = create_mock_event(9259, "challenge", &keys1);
        let accept_event = create_mock_event(9260, "accept", &keys2);
        let move1_event = create_mock_event(9261, "move1", &keys1);
        let move2_event = create_mock_event(9261, "move2", &keys2);
        let final1_event = create_mock_event(9262, "final1", &keys1);
        let final2_event = create_mock_event(9262, "final2", &keys2);
        
        let events = vec![
            challenge_event, accept_event, 
            move1_event, move2_event,
            final1_event, final2_event
        ];
        
        let is_complete = game.is_sequence_complete(&events).unwrap();
        assert!(is_complete);
    }

    #[test]
    fn test_move_event_parsing() {
        let game = CoinFlipGame::new();
        let keys = Keys::generate();
        
        // Create a move event with valid content
        let move_content = kirk::MoveContent {
            previous_event_id: EventId::from_slice(&[1u8; 32]).unwrap(),
            move_type: kirk::MoveType::Reveal,
            move_data: serde_json::to_value(CoinFlipMove {
                chosen_side: CoinSide::Heads,
                confidence: 200,
            }).unwrap(),
            revealed_tokens: Some(vec![create_test_token_with_c("abcd1234")]),
        };
        
        let content_json = serde_json::to_string(&move_content).unwrap();
        let move_event = create_mock_event(9261, &content_json, &keys);
        
        let result = game.validate_sequence(&[move_event]).unwrap();
        assert!(result.is_valid);
    }

    #[test]
    fn test_invalid_move_content() {
        let game = CoinFlipGame::new();
        let keys = Keys::generate();
        
        // Create move event with invalid JSON
        let move_event = create_mock_event(9261, "invalid json", &keys);
        
        let result = game.validate_sequence(&[move_event]).unwrap();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
        assert!(matches!(result.errors[0].error_type, ValidationErrorType::InvalidMove));
    }

    #[test]
    fn test_winner_determination_empty_sequence() {
        let game = CoinFlipGame::new();
        let events = vec![];
        
        let winner = game.determine_winner(&events).unwrap();
        assert!(winner.is_none());
    }
}

#[cfg(test)]
mod game_trait_interface_tests {
    use super::*;

    #[test]
    fn test_game_trait_associated_types() {
        let game = CoinFlipGame::new();
        
        // Test that we can create the associated types
        let c_value = [42u8; 32];
        let pieces = game.decode_c_value(&c_value).unwrap();
        
        // GamePiece type
        assert_eq!(pieces.len(), 1);
        let _piece: tests::mocks::reference_game::CoinFlipPiece = pieces[0].clone();
        
        // MoveData type
        let _move_data: CoinFlipMove = CoinFlipMove {
            chosen_side: CoinSide::Heads,
            confidence: 100,
        };
        
        // GameState type (implicitly tested through validation)
        let events = vec![];
        let _result = game.validate_sequence(&events).unwrap();
    }

    #[test]
    fn test_game_trait_methods_exist() {
        let game = CoinFlipGame::new();
        let c_value = [0u8; 32];
        let events = vec![];
        
        // All required trait methods should be callable
        let _pieces = game.decode_c_value(&c_value).unwrap();
        let _validation = game.validate_sequence(&events).unwrap();
        let _complete = game.is_sequence_complete(&events).unwrap();
        let _winner = game.determine_winner(&events).unwrap();
        let _final_count = game.required_final_events();
    }

    #[test]
    fn test_validation_result_structure() {
        let game = CoinFlipGame::new();
        let events = vec![];
        
        let result = game.validate_sequence(&events).unwrap();
        
        // ValidationResult should have all required fields
        let _is_valid: bool = result.is_valid;
        let _winner: Option<nostr::PublicKey> = result.winner;
        let _errors: Vec<ValidationError> = result.errors;
        let _forfeited: Option<nostr::PublicKey> = result.forfeited_player;
    }

    #[test]
    fn test_validation_error_structure() {
        let game = CoinFlipGame::new();
        let keys = Keys::generate();
        
        // Create an invalid event to trigger validation error
        let invalid_event = create_mock_event(9261, "invalid", &keys);
        let result = game.validate_sequence(&[invalid_event.clone()]).unwrap();
        
        if !result.errors.is_empty() {
            let error = &result.errors[0];
            
            // ValidationError should have all required fields
            let _event_id: EventId = error.event_id;
            let _error_type: ValidationErrorType = error.error_type.clone();
            let _message: String = error.message.clone();
            
            assert_eq!(error.event_id, invalid_event.id);
        }
    }
}

#[cfg(test)]
mod game_piece_decoding_tests {
    use super::*;

    #[test]
    fn test_c_value_edge_cases() {
        let game = CoinFlipGame::new();
        
        // Test with all zeros
        let c_value_zeros = [0u8; 32];
        let pieces = game.decode_c_value(&c_value_zeros).unwrap();
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].side, CoinSide::Heads); // 0 is even
        assert_eq!(pieces[0].strength, 0);
        
        // Test with all 255s
        let c_value_max = [255u8; 32];
        let pieces = game.decode_c_value(&c_value_max).unwrap();
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].side, CoinSide::Tails); // 255 is odd
        assert_eq!(pieces[0].strength, 255);
    }

    #[test]
    fn test_c_value_boundary_conditions() {
        let game = CoinFlipGame::new();
        
        // Test boundary between heads/tails
        let mut c_value = [0u8; 32];
        
        // Even numbers should be heads
        for even in [0, 2, 4, 254].iter() {
            c_value[0] = *even;
            let pieces = game.decode_c_value(&c_value).unwrap();
            assert_eq!(pieces[0].side, CoinSide::Heads);
        }
        
        // Odd numbers should be tails
        for odd in [1, 3, 5, 255].iter() {
            c_value[0] = *odd;
            let pieces = game.decode_c_value(&c_value).unwrap();
            assert_eq!(pieces[0].side, CoinSide::Tails);
        }
    }

    #[test]
    fn test_strength_extraction() {
        let game = CoinFlipGame::new();
        
        // Test various strength values
        for strength in [0, 1, 127, 128, 254, 255].iter() {
            let mut c_value = [0u8; 32];
            c_value[1] = *strength; // Second byte is strength
            
            let pieces = game.decode_c_value(&c_value).unwrap();
            assert_eq!(pieces[0].strength, *strength);
        }
    }

    #[test]
    fn test_c_value_independence() {
        let game = CoinFlipGame::new();
        
        // Changing bytes other than first two shouldn't affect result
        let mut base_c_value = [42u8; 32];
        let base_pieces = game.decode_c_value(&base_c_value).unwrap();
        
        // Change bytes 2-31
        for i in 2..32 {
            base_c_value[i] = 200;
            let pieces = game.decode_c_value(&base_c_value).unwrap();
            
            // Should be same as base
            assert_eq!(pieces[0].side, base_pieces[0].side);
            assert_eq!(pieces[0].strength, base_pieces[0].strength);
        }
    }
}

#[cfg(test)]
mod winner_calculation_tests {
    use super::*;

    #[test]
    fn test_winner_with_no_players() {
        let game = CoinFlipGame::new();
        let mut state = tests::mocks::reference_game::CoinFlipState {
            players: std::collections::HashMap::new(),
            moves_made: std::collections::HashMap::new(),
            winner: None,
        };
        
        let winner = game.calculate_winner(&state);
        assert!(winner.is_none());
    }

    #[test]
    fn test_winner_with_one_player() {
        let game = CoinFlipGame::new();
        let player1 = nostr::PublicKey::from_slice(&[1u8; 32]).unwrap();
        
        let mut state = tests::mocks::reference_game::CoinFlipState {
            players: std::collections::HashMap::new(),
            moves_made: std::collections::HashMap::new(),
            winner: None,
        };
        
        state.moves_made.insert(player1, CoinFlipMove {
            chosen_side: CoinSide::Heads,
            confidence: 100,
        });
        
        let winner = game.calculate_winner(&state);
        assert!(winner.is_none()); // Need exactly 2 players
    }

    #[test]
    fn test_winner_calculation_logic() {
        let game = CoinFlipGame::new();
        let player1 = nostr::PublicKey::from_slice(&[1u8; 32]).unwrap();
        let player2 = nostr::PublicKey::from_slice(&[2u8; 32]).unwrap();
        
        let mut state = tests::mocks::reference_game::CoinFlipState {
            players: std::collections::HashMap::new(),
            moves_made: std::collections::HashMap::new(),
            winner: None,
        };
        
        // Set up pieces that XOR to even (heads)
        state.players.insert(player1, vec![
            tests::mocks::reference_game::CoinFlipPiece { side: CoinSide::Heads, strength: 4 }
        ]);
        state.players.insert(player2, vec![
            tests::mocks::reference_game::CoinFlipPiece { side: CoinSide::Tails, strength: 6 }
        ]);
        
        // Player1 guesses heads (correct), player2 guesses tails (incorrect)
        state.moves_made.insert(player1, CoinFlipMove {
            chosen_side: CoinSide::Heads,
            confidence: 150,
        });
        state.moves_made.insert(player2, CoinFlipMove {
            chosen_side: CoinSide::Tails,
            confidence: 100,
        });
        
        // 4 XOR 6 = 2 (even) = Heads, so player1 should win
        let winner = game.calculate_winner(&state);
        assert_eq!(winner, Some(player1));
    }

    #[test]
    fn test_winner_confidence_tiebreaker() {
        let game = CoinFlipGame::new();
        let player1 = nostr::PublicKey::from_slice(&[1u8; 32]).unwrap();
        let player2 = nostr::PublicKey::from_slice(&[2u8; 32]).unwrap();
        
        let mut state = tests::mocks::reference_game::CoinFlipState {
            players: std::collections::HashMap::new(),
            moves_made: std::collections::HashMap::new(),
            winner: None,
        };
        
        // Set up pieces that XOR to odd (tails)
        state.players.insert(player1, vec![
            tests::mocks::reference_game::CoinFlipPiece { side: CoinSide::Heads, strength: 3 }
        ]);
        state.players.insert(player2, vec![
            tests::mocks::reference_game::CoinFlipPiece { side: CoinSide::Tails, strength: 4 }
        ]);
        
        // Both players guess tails (correct), but player2 has higher confidence
        state.moves_made.insert(player1, CoinFlipMove {
            chosen_side: CoinSide::Tails,
            confidence: 100,
        });
        state.moves_made.insert(player2, CoinFlipMove {
            chosen_side: CoinSide::Tails,
            confidence: 200,
        });
        
        // 3 XOR 4 = 7 (odd) = Tails, player2 should win due to higher confidence
        let winner = game.calculate_winner(&state);
        assert_eq!(winner, Some(player2));
    }
}
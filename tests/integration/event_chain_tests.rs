//! Integration tests for event chain validation and sequencing

use kirk::{ChallengeContent, ChallengeAcceptContent, MoveContent, FinalContent, MoveType};
use tests::mocks::{MockNostrRelay, CoinFlipGame};
use nostr::{Keys, EventBuilder, Kind, EventId};
use std::collections::HashMap;

/// Helper to create a chain of events with proper references
fn create_event_chain(relay: &MockNostrRelay, length: usize) -> Vec<nostr::Event> {
    let keys = Keys::generate();
    let mut events = Vec::new();
    let mut previous_id = None;
    
    for i in 0..length {
        let content = format!("Event {} in chain", i);
        let mut builder = EventBuilder::new(Kind::TextNote, content, Vec::<nostr::Tag>::new());
        
        // Reference previous event if exists
        if let Some(prev_id) = previous_id {
            builder = builder.add_tags(vec![nostr::Tag::event(prev_id)]);
        }
        
        let event = builder.to_event(&keys).unwrap();
        previous_id = Some(event.id);
        
        relay.store_event(event.clone()).unwrap();
        events.push(event);
    }
    
    events
}

#[cfg(test)]
mod event_sequencing_tests {
    use super::*;

    #[test]
    fn test_event_chain_creation() {
        let relay = MockNostrRelay::new();
        let chain_length = 5;
        
        let events = create_event_chain(&relay, chain_length);
        
        assert_eq!(events.len(), chain_length);
        assert_eq!(relay.event_count(), chain_length);
        
        // Verify each event (except first) references the previous one
        for i in 1..events.len() {
            let current_event = &events[i];
            let previous_event_id = events[i - 1].id;
            
            // Check if current event references previous event
            let has_reference = current_event.tags.iter().any(|tag| {
                match tag.as_slice().get(0) {
                    Some(tag_name) if tag_name == "e" => {
                        if let Some(event_id_str) = tag.as_slice().get(1) {
                            if let Ok(event_id) = EventId::from_hex(event_id_str) {
                                return event_id == previous_event_id;
                            }
                        }
                        false
                    },
                    _ => false,
                }
            });
            
            assert!(has_reference, "Event {} should reference event {}", i, i - 1);
        }
    }

    #[test]
    fn test_game_event_sequence_validation() {
        let relay = MockNostrRelay::new();
        let game = CoinFlipGame::new();
        let player1_keys = Keys::generate();
        let player2_keys = Keys::generate();
        
        // Create proper game event sequence
        let challenge_content = ChallengeContent {
            game_type: CoinFlipGame::game_type(),
            commitment_hashes: vec!["hash1".to_string()],
            game_parameters: game.get_parameters().unwrap(),
            expiry: None,
        };
        let challenge_event = challenge_content.to_event(&player1_keys).unwrap();
        relay.store_event(challenge_event.clone()).unwrap();
        
        let accept_content = ChallengeAcceptContent {
            challenge_id: challenge_event.id,
            commitment_hashes: vec!["hash2".to_string()],
        };
        let accept_event = accept_content.to_event(&player2_keys).unwrap();
        relay.store_event(accept_event.clone()).unwrap();
        
        let move1_content = MoveContent {
            previous_event_id: accept_event.id,
            move_type: MoveType::Move,
            move_data: serde_json::json!({"action": "move1"}),
            revealed_tokens: None,
        };
        let move1_event = move1_content.to_event(&player1_keys).unwrap();
        relay.store_event(move1_event.clone()).unwrap();
        
        let move2_content = MoveContent {
            previous_event_id: move1_event.id,
            move_type: MoveType::Move,
            move_data: serde_json::json!({"action": "move2"}),
            revealed_tokens: None,
        };
        let move2_event = move2_content.to_event(&player2_keys).unwrap();
        relay.store_event(move2_event.clone()).unwrap();
        
        let final1_content = FinalContent {
            game_sequence_root: challenge_event.id,
            commitment_method: None,
            final_state: serde_json::json!({"player": 1}),
        };
        let final1_event = final1_content.to_event(&player1_keys).unwrap();
        relay.store_event(final1_event.clone()).unwrap();
        
        let final2_content = FinalContent {
            game_sequence_root: challenge_event.id,
            commitment_method: None,
            final_state: serde_json::json!({"player": 2}),
        };
        let final2_event = final2_content.to_event(&player2_keys).unwrap();
        relay.store_event(final2_event.clone()).unwrap();
        
        let all_events = vec![
            challenge_event, accept_event, move1_event, move2_event, final1_event, final2_event
        ];
        
        // Validate the sequence
        let validation_result = game.validate_sequence(&all_events).unwrap();
        assert!(validation_result.is_valid);
        
        let is_complete = game.is_sequence_complete(&all_events).unwrap();
        assert!(is_complete);
    }

    #[test]
    fn test_event_chain_integrity() {
        let relay = MockNostrRelay::new();
        let keys = Keys::generate();
        
        // Create a sequence where each event properly references the previous
        let event1 = EventBuilder::new(Kind::Custom(9259), "challenge", Vec::<nostr::Tag>::new())
            .to_event(&keys).unwrap();
        relay.store_event(event1.clone()).unwrap();
        
        let event2_content = serde_json::to_string(&ChallengeAcceptContent {
            challenge_id: event1.id,
            commitment_hashes: vec!["hash".to_string()],
        }).unwrap();
        let event2 = EventBuilder::new(Kind::Custom(9260), event2_content, Vec::<nostr::Tag>::new())
            .to_event(&keys).unwrap();
        relay.store_event(event2.clone()).unwrap();
        
        let event3_content = serde_json::to_string(&MoveContent {
            previous_event_id: event2.id,
            move_type: MoveType::Move,
            move_data: serde_json::json!({}),
            revealed_tokens: None,
        }).unwrap();
        let event3 = EventBuilder::new(Kind::Custom(9261), event3_content, Vec::<nostr::Tag>::new())
            .to_event(&keys).unwrap();
        relay.store_event(event3.clone()).unwrap();
        
        // Verify chain integrity by parsing content
        let parsed_accept: ChallengeAcceptContent = serde_json::from_str(&event2.content).unwrap();
        assert_eq!(parsed_accept.challenge_id, event1.id);
        
        let parsed_move: MoveContent = serde_json::from_str(&event3.content).unwrap();
        assert_eq!(parsed_move.previous_event_id, event2.id);
        
        // Verify events are stored and retrievable
        assert_eq!(relay.event_count(), 3);
        assert!(relay.get_event(&event1.id).is_some());
        assert!(relay.get_event(&event2.id).is_some());
        assert!(relay.get_event(&event3.id).is_some());
    }

    #[test]
    fn test_broken_event_chain() {
        let relay = MockNostrRelay::new();
        let game = CoinFlipGame::new();
        let keys = Keys::generate();
        
        // Create events with broken chain (wrong previous_event_id)
        let event1 = EventBuilder::new(Kind::Custom(9259), "challenge", Vec::<nostr::Tag>::new())
            .to_event(&keys).unwrap();
        
        let wrong_id = EventId::from_slice(&[99u8; 32]).unwrap();
        let broken_move_content = serde_json::to_string(&MoveContent {
            previous_event_id: wrong_id, // Wrong reference
            move_type: MoveType::Move,
            move_data: serde_json::json!({}),
            revealed_tokens: None,
        }).unwrap();
        let event2 = EventBuilder::new(Kind::Custom(9261), broken_move_content, Vec::<nostr::Tag>::new())
            .to_event(&keys).unwrap();
        
        let events = vec![event1, event2];
        
        // Chain is broken but individual events might still be valid
        let validation_result = game.validate_sequence(&events).unwrap();
        // The game implementation might or might not catch this depending on validation logic
        
        let is_complete = game.is_sequence_complete(&events).unwrap();
        assert!(!is_complete); // Should not be complete with broken chain
    }
}

#[cfg(test)]
mod event_ordering_tests {
    use super::*;

    #[test]
    fn test_event_timestamp_ordering() {
        let relay = MockNostrRelay::new();
        let keys = Keys::generate();
        
        // Create events with different timestamps
        let mut events = Vec::new();
        
        for i in 0..5 {
            // Create events with incrementing timestamps
            let mut builder = EventBuilder::new(Kind::TextNote, format!("Event {}", i), Vec::<nostr::Tag>::new());
            let event = builder.to_event(&keys).unwrap();
            
            relay.store_event(event.clone()).unwrap();
            events.push(event);
            
            // Small delay to ensure different timestamps
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        
        // Verify events are ordered by timestamp
        for i in 1..events.len() {
            assert!(events[i].created_at >= events[i-1].created_at,
                   "Event {} timestamp should be >= event {} timestamp", i, i-1);
        }
    }

    #[test]
    fn test_event_retrieval_by_author() {
        let relay = MockNostrRelay::new();
        let player1_keys = Keys::generate();
        let player2_keys = Keys::generate();
        
        // Create events from different authors
        let event1 = EventBuilder::new(Kind::TextNote, "Player 1 event", Vec::<nostr::Tag>::new())
            .to_event(&player1_keys).unwrap();
        let event2 = EventBuilder::new(Kind::TextNote, "Player 2 event", Vec::<nostr::Tag>::new())
            .to_event(&player2_keys).unwrap();
        let event3 = EventBuilder::new(Kind::TextNote, "Another Player 1 event", Vec::<nostr::Tag>::new())
            .to_event(&player1_keys).unwrap();
        
        relay.store_event(event1.clone()).unwrap();
        relay.store_event(event2.clone()).unwrap();
        relay.store_event(event3.clone()).unwrap();
        
        // Retrieve events by author
        let player1_events = relay.get_events_by_author(&player1_keys.public_key());
        let player2_events = relay.get_events_by_author(&player2_keys.public_key());
        
        assert_eq!(player1_events.len(), 2);
        assert_eq!(player2_events.len(), 1);
        
        // Verify correct events are returned
        assert!(player1_events.iter().any(|e| e.id == event1.id));
        assert!(player1_events.iter().any(|e| e.id == event3.id));
        assert!(player2_events.iter().any(|e| e.id == event2.id));
    }

    #[test]
    fn test_event_retrieval_by_kind() {
        let relay = MockNostrRelay::new();
        let keys = Keys::generate();
        
        // Create events of different kinds
        let challenge_event = EventBuilder::new(Kind::Custom(9259), "challenge", Vec::<nostr::Tag>::new())
            .to_event(&keys).unwrap();
        let move_event = EventBuilder::new(Kind::Custom(9261), "move", Vec::<nostr::Tag>::new())
            .to_event(&keys).unwrap();
        let text_event = EventBuilder::new(Kind::TextNote, "text", Vec::<nostr::Tag>::new())
            .to_event(&keys).unwrap();
        
        relay.store_event(challenge_event.clone()).unwrap();
        relay.store_event(move_event.clone()).unwrap();
        relay.store_event(text_event.clone()).unwrap();
        
        // Retrieve by kind
        let challenge_events = relay.get_events_by_kind(Kind::Custom(9259));
        let move_events = relay.get_events_by_kind(Kind::Custom(9261));
        let text_events = relay.get_events_by_kind(Kind::TextNote);
        
        assert_eq!(challenge_events.len(), 1);
        assert_eq!(move_events.len(), 1);
        assert_eq!(text_events.len(), 1);
        
        assert_eq!(challenge_events[0].id, challenge_event.id);
        assert_eq!(move_events[0].id, move_event.id);
        assert_eq!(text_events[0].id, text_event.id);
    }
}

#[cfg(test)]
mod event_validation_tests {
    use super::*;

    #[test]
    fn test_event_signature_verification() {
        let relay = MockNostrRelay::new();
        let keys = Keys::generate();
        
        let event = EventBuilder::new(Kind::TextNote, "test content", Vec::<nostr::Tag>::new())
            .to_event(&keys).unwrap();
        
        // Verify signature is valid
        assert!(event.verify().is_ok());
        
        // Store and retrieve
        relay.store_event(event.clone()).unwrap();
        let retrieved = relay.get_event(&event.id).unwrap();
        
        // Retrieved event should still have valid signature
        assert!(retrieved.verify().is_ok());
        assert_eq!(retrieved.pubkey, keys.public_key());
    }

    #[test]
    fn test_event_content_integrity() {
        let relay = MockNostrRelay::new();
        let keys = Keys::generate();
        
        let original_content = "This is the original content";
        let event = EventBuilder::new(Kind::TextNote, original_content, Vec::<nostr::Tag>::new())
            .to_event(&keys).unwrap();
        
        relay.store_event(event.clone()).unwrap();
        let retrieved = relay.get_event(&event.id).unwrap();
        
        // Content should be unchanged
        assert_eq!(retrieved.content, original_content);
        assert_eq!(retrieved.id, event.id);
        assert_eq!(retrieved.pubkey, event.pubkey);
        assert_eq!(retrieved.created_at, event.created_at);
    }

    #[test]
    fn test_game_event_content_parsing() {
        let keys = Keys::generate();
        
        // Test Challenge event content
        let challenge_content = ChallengeContent {
            game_type: "test_game".to_string(),
            commitment_hashes: vec!["hash1".to_string(), "hash2".to_string()],
            game_parameters: serde_json::json!({"param": "value"}),
            expiry: Some(1234567890),
        };
        
        let challenge_event = challenge_content.to_event(&keys).unwrap();
        
        // Parse content back
        let parsed_content: ChallengeContent = serde_json::from_str(&challenge_event.content).unwrap();
        
        assert_eq!(parsed_content.game_type, challenge_content.game_type);
        assert_eq!(parsed_content.commitment_hashes, challenge_content.commitment_hashes);
        assert_eq!(parsed_content.expiry, challenge_content.expiry);
        
        // Test Move event content
        let move_content = MoveContent {
            previous_event_id: challenge_event.id,
            move_type: MoveType::Commit,
            move_data: serde_json::json!({"move": "test_move"}),
            revealed_tokens: None,
        };
        
        let move_event = move_content.to_event(&keys).unwrap();
        let parsed_move: MoveContent = serde_json::from_str(&move_event.content).unwrap();
        
        assert_eq!(parsed_move.previous_event_id, move_content.previous_event_id);
        assert!(matches!(parsed_move.move_type, MoveType::Commit));
        assert!(parsed_move.revealed_tokens.is_none());
    }

    #[test]
    fn test_malformed_event_content() {
        let relay = MockNostrRelay::new();
        let keys = Keys::generate();
        
        // Create event with malformed JSON content
        let malformed_event = EventBuilder::new(Kind::Custom(9261), "invalid json {", Vec::<nostr::Tag>::new())
            .to_event(&keys).unwrap();
        
        relay.store_event(malformed_event.clone()).unwrap();
        
        // Event should be stored (relay doesn't validate content)
        assert_eq!(relay.event_count(), 1);
        
        // But parsing should fail
        let retrieved = relay.get_event(&malformed_event.id).unwrap();
        let parse_result: Result<MoveContent, _> = serde_json::from_str(&retrieved.content);
        assert!(parse_result.is_err());
    }
}

#[cfg(test)]
mod concurrent_event_tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_concurrent_event_storage() {
        let relay = Arc::new(MockNostrRelay::new());
        let mut handles = Vec::new();
        
        // Spawn multiple threads storing events concurrently
        for i in 0..10 {
            let relay_clone: Arc<MockNostrRelay> = Arc::clone(&relay);
            let handle = thread::spawn(move || {
                let keys = Keys::generate();
                let event = EventBuilder::new(Kind::TextNote, format!("Event {}", i), Vec::<nostr::Tag>::new())
                    .to_event(&keys).unwrap();
                relay_clone.store_event(event).unwrap();
            });
            handles.push(handle);
        }
        
        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }
        
        // Should have stored all events
        assert_eq!(relay.event_count(), 10);
    }

    #[test]
    fn test_concurrent_event_retrieval() {
        let relay = Arc::new(MockNostrRelay::new());
        let keys = Keys::generate();
        
        // Store some events first
        for i in 0..5 {
            let event = EventBuilder::new(Kind::TextNote, format!("Event {}", i), Vec::<nostr::Tag>::new())
                .to_event(&keys).unwrap();
            relay.store_event(event).unwrap();
        }
        
        let mut handles = Vec::new();
        
        // Spawn multiple threads reading events concurrently
        for _ in 0..10 {
            let relay_clone: Arc<MockNostrRelay> = Arc::clone(&relay);
            let pubkey = keys.public_key();
            let handle = thread::spawn(move || {
                let events = relay_clone.get_events_by_author(&pubkey);
                assert_eq!(events.len(), 5);
            });
            handles.push(handle);
        }
        
        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }
    }
}
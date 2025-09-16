# Implementation Plan

- [x] 1. Set up project structure and core dependencies
  - Create Cargo.toml with CDK 0.12.1, nostr-sdk 0.35, and other dependencies
  - Set up basic library structure with modules: events, game, cashu, client, error
  - Create lib.rs with public module exports
  - _Requirements: 1.1, 1.2_

- [x] 2. Implement core error types and utilities
  - Define GameProtocolError enum with all error variants
  - Implement error conversions from CDK and nostr errors
  - Add utility functions for SHA256 hashing and hex encoding
  - _Requirements: 1.4_

- [x] 3. Implement nostr event content structures
  - Create ChallengeContent, ChallengeAcceptContent, MoveContent, FinalContent, RewardContent structs
  - Implement Serialize/Deserialize for all content types
  - Define CommitmentMethod enum (Concatenation, MerkleTreeRadix4)
  - Add MoveType enum (Move, Commit, Reveal)
  - _Requirements: 2.1, 2.2, 3.1, 4.1, 5.1_

- [x] 4. Implement nostr event builders
  - Create event kind constants (9259-9263) for all game event types
  - Implement to_event() methods for each content struct using nostr EventBuilder
  - Add event validation and parsing utilities
  - Write unit tests for event serialization/deserialization
  - _Requirements: 2.1, 3.1, 4.1, 5.1_

- [x] 5. Implement standardized hash commitment system
  - Create TokenCommitment struct with commitment_hash and commitment_type
  - Implement single token commitment using SHA256(token_hash)
  - Implement concatenation commitment for multiple tokens
  - Implement merkle tree radix 4 commitment algorithm
  - Add commitment verification methods
  - Write comprehensive tests for all commitment methods
  - _Requirements: 2.3, 2.4, 5a.1, 5a.2, 5a.3_

- [x] 6. Implement core game traits
  - Define Game trait with GamePiece, GameState, MoveData associated types
  - Add decode_c_value method for extracting game pieces from C values
  - Add validate_sequence, is_sequence_complete, determine_winner methods
  - Add required_final_events method for game-specific Final event requirements
  - Define CommitmentValidator trait with validation methods
  - _Requirements: 8.1, 8.2, 8.3, 8.4, 8.6_

- [x] 7. Implement CDK integration layer
  - Create GameTokenType enum (Game, Reward with P2PK locking)
  - Implement GameToken wrapper around CDK Token with game context
  - Add methods to extract C values from token proofs
  - Add P2PK locking detection and utilities
  - Write unit tests for token wrapper functionality
  - _Requirements: 6.1, 6.3, 6.8_

- [ ] 8. Implement mint operations wrapper
  - Create GameMint struct wrapping CDK Mint with nostr client
  - Implement mint_game_tokens using CDK's standard minting
  - Implement mint_reward_tokens using NUT-11 P2PK locking
  - Add token validation using CDK's verify_token
  - Add swap and melt operations using CDK request/response pattern
  - Add publish_game_result method for nostr reward events
  - _Requirements: 6.2, 6.4, 6.5, 6.6, 7.1, 7.4, 7.5, 7.7_

- [ ] 9. Implement game sequence validation
  - Create GameSequence struct with challenge_id, players, events, state
  - Implement SequenceState enum with proper state transitions
  - Add state transition validation (WaitingForAccept → InProgress → WaitingForFinal → Complete)
  - Add methods for checking state capabilities (can_accept_moves, needs_final_events, is_finished)
  - Implement sequence integrity validation (event chain verification)
  - _Requirements: 3.2, 3.3, 5.2, 7.2, 7.3_

- [ ] 10. Implement player client
  - Create PlayerClient struct with nostr client, CDK wallet, and keys
  - Implement create_challenge with configurable expiry (default 1 hour)
  - Add create_challenge_default convenience method
  - Implement accept_challenge with commitment creation
  - Add make_move method supporting all move types (Move, Commit, Reveal)
  - Implement finalize_game for publishing Final events
  - Add private create_commitments helper method
  - _Requirements: 2.1, 2.2, 3.1, 4.2, 5.1_

- [ ] 11. Implement commitment construction algorithms
  - Implement build_merkle_tree_radix4 with proper padding and hashing
  - Implement build_concatenation_commitment with sorted token ordering
  - Add hash_token standardized function for consistent token hashing
  - Ensure all algorithms sort tokens in ascending order by hash
  - Write property-based tests for commitment determinism
  - _Requirements: 5a.2, 5a.3, 8.1_

- [ ] 12. Implement game sequence processor for mints
  - Create sequence collection and validation logic
  - Add fraud detection for invalid moves and commitment violations
  - Implement winner determination based on game rules
  - Add forfeiture handling for rule violations and timeouts
  - Implement reward calculation and distribution
  - Add ValidationFailure event publishing for system errors
  - _Requirements: 7.1, 7.2, 7.3, 7.4, 7.5, 7.8_

- [ ] 13. Create comprehensive test suite
  - Write unit tests for all commitment methods and validation
  - Create integration tests for full game sequences
  - Add property-based tests for C value randomness and commitment security
  - Implement mock nostr relay and CDK mint for testing
  - Create reference game implementation for testing framework
  - Add end-to-end game simulation tests
  - _Requirements: 1.3, 8.7_

- [ ] 14. Implement validation client for observers
  - Create ValidationClient for third-party game sequence verification
  - Add event collection and filtering by game sequence
  - Implement independent sequence validation without mint authority
  - Add commitment verification against revealed tokens
  - Create validation reporting and error detection
  - _Requirements: 3.6, 5.2, 5a.1, 5a.4_

- [ ] 15. Add P2PK token lifecycle management
  - Implement RewardTokenState enum (P2PKLocked, Unlocked)
  - Add can_spend method for P2PK token validation
  - Implement create_p2pk_locked utility using NUT-11
  - Add token unlocking process through standard CDK operations
  - Write tests for P2PK token operations and state transitions
  - _Requirements: 6.4, 6.8_

- [ ] 16. Create example game implementation
  - Implement a simple reference game (e.g., coin flip or dice game)
  - Demonstrate C value decoding into game pieces
  - Show complete game flow from challenge to reward distribution
  - Add documentation and usage examples
  - Validate framework flexibility with second game type
  - _Requirements: 8.1, 8.2, 8.3, 8.7_

- [ ] 17. Add comprehensive documentation
  - Write API documentation for all public interfaces
  - Create usage guide with examples for each actor (player, mint, validator)
  - Document commitment construction algorithms and standards
  - Add security considerations and best practices
  - Create troubleshooting guide for common issues
  - _Requirements: 1.2, 1.3_

- [ ] 18. Implement optional timeout and deadline management
  - Add configurable timeouts for commit/reveal sequences
  - Implement deadline checking in game validation
  - Add timeout-based forfeiture detection
  - Create time-based state transition triggers
  - Write tests for timeout scenarios and edge cases
  - _Requirements: 4.5, 8.5_
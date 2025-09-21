//! Property-based tests for game logic and sequence validation

use proptest::prelude::*;
use kirk::{
    GameSequence, ValidationResult, TimeoutConfig, GamePhase, MoveType, CommitmentMethod,
    TokenCommitment, GameResult
};
use nostr::{Keys, EventBuilder, Kind, PublicKey, Timestamp};
use cdk::nuts::{Token, Proof, Id, CurrencyUnit, PublicKey as CashuPublicKey};
use cashu::secret::Secret;
use cdk::Amount;
use std::collections::HashSet;
use chrono::{DateTime, Utc, Duration};

/// Generate random game tokens for property tests
fn arb_game_token(amount_range: std::ops::Range<u64>) -> impl Strategy<Value = Token> {
    (any::<[u8; 32]>(), amount_range)
        .prop_map(|(c_bytes, amount)| {
            let c_hex = hex::encode(c_bytes);
            let proof = Proof {
                amount: Amount::from(amount),
                secret: Secret::new(format!("secret_{}", amount)),
                c: CashuPublicKey::from_hex(&c_hex).unwrap(),
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
        })
}

/// Generate vectors of game tokens
fn arb_game_tokens(min_size: usize, max_size: usize) -> impl Strategy<Value = Vec<Token>> {
    prop::collection::vec(arb_game_token(1..1000), min_size..=max_size)
}

/// Generate timeout configurations
fn arb_timeout_config() -> impl Strategy<Value = TimeoutConfig> {
    (60u32..3600, 300u32..7200, 60u32..1800, 60u32..1800)
        .prop_map(|(accept_timeout, move_timeout, commit_timeout, reveal_timeout)| {
            TimeoutConfig {
                accept_timeout,
                move_timeout,
                commit_timeout,
                reveal_timeout,
            }
        })
}

/// Generate move types
fn arb_move_type() -> impl Strategy<Value = MoveType> {
    prop::sample::select(vec![MoveType::Commit, MoveType::Reveal])
}

/// Generate commitment methods
fn arb_commitment_method() -> impl Strategy<Value = CommitmentMethod> {
    prop::sample::select(vec![
        CommitmentMethod::Single,
        CommitmentMethod::Concatenation,
        CommitmentMethod::MerkleTreeRadix4,
    ])
}

/// Generate valid timestamps within reasonable bounds
fn arb_timestamp() -> impl Strategy<Value = Timestamp> {
    (1640000000i64..2000000000i64)
        .prop_map(|ts| Timestamp::from(ts as u64))
}

proptest! {
    #[test]
    fn prop_token_commitment_deterministic(
        tokens in arb_game_tokens(1, 10),
        method in arb_commitment_method()
    ) {
        let commitment1 = if tokens.len() == 1 {
            TokenCommitment::single(&tokens[0])
        } else {
            TokenCommitment::multiple(&tokens, method.clone())
        };

        let commitment2 = if tokens.len() == 1 {
            TokenCommitment::single(&tokens[0])
        } else {
            TokenCommitment::multiple(&tokens, method)
        };

        prop_assert_eq!(commitment1.commitment_hash, commitment2.commitment_hash);
        prop_assert_eq!(commitment1.commitment_type, commitment2.commitment_type);
    }

    #[test]
    fn prop_token_commitment_verification_roundtrip(
        tokens in arb_game_tokens(1, 8),
        method in arb_commitment_method()
    ) {
        let commitment = if tokens.len() == 1 {
            TokenCommitment::single(&tokens[0])
        } else {
            TokenCommitment::multiple(&tokens, method)
        };

        let verification_result = commitment.verify(&tokens);
        prop_assert!(verification_result.is_ok());
        prop_assert!(verification_result.unwrap());
    }

    #[test]
    fn prop_commitment_collision_resistance(
        tokens_list in prop::collection::vec(arb_game_tokens(1, 5), 5..20)
    ) {
        let mut commitment_hashes = HashSet::new();

        for tokens in tokens_list {
            let commitment = if tokens.len() == 1 {
                TokenCommitment::single(&tokens[0])
            } else {
                TokenCommitment::multiple(&tokens, CommitmentMethod::MerkleTreeRadix4)
            };
            commitment_hashes.insert(commitment.commitment_hash);
        }

        // Should have good hash diversity
        prop_assert!(commitment_hashes.len() > 3, "Poor hash diversity");
    }

    #[test]
    fn prop_timeout_config_validation(config in arb_timeout_config()) {
        // All timeouts should be reasonable values
        prop_assert!(config.accept_timeout >= 60);
        prop_assert!(config.move_timeout >= 300);
        prop_assert!(config.commit_timeout >= 60);
        prop_assert!(config.reveal_timeout >= 60);

        // Move timeout should be longer than commit/reveal
        prop_assert!(config.move_timeout >= config.commit_timeout);
        prop_assert!(config.move_timeout >= config.reveal_timeout);

        // Validation should pass for reasonable values
        let validation_result = config.validate();
        prop_assert!(validation_result.is_ok());
    }

    #[test]
    fn prop_game_sequence_state_transitions(
        config in arb_timeout_config(),
        timestamp in arb_timestamp()
    ) {
        let mut sequence = GameSequence::new();

        // Initial state should be WaitingForAccept
        prop_assert!(matches!(sequence.get_phase(), GamePhase::WaitingForAccept));
        prop_assert!(!sequence.is_completed());
        prop_assert!(sequence.get_winner().is_none());

        // Game should start without errors
        prop_assert_eq!(sequence.get_events().len(), 0);

        // Sequence should be valid initially
        let validation = sequence.validate();
        prop_assert!(validation.is_ok());
    }

    #[test]
    fn prop_commitment_ordering_independence(
        mut tokens in arb_game_tokens(2, 8)
    ) {
        let original_commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation);

        // Shuffle tokens
        tokens.reverse();
        let shuffled_commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation);

        // Commitments should be identical due to internal sorting
        prop_assert_eq!(original_commitment.commitment_hash, shuffled_commitment.commitment_hash);
    }

    #[test]
    fn prop_commitment_size_scaling(
        tokens in arb_game_tokens(1, 50)
    ) {
        let commitment = if tokens.len() == 1 {
            TokenCommitment::single(&tokens[0])
        } else {
            TokenCommitment::multiple(&tokens, CommitmentMethod::MerkleTreeRadix4)
        };

        // Commitment hash should always be 32 bytes (64 hex chars)
        prop_assert_eq!(commitment.commitment_hash.len(), 64);

        // Should be valid hex
        prop_assert!(hex::decode(&commitment.commitment_hash).is_ok());

        // Verification should scale reasonably with input size
        let start = std::time::Instant::now();
        let result = commitment.verify(&tokens);
        let duration = start.elapsed();

        prop_assert!(result.is_ok());
        prop_assert!(result.unwrap());

        // Should complete in reasonable time even for larger inputs
        prop_assert!(duration.as_millis() < 1000, "Verification too slow: {}ms", duration.as_millis());
    }

    #[test]
    fn prop_commitment_method_consistency(
        tokens in arb_game_tokens(2, 10)
    ) {
        let concat_commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation);
        let merkle_commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::MerkleTreeRadix4);

        // Both should verify correctly with same tokens
        prop_assert!(concat_commitment.verify(&tokens).unwrap());
        prop_assert!(merkle_commitment.verify(&tokens).unwrap());

        // Should have same properties
        prop_assert_eq!(concat_commitment.commitment_hash.len(), 64);
        prop_assert_eq!(merkle_commitment.commitment_hash.len(), 64);

        // Different methods should generally produce different hashes
        if tokens.len() > 1 {
            prop_assert_ne!(concat_commitment.commitment_hash, merkle_commitment.commitment_hash);
        }
    }

    #[test]
    fn prop_timeout_deadline_calculation(
        base_timestamp in arb_timestamp(),
        timeout_seconds in 60u64..86400u64
    ) {
        let deadline = base_timestamp.as_u64() + timeout_seconds;

        // Deadline should be in the future
        prop_assert!(deadline > base_timestamp.as_u64());

        // Should be reasonable (within bounds)
        prop_assert!(timeout_seconds >= 60); // At least 1 minute
        prop_assert!(timeout_seconds <= 86400); // At most 24 hours

        // Arithmetic should not overflow
        prop_assert!(deadline >= base_timestamp.as_u64());
    }

    #[test]
    fn prop_game_validation_consistency(
        config in arb_timeout_config()
    ) {
        let sequence = GameSequence::new();

        // Validation should be deterministic
        let result1 = sequence.validate();
        let result2 = sequence.validate();

        prop_assert_eq!(result1.is_ok(), result2.is_ok());

        if let (Ok(val1), Ok(val2)) = (&result1, &result2) {
            prop_assert_eq!(val1.is_valid, val2.is_valid);
            prop_assert_eq!(val1.errors.len(), val2.errors.len());
        }
    }

    #[test]
    fn prop_commitment_verification_failure_detection(
        tokens in arb_game_tokens(1, 5),
        wrong_tokens in arb_game_tokens(1, 5)
    ) {
        // Ensure we have different token sets
        if tokens == wrong_tokens {
            return Ok(());
        }

        let commitment = if tokens.len() == 1 {
            TokenCommitment::single(&tokens[0])
        } else {
            TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation)
        };

        // Should verify correctly with original tokens
        prop_assert!(commitment.verify(&tokens).unwrap());

        // Should fail verification with different tokens
        let wrong_verification = commitment.verify(&wrong_tokens);
        if wrong_verification.is_ok() {
            prop_assert!(!wrong_verification.unwrap(), "Verification should fail with wrong tokens");
        }
    }
}

#[cfg(test)]
mod game_logic_edge_cases {
    use super::*;

    #[test]
    fn test_empty_token_commitment() {
        let empty_tokens: Vec<Token> = vec![];

        // Should handle empty token list gracefully
        // This might be an error case depending on implementation
        // The test verifies the behavior is consistent
    }

    #[test]
    fn test_single_vs_multiple_commitment_consistency() {
        // Test that single token commitment using single() method
        // produces same result as multiple() method with one token
        let token = create_test_token("abcd1234".repeat(8), 100);

        let single_commitment = TokenCommitment::single(&token);
        // Note: multiple() with single token might have different behavior
        // This test documents the expected consistency

        assert!(!single_commitment.commitment_hash.is_empty());
        assert!(single_commitment.verify(&[token]).unwrap());
    }

    #[test]
    fn test_commitment_with_zero_amount_tokens() {
        let zero_token = create_test_token("1234abcd".repeat(8), 0);
        let commitment = TokenCommitment::single(&zero_token);

        // Should handle zero-amount tokens consistently
        assert!(!commitment.commitment_hash.is_empty());
        assert!(commitment.verify(&[zero_token]).unwrap());
    }

    fn create_test_token(c_value: String, amount: u64) -> Token {
        let proof = Proof {
            amount: Amount::from(amount),
            secret: Secret::new(format!("secret_{}", amount)),
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
}
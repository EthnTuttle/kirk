//! Comprehensive unit tests for commitment methods and validation

use kirk::{TokenCommitment, CommitmentMethod, CommitmentValidator, GameProtocolError};
use cdk::nuts::{Token, Proof, Id, CurrencyUnit, PublicKey as CashuPublicKey};
use cashu::secret::Secret;
use cdk::Amount;
use proptest::prelude::*;
use std::collections::HashSet;

/// Test helper to create a mock token with specific C values
fn create_mock_token_with_c_values(c_values: Vec<String>) -> Token {
    let proofs: Vec<Proof> = c_values.into_iter().enumerate().map(|(i, c)| {
        Proof {
            amount: Amount::from(1u64 << i), // Powers of 2 for amounts
            secret: Secret::new(format!("secret_{}", i)),
            c: CashuPublicKey::from_hex(&c).unwrap(),
            keyset_id: Id::from_bytes(&[0u8; 8]).unwrap(),
            witness: None,
            dleq: None,
        }
    }).collect();

    Token::new(
        "https://test-mint.example.com".parse().unwrap(),
        proofs,
        None,
        CurrencyUnit::Sat,
    )
}

/// Test helper to create mock C value (hex-encoded 32 bytes)
fn create_mock_c_value(seed: u8) -> String {
    let mut bytes = [0u8; 32];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = seed.wrapping_add(i as u8);
    }
    hex::encode(bytes)
}

#[cfg(test)]
mod single_token_commitment_tests {
    use super::*;

    #[test]
    fn test_single_token_commitment_creation() {
        let c_value = create_mock_c_value(42);
        let token = create_mock_token_with_c_values(vec![c_value.clone()]);
        
        let commitment = TokenCommitment::single(&token);
        
        // Verify commitment hash is not empty and is hex-encoded
        assert!(!commitment.commitment_hash.is_empty());
        assert!(hex::decode(&commitment.commitment_hash).is_ok());
        
        // Verify commitment type
        assert!(matches!(commitment.commitment_type, kirk::CommitmentType::Single));
    }

    #[test]
    fn test_single_token_commitment_deterministic() {
        let c_value = create_mock_c_value(123);
        let token1 = create_mock_token_with_c_values(vec![c_value.clone()]);
        let token2 = create_mock_token_with_c_values(vec![c_value]);
        
        let commitment1 = TokenCommitment::single(&token1);
        let commitment2 = TokenCommitment::single(&token2);
        
        // Same C value should produce same commitment
        assert_eq!(commitment1.commitment_hash, commitment2.commitment_hash);
    }

    #[test]
    fn test_single_token_commitment_different_c_values() {
        let token1 = create_mock_token_with_c_values(vec![create_mock_c_value(1)]);
        let token2 = create_mock_token_with_c_values(vec![create_mock_c_value(2)]);
        
        let commitment1 = TokenCommitment::single(&token1);
        let commitment2 = TokenCommitment::single(&token2);
        
        // Different C values should produce different commitments
        assert_ne!(commitment1.commitment_hash, commitment2.commitment_hash);
    }

    #[test]
    fn test_single_token_commitment_verification() {
        let token = create_mock_token_with_c_values(vec![create_mock_c_value(99)]);
        let commitment = TokenCommitment::single(&token);
        
        // Verify against the same token
        let is_valid = commitment.verify(&[token.clone()]).unwrap();
        assert!(is_valid);
        
        // Verify against different token should fail
        let different_token = create_mock_token_with_c_values(vec![create_mock_c_value(100)]);
        let is_invalid = commitment.verify(&[different_token]).unwrap();
        assert!(!is_invalid);
    }
}

#[cfg(test)]
mod multi_token_commitment_tests {
    use super::*;

    #[test]
    fn test_concatenation_commitment_creation() {
        let c_values = vec![
            create_mock_c_value(1),
            create_mock_c_value(2),
            create_mock_c_value(3),
        ];
        let tokens: Vec<Token> = c_values.iter().map(|c| {
            create_mock_token_with_c_values(vec![c.clone()])
        }).collect();
        
        let commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation);
        
        // Verify commitment structure
        assert!(!commitment.commitment_hash.is_empty());
        assert!(hex::decode(&commitment.commitment_hash).is_ok());
        
        match commitment.commitment_type {
            kirk::CommitmentType::Multiple { method } => {
                assert!(matches!(method, CommitmentMethod::Concatenation));
            },
            _ => panic!("Expected Multiple commitment type"),
        }
    }

    #[test]
    fn test_concatenation_commitment_ordering() {
        // Create tokens with C values that will be sorted differently
        let c_values = vec![
            create_mock_c_value(200), // Will sort last
            create_mock_c_value(50),  // Will sort first  
            create_mock_c_value(100), // Will sort middle
        ];
        let tokens: Vec<Token> = c_values.iter().map(|c| {
            create_mock_token_with_c_values(vec![c.clone()])
        }).collect();
        
        // Create commitment with original order
        let commitment1 = TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation);
        
        // Create commitment with different order (should be same due to sorting)
        let reordered_tokens = vec![tokens[1].clone(), tokens[2].clone(), tokens[0].clone()];
        let commitment2 = TokenCommitment::multiple(&reordered_tokens, CommitmentMethod::Concatenation);
        
        // Should be identical due to internal sorting
        assert_eq!(commitment1.commitment_hash, commitment2.commitment_hash);
    }

    #[test]
    fn test_merkle_tree_commitment_creation() {
        let c_values = vec![
            create_mock_c_value(10),
            create_mock_c_value(20),
            create_mock_c_value(30),
            create_mock_c_value(40),
        ];
        let tokens: Vec<Token> = c_values.iter().map(|c| {
            create_mock_token_with_c_values(vec![c.clone()])
        }).collect();
        
        let commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::MerkleTreeRadix4);
        
        // Verify commitment structure
        assert!(!commitment.commitment_hash.is_empty());
        assert!(hex::decode(&commitment.commitment_hash).is_ok());
        
        match commitment.commitment_type {
            kirk::CommitmentType::Multiple { method } => {
                assert!(matches!(method, CommitmentMethod::MerkleTreeRadix4));
            },
            _ => panic!("Expected Multiple commitment type"),
        }
    }

    #[test]
    fn test_merkle_tree_commitment_different_sizes() {
        // Test with different numbers of tokens
        for num_tokens in 1..=10 {
            let c_values: Vec<String> = (0..num_tokens)
                .map(|i| create_mock_c_value(i as u8))
                .collect();
            let tokens: Vec<Token> = c_values.iter().map(|c| {
                create_mock_token_with_c_values(vec![c.clone()])
            }).collect();
            
            let commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::MerkleTreeRadix4);
            
            // Should create valid commitment regardless of size
            assert!(!commitment.commitment_hash.is_empty());
            assert!(hex::decode(&commitment.commitment_hash).is_ok());
        }
    }

    #[test]
    fn test_multi_token_commitment_verification() {
        let tokens = vec![
            create_mock_token_with_c_values(vec![create_mock_c_value(1)]),
            create_mock_token_with_c_values(vec![create_mock_c_value(2)]),
            create_mock_token_with_c_values(vec![create_mock_c_value(3)]),
        ];
        
        // Test concatenation method
        let concat_commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation);
        assert!(concat_commitment.verify(&tokens).unwrap());
        
        // Test merkle tree method
        let merkle_commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::MerkleTreeRadix4);
        assert!(merkle_commitment.verify(&tokens).unwrap());
        
        // Test with wrong tokens
        let wrong_tokens = vec![
            create_mock_token_with_c_values(vec![create_mock_c_value(4)]),
            create_mock_token_with_c_values(vec![create_mock_c_value(5)]),
        ];
        
        assert!(!concat_commitment.verify(&wrong_tokens).unwrap());
        assert!(!merkle_commitment.verify(&wrong_tokens).unwrap());
    }
}

#[cfg(test)]
mod commitment_security_tests {
    use super::*;

    #[test]
    fn test_commitment_collision_resistance() {
        // Generate many different tokens and ensure no hash collisions
        let mut commitment_hashes = HashSet::new();
        
        for i in 0..1000 {
            let token = create_mock_token_with_c_values(vec![create_mock_c_value(i as u8)]);
            let commitment = TokenCommitment::single(&token);
            
            // Should not have seen this hash before
            assert!(commitment_hashes.insert(commitment.commitment_hash.clone()),
                   "Hash collision detected for token {}", i);
        }
    }

    #[test]
    fn test_commitment_avalanche_effect() {
        // Small changes in input should cause large changes in output
        let base_c_value = create_mock_c_value(100);
        let mut modified_c_value = base_c_value.clone();
        
        // Flip one bit in the hex string
        let mut chars: Vec<char> = modified_c_value.chars().collect();
        chars[0] = if chars[0] == '0' { '1' } else { '0' };
        modified_c_value = chars.into_iter().collect();
        
        let token1 = create_mock_token_with_c_values(vec![base_c_value]);
        let token2 = create_mock_token_with_c_values(vec![modified_c_value]);
        
        let commitment1 = TokenCommitment::single(&token1);
        let commitment2 = TokenCommitment::single(&token2);
        
        // Should be completely different
        assert_ne!(commitment1.commitment_hash, commitment2.commitment_hash);
        
        // Calculate Hamming distance in hex representation
        let hash1_bytes = hex::decode(&commitment1.commitment_hash).unwrap();
        let hash2_bytes = hex::decode(&commitment2.commitment_hash).unwrap();
        
        let mut different_bits = 0;
        for (b1, b2) in hash1_bytes.iter().zip(hash2_bytes.iter()) {
            different_bits += (b1 ^ b2).count_ones();
        }
        
        // Should have changed many bits (avalanche effect)
        assert!(different_bits > 50, "Insufficient avalanche effect: {} bits changed", different_bits);
    }
}

// Property-based tests using proptest
proptest! {
    #[test]
    fn prop_commitment_deterministic(c_values in prop::collection::vec(prop::num::u8::ANY, 1..10)) {
        let c_value_strings: Vec<String> = c_values.iter().map(|&v| create_mock_c_value(v)).collect();
        let tokens: Vec<Token> = c_value_strings.iter().map(|c| {
            create_mock_token_with_c_values(vec![c.clone()])
        }).collect();
        
        // Same tokens should always produce same commitment
        let commitment1 = if tokens.len() == 1 {
            TokenCommitment::single(&tokens[0])
        } else {
            TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation)
        };
        
        let commitment2 = if tokens.len() == 1 {
            TokenCommitment::single(&tokens[0])
        } else {
            TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation)
        };
        
        prop_assert_eq!(commitment1.commitment_hash, commitment2.commitment_hash);
    }

    #[test]
    fn prop_commitment_verification_roundtrip(c_values in prop::collection::vec(prop::num::u8::ANY, 1..5)) {
        let c_value_strings: Vec<String> = c_values.iter().map(|&v| create_mock_c_value(v)).collect();
        let tokens: Vec<Token> = c_value_strings.iter().map(|c| {
            create_mock_token_with_c_values(vec![c.clone()])
        }).collect();
        
        // Test both commitment methods for multi-token
        let methods = if tokens.len() == 1 {
            vec![] // Single token doesn't use methods
        } else {
            vec![CommitmentMethod::Concatenation, CommitmentMethod::MerkleTreeRadix4]
        };
        
        if tokens.len() == 1 {
            let commitment = TokenCommitment::single(&tokens[0]);
            prop_assert!(commitment.verify(&tokens).unwrap());
        } else {
            for method in methods {
                let commitment = TokenCommitment::multiple(&tokens, method);
                prop_assert!(commitment.verify(&tokens).unwrap());
            }
        }
    }

    #[test]
    fn prop_commitment_ordering_independence(
        c_values in prop::collection::vec(prop::num::u8::ANY, 2..5)
    ) {
        let c_value_strings: Vec<String> = c_values.iter().map(|&v| create_mock_c_value(v)).collect();
        let mut tokens: Vec<Token> = c_value_strings.iter().map(|c| {
            create_mock_token_with_c_values(vec![c.clone()])
        }).collect();
        
        let original_commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation);
        
        // Shuffle the tokens
        tokens.reverse();
        let shuffled_commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation);
        
        // Should be the same due to internal sorting
        prop_assert_eq!(original_commitment.commitment_hash, shuffled_commitment.commitment_hash);
    }
}

#[cfg(test)]
mod commitment_validator_tests {
    use super::*;
    use kirk::CommitmentValidator;

    struct TestCommitmentValidator;

    impl CommitmentValidator for TestCommitmentValidator {
        fn validate_single_commitment(
            &self,
            commitment_hash: &str,
            revealed_token: &Token,
        ) -> Result<bool, kirk::ValidationError> {
            let computed_commitment = TokenCommitment::single(revealed_token);
            Ok(computed_commitment.commitment_hash == commitment_hash)
        }

        fn validate_multi_commitment(
            &self,
            commitment_hash: &str,
            revealed_tokens: &[Token],
            method: &CommitmentMethod,
        ) -> Result<bool, kirk::ValidationError> {
            let computed_commitment = TokenCommitment::multiple(revealed_tokens, method.clone());
            Ok(computed_commitment.commitment_hash == commitment_hash)
        }
    }

    #[test]
    fn test_commitment_validator_single_token() {
        let validator = TestCommitmentValidator;
        let token = create_mock_token_with_c_values(vec![create_mock_c_value(42)]);
        let commitment = TokenCommitment::single(&token);
        
        // Valid case
        let is_valid = validator.validate_single_commitment(
            &commitment.commitment_hash,
            &token
        ).unwrap();
        assert!(is_valid);
        
        // Invalid case
        let wrong_token = create_mock_token_with_c_values(vec![create_mock_c_value(43)]);
        let is_invalid = validator.validate_single_commitment(
            &commitment.commitment_hash,
            &wrong_token
        ).unwrap();
        assert!(!is_invalid);
    }

    #[test]
    fn test_commitment_validator_multi_token() {
        let validator = TestCommitmentValidator;
        let tokens = vec![
            create_mock_token_with_c_values(vec![create_mock_c_value(1)]),
            create_mock_token_with_c_values(vec![create_mock_c_value(2)]),
        ];
        
        let commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation);
        
        // Valid case
        let is_valid = validator.validate_multi_commitment(
            &commitment.commitment_hash,
            &tokens,
            &CommitmentMethod::Concatenation
        ).unwrap();
        assert!(is_valid);
        
        // Invalid case - wrong method
        let is_invalid = validator.validate_multi_commitment(
            &commitment.commitment_hash,
            &tokens,
            &CommitmentMethod::MerkleTreeRadix4
        ).unwrap();
        assert!(!is_invalid);
    }
}
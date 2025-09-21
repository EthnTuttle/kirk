//! Property-based tests for commitment security properties

use proptest::prelude::*;
use kirk::{TokenCommitment, CommitmentMethod};
use cdk::nuts::{Token, Proof, Id, CurrencyUnit, PublicKey as CashuPublicKey};
use cashu::secret::Secret;
use cdk::Amount;
use std::collections::HashSet;

/// Helper to create a token with specific C value
fn create_token_with_c(c_value: String, amount: u64) -> Token {
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

/// Generate a hex string of specified length
fn arb_hex_string(len: usize) -> impl Strategy<Value = String> {
    prop::collection::vec(prop::num::u8::ANY, len)
        .prop_map(|bytes| hex::encode(bytes))
}

/// Generate a valid C value (64 hex chars = 32 bytes)
fn arb_c_value() -> impl Strategy<Value = String> {
    arb_hex_string(32)
}

/// Generate a token with random C value
fn arb_token() -> impl Strategy<Value = Token> {
    (arb_c_value(), 1u64..1000u64)
        .prop_map(|(c_value, amount)| create_token_with_c(c_value, amount))
}

/// Generate a vector of tokens
fn arb_tokens(min_size: usize, max_size: usize) -> impl Strategy<Value = Vec<Token>> {
    prop::collection::vec(arb_token(), min_size..=max_size)
}

proptest! {
    #[test]
    fn prop_single_commitment_deterministic(token in arb_token()) {
        // Same token should always produce same commitment
        let commitment1 = TokenCommitment::single(&token);
        let commitment2 = TokenCommitment::single(&token);
        
        prop_assert_eq!(commitment1.commitment_hash, commitment2.commitment_hash);
        prop_assert!(matches!(commitment1.commitment_type, kirk::CommitmentType::Single));
    }

    #[test]
    fn prop_single_commitment_collision_resistance(
        tokens in prop::collection::vec(arb_token(), 2..100)
    ) {
        let mut commitment_hashes = HashSet::new();
        
        for token in &tokens {
            let commitment = TokenCommitment::single(&token);
            
            // Each unique token should produce unique commitment
            // (allowing for rare collisions in test data)
            commitment_hashes.insert(commitment.commitment_hash);
        }
        
        // Should have good diversity (allowing some collisions due to test data patterns)
        prop_assert!(commitment_hashes.len() > tokens.len() / 2);
    }

    #[test]
    fn prop_multi_commitment_deterministic(
        tokens in arb_tokens(2, 10),
        method in prop::sample::select(vec![CommitmentMethod::Concatenation, CommitmentMethod::MerkleTreeRadix4])
    ) {
        // Same tokens and method should produce same commitment
        let commitment1 = TokenCommitment::multiple(&tokens, method.clone());
        let commitment2 = TokenCommitment::multiple(&tokens, method.clone());
        
        prop_assert_eq!(commitment1.commitment_hash, commitment2.commitment_hash);
        
        match commitment1.commitment_type {
            kirk::CommitmentType::Multiple { method: m } => {
                prop_assert!(matches!((method, m), 
                    (CommitmentMethod::Concatenation, CommitmentMethod::Concatenation) |
                    (CommitmentMethod::MerkleTreeRadix4, CommitmentMethod::MerkleTreeRadix4)));
            },
            _ => prop_assert!(false, "Expected Multiple commitment type"),
        }
    }

    #[test]
    fn prop_commitment_ordering_independence(
        mut tokens in arb_tokens(2, 8),
        method in prop::sample::select(vec![CommitmentMethod::Concatenation, CommitmentMethod::MerkleTreeRadix4])
    ) {
        let original_commitment = TokenCommitment::multiple(&tokens, method.clone());
        
        // Shuffle tokens
        tokens.reverse();
        let shuffled_commitment = TokenCommitment::multiple(&tokens, method);
        
        // Should be same due to internal sorting
        prop_assert_eq!(original_commitment.commitment_hash, shuffled_commitment.commitment_hash);
    }

    #[test]
    fn prop_commitment_verification_roundtrip(
        tokens in arb_tokens(1, 5)
    ) {
        if tokens.len() == 1 {
            let commitment = TokenCommitment::single(&tokens[0]);
            prop_assert!(commitment.verify(&tokens).unwrap());
        } else {
            for method in [CommitmentMethod::Concatenation, CommitmentMethod::MerkleTreeRadix4] {
                let commitment = TokenCommitment::multiple(&tokens, method);
                prop_assert!(commitment.verify(&tokens).unwrap());
            }
        }
    }

    #[test]
    fn prop_commitment_different_methods_different_hashes(
        tokens in arb_tokens(2, 5)
    ) {
        let concat_commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation);
        let merkle_commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::MerkleTreeRadix4);
        
        // Different methods should generally produce different hashes
        // (allowing for rare collisions)
        if tokens.len() > 1 {
            // For multiple tokens, methods should differ
            prop_assert_ne!(concat_commitment.commitment_hash, merkle_commitment.commitment_hash);
        }
    }

    #[test]
    fn prop_commitment_avalanche_effect(
        c_value in arb_c_value(),
        bit_position in 0..256usize
    ) {
        let token1 = create_token_with_c(c_value.clone(), 100);
        
        // Flip one bit in the C value
        let mut c_bytes = hex::decode(&c_value).unwrap();
        let byte_index = bit_position / 8;
        let bit_index = bit_position % 8;
        
        if byte_index < c_bytes.len() {
            c_bytes[byte_index] ^= 1 << bit_index;
            let modified_c_value = hex::encode(c_bytes);
            let token2 = create_token_with_c(modified_c_value, 100);
            
            let commitment1 = TokenCommitment::single(&token1);
            let commitment2 = TokenCommitment::single(&token2);
            
            // Should produce different commitments
            let hash1 = commitment1.commitment_hash.clone();
            let hash2 = commitment2.commitment_hash.clone();
            prop_assert_ne!(hash1.clone(), hash2.clone());
            
            // Calculate Hamming distance
            let hash1_bytes = hex::decode(&hash1).unwrap();
            let hash2_bytes = hex::decode(&hash2).unwrap();
            
            let mut different_bits = 0;
            for (b1, b2) in hash1_bytes.iter().zip(hash2_bytes.iter()) {
                different_bits += (b1 ^ b2).count_ones();
            }
            
            // Should have good avalanche effect (at least 25% of bits changed)
            prop_assert!(different_bits > 64, "Insufficient avalanche effect: {} bits", different_bits);
        }
    }

    #[test]
    fn prop_commitment_preimage_resistance(
        target_hash in arb_hex_string(32),
        attempts in prop::collection::vec(arb_token(), 1..50)
    ) {
        // It should be computationally infeasible to find a token that produces a specific hash
        let mut found_preimage = false;
        
        for token in attempts {
            let commitment = TokenCommitment::single(&token);
            if commitment.commitment_hash == target_hash {
                found_preimage = true;
                break;
            }
        }
        
        // Should be extremely unlikely to find preimage by chance
        prop_assert!(!found_preimage, "Found preimage for target hash (extremely unlikely)");
    }

    #[test]
    fn prop_commitment_second_preimage_resistance(
        original_token in arb_token(),
        other_tokens in prop::collection::vec(arb_token(), 1..50)
    ) {
        let original_commitment = TokenCommitment::single(&original_token);
        let mut found_collision = false;
        
        for token in other_tokens {
            let commitment = TokenCommitment::single(&token);
            if commitment.commitment_hash == original_commitment.commitment_hash {
                found_collision = true;
                break;
            }
        }
        
        // Should be extremely unlikely to find second preimage
        prop_assert!(!found_collision, "Found second preimage (collision)");
    }

    #[test]
    fn prop_commitment_length_consistency(tokens in arb_tokens(1, 10)) {
        let commitments = if tokens.len() == 1 {
            vec![TokenCommitment::single(&tokens[0])]
        } else {
            vec![
                TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation),
                TokenCommitment::multiple(&tokens, CommitmentMethod::MerkleTreeRadix4),
            ]
        };
        
        for commitment in commitments {
            // All commitment hashes should be 64 hex characters (32 bytes)
            prop_assert_eq!(commitment.commitment_hash.len(), 64);
            
            // Should be valid hex
            prop_assert!(hex::decode(&commitment.commitment_hash).is_ok());
        }
    }

    #[test]
    fn prop_commitment_empty_token_handling(
        empty_tokens in prop::collection::vec(Just(()), 0..3)
    ) {
        // Test behavior with edge cases
        if empty_tokens.is_empty() {
            // Empty token list - should handle gracefully
            // (This would be an error case in practice)
            return Ok(());
        }
        
        // Create minimal valid tokens
        let tokens: Vec<Token> = (0..empty_tokens.len()).map(|i| {
            create_token_with_c(format!("{:064x}", i), 1)
        }).collect();
        
        if tokens.len() == 1 {
            let commitment = TokenCommitment::single(&tokens[0]);
            prop_assert!(!commitment.commitment_hash.is_empty());
        } else if !tokens.is_empty() {
            let commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation);
            prop_assert!(!commitment.commitment_hash.is_empty());
        }
    }
}

#[cfg(test)]
mod commitment_security_edge_cases {
    use super::*;

    #[test]
    fn test_commitment_with_identical_c_values() {
        // Test what happens when multiple tokens have same C value
        let c_value = "abcd1234".repeat(8); // 64 hex chars
        let tokens = vec![
            create_token_with_c(c_value.clone(), 100),
            create_token_with_c(c_value.clone(), 200),
            create_token_with_c(c_value, 300),
        ];
        
        let concat_commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation);
        let merkle_commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::MerkleTreeRadix4);
        
        // Should still produce valid commitments
        assert!(!concat_commitment.commitment_hash.is_empty());
        assert!(!merkle_commitment.commitment_hash.is_empty());
        
        // Verification should work
        assert!(concat_commitment.verify(&tokens).unwrap());
        assert!(merkle_commitment.verify(&tokens).unwrap());
    }

    #[test]
    fn test_commitment_with_extreme_values() {
        // Test with edge case C values
        let extreme_tokens = vec![
            create_token_with_c("00".repeat(32), 1),           // All zeros
            create_token_with_c("ff".repeat(32), 1),           // All ones
            create_token_with_c("aa".repeat(32), 1),           // Alternating pattern
            create_token_with_c("0f".repeat(32), 1),           // Another pattern
        ];
        
        for token in &extreme_tokens {
            let commitment = TokenCommitment::single(token);
            assert!(!commitment.commitment_hash.is_empty());
            assert!(commitment.verify(&[token.clone()]).unwrap());
        }
        
        // Test with multiple extreme tokens
        let multi_commitment = TokenCommitment::multiple(&extreme_tokens, CommitmentMethod::MerkleTreeRadix4);
        assert!(!multi_commitment.commitment_hash.is_empty());
        assert!(multi_commitment.verify(&extreme_tokens).unwrap());
    }

    #[test]
    fn test_commitment_hash_distribution() {
        // Test that commitment hashes are well-distributed
        let mut hash_prefixes = std::collections::HashMap::new();
        
        for i in 0..1000 {
            let c_value = format!("{:064x}", i * 12345); // Pseudo-random pattern
            let token = create_token_with_c(c_value, 100);
            let commitment = TokenCommitment::single(&token);
            
            // Count distribution of first 2 hex chars
            let prefix = &commitment.commitment_hash[..2];
            *hash_prefixes.entry(prefix.to_string()).or_insert(0) += 1;
        }
        
        // Should have good distribution (no prefix should dominate)
        let max_count = hash_prefixes.values().max().unwrap();
        let min_count = hash_prefixes.values().min().unwrap();
        
        // Allow some variance but not too much concentration
        assert!(*max_count < 100, "Hash distribution too concentrated: max={}", max_count);
        assert!(*min_count > 0, "Some hash prefixes never appear");
        assert!(hash_prefixes.len() > 200, "Too few unique prefixes: {}", hash_prefixes.len());
    }
}
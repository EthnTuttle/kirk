//! Hash commitment construction utilities

use sha2::{Sha256, Digest};
use hex;
use cdk::nuts::Token as CashuToken;
use crate::events::CommitmentMethod;
use crate::error::ValidationError;

/// Token commitment with hash and type information
#[derive(Debug, Clone)]
pub struct TokenCommitment {
    pub commitment_hash: String,
    pub commitment_type: CommitmentType,
}

/// Type of commitment construction used
#[derive(Debug, Clone, PartialEq)]
pub enum CommitmentType {
    Single,
    Multiple { method: CommitmentMethod },
}

impl TokenCommitment {
    /// Create commitment for single token
    pub fn single(token: &CashuToken) -> Self {
        let token_hash = Self::hash_token(token);
        let commitment_hash = hex::encode(Sha256::digest(&token_hash));
        Self {
            commitment_hash,
            commitment_type: CommitmentType::Single,
        }
    }
    
    /// Create commitment for multiple tokens using specified method
    pub fn multiple(tokens: &[CashuToken], method: CommitmentMethod) -> Self {
        let mut sorted_tokens = tokens.to_vec();
        sorted_tokens.sort_by_key(|t| Self::hash_token(t));
        
        let commitment_hash = match method {
            CommitmentMethod::Single => {
                if sorted_tokens.len() == 1 {
                    hex::encode(Sha256::digest(&Self::hash_token(&sorted_tokens[0])))
                } else {
                    // For multiple tokens, fall back to concatenation
                    Self::concatenation_commitment(&sorted_tokens)
                }
            },
            CommitmentMethod::Concatenation => Self::concatenation_commitment(&sorted_tokens),
            CommitmentMethod::MerkleTreeRadix4 => Self::merkle_tree_radix4_commitment(&sorted_tokens),
        };
        
        Self {
            commitment_hash,
            commitment_type: CommitmentType::Multiple { method },
        }
    }
    
    /// Verify commitment against revealed tokens
    pub fn verify(&self, tokens: &[CashuToken]) -> Result<bool, ValidationError> {
        match &self.commitment_type {
            CommitmentType::Single => {
                if tokens.len() != 1 {
                    return Ok(false);
                }
                let expected = Self::single(&tokens[0]);
                Ok(expected.commitment_hash == self.commitment_hash)
            }
            CommitmentType::Multiple { method } => {
                let expected = Self::multiple(tokens, method.clone());
                Ok(expected.commitment_hash == self.commitment_hash)
            }
        }
    }
    
    /// Standard token hashing function
    /// Uses the token's serialized JSON representation for consistent hashing
    fn hash_token(token: &CashuToken) -> [u8; 32] {
        // Serialize token to JSON for consistent hashing
        let token_json = serde_json::to_string(token)
            .unwrap_or_else(|_| format!("{:?}", token));
        
        let mut hasher = Sha256::new();
        hasher.update(token_json.as_bytes());
        hasher.finalize().into()
    }
    
    /// Concatenation commitment construction
    fn concatenation_commitment(sorted_tokens: &[CashuToken]) -> String {
        let mut concatenated = Vec::new();
        
        for token in sorted_tokens {
            let token_hash = Self::hash_token(token);
            concatenated.extend_from_slice(&token_hash);
        }
        
        hex::encode(Sha256::digest(&concatenated))
    }
    
    /// Merkle tree radix 4 commitment construction
    fn merkle_tree_radix4_commitment(sorted_tokens: &[CashuToken]) -> String {
        let token_hashes: Vec<[u8; 32]> = sorted_tokens.iter()
            .map(|t| Self::hash_token(t))
            .collect();
        
        let merkle_root = Self::build_merkle_tree_radix4(&token_hashes);
        hex::encode(merkle_root)
    }
    
    /// Build merkle tree with radix 4 from sorted token hashes
    /// This creates a proper tree structure, not just concatenation
    fn build_merkle_tree_radix4(token_hashes: &[[u8; 32]]) -> [u8; 32] {
        if token_hashes.is_empty() {
            return [0u8; 32]; // Empty tree root
        }
        
        if token_hashes.len() == 1 {
            return token_hashes[0]; // Single leaf is the root
        }
        
        let mut current_level = token_hashes.to_vec();
        
        while current_level.len() > 1 {
            let mut next_level = Vec::new();
            
            // Process nodes in groups of up to 4, but ensure we build a tree
            // For radix 4, we want to group in 4s, but if we have exactly 4 items
            // at the root level, we should still create intermediate nodes
            for chunk in current_level.chunks(4) {
                if chunk.len() == 1 {
                    // Single node passes through unchanged
                    next_level.push(chunk[0]);
                } else {
                    // Create parent node from 2-4 children
                    let mut node_data = Vec::new();
                    
                    // Add a prefix to distinguish merkle tree from concatenation
                    node_data.extend_from_slice(b"MERKLE_NODE:");
                    
                    // Concatenate child hashes
                    for hash in chunk {
                        node_data.extend_from_slice(hash);
                    }
                    
                    // Hash to create parent node
                    let parent_hash: [u8; 32] = Sha256::digest(&node_data).into();
                    next_level.push(parent_hash);
                }
            }
            
            current_level = next_level;
        }
        
        current_level[0] // Return merkle root
    }
}

// Additional utility functions for commitment construction
impl TokenCommitment {
    /// Create hash commitment for a single token (convenience function)
    pub fn hash_single_token(token: &CashuToken) -> String {
        let commitment = Self::single(token);
        commitment.commitment_hash
    }
    
    /// Create hash commitment for multiple tokens using concatenation
    pub fn hash_concatenation(tokens: &[CashuToken]) -> String {
        let commitment = Self::multiple(tokens, CommitmentMethod::Concatenation);
        commitment.commitment_hash
    }
    
    /// Create hash commitment for multiple tokens using merkle tree radix 4
    pub fn hash_merkle_tree_radix4(tokens: &[CashuToken]) -> String {
        let commitment = Self::multiple(tokens, CommitmentMethod::MerkleTreeRadix4);
        commitment.commitment_hash
    }
    
    /// Verify a commitment hash against revealed tokens with specified method
    pub fn verify_commitment_hash(
        commitment_hash: &str,
        tokens: &[CashuToken],
        method: Option<CommitmentMethod>
    ) -> Result<bool, ValidationError> {
        let commitment = match (tokens.len(), method) {
            (1, None) => Self::single(&tokens[0]),
            (_, Some(method)) => Self::multiple(tokens, method),
            (_, None) => {
                return Err(ValidationError::new(
                    nostr::EventId::from_hex("0".repeat(64)).unwrap(),
                    crate::error::ValidationErrorType::InvalidCommitment,
                    "Multiple tokens require commitment method specification".to_string(),
                ));
            }
        };
        
        Ok(commitment.commitment_hash == commitment_hash)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::CommitmentMethod;
    use cdk::nuts::Token as CashuToken;

    /// Create a test token for commitment testing
    /// Since we're testing commitment algorithms, not CDK integration,
    /// we'll create a simple mock token that serializes deterministically
    fn create_test_token(seed: u8) -> CashuToken {
        // Create a simple JSON structure that can be parsed as a CDK Token
        // Make each token significantly different to ensure proper testing
        let unique_id = (seed as u32).wrapping_mul(31).wrapping_add(17).wrapping_rem(1000);
        let token_json = format!(
            r#"{{"token":[{{"mint":"https://mint{}.example.com","proofs":[]}}],"memo":"test_token_seed_{}_unique_{}","unit":"sat"}}"#,
            seed, seed, unique_id
        );
        
        // Parse as CDK token - this should work with the basic structure
        serde_json::from_str(&token_json)
            .unwrap_or_else(|e| {
                // If CDK structure is different, create a deterministic fallback
                // that will still allow us to test commitment algorithms
                panic!("Failed to create test token for seed {}: {}. CDK Token structure may have changed.", seed, e);
            })
    }

    /// Create multiple test tokens
    fn create_test_tokens(count: usize) -> Vec<CashuToken> {
        (0..count).map(|i| create_test_token(i as u8)).collect()
    }

    #[test]
    fn test_single_token_commitment() {
        let token = create_test_token(1);
        let commitment = TokenCommitment::single(&token);
        
        // Verify commitment structure
        assert!(!commitment.commitment_hash.is_empty());
        assert_eq!(commitment.commitment_hash.len(), 64); // SHA256 hex = 64 chars
        assert!(matches!(commitment.commitment_type, CommitmentType::Single));
        
        // Verify hex format
        assert!(hex::decode(&commitment.commitment_hash).is_ok());
    }

    #[test]
    fn test_single_token_commitment_deterministic() {
        let token1 = create_test_token(1);
        let token2 = create_test_token(1); // Same seed = same token
        
        let commitment1 = TokenCommitment::single(&token1);
        let commitment2 = TokenCommitment::single(&token2);
        
        // Same token should produce same commitment
        assert_eq!(commitment1.commitment_hash, commitment2.commitment_hash);
    }

    #[test]
    fn test_single_token_commitment_different_tokens() {
        let token1 = create_test_token(1);
        let token2 = create_test_token(2);
        
        let commitment1 = TokenCommitment::single(&token1);
        let commitment2 = TokenCommitment::single(&token2);
        
        // Different tokens should produce different commitments
        assert_ne!(commitment1.commitment_hash, commitment2.commitment_hash);
    }

    #[test]
    fn test_multiple_token_concatenation_commitment() {
        let tokens = create_test_tokens(3);
        let commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation);
        
        // Verify commitment structure
        assert!(!commitment.commitment_hash.is_empty());
        assert_eq!(commitment.commitment_hash.len(), 64);
        assert!(matches!(
            commitment.commitment_type, 
            CommitmentType::Multiple { method: CommitmentMethod::Concatenation }
        ));
    }

    #[test]
    fn test_multiple_token_merkle_commitment() {
        let tokens = create_test_tokens(4);
        let commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::MerkleTreeRadix4);
        
        // Verify commitment structure
        assert!(!commitment.commitment_hash.is_empty());
        assert_eq!(commitment.commitment_hash.len(), 64);
        assert!(matches!(
            commitment.commitment_type, 
            CommitmentType::Multiple { method: CommitmentMethod::MerkleTreeRadix4 }
        ));
    }

    #[test]
    fn test_concatenation_commitment_deterministic() {
        let tokens1 = create_test_tokens(3);
        let tokens2 = create_test_tokens(3); // Same tokens
        
        let commitment1 = TokenCommitment::multiple(&tokens1, CommitmentMethod::Concatenation);
        let commitment2 = TokenCommitment::multiple(&tokens2, CommitmentMethod::Concatenation);
        
        // Same tokens should produce same commitment
        assert_eq!(commitment1.commitment_hash, commitment2.commitment_hash);
    }

    #[test]
    fn test_merkle_commitment_deterministic() {
        let tokens1 = create_test_tokens(4);
        let tokens2 = create_test_tokens(4); // Same tokens
        
        let commitment1 = TokenCommitment::multiple(&tokens1, CommitmentMethod::MerkleTreeRadix4);
        let commitment2 = TokenCommitment::multiple(&tokens2, CommitmentMethod::MerkleTreeRadix4);
        
        // Same tokens should produce same commitment
        assert_eq!(commitment1.commitment_hash, commitment2.commitment_hash);
    }

    #[test]
    fn test_concatenation_vs_merkle_different() {
        let tokens = create_test_tokens(4);
        
        let concat_commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation);
        let merkle_commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::MerkleTreeRadix4);
        
        // Different methods should produce different commitments
        assert_ne!(concat_commitment.commitment_hash, merkle_commitment.commitment_hash);
    }

    #[test]
    fn test_token_ordering_consistency() {
        // Create tokens in different orders
        let tokens1 = create_test_tokens(3);
        let mut tokens2 = tokens1.clone();
        tokens2.reverse(); // Reverse order
        
        let commitment1 = TokenCommitment::multiple(&tokens1, CommitmentMethod::Concatenation);
        let commitment2 = TokenCommitment::multiple(&tokens2, CommitmentMethod::Concatenation);
        
        // Should produce same commitment regardless of input order (due to internal sorting)
        assert_eq!(commitment1.commitment_hash, commitment2.commitment_hash);
    }

    #[test]
    fn test_merkle_tree_ordering_consistency() {
        let tokens1 = create_test_tokens(4);
        let mut tokens2 = tokens1.clone();
        tokens2.reverse();
        
        let commitment1 = TokenCommitment::multiple(&tokens1, CommitmentMethod::MerkleTreeRadix4);
        let commitment2 = TokenCommitment::multiple(&tokens2, CommitmentMethod::MerkleTreeRadix4);
        
        // Should produce same commitment regardless of input order
        assert_eq!(commitment1.commitment_hash, commitment2.commitment_hash);
    }

    #[test]
    fn test_single_token_verification() {
        let token = create_test_token(1);
        let commitment = TokenCommitment::single(&token);
        
        // Verify with correct token
        assert!(commitment.verify(&[token.clone()]).unwrap());
        
        // Verify with wrong token
        let wrong_token = create_test_token(2);
        assert!(!commitment.verify(&[wrong_token]).unwrap());
        
        // Verify with multiple tokens (should fail for single commitment)
        let multiple_tokens = create_test_tokens(2);
        assert!(!commitment.verify(&multiple_tokens).unwrap());
    }

    #[test]
    fn test_multiple_token_concatenation_verification() {
        let tokens = create_test_tokens(3);
        let commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation);
        
        // Verify with correct tokens
        assert!(commitment.verify(&tokens).unwrap());
        
        // Verify with wrong tokens
        let wrong_tokens = create_test_tokens(2);
        assert!(!commitment.verify(&wrong_tokens).unwrap());
        
        // Verify with tokens in different order (should still work due to sorting)
        let mut reordered_tokens = tokens.clone();
        reordered_tokens.reverse();
        assert!(commitment.verify(&reordered_tokens).unwrap());
    }

    #[test]
    fn test_multiple_token_merkle_verification() {
        let tokens = create_test_tokens(4);
        let commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::MerkleTreeRadix4);
        
        // Verify with correct tokens
        assert!(commitment.verify(&tokens).unwrap());
        
        // Verify with wrong tokens
        let wrong_tokens = create_test_tokens(3);
        assert!(!commitment.verify(&wrong_tokens).unwrap());
        
        // Verify with tokens in different order
        let mut reordered_tokens = tokens.clone();
        reordered_tokens.reverse();
        assert!(commitment.verify(&reordered_tokens).unwrap());
    }

    #[test]
    fn test_merkle_tree_edge_cases() {
        // Test empty tokens
        let merkle_root = TokenCommitment::build_merkle_tree_radix4(&[]);
        assert_eq!(merkle_root, [0u8; 32]);
        
        // Test single token
        let single_hash = [1u8; 32];
        let merkle_root = TokenCommitment::build_merkle_tree_radix4(&[single_hash]);
        assert_eq!(merkle_root, single_hash);
        
        // Test two tokens
        let two_hashes = [[1u8; 32], [2u8; 32]];
        let merkle_root = TokenCommitment::build_merkle_tree_radix4(&two_hashes);
        assert_ne!(merkle_root, [0u8; 32]);
        assert_ne!(merkle_root, [1u8; 32]);
        assert_ne!(merkle_root, [2u8; 32]);
    }

    #[test]
    fn test_merkle_tree_radix4_properties() {
        // Test that merkle tree handles various sizes correctly
        for size in 1..=17 {
            let hashes: Vec<[u8; 32]> = (0..size).map(|i| {
                let mut hash = [0u8; 32];
                hash[0] = (i + 1) as u8; // Start from 1 to avoid all-zero hash
                hash
            }).collect();
            
            let merkle_root = TokenCommitment::build_merkle_tree_radix4(&hashes);
            
            // Root should not be all zeros (unless input was empty)
            if !hashes.is_empty() {
                // For single element, root equals the element
                if hashes.len() == 1 {
                    assert_eq!(merkle_root, hashes[0]);
                } else {
                    // For multiple elements, root should be different from any input
                    assert!(!hashes.contains(&merkle_root));
                }
            }
            
            // Root should be deterministic
            let merkle_root2 = TokenCommitment::build_merkle_tree_radix4(&hashes);
            assert_eq!(merkle_root, merkle_root2);
        }
    }

    #[test]
    fn test_concatenation_commitment_properties() {
        // Test various sizes
        for size in 1..=10 {
            let tokens = create_test_tokens(size);
            let commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation);
            
            // Should be valid hex
            assert!(hex::decode(&commitment.commitment_hash).is_ok());
            
            // Should be 64 characters (SHA256 hex)
            assert_eq!(commitment.commitment_hash.len(), 64);
            
            // Should verify correctly
            assert!(commitment.verify(&tokens).unwrap());
        }
    }

    #[test]
    fn test_hash_token_consistency() {
        let token = create_test_token(1);
        
        // Hash same token multiple times
        let hash1 = TokenCommitment::hash_token(&token);
        let hash2 = TokenCommitment::hash_token(&token);
        let hash3 = TokenCommitment::hash_token(&token);
        
        // Should always produce same hash
        assert_eq!(hash1, hash2);
        assert_eq!(hash2, hash3);
    }

    #[test]
    fn test_hash_token_different_tokens() {
        let token1 = create_test_token(1);
        let token2 = create_test_token(2);
        
        let hash1 = TokenCommitment::hash_token(&token1);
        let hash2 = TokenCommitment::hash_token(&token2);
        
        // Different tokens should produce different hashes
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_commitment_type_matching() {
        let token = create_test_token(1);
        let tokens = create_test_tokens(3);
        
        let single_commitment = TokenCommitment::single(&token);
        let concat_commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation);
        let merkle_commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::MerkleTreeRadix4);
        
        // Verify type matching
        assert!(matches!(single_commitment.commitment_type, CommitmentType::Single));
        assert!(matches!(
            concat_commitment.commitment_type, 
            CommitmentType::Multiple { method: CommitmentMethod::Concatenation }
        ));
        assert!(matches!(
            merkle_commitment.commitment_type, 
            CommitmentType::Multiple { method: CommitmentMethod::MerkleTreeRadix4 }
        ));
    }

    #[test]
    fn test_verification_with_wrong_method() {
        let tokens = create_test_tokens(3);
        
        // Create commitment with concatenation
        let mut commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation);
        
        // Change the method to merkle (simulating wrong method in verification)
        commitment.commitment_type = CommitmentType::Multiple { 
            method: CommitmentMethod::MerkleTreeRadix4 
        };
        
        // Should fail verification because method doesn't match original
        assert!(!commitment.verify(&tokens).unwrap());
    }

    #[test]
    fn test_utility_functions() {
        let token = create_test_token(1);
        let tokens = create_test_tokens(3);
        
        // Test single token hash utility
        let single_hash = TokenCommitment::hash_single_token(&token);
        assert_eq!(single_hash.len(), 64);
        assert!(hex::decode(&single_hash).is_ok());
        
        // Test concatenation utility
        let concat_hash = TokenCommitment::hash_concatenation(&tokens);
        assert_eq!(concat_hash.len(), 64);
        assert!(hex::decode(&concat_hash).is_ok());
        
        // Test merkle utility
        let merkle_hash = TokenCommitment::hash_merkle_tree_radix4(&tokens);
        assert_eq!(merkle_hash.len(), 64);
        assert!(hex::decode(&merkle_hash).is_ok());
        
        // Different methods should produce different hashes
        assert_ne!(concat_hash, merkle_hash);
    }

    #[test]
    fn test_verify_commitment_hash_utility() {
        let token = create_test_token(1);
        let tokens = create_test_tokens(3);
        
        // Test single token verification
        let single_hash = TokenCommitment::hash_single_token(&token);
        assert!(TokenCommitment::verify_commitment_hash(&single_hash, &[token.clone()], None).unwrap());
        
        // Test concatenation verification
        let concat_hash = TokenCommitment::hash_concatenation(&tokens);
        assert!(TokenCommitment::verify_commitment_hash(
            &concat_hash, 
            &tokens, 
            Some(CommitmentMethod::Concatenation)
        ).unwrap());
        
        // Test merkle verification
        let merkle_hash = TokenCommitment::hash_merkle_tree_radix4(&tokens);
        assert!(TokenCommitment::verify_commitment_hash(
            &merkle_hash, 
            &tokens, 
            Some(CommitmentMethod::MerkleTreeRadix4)
        ).unwrap());
        
        // Test wrong hash should fail
        assert!(!TokenCommitment::verify_commitment_hash(
            "invalid_hash", 
            &tokens, 
            Some(CommitmentMethod::Concatenation)
        ).unwrap());
        
        // Test multiple tokens without method should fail
        assert!(TokenCommitment::verify_commitment_hash(&concat_hash, &tokens, None).is_err());
    }

    #[test]
    fn test_large_token_set_performance() {
        // Test with larger token sets to ensure algorithms scale reasonably
        let large_token_set = create_test_tokens(50);
        
        // Both methods should complete without panic
        let concat_commitment = TokenCommitment::multiple(&large_token_set, CommitmentMethod::Concatenation);
        let merkle_commitment = TokenCommitment::multiple(&large_token_set, CommitmentMethod::MerkleTreeRadix4);
        
        // Should produce valid commitments
        assert_eq!(concat_commitment.commitment_hash.len(), 64);
        assert_eq!(merkle_commitment.commitment_hash.len(), 64);
        
        // Should verify correctly
        assert!(concat_commitment.verify(&large_token_set).unwrap());
        assert!(merkle_commitment.verify(&large_token_set).unwrap());
    }
}

// Property-based tests for commitment determinism and security
#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;
    use crate::events::CommitmentMethod;

    /// Generate a test token with deterministic content based on seed
    fn generate_test_token(seed: u32) -> CashuToken {
        // Use wrapping arithmetic to avoid overflow panics
        let mint_id = seed.wrapping_rem(100);
        let unique_id = seed.wrapping_mul(31).wrapping_add(17).wrapping_rem(10000);
        
        let token_json = format!(
            r#"{{"token":[{{"mint":"https://mint{}.example.com","proofs":[]}}],"memo":"test_token_seed_{}_unique_{}","unit":"sat"}}"#,
            mint_id, seed, unique_id
        );
        
        serde_json::from_str(&token_json)
            .unwrap_or_else(|e| panic!("Failed to create test token for seed {}: {}", seed, e))
    }

    /// Strategy for generating test tokens
    fn token_strategy() -> impl Strategy<Value = CashuToken> {
        any::<u32>().prop_map(generate_test_token)
    }

    /// Strategy for generating vectors of test tokens
    fn tokens_strategy(min_size: usize, max_size: usize) -> impl Strategy<Value = Vec<CashuToken>> {
        prop::collection::vec(token_strategy(), min_size..=max_size)
    }

    proptest! {
        /// Property: Single token commitments are deterministic
        /// The same token should always produce the same commitment hash
        #[test]
        fn prop_single_token_commitment_deterministic(token in token_strategy()) {
            let commitment1 = TokenCommitment::single(&token);
            let commitment2 = TokenCommitment::single(&token);
            
            prop_assert_eq!(&commitment1.commitment_hash, &commitment2.commitment_hash);
            prop_assert!(matches!(commitment1.commitment_type, CommitmentType::Single));
            prop_assert_eq!(commitment1.commitment_hash.len(), 64);
            prop_assert!(hex::decode(&commitment1.commitment_hash).is_ok());
        }

        /// Property: Concatenation commitments are deterministic regardless of input order
        /// The same set of tokens should produce the same commitment hash regardless of order
        #[test]
        fn prop_concatenation_commitment_order_independent(
            mut tokens in tokens_strategy(2, 10)
        ) {
            // Skip if we don't have enough unique tokens
            tokens.sort_by_key(|t| TokenCommitment::hash_token(t));
            tokens.dedup_by_key(|t| TokenCommitment::hash_token(t));
            prop_assume!(tokens.len() >= 2);
            
            let commitment1 = TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation);
            
            // Shuffle the tokens
            let mut shuffled_tokens = tokens.clone();
            shuffled_tokens.reverse();
            let commitment2 = TokenCommitment::multiple(&shuffled_tokens, CommitmentMethod::Concatenation);
            
            prop_assert_eq!(&commitment1.commitment_hash, &commitment2.commitment_hash);
            prop_assert_eq!(commitment1.commitment_hash.len(), 64);
            prop_assert!(hex::decode(&commitment1.commitment_hash).is_ok());
        }

        /// Property: Merkle tree commitments are deterministic regardless of input order
        #[test]
        fn prop_merkle_commitment_order_independent(
            mut tokens in tokens_strategy(2, 15)
        ) {
            // Skip if we don't have enough unique tokens
            tokens.sort_by_key(|t| TokenCommitment::hash_token(t));
            tokens.dedup_by_key(|t| TokenCommitment::hash_token(t));
            prop_assume!(tokens.len() >= 2);
            
            let commitment1 = TokenCommitment::multiple(&tokens, CommitmentMethod::MerkleTreeRadix4);
            
            // Shuffle the tokens
            let mut shuffled_tokens = tokens.clone();
            shuffled_tokens.reverse();
            let commitment2 = TokenCommitment::multiple(&shuffled_tokens, CommitmentMethod::MerkleTreeRadix4);
            
            prop_assert_eq!(&commitment1.commitment_hash, &commitment2.commitment_hash);
            prop_assert_eq!(commitment1.commitment_hash.len(), 64);
            prop_assert!(hex::decode(&commitment1.commitment_hash).is_ok());
        }

        /// Property: Different commitment methods produce different hashes for the same tokens
        #[test]
        fn prop_different_methods_produce_different_hashes(
            mut tokens in tokens_strategy(2, 10)
        ) {
            // Skip if we don't have enough unique tokens
            tokens.sort_by_key(|t| TokenCommitment::hash_token(t));
            tokens.dedup_by_key(|t| TokenCommitment::hash_token(t));
            prop_assume!(tokens.len() >= 2);
            
            let concat_commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation);
            let merkle_commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::MerkleTreeRadix4);
            
            prop_assert_ne!(&concat_commitment.commitment_hash, &merkle_commitment.commitment_hash);
        }

        /// Property: Commitment verification works correctly
        #[test]
        fn prop_commitment_verification_correctness(
            mut tokens in tokens_strategy(1, 10)
        ) {
            // Ensure unique tokens
            tokens.sort_by_key(|t| TokenCommitment::hash_token(t));
            tokens.dedup_by_key(|t| TokenCommitment::hash_token(t));
            prop_assume!(!tokens.is_empty());
            
            if tokens.len() == 1 {
                let commitment = TokenCommitment::single(&tokens[0]);
                prop_assert!(commitment.verify(&tokens).unwrap());
                
                // Should fail with different token
                if tokens.len() > 0 {
                    let different_token = generate_test_token(999999);
                    prop_assert!(!commitment.verify(&[different_token]).unwrap());
                }
            } else {
                let concat_commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation);
                let merkle_commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::MerkleTreeRadix4);
                
                prop_assert!(concat_commitment.verify(&tokens).unwrap());
                prop_assert!(merkle_commitment.verify(&tokens).unwrap());
                
                // Should fail with subset of tokens
                if tokens.len() > 1 {
                    let subset = &tokens[0..tokens.len()-1];
                    prop_assert!(!concat_commitment.verify(subset).unwrap());
                    prop_assert!(!merkle_commitment.verify(subset).unwrap());
                }
            }
        }

        /// Property: Hash token function is deterministic and collision-resistant
        #[test]
        fn prop_hash_token_deterministic_and_unique(
            seed1 in any::<u32>(),
            seed2 in any::<u32>()
        ) {
            prop_assume!(seed1 != seed2);
            
            let token1 = generate_test_token(seed1);
            let token2 = generate_test_token(seed2);
            
            let hash1a = TokenCommitment::hash_token(&token1);
            let hash1b = TokenCommitment::hash_token(&token1);
            let hash2 = TokenCommitment::hash_token(&token2);
            
            // Same token should produce same hash
            prop_assert_eq!(hash1a, hash1b);
            
            // Different tokens should produce different hashes (with very high probability)
            prop_assert_ne!(hash1a, hash2);
        }

        /// Property: Merkle tree construction handles various sizes correctly
        #[test]
        fn prop_merkle_tree_construction_correctness(
            size in 1usize..20
        ) {
            let hashes: Vec<[u8; 32]> = (0..size).map(|i| {
                let mut hash = [0u8; 32];
                // Use a more complex pattern to ensure uniqueness
                hash[0] = ((i + 1) % 256) as u8;
                hash[1] = ((i + 1) / 256) as u8;
                hash[2] = ((i * 31 + 17) % 256) as u8;
                hash
            }).collect();
            
            let merkle_root = TokenCommitment::build_merkle_tree_radix4(&hashes);
            
            if size == 1 {
                // Single element should equal the root
                prop_assert_eq!(merkle_root, hashes[0]);
            } else {
                // Multiple elements should produce a different root
                prop_assert!(!hashes.contains(&merkle_root));
            }
            
            // Root should be deterministic
            let merkle_root2 = TokenCommitment::build_merkle_tree_radix4(&hashes);
            prop_assert_eq!(merkle_root, merkle_root2);
        }

        /// Property: Concatenation commitment construction is correct
        #[test]
        fn prop_concatenation_construction_correctness(
            mut tokens in tokens_strategy(1, 15)
        ) {
            // Ensure unique tokens
            tokens.sort_by_key(|t| TokenCommitment::hash_token(t));
            tokens.dedup_by_key(|t| TokenCommitment::hash_token(t));
            prop_assume!(!tokens.is_empty());
            
            let commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation);
            
            // Should be valid hex
            prop_assert!(hex::decode(&commitment.commitment_hash).is_ok());
            
            // Should be 64 characters (SHA256 hex)
            prop_assert_eq!(commitment.commitment_hash.len(), 64);
            
            // Should verify correctly
            prop_assert!(commitment.verify(&tokens).unwrap());
            
            // Should be deterministic
            let commitment2 = TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation);
            prop_assert_eq!(&commitment.commitment_hash, &commitment2.commitment_hash);
        }

        /// Property: Token sorting is consistent and stable
        #[test]
        fn prop_token_sorting_consistency(
            mut tokens in tokens_strategy(2, 10)
        ) {
            // Ensure we have at least 2 unique tokens
            tokens.sort_by_key(|t| TokenCommitment::hash_token(t));
            tokens.dedup_by_key(|t| TokenCommitment::hash_token(t));
            prop_assume!(tokens.len() >= 2);
            
            // Sort tokens multiple times and ensure consistency
            let mut tokens1 = tokens.clone();
            let mut tokens2 = tokens.clone();
            let mut tokens3 = tokens.clone();
            
            tokens1.sort_by_key(|t| TokenCommitment::hash_token(t));
            tokens2.sort_by_key(|t| TokenCommitment::hash_token(t));
            tokens3.sort_by_key(|t| TokenCommitment::hash_token(t));
            
            // All sorted versions should be identical
            for i in 0..tokens1.len() {
                let hash1 = TokenCommitment::hash_token(&tokens1[i]);
                let hash2 = TokenCommitment::hash_token(&tokens2[i]);
                let hash3 = TokenCommitment::hash_token(&tokens3[i]);
                prop_assert_eq!(hash1, hash2);
                prop_assert_eq!(hash2, hash3);
            }
        }

        /// Property: Utility functions produce consistent results
        #[test]
        fn prop_utility_functions_consistency(
            mut tokens in tokens_strategy(1, 8)
        ) {
            // Ensure unique tokens
            tokens.sort_by_key(|t| TokenCommitment::hash_token(t));
            tokens.dedup_by_key(|t| TokenCommitment::hash_token(t));
            prop_assume!(!tokens.is_empty());
            
            if tokens.len() == 1 {
                let hash1 = TokenCommitment::hash_single_token(&tokens[0]);
                let commitment = TokenCommitment::single(&tokens[0]);
                prop_assert_eq!(hash1, commitment.commitment_hash);
            } else {
                let concat_hash1 = TokenCommitment::hash_concatenation(&tokens);
                let concat_commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::Concatenation);
                prop_assert_eq!(&concat_hash1, &concat_commitment.commitment_hash);
                
                let merkle_hash1 = TokenCommitment::hash_merkle_tree_radix4(&tokens);
                let merkle_commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::MerkleTreeRadix4);
                prop_assert_eq!(&merkle_hash1, &merkle_commitment.commitment_hash);
                
                // Verify utility function
                prop_assert!(TokenCommitment::verify_commitment_hash(
                    &concat_hash1, 
                    &tokens, 
                    Some(CommitmentMethod::Concatenation)
                ).unwrap());
                
                prop_assert!(TokenCommitment::verify_commitment_hash(
                    &merkle_hash1, 
                    &tokens, 
                    Some(CommitmentMethod::MerkleTreeRadix4)
                ).unwrap());
            }
        }
    }
}
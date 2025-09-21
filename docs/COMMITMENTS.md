# Commitment Construction Algorithms

Kirk uses standardized hash commitment algorithms to ensure consistency and security across all implementations. This document details the exact algorithms and their implementation.

## Table of Contents

- [Overview](#overview)
- [Token Ordering](#token-ordering)
- [Single Token Commitments](#single-token-commitments)
- [Multi-Token Commitments](#multi-token-commitments)
- [Verification Process](#verification-process)
- [Implementation Examples](#implementation-examples)
- [Security Considerations](#security-considerations)

## Overview

Hash commitments in Kirk serve two purposes:
1. **Privacy**: Hide token details until reveal time
2. **Integrity**: Prove that revealed tokens match original commitments

All commitment algorithms follow these principles:
- **Deterministic**: Same inputs always produce same outputs
- **Standardized**: All implementations must use identical algorithms
- **Secure**: Use cryptographically secure hash functions (SHA256)
- **Ordered**: Tokens are always sorted before hashing

## Token Ordering

Before any commitment construction, tokens MUST be sorted in ascending order by their hash value.

### Token Hash Function

```rust
fn hash_token(token: &CashuToken) -> [u8; 32] {
    let mut hasher = Sha256::new();
    
    // Hash all proofs in the token
    for proof in &token.proofs {
        // Include all proof components for uniqueness
        hasher.update(&proof.amount.to_be_bytes());
        hasher.update(&proof.secret);
        hasher.update(&proof.c);
        hasher.update(&proof.id);
    }
    
    hasher.finalize().into()
}
```

### Sorting Algorithm

```rust
fn sort_tokens_by_hash(tokens: &mut [CashuToken]) {
    tokens.sort_by_key(|token| hash_token(token));
}
```

**Important**: This sorting MUST be performed before any commitment construction to ensure deterministic results across all implementations.

## Single Token Commitments

For single token commitments, the algorithm is straightforward:

### Algorithm

```
commitment_hash = SHA256(token_hash)
```

### Implementation

```rust
impl TokenCommitment {
    pub fn single(token: &CashuToken) -> Self {
        let token_hash = Self::hash_token(token);
        let commitment_hash = sha256(&token_hash);
        
        Self {
            commitment_hash: hex::encode(commitment_hash),
            commitment_type: CommitmentType::Single,
        }
    }
}
```

### Example

```rust
// Input: Single Cashu token
let token = CashuToken { /* token data */ };

// Step 1: Hash the token
let token_hash = hash_token(&token);
// Result: [0x1a, 0x2b, 0x3c, ...] (32 bytes)

// Step 2: Hash the token hash
let commitment_hash = sha256(&token_hash);
// Result: [0x4d, 0x5e, 0x6f, ...] (32 bytes)

// Step 3: Encode as hex string
let commitment = hex::encode(commitment_hash);
// Result: "4d5e6f..."
```

## Multi-Token Commitments

For multiple tokens, Kirk supports two standardized methods:

### Method 1: Concatenation

Simple concatenation of sorted token hashes.

#### Algorithm

```
sorted_tokens = sort_by_hash(tokens)
concatenated = token_hash_1 || token_hash_2 || ... || token_hash_n
commitment_hash = SHA256(concatenated)
```

#### Implementation

```rust
fn concatenation_commitment(tokens: &[CashuToken]) -> String {
    let mut sorted_tokens = tokens.to_vec();
    sort_tokens_by_hash(&mut sorted_tokens);
    
    let mut concatenated = Vec::new();
    for token in &sorted_tokens {
        let token_hash = hash_token(token);
        concatenated.extend_from_slice(&token_hash);
    }
    
    let commitment_hash = sha256(&concatenated);
    hex::encode(commitment_hash)
}
```

#### Example

```rust
// Input: 3 tokens
let tokens = vec![token_a, token_b, token_c];

// Step 1: Hash each token
let hash_a = hash_token(&token_a); // [0x1a, 0x2b, ...]
let hash_b = hash_token(&token_b); // [0x3c, 0x4d, ...]
let hash_c = hash_token(&token_c); // [0x5e, 0x6f, ...]

// Step 2: Sort by hash value (assume hash_a < hash_b < hash_c)
let sorted_hashes = [hash_a, hash_b, hash_c];

// Step 3: Concatenate
let concatenated = hash_a || hash_b || hash_c;
// Result: 96 bytes total (32 * 3)

// Step 4: Hash the concatenation
let commitment_hash = sha256(&concatenated);
// Result: [0x7a, 0x8b, ...] (32 bytes)
```

### Method 2: Merkle Tree Radix 4

Merkle tree with radix 4 (each node has up to 4 children).

#### Algorithm

```
1. Sort tokens by hash value
2. Create leaf nodes from token hashes
3. Build tree bottom-up with radix 4
4. Each internal node = SHA256(child1 || child2 || child3 || child4)
5. Pad with zeros if fewer than 4 children
6. Root hash is the commitment
```

#### Implementation

```rust
fn merkle_tree_radix4_commitment(tokens: &[CashuToken]) -> String {
    let mut sorted_tokens = tokens.to_vec();
    sort_tokens_by_hash(&mut sorted_tokens);
    
    // Create leaf nodes
    let mut current_level: Vec<[u8; 32]> = sorted_tokens
        .iter()
        .map(|token| hash_token(token))
        .collect();
    
    // Build tree bottom-up
    while current_level.len() > 1 {
        let mut next_level = Vec::new();
        
        // Process nodes in groups of 4
        for chunk in current_level.chunks(4) {
            let mut node_data = Vec::new();
            
            // Concatenate up to 4 child hashes
            for hash in chunk {
                node_data.extend_from_slice(hash);
            }
            
            // Pad with zeros if less than 4 children
            while node_data.len() < 128 { // 4 * 32 bytes
                node_data.push(0);
            }
            
            // Hash to create parent node
            let parent_hash = sha256(&node_data);
            next_level.push(parent_hash);
        }
        
        current_level = next_level;
    }
    
    hex::encode(current_level[0])
}
```

#### Example

```rust
// Input: 5 tokens
let tokens = vec![token_a, token_b, token_c, token_d, token_e];

// Step 1: Create leaf nodes (sorted by hash)
let leaves = [hash_a, hash_b, hash_c, hash_d, hash_e];

// Step 2: Build level 1 (group leaves by 4)
// Group 1: [hash_a, hash_b, hash_c, hash_d]
let node1_data = hash_a || hash_b || hash_c || hash_d; // 128 bytes
let node1 = sha256(&node1_data);

// Group 2: [hash_e] (pad with zeros)
let node2_data = hash_e || [0; 32] || [0; 32] || [0; 32]; // 128 bytes
let node2 = sha256(&node2_data);

// Level 1: [node1, node2]

// Step 3: Build level 2 (root)
let root_data = node1 || node2 || [0; 32] || [0; 32]; // 128 bytes
let root = sha256(&root_data);

// Result: root is the commitment hash
```

## Verification Process

To verify a commitment against revealed tokens:

### Single Token Verification

```rust
fn verify_single_commitment(
    commitment_hash: &str,
    revealed_token: &CashuToken
) -> Result<bool, ValidationError> {
    let expected_commitment = TokenCommitment::single(revealed_token);
    Ok(expected_commitment.commitment_hash == commitment_hash)
}
```

### Multi-Token Verification

```rust
fn verify_multi_commitment(
    commitment_hash: &str,
    revealed_tokens: &[CashuToken],
    method: &CommitmentMethod
) -> Result<bool, ValidationError> {
    let expected_commitment = TokenCommitment::multiple(revealed_tokens, method.clone());
    Ok(expected_commitment.commitment_hash == commitment_hash)
}
```

### Verification Steps

1. **Collect revealed tokens** from Move events
2. **Find original commitment** from Challenge/ChallengeAccept events
3. **Determine commitment method** from Final events (for multi-token)
4. **Reconstruct commitment** using same algorithm
5. **Compare hashes** for equality

## Implementation Examples

### Complete Commitment Construction

```rust
use sha2::{Sha256, Digest};

pub struct CommitmentBuilder;

impl CommitmentBuilder {
    /// Create commitment for any number of tokens
    pub fn create_commitment(
        tokens: &[CashuToken],
        method: Option<CommitmentMethod>
    ) -> TokenCommitment {
        if tokens.len() == 1 {
            Self::single_commitment(&tokens[0])
        } else {
            let method = method.unwrap_or(CommitmentMethod::MerkleTreeRadix4);
            Self::multi_commitment(tokens, method)
        }
    }
    
    fn single_commitment(token: &CashuToken) -> TokenCommitment {
        let token_hash = Self::hash_token(token);
        let commitment_hash = Self::sha256(&token_hash);
        
        TokenCommitment {
            commitment_hash: hex::encode(commitment_hash),
            commitment_type: CommitmentType::Single,
        }
    }
    
    fn multi_commitment(tokens: &[CashuToken], method: CommitmentMethod) -> TokenCommitment {
        let commitment_hash = match method {
            CommitmentMethod::Concatenation => Self::concatenation_commitment(tokens),
            CommitmentMethod::MerkleTreeRadix4 => Self::merkle_tree_commitment(tokens),
        };
        
        TokenCommitment {
            commitment_hash,
            commitment_type: CommitmentType::Multiple { method },
        }
    }
    
    fn hash_token(token: &CashuToken) -> [u8; 32] {
        let mut hasher = Sha256::new();
        
        for proof in &token.proofs {
            hasher.update(&proof.amount.to_be_bytes());
            hasher.update(&proof.secret);
            hasher.update(&proof.c);
            hasher.update(&proof.id);
        }
        
        hasher.finalize().into()
    }
    
    fn sha256(data: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher.finalize().into()
    }
    
    fn concatenation_commitment(tokens: &[CashuToken]) -> String {
        let mut sorted_tokens = tokens.to_vec();
        sorted_tokens.sort_by_key(|t| Self::hash_token(t));
        
        let mut concatenated = Vec::new();
        for token in &sorted_tokens {
            let token_hash = Self::hash_token(token);
            concatenated.extend_from_slice(&token_hash);
        }
        
        let commitment_hash = Self::sha256(&concatenated);
        hex::encode(commitment_hash)
    }
    
    fn merkle_tree_commitment(tokens: &[CashuToken]) -> String {
        let mut sorted_tokens = tokens.to_vec();
        sorted_tokens.sort_by_key(|t| Self::hash_token(t));
        
        let mut current_level: Vec<[u8; 32]> = sorted_tokens
            .iter()
            .map(|t| Self::hash_token(t))
            .collect();
        
        while current_level.len() > 1 {
            let mut next_level = Vec::new();
            
            for chunk in current_level.chunks(4) {
                let mut node_data = Vec::with_capacity(128);
                
                for hash in chunk {
                    node_data.extend_from_slice(hash);
                }
                
                // Pad to 128 bytes (4 * 32)
                node_data.resize(128, 0);
                
                let parent_hash = Self::sha256(&node_data);
                next_level.push(parent_hash);
            }
            
            current_level = next_level;
        }
        
        hex::encode(current_level[0])
    }
}
```

### Testing Commitments

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_single_token_commitment() {
        let token = create_test_token();
        let commitment = CommitmentBuilder::create_commitment(&[token.clone()], None);
        
        // Verify commitment
        assert!(verify_single_commitment(&commitment.commitment_hash, &token).unwrap());
    }
    
    #[test]
    fn test_concatenation_commitment() {
        let tokens = create_test_tokens(3);
        let commitment = CommitmentBuilder::create_commitment(
            &tokens, 
            Some(CommitmentMethod::Concatenation)
        );
        
        // Verify commitment
        assert!(verify_multi_commitment(
            &commitment.commitment_hash,
            &tokens,
            &CommitmentMethod::Concatenation
        ).unwrap());
    }
    
    #[test]
    fn test_merkle_tree_commitment() {
        let tokens = create_test_tokens(5);
        let commitment = CommitmentBuilder::create_commitment(
            &tokens,
            Some(CommitmentMethod::MerkleTreeRadix4)
        );
        
        // Verify commitment
        assert!(verify_multi_commitment(
            &commitment.commitment_hash,
            &tokens,
            &CommitmentMethod::MerkleTreeRadix4
        ).unwrap());
    }
    
    #[test]
    fn test_commitment_determinism() {
        let tokens = create_test_tokens(4);
        
        // Create commitment multiple times
        let commitment1 = CommitmentBuilder::create_commitment(&tokens, None);
        let commitment2 = CommitmentBuilder::create_commitment(&tokens, None);
        
        // Should be identical
        assert_eq!(commitment1.commitment_hash, commitment2.commitment_hash);
    }
    
    #[test]
    fn test_token_ordering() {
        let mut tokens = create_test_tokens(3);
        let original_order = tokens.clone();
        
        // Shuffle tokens
        tokens.reverse();
        
        // Commitments should be identical regardless of input order
        let commitment1 = CommitmentBuilder::create_commitment(&original_order, None);
        let commitment2 = CommitmentBuilder::create_commitment(&tokens, None);
        
        assert_eq!(commitment1.commitment_hash, commitment2.commitment_hash);
    }
}
```

## Security Considerations

### Cryptographic Security

1. **Hash Function**: SHA256 provides 256-bit security level
2. **Collision Resistance**: Extremely unlikely to find two inputs with same hash
3. **Preimage Resistance**: Cannot reverse hash to find original input
4. **Deterministic**: Same input always produces same output

### Implementation Security

1. **Consistent Ordering**: Always sort tokens before hashing
2. **Complete Token Data**: Hash all relevant token components
3. **Proper Padding**: Use zero padding for merkle tree consistency
4. **Secure Random**: Ensure C values provide sufficient entropy

### Attack Vectors

1. **Hash Collision**: Practically impossible with SHA256
2. **Preimage Attack**: Cannot determine tokens from commitment alone
3. **Timing Attack**: Use constant-time operations where possible
4. **Implementation Bugs**: Follow reference implementation exactly

### Best Practices

1. **Use Reference Implementation**: Don't create custom algorithms
2. **Test Thoroughly**: Verify against known test vectors
3. **Validate Inputs**: Check token format and completeness
4. **Handle Errors**: Gracefully handle malformed data
5. **Audit Code**: Review commitment construction carefully

## Compatibility

### Version Compatibility

All Kirk implementations MUST use these exact algorithms to ensure compatibility:

- **Token Hashing**: Include amount, secret, C value, and ID
- **Sorting**: Ascending order by token hash
- **Concatenation**: Direct concatenation of sorted hashes
- **Merkle Tree**: Radix 4 with zero padding
- **Output Format**: Lowercase hexadecimal strings

### Cross-Implementation Testing

Test vectors for verifying implementation compatibility:

```rust
// Test vector 1: Single token
let token = TestToken {
    amount: 100,
    secret: "test_secret_123",
    c: [0x01, 0x02, 0x03, ...],
    id: "test_id_456",
};
let expected_commitment = "a1b2c3d4e5f6...";

// Test vector 2: Multiple tokens (concatenation)
let tokens = [token_a, token_b, token_c];
let expected_commitment = "f6e5d4c3b2a1...";

// Test vector 3: Multiple tokens (merkle tree)
let tokens = [token_a, token_b, token_c, token_d, token_e];
let expected_commitment = "9f8e7d6c5b4a...";
```

These test vectors ensure all implementations produce identical results.
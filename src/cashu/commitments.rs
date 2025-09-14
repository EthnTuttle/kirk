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
#[derive(Debug, Clone)]
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
    fn hash_token(token: &CashuToken) -> [u8; 32] {
        // Hash the token's serialized representation
        // This is a placeholder - actual implementation will use token-specific data
        let token_data = format!("{:?}", token);
        let mut hasher = Sha256::new();
        hasher.update(token_data.as_bytes());
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
                
                // Hash the concatenated children to create parent node
                let parent_hash: [u8; 32] = Sha256::digest(&node_data).into();
                next_level.push(parent_hash);
            }
            
            current_level = next_level;
        }
        
        current_level[0] // Return merkle root
    }
}
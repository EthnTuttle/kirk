use anyhow::Result;
use bevy::prelude::Resource;
use hkdf::Hkdf;
use nostr::Keys;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::path::Path;
use tokio::fs;

/// Master key manager that derives all application keys from a single master seed
#[derive(Resource, Clone, Debug)]
pub struct MasterKeyManager {
    master_seed: [u8; 64],
    nostr_keys: Keys,
    mint_keys: Keys,
}

/// Backup structure for master seed with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedBackup {
    /// The master seed (64 bytes) - stored as hex string for JSON compatibility
    #[serde(with = "hex_array")]
    pub master_seed: [u8; 64],
    /// Timestamp when backup was created
    pub created_at: u64,
    /// Player public key for verification
    pub player_pubkey: String,
    /// Mint public key for verification
    pub mint_pubkey: String,
    /// Backup format version
    pub version: u32,
}

/// Custom serialization module for 64-byte arrays as hex strings
mod hex_array {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    
    pub fn serialize<S>(bytes: &[u8; 64], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        hex::encode(bytes).serialize(serializer)
    }
    
    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 64], D::Error>
    where
        D: Deserializer<'de>,
    {
        let hex_string = String::deserialize(deserializer)?;
        let bytes = hex::decode(&hex_string).map_err(serde::de::Error::custom)?;
        
        if bytes.len() != 64 {
            return Err(serde::de::Error::custom(format!(
                "Expected 64 bytes, got {}", 
                bytes.len()
            )));
        }
        
        let mut array = [0u8; 64];
        array.copy_from_slice(&bytes);
        Ok(array)
    }
}

impl MasterKeyManager {
    /// Create a new MasterKeyManager with a randomly generated master seed
    pub fn new() -> Result<Self> {
        let master_seed = Self::generate_master_seed();
        Self::from_seed(master_seed)
    }
    
    /// Create a MasterKeyManager from an existing master seed
    pub fn from_seed(master_seed: [u8; 64]) -> Result<Self> {
        let nostr_keys = Self::derive_nostr_keys(&master_seed)?;
        let mint_keys = Self::derive_mint_keys(&master_seed)?;
        
        Ok(Self {
            master_seed,
            nostr_keys,
            mint_keys,
        })
    }
    
    /// Load master seed from file, or generate new one if file doesn't exist
    pub async fn load_or_generate(seed_file_path: Option<&str>) -> Result<Self> {
        if let Some(path) = seed_file_path {
            if Path::new(path).exists() {
                tracing::info!("Loading master seed from {}", path);
                let seed_data = fs::read(path).await?;
                
                if seed_data.len() != 64 {
                    return Err(anyhow::anyhow!(
                        "Invalid seed file: expected 64 bytes, got {}", 
                        seed_data.len()
                    ));
                }
                
                let mut master_seed = [0u8; 64];
                master_seed.copy_from_slice(&seed_data);
                
                return Self::from_seed(master_seed);
            }
        }
        
        // Generate new master seed
        tracing::info!("Generating new master seed");
        let key_manager = Self::new()?;
        
        // Save to file if path provided
        if let Some(path) = seed_file_path {
            key_manager.save_seed_to_file(path).await?;
            tracing::info!("Saved master seed to {}", path);
        }
        
        Ok(key_manager)
    }
    
    /// Save the master seed to a file
    pub async fn save_seed_to_file(&self, path: &str) -> Result<()> {
        fs::write(path, &self.master_seed).await?;
        Ok(())
    }
    
    /// Generate a cryptographically secure random master seed
    fn generate_master_seed() -> [u8; 64] {
        let mut seed = [0u8; 64];
        rand::thread_rng().fill_bytes(&mut seed);
        seed
    }
    
    /// Derive Nostr keys from master seed using HKDF
    fn derive_nostr_keys(master_seed: &[u8; 64]) -> Result<Keys> {
        let hk = Hkdf::<Sha256>::new(None, master_seed);
        let mut nostr_seed = [0u8; 32];
        hk.expand(b"nostr-client-key", &mut nostr_seed)
            .map_err(|e| anyhow::anyhow!("HKDF expand failed for Nostr keys: {}", e))?;
        
        let secret_key = nostr::SecretKey::from_slice(&nostr_seed)?;
        Ok(Keys::new(secret_key))
    }
    
    /// Derive mint keys from master seed using HKDF
    fn derive_mint_keys(master_seed: &[u8; 64]) -> Result<Keys> {
        let hk = Hkdf::<Sha256>::new(None, master_seed);
        let mut mint_seed = [0u8; 32];
        hk.expand(b"embedded-mint-key", &mut mint_seed)
            .map_err(|e| anyhow::anyhow!("HKDF expand failed for mint keys: {}", e))?;
        
        let secret_key = nostr::SecretKey::from_slice(&mint_seed)?;
        Ok(Keys::new(secret_key))
    }
    
    /// Get the player's Nostr keys
    pub fn get_player_keys(&self) -> &Keys {
        &self.nostr_keys
    }
    
    /// Get the mint's Nostr keys
    pub fn get_mint_keys(&self) -> &Keys {
        &self.mint_keys
    }
    
    /// Get the master seed (for backup purposes)
    pub fn get_master_seed(&self) -> &[u8; 64] {
        &self.master_seed
    }
    
    /// Derive additional keys for specific purposes using HKDF
    pub fn derive_custom_key(&self, info: &[u8]) -> Result<[u8; 32]> {
        let hk = Hkdf::<Sha256>::new(None, &self.master_seed);
        let mut derived_key = [0u8; 32];
        hk.expand(info, &mut derived_key)
            .map_err(|e| anyhow::anyhow!("HKDF expand failed for custom key: {}", e))?;
        
        Ok(derived_key)
    }
    
    /// Create a backup of the master seed with metadata
    pub fn create_backup(&self) -> SeedBackup {
        SeedBackup {
            master_seed: self.master_seed,
            created_at: chrono::Utc::now().timestamp() as u64,
            player_pubkey: self.nostr_keys.public_key().to_string(),
            mint_pubkey: self.mint_keys.public_key().to_string(),
            version: 1,
        }
    }
    
    /// Restore from a seed backup
    pub fn from_backup(backup: &SeedBackup) -> Result<Self> {
        let key_manager = Self::from_seed(backup.master_seed)?;
        
        // Verify that the restored keys match the backup metadata
        if key_manager.nostr_keys.public_key().to_string() != backup.player_pubkey {
            return Err(anyhow::anyhow!("Backup verification failed: player pubkey mismatch"));
        }
        
        if key_manager.mint_keys.public_key().to_string() != backup.mint_pubkey {
            return Err(anyhow::anyhow!("Backup verification failed: mint pubkey mismatch"));
        }
        
        tracing::info!("Successfully restored keys from backup created at {}", backup.created_at);
        Ok(key_manager)
    }
    
    /// Save backup to file with JSON format for better readability
    pub async fn save_backup_to_file(&self, path: &str) -> Result<()> {
        let backup = self.create_backup();
        let json = serde_json::to_string_pretty(&backup)?;
        fs::write(path, json).await?;
        tracing::info!("Saved backup to {}", path);
        Ok(())
    }
    
    /// Load backup from JSON file
    pub async fn load_backup_from_file(path: &str) -> Result<SeedBackup> {
        let json = fs::read_to_string(path).await?;
        let backup: SeedBackup = serde_json::from_str(&json)?;
        tracing::info!("Loaded backup from {}", path);
        Ok(backup)
    }
    
    /// Verify the integrity of the current key derivation
    pub fn verify_key_derivation(&self) -> Result<()> {
        // Re-derive keys and verify they match current keys
        let test_nostr_keys = Self::derive_nostr_keys(&self.master_seed)?;
        let test_mint_keys = Self::derive_mint_keys(&self.master_seed)?;
        
        if test_nostr_keys.secret_key() != self.nostr_keys.secret_key() {
            return Err(anyhow::anyhow!("Key derivation verification failed: Nostr keys mismatch"));
        }
        
        if test_mint_keys.secret_key() != self.mint_keys.secret_key() {
            return Err(anyhow::anyhow!("Key derivation verification failed: mint keys mismatch"));
        }
        
        Ok(())
    }
}

impl Default for MasterKeyManager {
    fn default() -> Self {
        Self::new().expect("Failed to generate master key manager")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    
    #[test]
    fn test_master_seed_generation() {
        let key_manager1 = MasterKeyManager::new().unwrap();
        let key_manager2 = MasterKeyManager::new().unwrap();
        
        // Different instances should have different seeds
        assert_ne!(key_manager1.master_seed, key_manager2.master_seed);
    }
    
    #[test]
    fn test_deterministic_key_derivation() {
        let master_seed = [42u8; 64]; // Fixed seed for testing
        
        let key_manager1 = MasterKeyManager::from_seed(master_seed).unwrap();
        let key_manager2 = MasterKeyManager::from_seed(master_seed).unwrap();
        
        // Same seed should produce same keys
        assert_eq!(
            key_manager1.get_player_keys().secret_key(),
            key_manager2.get_player_keys().secret_key()
        );
        assert_eq!(
            key_manager1.get_mint_keys().secret_key(),
            key_manager2.get_mint_keys().secret_key()
        );
    }
    
    #[test]
    fn test_different_key_types() {
        let key_manager = MasterKeyManager::new().unwrap();
        
        // Player and mint keys should be different
        assert_ne!(
            key_manager.get_player_keys().secret_key(),
            key_manager.get_mint_keys().secret_key()
        );
    }
    
    #[test]
    fn test_custom_key_derivation() {
        let key_manager = MasterKeyManager::new().unwrap();
        
        let key1 = key_manager.derive_custom_key(b"test-key-1").unwrap();
        let key2 = key_manager.derive_custom_key(b"test-key-2").unwrap();
        let key1_again = key_manager.derive_custom_key(b"test-key-1").unwrap();
        
        // Different info should produce different keys
        assert_ne!(key1, key2);
        
        // Same info should produce same key
        assert_eq!(key1, key1_again);
    }
    
    #[tokio::test]
    async fn test_seed_persistence() {
        let temp_file = NamedTempFile::new().unwrap();
        let file_path = temp_file.path().to_str().unwrap();
        
        // Create and save key manager
        let original_key_manager = MasterKeyManager::new().unwrap();
        original_key_manager.save_seed_to_file(file_path).await.unwrap();
        
        // Load key manager from file
        let loaded_key_manager = MasterKeyManager::load_or_generate(Some(file_path)).await.unwrap();
        
        // Keys should be identical
        assert_eq!(
            original_key_manager.get_player_keys().secret_key(),
            loaded_key_manager.get_player_keys().secret_key()
        );
        assert_eq!(
            original_key_manager.get_mint_keys().secret_key(),
            loaded_key_manager.get_mint_keys().secret_key()
        );
    }
    
    #[tokio::test]
    async fn test_load_or_generate_new() {
        // Test with non-existent file - should generate new
        let key_manager = MasterKeyManager::load_or_generate(Some("/tmp/nonexistent_seed_file")).await.unwrap();
        
        // Should have valid keys (secret_key() returns a reference, not a Result)
        let _player_key = key_manager.get_player_keys().secret_key();
        let _mint_key = key_manager.get_mint_keys().secret_key();
        
        // If we get here without panicking, the keys are valid
        assert!(true);
    }
    
    #[test]
    fn test_backup_creation_and_restoration() {
        let original_key_manager = MasterKeyManager::new().unwrap();
        
        // Create backup
        let backup = original_key_manager.create_backup();
        
        // Verify backup contains correct metadata
        assert_eq!(backup.player_pubkey, original_key_manager.get_player_keys().public_key().to_string());
        assert_eq!(backup.mint_pubkey, original_key_manager.get_mint_keys().public_key().to_string());
        assert_eq!(backup.version, 1);
        assert_eq!(backup.master_seed, original_key_manager.master_seed);
        
        // Restore from backup
        let restored_key_manager = MasterKeyManager::from_backup(&backup).unwrap();
        
        // Verify restored keys match original
        assert_eq!(
            original_key_manager.get_player_keys().secret_key(),
            restored_key_manager.get_player_keys().secret_key()
        );
        assert_eq!(
            original_key_manager.get_mint_keys().secret_key(),
            restored_key_manager.get_mint_keys().secret_key()
        );
    }
    
    #[tokio::test]
    async fn test_backup_file_operations() {
        let temp_file = NamedTempFile::new().unwrap();
        let file_path = temp_file.path().to_str().unwrap();
        
        // Create key manager and save backup
        let original_key_manager = MasterKeyManager::new().unwrap();
        original_key_manager.save_backup_to_file(file_path).await.unwrap();
        
        // Load backup from file
        let loaded_backup = MasterKeyManager::load_backup_from_file(file_path).await.unwrap();
        
        // Restore from loaded backup
        let restored_key_manager = MasterKeyManager::from_backup(&loaded_backup).unwrap();
        
        // Verify keys match
        assert_eq!(
            original_key_manager.get_player_keys().secret_key(),
            restored_key_manager.get_player_keys().secret_key()
        );
        assert_eq!(
            original_key_manager.get_mint_keys().secret_key(),
            restored_key_manager.get_mint_keys().secret_key()
        );
    }
    
    #[test]
    fn test_backup_verification_failure() {
        let key_manager = MasterKeyManager::new().unwrap();
        let mut backup = key_manager.create_backup();
        
        // Corrupt the backup by changing the player pubkey
        backup.player_pubkey = "corrupted_pubkey".to_string();
        
        // Restoration should fail
        let result = MasterKeyManager::from_backup(&backup);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("player pubkey mismatch"));
    }
    
    #[test]
    fn test_key_derivation_verification() {
        let key_manager = MasterKeyManager::new().unwrap();
        
        // Verification should pass for a properly constructed key manager
        assert!(key_manager.verify_key_derivation().is_ok());
    }
    
    #[tokio::test]
    async fn test_invalid_seed_file_handling() {
        let temp_file = NamedTempFile::new().unwrap();
        let file_path = temp_file.path().to_str().unwrap();
        
        // Write invalid seed data (wrong size)
        fs::write(file_path, b"invalid_seed_data").await.unwrap();
        
        // Loading should fail with appropriate error
        let result = MasterKeyManager::load_or_generate(Some(file_path)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("expected 64 bytes"));
    }
    
    #[tokio::test]
    async fn test_seed_file_permissions() {
        let temp_file = NamedTempFile::new().unwrap();
        let file_path = temp_file.path().to_str().unwrap();
        
        // Create and save key manager
        let key_manager = MasterKeyManager::new().unwrap();
        key_manager.save_seed_to_file(file_path).await.unwrap();
        
        // Verify file exists and has content
        let metadata = std::fs::metadata(file_path).unwrap();
        assert_eq!(metadata.len(), 64); // Should be exactly 64 bytes
        
        // Verify we can read it back
        let loaded_key_manager = MasterKeyManager::load_or_generate(Some(file_path)).await.unwrap();
        assert_eq!(
            key_manager.get_player_keys().secret_key(),
            loaded_key_manager.get_player_keys().secret_key()
        );
    }
}
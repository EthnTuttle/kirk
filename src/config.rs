//! Configuration management for the Kirk gaming protocol

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use crate::security::SecurityConfig;
use crate::error::GameProtocolError;

/// Main configuration for the Kirk protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KirkConfig {
    /// Security configuration
    pub security: SecurityConfig,
    /// Network configuration
    pub network: NetworkConfig,
    /// Game configuration
    pub game: GameConfig,
}

impl Default for KirkConfig {
    fn default() -> Self {
        Self {
            security: SecurityConfig::default(),
            network: NetworkConfig::default(),
            game: GameConfig::default(),
        }
    }
}

/// Network-related configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Default Nostr relays to connect to
    pub default_relays: Vec<String>,
    /// Connection timeout in seconds
    pub connection_timeout: u64,
    /// Maximum concurrent connections
    pub max_connections: u32,
    /// Retry attempts for failed connections
    pub retry_attempts: u32,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            default_relays: vec![
                "wss://relay.damus.io".to_string(),
                "wss://nos.lol".to_string(),
            ],
            connection_timeout: 10,
            max_connections: 10,
            retry_attempts: 3,
        }
    }
}

/// Game-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameConfig {
    /// Default timeout for game phases (seconds)
    pub default_timeout: u32,
    /// Maximum number of concurrent games per client
    pub max_concurrent_games: u32,
    /// Minimum token amount for games
    pub min_token_amount: u64,
    /// Maximum token amount for games
    pub max_token_amount: u64,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            default_timeout: 300,        // 5 minutes
            max_concurrent_games: 5,
            min_token_amount: 1,         // 1 sat minimum
            max_token_amount: 1000000,   // 1M sats maximum
        }
    }
}

impl KirkConfig {
    /// Load configuration from a file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, GameProtocolError> {
        let content = fs::read_to_string(path).map_err(|e| {
            GameProtocolError::Configuration {
                message: format!("Failed to read config file: {}", e),
                field: "config_file".to_string(),
            }
        })?;

        let config: KirkConfig = toml::from_str(&content).map_err(|e| {
            GameProtocolError::Configuration {
                message: format!("Failed to parse config file: {}", e),
                field: "config_format".to_string(),
            }
        })?;

        config.validate()?;
        Ok(config)
    }

    /// Save configuration to a file
    pub fn to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), GameProtocolError> {
        let content = toml::to_string_pretty(self).map_err(|e| {
            GameProtocolError::Configuration {
                message: format!("Failed to serialize config: {}", e),
                field: "config_serialization".to_string(),
            }
        })?;

        fs::write(path, content).map_err(|e| {
            GameProtocolError::Configuration {
                message: format!("Failed to write config file: {}", e),
                field: "config_write".to_string(),
            }
        })?;

        Ok(())
    }

    /// Validate configuration values
    pub fn validate(&self) -> Result<(), GameProtocolError> {
        // Validate network configuration
        if self.network.connection_timeout == 0 {
            return Err(GameProtocolError::Configuration {
                message: "Connection timeout must be greater than 0".to_string(),
                field: "network.connection_timeout".to_string(),
            });
        }

        if self.network.max_connections == 0 {
            return Err(GameProtocolError::Configuration {
                message: "Max connections must be greater than 0".to_string(),
                field: "network.max_connections".to_string(),
            });
        }

        // Validate game configuration
        if self.game.min_token_amount >= self.game.max_token_amount {
            return Err(GameProtocolError::Configuration {
                message: "Min token amount must be less than max token amount".to_string(),
                field: "game.token_amounts".to_string(),
            });
        }

        if self.game.default_timeout < 60 {
            return Err(GameProtocolError::Configuration {
                message: "Default timeout must be at least 60 seconds".to_string(),
                field: "game.default_timeout".to_string(),
            });
        }

        // Validate security configuration through the security module
        // (SecurityConfig validates itself through its constructors)

        Ok(())
    }

    /// Create a production-ready configuration
    pub fn production() -> Self {
        Self {
            security: SecurityConfig {
                rate_limit: crate::security::RateLimitConfig {
                    requests_per_minute: 30,    // Stricter rate limiting
                    burst_size: 5,
                    global_rate_limit: 500,     // Reduced global limit
                    window_duration: 60,
                },
                timeout_config: crate::security::SecureTimeoutConfig {
                    min_timeout_seconds: 120,   // Longer minimum timeouts
                    max_timeout_seconds: 3600,  // Shorter maximum timeouts
                    clock_skew_tolerance: 15,   // Tighter clock skew tolerance
                    network_grace_period: 5,    // Shorter grace period
                },
                validation_rules: crate::security::ValidationRules {
                    max_content_length: 32768,  // Smaller max content
                    max_tags_per_event: 50,     // Fewer tags allowed
                    max_tag_value_length: 512,  // Shorter tag values
                    allowed_content_patterns: vec![
                        r"^[a-zA-Z0-9\s\{\}\[\],:._-]+$".to_string(),
                    ],
                    blocked_content_patterns: vec![
                        "<script.*?</script>".to_string(),
                        "javascript:".to_string(),
                        "data:.*base64".to_string(),
                        r"<.*?on\w+\s*=".to_string(),  // Block event handlers
                    ],
                },
                crypto_config: crate::security::CryptoSecurityConfig {
                    min_hash_bits: 256,
                    commitment_entropy_bits: 128,
                    enable_constant_time_ops: true,
                    max_commitment_batch_size: 100,  // Smaller batches
                },
            },
            network: NetworkConfig {
                default_relays: vec![
                    "wss://relay.damus.io".to_string(),
                    "wss://nos.lol".to_string(),
                    "wss://relay.snort.social".to_string(),
                ],
                connection_timeout: 5,          // Shorter timeout
                max_connections: 5,             // Fewer connections
                retry_attempts: 2,              // Fewer retries
            },
            game: GameConfig {
                default_timeout: 600,           // 10 minutes default
                max_concurrent_games: 3,        // Fewer concurrent games
                min_token_amount: 100,          // Higher minimum
                max_token_amount: 100000,       // Lower maximum
            },
        }
    }

    /// Create a development configuration with relaxed settings
    pub fn development() -> Self {
        Self {
            security: SecurityConfig {
                rate_limit: crate::security::RateLimitConfig {
                    requests_per_minute: 120,   // More permissive
                    burst_size: 20,
                    global_rate_limit: 2000,
                    window_duration: 60,
                },
                timeout_config: crate::security::SecureTimeoutConfig {
                    min_timeout_seconds: 30,    // Shorter for testing
                    max_timeout_seconds: 86400,
                    clock_skew_tolerance: 60,   // More tolerant
                    network_grace_period: 30,
                },
                ..Default::default()
            },
            network: NetworkConfig {
                default_relays: vec![
                    "ws://localhost:8080".to_string(), // Local relay for testing
                ],
                connection_timeout: 30,
                max_connections: 20,
                retry_attempts: 5,
            },
            game: GameConfig {
                default_timeout: 120,           // 2 minutes for faster testing
                max_concurrent_games: 10,
                min_token_amount: 1,
                max_token_amount: 10000000,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[test]
    fn test_default_config_validation() {
        let config = KirkConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_production_config_validation() {
        let config = KirkConfig::production();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_development_config_validation() {
        let config = KirkConfig::development();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_invalid_timeout_validation() {
        let mut config = KirkConfig::default();
        config.game.default_timeout = 30; // Too short

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_invalid_token_amounts() {
        let mut config = KirkConfig::default();
        config.game.min_token_amount = 1000;
        config.game.max_token_amount = 100; // Min > Max

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_file_roundtrip() {
        let original_config = KirkConfig::production();

        // Create a temporary file
        let mut temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path();

        // Save config to file
        assert!(original_config.to_file(temp_path).is_ok());

        // Load config from file
        let loaded_config = KirkConfig::from_file(temp_path).unwrap();

        // Verify they match (using debug format for comparison)
        assert_eq!(format!("{:?}", original_config), format!("{:?}", loaded_config));
    }
}
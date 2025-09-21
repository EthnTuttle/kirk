//! Error types for the Kirk gaming protocol

use nostr::EventId;
use thiserror::Error;
use std::collections::HashMap;

/// Main error type for the Kirk gaming protocol
#[derive(Debug, Clone, Error)]
pub enum GameProtocolError {
    #[error("Network error: {source}")]
    Network {
        source: NetworkError,
        context: String,
    },

    #[error("Validation failed: {message}")]
    Validation {
        message: String,
        field: Option<String>,
        event_id: Option<EventId>,
    },

    #[error("Cryptographic error: {source}")]
    Cryptographic {
        source: CryptoError,
        context: String,
    },

    #[error("Configuration error: {message}")]
    Configuration {
        message: String,
        field: String
    },

    #[error("Timeout error: {message}")]
    Timeout {
        message: String,
        duration_ms: u64,
        operation: String,
    },

    #[error("Rate limit exceeded: {message}")]
    RateLimit {
        message: String,
        client_id: Option<String>,
        retry_after_ms: Option<u64>,
    },

    #[error("Serialization error: {message}")]
    Serialization { message: String },

    #[error("Hex decoding error: {0}")]
    HexDecode(#[from] hex::FromHexError),

    // Legacy error variants for backward compatibility
    #[error("Nostr error: {0}")]
    Nostr(String),

    #[error("Nostr SDK error: {0}")]
    NostrSdk(String),

    #[error("CDK error: {0}")]
    Cdk(String),

    #[error("Game validation error: {0}")]
    GameValidation(String),

    #[error("Invalid commitment: {0}")]
    InvalidCommitment(String),

    #[error("Sequence error: {0}")]
    SequenceError(String),

    #[error("Mint error: {0}")]
    MintError(String),

    #[error("Invalid expiry time")]
    InvalidExpiry,

    #[error("Invalid token: {0}")]
    InvalidToken(String),

    #[error("Invalid game piece: {0}")]
    InvalidGamePiece(String),

    #[error("Invalid move: {0}")]
    InvalidMove(String),
}

/// Network-specific error types
#[derive(Debug, Clone, Error)]
pub enum NetworkError {
    #[error("Connection failed: {message}")]
    ConnectionFailed { message: String },

    #[error("Request timeout: {duration_ms}ms")]
    RequestTimeout { duration_ms: u64 },

    #[error("Invalid response: {message}")]
    InvalidResponse { message: String },

    #[error("Service unavailable: {service}")]
    ServiceUnavailable { service: String },
}

/// Cryptographic error types
#[derive(Debug, Clone, Error)]
pub enum CryptoError {
    #[error("Invalid key: {message}")]
    InvalidKey { message: String },

    #[error("Hash verification failed")]
    HashVerificationFailed,

    #[error("Signature verification failed")]
    SignatureVerificationFailed,

    #[error("Commitment verification failed: {message}")]
    CommitmentVerificationFailed { message: String },
}

// Manual From implementations for external error types
impl From<nostr::event::builder::Error> for GameProtocolError {
    fn from(err: nostr::event::builder::Error) -> Self {
        GameProtocolError::Nostr(err.to_string())
    }
}

impl From<nostr::key::Error> for GameProtocolError {
    fn from(err: nostr::key::Error) -> Self {
        GameProtocolError::Nostr(err.to_string())
    }
}

impl From<nostr_sdk::client::Error> for GameProtocolError {
    fn from(err: nostr_sdk::client::Error) -> Self {
        GameProtocolError::NostrSdk(err.to_string())
    }
}

impl From<NetworkError> for GameProtocolError {
    fn from(err: NetworkError) -> Self {
        GameProtocolError::Network {
            source: err,
            context: String::new(),
        }
    }
}

impl From<serde_json::Error> for GameProtocolError {
    fn from(err: serde_json::Error) -> Self {
        GameProtocolError::Serialization {
            message: err.to_string(),
        }
    }
}

impl From<CryptoError> for GameProtocolError {
    fn from(err: CryptoError) -> Self {
        GameProtocolError::Cryptographic {
            source: err,
            context: String::new(),
        }
    }
}

/// Error context for tracking errors through the system
#[derive(Debug, Clone)]
pub struct ErrorContext {
    pub correlation_id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub component: String,
    pub operation: String,
    pub metadata: HashMap<String, String>,
}

impl ErrorContext {
    pub fn new(component: &str, operation: &str) -> Self {
        Self {
            correlation_id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now(),
            component: component.to_string(),
            operation: operation.to_string(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }
}

/// Enhanced validation error with context
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub event_id: EventId,
    pub error_type: ValidationErrorType,
    pub message: String,
    pub context: Option<ErrorContext>,
}

impl ValidationError {
    /// Create a new validation error without context (for backward compatibility)
    pub fn new(event_id: EventId, error_type: ValidationErrorType, message: String) -> Self {
        Self {
            event_id,
            error_type,
            message,
            context: None,
        }
    }

    /// Create a new validation error with context
    pub fn with_context(event_id: EventId, error_type: ValidationErrorType, message: String, context: ErrorContext) -> Self {
        Self {
            event_id,
            error_type,
            message,
            context: Some(context),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ValidationErrorType {
    InvalidToken,
    InvalidCommitment,
    InvalidSequence,
    InvalidMove,
    TimeoutViolation,
    RateLimitExceeded,
    InputSanitization,
}

/// Result of game sequence validation with enhanced context
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub winner: Option<nostr::PublicKey>,
    pub errors: Vec<ValidationError>,
    pub forfeited_player: Option<nostr::PublicKey>,
    pub context: Option<ErrorContext>,
}

impl ValidationResult {
    /// Create a new validation result without context (for backward compatibility)
    pub fn new(is_valid: bool, winner: Option<nostr::PublicKey>, errors: Vec<ValidationError>, forfeited_player: Option<nostr::PublicKey>) -> Self {
        Self {
            is_valid,
            winner,
            errors,
            forfeited_player,
            context: None,
        }
    }

    /// Create a new validation result with context
    pub fn with_context(is_valid: bool, winner: Option<nostr::PublicKey>, errors: Vec<ValidationError>, forfeited_player: Option<nostr::PublicKey>, context: ErrorContext) -> Self {
        Self {
            is_valid,
            winner,
            errors,
            forfeited_player,
            context: Some(context),
        }
    }
}

/// Utility functions for cryptographic operations
pub mod utils {
    use sha2::{Sha256, Digest};
    
    /// Compute SHA256 hash of input data
    pub fn sha256(data: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher.finalize().into()
    }
    
    /// Convert bytes to hexadecimal string
    pub fn to_hex(bytes: &[u8]) -> String {
        hex::encode(bytes)
    }
    
    /// Convert hexadecimal string to bytes
    pub fn from_hex(hex_str: &str) -> Result<Vec<u8>, hex::FromHexError> {
        hex::decode(hex_str)
    }
    
    /// Compute SHA256 hash and return as hex string
    pub fn sha256_hex(data: &[u8]) -> String {
        to_hex(&sha256(data))
    }
}

/// Type alias for the main result type used throughout the library
pub type GameResult<T> = Result<T, GameProtocolError>;

/// Logging configuration and initialization
pub mod logging {
    use tracing::Level;
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    use std::env;

    /// Logging output format
    #[derive(Debug, Clone)]
    pub enum LogFormat {
        Human,
        Json,
    }

    /// Logging output destination
    #[derive(Debug, Clone)]
    pub enum LogOutput {
        Stdout,
        Stderr,
    }

    /// Logging configuration
    #[derive(Debug, Clone)]
    pub struct LoggingConfig {
        pub level: Level,
        pub format: LogFormat,
        pub output: LogOutput,
    }

    impl Default for LoggingConfig {
        fn default() -> Self {
            Self {
                level: Level::INFO,
                format: LogFormat::Human,
                output: LogOutput::Stdout,
            }
        }
    }

    /// Initialize structured logging with the given configuration
    pub fn init_logging(config: LoggingConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let env_filter = EnvFilter::builder()
            .with_default_directive(config.level.into())
            .from_env_lossy()
            .add_directive("kirk=trace".parse()?)
            .add_directive("tokio=info".parse()?)
            .add_directive("hyper=info".parse()?);

        let registry = tracing_subscriber::registry()
            .with(env_filter);

        match config.format {
            LogFormat::Human => {
                let fmt_layer = fmt::layer()
                    .with_target(true)
                    .with_thread_ids(true)
                    .with_file(true)
                    .with_line_number(true);

                match config.output {
                    LogOutput::Stdout => registry.with(fmt_layer.with_writer(std::io::stdout)).init(),
                    LogOutput::Stderr => registry.with(fmt_layer.with_writer(std::io::stderr)).init(),
                }
            }
            LogFormat::Json => {
                let fmt_layer = fmt::layer()
                    .json()
                    .with_target(true)
                    .with_thread_ids(true)
                    .with_file(true)
                    .with_line_number(true)
                    .with_span_events(fmt::format::FmtSpan::CLOSE);

                match config.output {
                    LogOutput::Stdout => registry.with(fmt_layer.with_writer(std::io::stdout)).init(),
                    LogOutput::Stderr => registry.with(fmt_layer.with_writer(std::io::stderr)).init(),
                }
            }
        }

        Ok(())
    }

    /// Initialize logging with environment-based configuration
    pub fn init_from_env() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let level = env::var("KIRK_LOG_LEVEL")
            .unwrap_or_else(|_| "info".to_string())
            .parse::<Level>()
            .unwrap_or(Level::INFO);

        let format = match env::var("KIRK_LOG_FORMAT").as_ref().map(|s| s.as_str()) {
            Ok("json") => LogFormat::Json,
            _ => LogFormat::Human,
        };

        let output = match env::var("KIRK_LOG_OUTPUT").as_ref().map(|s| s.as_str()) {
            Ok("stderr") => LogOutput::Stderr,
            _ => LogOutput::Stdout,
        };

        let config = LoggingConfig { level, format, output };
        init_logging(config)
    }
}
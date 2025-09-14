//! Error types for the Kirk gaming protocol

use nostr::EventId;
use thiserror::Error;

/// Main error type for the Kirk gaming protocol
#[derive(Debug, Error)]
pub enum GameProtocolError {
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
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Timeout error: {0}")]
    Timeout(String),
    
    #[error("Hex decoding error: {0}")]
    HexDecode(#[from] hex::FromHexError),
    
    #[error("Invalid token: {0}")]
    InvalidToken(String),
    
    #[error("Invalid game piece: {0}")]
    InvalidGamePiece(String),
    
    #[error("Invalid move: {0}")]
    InvalidMove(String),
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

/// Validation-specific error types
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub event_id: EventId,
    pub error_type: ValidationErrorType,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum ValidationErrorType {
    InvalidToken,
    InvalidCommitment,
    InvalidSequence,
    InvalidMove,
    TimeoutViolation,
}

/// Result of game sequence validation
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub winner: Option<nostr::PublicKey>,
    pub errors: Vec<ValidationError>,
    pub forfeited_player: Option<nostr::PublicKey>,
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
//! Kirk - A trustless gaming protocol combining Cashu ecash tokens with Nostr events
//!
//! Kirk enables cryptographically-secured gaming through:
//! - Cashu ecash tokens for game piece commitments and rewards
//! - Nostr events for decentralized game coordination
//! - Commit-and-reveal mechanics for strategic gameplay
//! - Trustless validation by mint operators and third parties

pub mod events;
pub mod game;
pub mod cashu;
pub mod client;
pub mod error;
pub mod config;

// Re-export commonly used types for convenience
pub use error::GameProtocolError;

// Re-export key event types
pub use events::{
    ChallengeContent, ChallengeAcceptContent, MoveContent, FinalContent, RewardContent,
    MoveType, CommitmentMethod
};

// Re-export core game traits
pub use game::{Game, CommitmentValidator};

// Re-export Cashu integration types
pub use cashu::{GameToken, GameTokenType, GameMint, TokenCommitment, SequenceProcessor, SequenceProcessorConfig, ProcessingResult};

// Re-export client interfaces
pub use client::{PlayerClient, ValidationClient};

// Re-export configuration interfaces
pub use config::{KirkConfig, NetworkConfig, GameConfig};

// Re-export external dependencies for user convenience
pub use nostr::{Event, EventId, PublicKey, Keys};
pub use cdk::nuts::Token as CashuToken;
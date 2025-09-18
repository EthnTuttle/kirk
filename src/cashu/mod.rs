//! Cashu integration layer for Kirk gaming protocol

pub mod tokens;
pub mod commitments;
pub mod mint;
pub mod sequence_processor;

// Re-export key types
pub use tokens::{GameToken, GameTokenType, RewardTokenState};
pub use commitments::{TokenCommitment, CommitmentType};
pub use mint::GameMint;
pub use sequence_processor::{SequenceProcessor, SequenceProcessorConfig, ProcessingResult, SequenceStatistics};
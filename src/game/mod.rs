//! Game trait definitions and implementations

pub mod traits;
pub mod pieces;
pub mod validation;

// Re-export core traits
pub use traits::{Game, CommitmentValidator};
pub use validation::{GameSequence, SequenceState};
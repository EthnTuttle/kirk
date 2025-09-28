//! Game trait definitions and implementations

pub mod traits;
pub mod pieces;
pub mod validation;
pub mod card_game;

#[cfg(test)]
mod timeout_validation_tests;

// Re-export core traits and implementations
pub use traits::{Game, CommitmentValidator};
pub use validation::{GameSequence, SequenceState};
pub use card_game::{CardGame, CardGameState, CardGamePhase, CardMove, CardAction};
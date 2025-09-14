//! Nostr event types and handling for Kirk gaming protocol

pub mod challenge;
pub mod move_event;
pub mod final_event;
pub mod reward;

// Re-export all event content types
pub use challenge::{ChallengeContent, ChallengeAcceptContent};
pub use move_event::{MoveContent, MoveType};
pub use final_event::{FinalContent, CommitmentMethod};
pub use reward::{RewardContent, ValidationFailureContent};

// Event kind constants - using contiguous unused kind numbers
use nostr::Kind;

pub const CHALLENGE_KIND: Kind = Kind::Custom(9259);
pub const CHALLENGE_ACCEPT_KIND: Kind = Kind::Custom(9260);
pub const MOVE_KIND: Kind = Kind::Custom(9261);
pub const FINAL_KIND: Kind = Kind::Custom(9262);
pub const REWARD_KIND: Kind = Kind::Custom(9263);
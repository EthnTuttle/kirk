//! Client interfaces for players and validators

pub mod player;
pub mod validator;

// Re-export client types
pub use player::PlayerClient;
pub use validator::ValidationClient;
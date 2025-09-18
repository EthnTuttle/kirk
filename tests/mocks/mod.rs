//! Mock implementations for testing

pub mod nostr_relay;
pub mod cashu_mint;
pub mod reference_game;

pub use nostr_relay::MockNostrRelay;
pub use cashu_mint::MockCashuMint;
pub use reference_game::CoinFlipGame;
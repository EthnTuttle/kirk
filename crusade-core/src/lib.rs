//! crusade-core: deterministic, I/O-free battle core for
//! "Deus Vult: Champions of the Cross". Compiles to wasm32-unknown-unknown.
//!
//! Determinism contract: all randomness flows through [`rng::BattleRng`],
//! seeded per battle from the kirk commit-reveal protocol. No wall-clock,
//! no HashMap iteration affecting logic, no floats (percent u8 math).

pub mod types;
pub mod rng;
pub mod data;
pub mod engine;

pub use types::*;

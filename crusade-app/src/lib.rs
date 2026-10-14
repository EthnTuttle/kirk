//! crusade-app: frontend-agnostic client core (M5a).
//!
//! A UI (Bevy, egui, web bindings...) drives ONE `ClientGame`. It reads
//! [`ViewSnapshot`]s for rendering and feeds [`ClientInput`]s. No async
//! leaks into the view surface; network duels advance via explicit poll
//! calls so both native and wasm event loops stay simple.
//!
//! Phases: Menu -> Roster -> BattleSetup (vs AI practice, vs net peer)
//!   -> InBattle -> BattleOver -> Menu.

pub mod client;
pub mod policy;
pub mod view;

pub use client::ClientGame;
pub use view::*;

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AppError {
    #[error("store: {0}")]
    Store(#[from] crusade_data::StoreError),
    #[error("engine: {0}")]
    Engine(#[from] crusade_core::engine::BattleError),
    #[error("net: {0}")]
    Net(#[from] crusade_net::NetError),
    #[error("illegal ui transition: {0}")]
    IllegalTransition(String),
}

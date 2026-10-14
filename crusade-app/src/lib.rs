//! crusade-app: frontend-agnostic client core (M5a + M6 campaign & sync).
//!
//! A UI (Bevy, egui, web bindings...) drives ONE `ClientGame`. It reads
//! [`ViewSnapshot`]s for rendering and feeds [`ClientInput`]s. No async
//! leaks into the view surface; network duels advance via explicit poll
//! calls so both native and wasm event loops stay simple.
//!
//! Phases: Menu -> Roster -> BattleSetup (vs AI practice, vs net peer)
//!   -> InBattle -> BattleOver -> Menu.
//! Campaign (PvE gauntlet): Menu -> StartCampaign -> [InBattle x node,
//!   CampaignRest between nodes] -> BattleOver (clear or wipe) -> Menu.
//!   See [`view::CampaignView`].
//!
//! Network PvP (M6 wire) deliberately lives OUTSIDE [`ClientGame`]: the
//! client is a synchronous state machine while a `crusade_net::Transport`
//! is async, so `ClientGame` itself holds no net state (no lobby phase, no
//! net inputs). The UI's async task owns a [`pvp::NetDuelDriver`] instead
//! and composes it with the `ClientGame` it drives for roster/menus. The
//! driver renders via its own `snapshot()` into the same in-battle
//! [`ViewSnapshot`] shapes, so one UI renderer serves practice, campaign,
//! and net duels unchanged.

pub mod client;
pub mod lobby;
pub mod persist;
pub mod policy;
pub mod pvp;
pub mod sync;
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
    #[error("bridge: {0}")]
    Bridge(#[from] crusade_bridge::BridgeError),
    #[error("illegal ui transition: {0}")]
    IllegalTransition(String),
}

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
//! Tournament of Virtue (PvP): Menu -> Lobby (list / post / accept
//!   challenges over nostr relays) -> BattleSetup (the net duel,
//!   bridge-rendered) -> BattleOver -> Menu.
//! Lore: Menu -> Hagiography (the witness roll of the revered dead and the
//!   Manuscript of Sins bestiary of defeated gauntlet bosses, read-only)
//!   -> Menu.
//!
//! Network PvP (M6 wire) deliberately keeps all async handles OUT of
//! [`ClientGame`]: the client is a synchronous state machine while a
//! `crusade_net::Transport` is async. The Tournament of Virtue therefore
//! splits work: `ClientGame` owns the lobby's sync view-state (`Lobby`,
//! `LobbyView`, `LobbyNet` reports in — everything renderable) and the
//! shell owns ONE background async task (relay session +
//! [`pvp::NetDuelDriver`]) which renders via the driver's own `snapshot()`
//! into the same in-battle [`ViewSnapshot`] shapes. One renderer serves
//! practice, campaign, and net duels unchanged.

pub mod client;
pub mod conversion;
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

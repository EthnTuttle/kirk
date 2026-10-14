//! Transport trait: an established bidirectional link to ONE peer for ONE
//! duel. Discovery/pairing lives in kirk's nostr events and (M2b) iroh.

use async_trait::async_trait;

use crate::msg::NetMsg;

/// Threading bound for [`Transport`] implementations: `Send + Sync` on
/// native targets (multi-threaded executors), unbounded on wasm32 where the
/// runtime is single-threaded and `nostr_sdk::Client` is `!Send`/`!Sync`.
#[cfg(not(target_arch = "wasm32"))]
pub trait ThreadBound: Send + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync> ThreadBound for T {}
#[cfg(target_arch = "wasm32")]
pub trait ThreadBound {}
#[cfg(target_arch = "wasm32")]
impl<T> ThreadBound for T {}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait Transport: ThreadBound {
    async fn send(&self, msg: &NetMsg) -> Result<(), crate::NetError>;
    /// Next message from the peer; Err(Closed) when the link is dead.
    async fn recv(&self) -> Result<NetMsg, crate::NetError>;
}

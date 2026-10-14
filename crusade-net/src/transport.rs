//! Transport trait: an established bidirectional link to ONE peer for ONE
//! duel. Discovery/pairing lives in kirk's nostr events and (M2b) iroh.

use async_trait::async_trait;

use crate::msg::NetMsg;

#[async_trait]
pub trait Transport: Send + Sync {
    async fn send(&self, msg: &NetMsg) -> Result<(), crate::NetError>;
    /// Next message from the peer; Err(Closed) when the link is dead.
    async fn recv(&self) -> Result<NetMsg, crate::NetError>;
}

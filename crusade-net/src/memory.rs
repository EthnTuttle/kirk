//! In-memory duplex Transport for tests and local play (worker C fills).

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::msg::NetMsg;
use crate::transport::Transport;
use crate::NetError;

pub struct MemoryTransport {
    tx: mpsc::UnboundedSender<NetMsg>,
    rx: tokio::sync::Mutex<mpsc::UnboundedReceiver<NetMsg>>,
}

/// A connected pair. Worker C implements.
pub fn memory_pair() -> (MemoryTransport, MemoryTransport) {
    let (a_tx, a_rx) = mpsc::unbounded_channel();
    let (b_tx, b_rx) = mpsc::unbounded_channel();
    (
        MemoryTransport {
            tx: a_tx,
            rx: tokio::sync::Mutex::new(b_rx),
        },
        MemoryTransport {
            tx: b_tx,
            rx: tokio::sync::Mutex::new(a_rx),
        },
    )
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Transport for MemoryTransport {
    async fn send(&self, msg: &NetMsg) -> Result<(), NetError> {
        self.tx.send(msg.clone()).map_err(|_| NetError::Closed)
    }
    async fn recv(&self) -> Result<NetMsg, NetError> {
        self.rx.lock().await.recv().await.ok_or(NetError::Closed)
    }
}

//! iroh QUIC transport (native builds only, feature `iroh-quic`).
//!
//! Framing: one persistent bidirectional QUIC stream per duel; every NetMsg
//! is one length-prefixed frame on it: u32 BE length + serde_json payload.
//! A persistent stream is used instead of one stream per message so stream
//! IDs double as implicit message ordering and no FIN bookkeeping is needed.
//! ALPN: b"/deus-vult/duel/0". Frames larger than [`MAX_FRAME`] are rejected
//! on read before allocation.
//!
//! Tickets: iroh 1.3 has no built-in ticket type (the 0.x `NodeTicket` moved
//! to the separate `iroh-tickets` crate, which is not a dependency here), so
//! we use a minimal self-contained encoding: hex(serde_json(EndpointAddr)).
//!
//! Binding uses the n0 preset (default relay servers + n0 DNS address
//! lookup). Those services run in the background, so `bind` only needs local
//! UDP sockets and works offline / on loopback-only hosts. Without relay
//! connectivity the ticket carries direct IP addresses only, which is enough
//! for 127.0.0.1 duels. If you need internet-dialable tickets, wait for
//! [`iroh::Endpoint::online`] before calling [`DuelEndpoint::node_ticket`].

use async_trait::async_trait;
use iroh::endpoint::{presets, Connection, RecvStream, SendStream};
use tokio::sync::Mutex;

use crate::msg::{netmsg_from_json, netmsg_to_json, NetMsg};
use crate::transport::Transport;
use crate::NetError;

pub const DUEL_ALPN: &[u8] = b"/deus-vult/duel/0";
pub const MAX_FRAME: u32 = 1 << 16;

/// Truncate an iroh/n0 stack error's Display into a short protocol string.
fn short(e: impl std::fmt::Display) -> String {
    let s = e.to_string();
    if s.chars().count() > 120 {
        let mut t: String = s.chars().take(120).collect();
        t.push('…');
        t
    } else {
        s
    }
}

/// An iroh endpoint bound for duels. One endpoint can serve many duels.
pub struct DuelEndpoint {
    endpoint: iroh::Endpoint,
}

impl DuelEndpoint {
    /// Bind with default relay + discovery (n0).
    ///
    /// Discovery publishes/resolves and relay connections all happen in the
    /// background after bind; their failure never fails `bind`, so this also
    /// works fully offline as long as a UDP socket can be bound (loopback-
    /// only hosts included).
    pub async fn bind() -> Result<Self, NetError> {
        let endpoint = iroh::Endpoint::builder(presets::N0)
            .alpns(vec![DUEL_ALPN.to_vec()])
            .bind()
            .await
            .map_err(|e| NetError::Protocol(format!("iroh bind: {}", short(e))))?;
        Ok(Self { endpoint })
    }

    /// String ticket to hand to the opponent out-of-band (nostr challenge).
    ///
    /// Waits briefly for the freshly bound sockets to report at least one
    /// address so the ticket is immediately dialable.
    pub async fn node_ticket(&self) -> Result<String, NetError> {
        let mut addr = self.endpoint.addr();
        for _ in 0..20 {
            if !addr.addrs.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            addr = self.endpoint.addr();
        }
        if addr.addrs.is_empty() {
            return Err(NetError::Protocol("endpoint has no addresses".into()));
        }
        let json = serde_json::to_string(&addr).map_err(|e| NetError::Serde(e.to_string()))?;
        Ok(hex_encode(json.as_bytes()))
    }

    /// Dial a peer from their ticket; returns a live duel Transport.
    pub async fn connect(&self, ticket: &str) -> Result<IrohTransport, NetError> {
        let bad = || NetError::Protocol("bad ticket".into());
        let bytes = hex_decode(ticket.trim()).ok_or_else(bad)?;
        let addr: iroh::EndpointAddr = serde_json::from_slice(&bytes).map_err(|_| bad())?;
        let conn = self
            .endpoint
            .connect(addr, DUEL_ALPN)
            .await
            .map_err(|_| NetError::Closed)?;
        // The dialing side opens the single duel stream.
        let (send, recv) = conn.open_bi().await.map_err(|_| NetError::Closed)?;
        Ok(IrohTransport::new(conn, send, recv))
    }

    /// Accept the next inbound duel connection.
    pub async fn accept(&self) -> Result<IrohTransport, NetError> {
        let incoming = self.endpoint.accept().await.ok_or(NetError::Closed)?;
        let conn = incoming.await.map_err(|_| NetError::Closed)?;
        let (send, recv) = conn.accept_bi().await.map_err(|_| NetError::Closed)?;
        Ok(IrohTransport::new(conn, send, recv))
    }
}

pub struct IrohTransport {
    send: Mutex<SendStream>,
    recv: Mutex<RecvStream>,
    /// Kept so the connection (and its streams) stays alive for the duel.
    _conn: Connection,
}

impl IrohTransport {
    fn new(conn: Connection, send: SendStream, recv: RecvStream) -> Self {
        Self {
            send: Mutex::new(send),
            recv: Mutex::new(recv),
            _conn: conn,
        }
    }
}

#[async_trait]
impl Transport for IrohTransport {
    async fn send(&self, msg: &NetMsg) -> Result<(), NetError> {
        let payload = netmsg_to_json(msg)?;
        let len = u32::try_from(payload.len())
            .map_err(|_| NetError::Protocol("frame too large".into()))?;
        if len > MAX_FRAME {
            return Err(NetError::Protocol("frame too large".into()));
        }
        let mut tx = self.send.lock().await;
        tx.write_all(&len.to_be_bytes())
            .await
            .map_err(|_| NetError::Closed)?;
        tx.write_all(&payload).await.map_err(|_| NetError::Closed)?;
        Ok(())
    }

    async fn recv(&self) -> Result<NetMsg, NetError> {
        let mut rx = self.recv.lock().await;
        let mut hdr = [0u8; 4];
        rx.read_exact(&mut hdr)
            .await
            .map_err(|_| NetError::Closed)?;
        let len = u32::from_be_bytes(hdr);
        if len > MAX_FRAME {
            return Err(NetError::Protocol("frame too large".into()));
        }
        let mut buf = vec![0u8; len as usize];
        rx.read_exact(&mut buf)
            .await
            .map_err(|_| NetError::Closed)?;
        netmsg_from_json(&buf)
    }
}

// --- ticket hex encoding (self-contained; no extra deps) ---

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            b'A'..=b'F' => Some(c - b'A' + 10),
            _ => None,
        }
    }
    let s = s.as_bytes();
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    for pair in s.chunks_exact(2) {
        out.push((val(pair[0])? << 4) | val(pair[1])?);
    }
    Some(out)
}

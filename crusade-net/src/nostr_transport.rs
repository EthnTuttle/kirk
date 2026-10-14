//! Nostr relay fallback Transport — the wasm-safe path (websockets only).
//! One nostr event per NetMsg: kind 9264, NIP-44-encrypted JSON content,
//! d-tag = battle id hex, p-tag = peer pubkey. Slow (~1-30s latency);
//! production clients upgrade to iroh when both sides support it.
//!
//! Ordering: nostr relays are eventually consistent, so `recv` awaits each
//! message with a generous cumulative timeout instead of assuming immediate
//! delivery. Relays may redeliver events, so duplicates are dropped ("first
//! matching unseen event wins"); the duel layer enforces turn ordering.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use nostr::nips::nip44::{self, Version};
use nostr::prelude::*;
use nostr_sdk::async_utility::time;
use nostr_sdk::{Client, RelayPoolNotification, SubscriptionId};
use tokio::sync::{broadcast, Mutex};

use crate::msg::{netmsg_from_json, netmsg_to_json, NetMsg};
use crate::transport::Transport;
use crate::NetError;

/// Custom kind for duel messages (contiguous with kirk's 9259..=9263).
pub const DUEL_KIND: u16 = 9264;

/// How long `recv` waits for the next message before giving up. Nostr
/// relay latency is unbounded in the worst case; 30s is the production cap.
const RECV_TIMEOUT: Duration = Duration::from_secs(30);
/// How many times the notifications stream is re-created when it closes
/// (e.g. relay pool reconnect) before the transport reports itself dead.
const MAX_STREAM_RETRIES: usize = 3;

/// Lazily materialized receive path: ONE relay subscription per transport.
/// The `broadcast::Receiver` is created BEFORE the REQ so no matching event
/// can land while the subscription is being registered. `seen` persists
/// across recv calls so relay-redelivered events never resurface.
struct RecvState {
    sub_id: SubscriptionId,
    rx: broadcast::Receiver<RelayPoolNotification>,
    seen: HashSet<EventId>,
}

pub struct NostrTransport {
    client: Client,
    my_pubkey: PublicKey,
    secret_key: SecretKey,
    peer: PublicKey,
    battle_id: [u8; 32],
    created_at: Timestamp,
    recv_state: Mutex<Option<RecvState>>,
}

impl NostrTransport {
    /// Build over an existing configured client (relays already added by
    /// the host app; crusade-app owns relay config). `keys` is the local
    /// identity (signs outgoing events, decrypts incoming ones), `peer` the
    /// opponent's nostr pubkey, `battle_id` the duel-scoping token.
    pub async fn new(
        client: Client,
        keys: Keys,
        peer: PublicKey,
        battle_id: [u8; 32],
    ) -> Result<Self, NetError> {
        let my_pubkey = keys.public_key();
        if peer == my_pubkey {
            return Err(NetError::Protocol(
                "nostr peer pubkey must differ from our own".into(),
            ));
        }
        Ok(Self {
            client,
            my_pubkey,
            secret_key: keys.secret_key().clone(),
            peer,
            battle_id,
            created_at: Timestamp::now(),
            recv_state: Mutex::new(None),
        })
    }
}

/// Hex string for the d-tag (`hex` crate isn't a dependency).
fn to_hex(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// The relay-side view of our inbox: kind-9264 events authored by `peer`,
/// p-tagged to us, d-tagged with this battle.
fn filter_for(battle_id: &[u8; 32], my_pubkey: PublicKey, peer: PublicKey) -> Filter {
    Filter::new()
        .kind(Kind::from_u16(DUEL_KIND))
        .pubkey(peer)
        .custom_tag(
            SingleLetterTag::lowercase(Alphabet::P),
            [my_pubkey.to_hex()],
        )
        .identifier(to_hex(battle_id))
}

/// JSON-encode `msg`, NIP-44-encrypt to `peer`, stamp kind/d-tag/p-tag.
/// Pure: the crypto+serde path is testable without a relay.
fn encode(
    msg: &NetMsg,
    secret_key: &SecretKey,
    peer: PublicKey,
    battle_id: &[u8; 32],
) -> Result<EventBuilder, NetError> {
    let json = netmsg_to_json(msg)?;
    let content = nip44::encrypt(secret_key, &peer, &json, Version::V2)
        .map_err(|e| NetError::Protocol(format!("NIP-44 encrypt failed: {e}")))?;
    Ok(EventBuilder::new(
        Kind::from_u16(DUEL_KIND),
        content,
        [Tag::identifier(to_hex(battle_id)), Tag::public_key(peer)],
    ))
}

/// Decode a NetMsg from a kind-9264 event addressed to `my_pubkey`.
/// Structural/decrypt failures are `Protocol` (callers skip or surface);
/// serde failures pass through as `Serde`.
fn decode_event(
    event: &Event,
    secret_key: &SecretKey,
    my_pubkey: PublicKey,
    expected_battle: &[u8; 32],
) -> Result<NetMsg, NetError> {
    if event.kind != Kind::from_u16(DUEL_KIND) {
        return Err(NetError::Protocol(format!(
            "unexpected event kind {}",
            event.kind.as_u16()
        )));
    }
    let d_tag = event
        .tags
        .iter()
        .find(|t| t.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::D)))
        .and_then(|t| t.content())
        .ok_or_else(|| NetError::Protocol("missing d-tag".into()))?;
    if d_tag != to_hex(expected_battle) {
        return Err(NetError::Protocol("wrong battle id".into()));
    }
    let p_tag = event
        .tags
        .iter()
        .find(|t| t.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::P)))
        .and_then(|t| t.content())
        .ok_or_else(|| NetError::Protocol("missing p-tag".into()))?;
    if p_tag != my_pubkey.to_hex() {
        return Err(NetError::Protocol("event not addressed to us".into()));
    }
    let json = nip44::decrypt(secret_key, &event.pubkey, &event.content)
        .map_err(|e| NetError::Protocol(format!("NIP-44 decrypt failed: {e}")))?;
    netmsg_from_json(json.as_bytes())
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Transport for NostrTransport {
    async fn send(&self, msg: &NetMsg) -> Result<(), NetError> {
        let builder = encode(msg, &self.secret_key, self.peer, &self.battle_id)?;
        let output = self
            .client
            .send_event_builder(builder)
            .await
            .map_err(|e| NetError::Protocol(format!("nostr send failed: {e}")))?;
        if output.success.is_empty() {
            // No relay acknowledged the event (e.g. all connections down or
            // every relay rejected it): the link is unusable.
            return Err(NetError::Closed);
        }
        Ok(())
    }

    async fn recv(&self) -> Result<NetMsg, NetError> {
        let mut guard = self.recv_state.lock().await;
        if guard.is_none() {
            // Retry the initial REQ a few times: the client may still be
            // warming up its relay connections.
            let mut last_err = None;
            for _ in 0..=MAX_STREAM_RETRIES {
                let rx = self.client.notifications();
                let filter =
                    filter_for(&self.battle_id, self.my_pubkey, self.peer).since(self.created_at);
                match self.client.subscribe(vec![filter], None).await {
                    Ok(output) => {
                        *guard = Some(RecvState {
                            sub_id: output.val,
                            rx,
                            seen: HashSet::new(),
                        });
                        break;
                    }
                    Err(e) => {
                        last_err = Some(format!("nostr subscribe failed: {e}"));
                        time::timeout(Some(Duration::from_millis(250)), async {
                            tokio::task::yield_now().await;
                        })
                        .await;
                    }
                }
            }
            if guard.is_none() {
                return Err(NetError::Protocol(
                    last_err.unwrap_or_else(|| "nostr subscribe failed".into()),
                ));
            }
        }
        let st = guard.as_mut().ok_or(NetError::Closed)?;

        let deadline = Instant::now() + RECV_TIMEOUT;
        let mut stream_retries = 0usize;
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(NetError::Closed);
            }
            let remaining = deadline.saturating_duration_since(now);
            match time::timeout(Some(remaining), st.rx.recv()).await {
                // Permanently no peer message within the window: treat as a
                // dead link.
                None => return Err(NetError::Closed),
                // Missed notifications break the turn protocol; refuse to
                // guess what was lost.
                Some(Err(broadcast::error::RecvError::Lagged(_))) => {
                    return Err(NetError::Protocol(
                        "nostr notification channel lagged (events lost)".into(),
                    ));
                }
                Some(Err(broadcast::error::RecvError::Closed)) => {
                    stream_retries += 1;
                    if stream_retries > MAX_STREAM_RETRIES {
                        return Err(NetError::Closed);
                    }
                    st.rx = self.client.notifications();
                }
                Some(Ok(RelayPoolNotification::Event {
                    subscription_id,
                    event,
                    ..
                })) => {
                    if subscription_id != st.sub_id || event.pubkey != self.peer {
                        continue;
                    }
                    if !st.seen.insert(event.id) {
                        continue;
                    }
                    match decode_event(&event, &self.secret_key, self.my_pubkey, &self.battle_id) {
                        Ok(msg) => return Ok(msg),
                        // Well-formed delivery that isn't ours (e.g. stale
                        // backlog or spoofed d-tag): skip, keep awaiting.
                        Err(NetError::Protocol(_)) => continue,
                        Err(e) => return Err(e),
                    }
                }
                Some(Ok(_)) => continue,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::msg::commit_hash;
    use crusade_core::engine::Action;

    /// Relay-free fallback gate: the exact wire shape (kind/d-tag/p-tag/
    /// NIP-44 ciphertext) round-trips through decode_event using two
    /// keypairs, no network involved.
    #[test]
    fn encode_decode_roundtrip_pure() {
        let alice = Keys::generate();
        let bob = Keys::generate();
        let battle_id = [7u8; 32];
        let msg = NetMsg::Commit {
            turn: 3,
            hash: commit_hash(&Action::Rite { slot: 1 }),
        };
        let builder = encode(&msg, alice.secret_key(), bob.public_key(), &battle_id).unwrap();
        let event = builder.to_event(&alice).unwrap();
        assert_eq!(event.kind.as_u16(), DUEL_KIND);

        let got = decode_event(&event, bob.secret_key(), bob.public_key(), &battle_id).unwrap();
        assert_eq!(got, msg);

        // A different battle id must reject the same ciphertext.
        let other = decode_event(&event, bob.secret_key(), bob.public_key(), &[8u8; 32]);
        assert!(matches!(other, Err(NetError::Protocol(_))));

        // Decrypting to a third party fails (NIP-44 XChaCha20 key mismatch).
        let eve = Keys::generate();
        let wrong = decode_event(&event, eve.secret_key(), eve.public_key(), &battle_id);
        assert!(wrong.is_err());
    }
}

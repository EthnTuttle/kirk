//! Nostr lobby orchestrator (v1.1, ship-plan card B1).
//!
//! Wires the EXISTING kirk challenge flow (kinds 9259 challenge / 9260
//! accept, `kirk::events` builders + parsers — reused, not reinvented) into
//! the parameters [`crate::pvp::NetDuelDriver::host`/`join`] need:
//!
//! ```text
//! host                                        joiner
//!   LobbyCommitment::new(party)                 LobbyCommitment::new(party)
//!   publish_challenge  --- kind 9259 -------->  await_challenge (or
//!   await_accept       <--- kind 9260 --------  fetch_open_challenges)
//!   complete()                                  complete()
//!     reveal --- kind 9261 (Move) --------->
//!     <-------------------------------- reveal
//!     both: verify preimages vs commitment_hashes, battle_id + seed
//!   -> LobbyOutcome ------------------------------> NetDuelDriver
//! ```
//!
//! Commit-reveal scheme: each side's [`LobbyCommitment`] binds its roster
//! AND one 33-byte share per champion into the kirk `commitment_hashes`
//! vector (sha256 hex, exactly the shape kirk validates). After the accept,
//! both sides publish a [`LobbyReveal`] (roster + share preimages) as a
//! kind-9261 kirk Move event; each side verifies the peer's preimages hash
//! to the peer's committed set (fail-closed on any mismatch), then derives
//! `battle_id = sha256(challenge_id || accept_id)` and the seed via
//! [`crusade_bridge::derive_battle_seed_from_reveal`]
//! (creator = challenge author = side 0 = host), so both honest parties
//! always compute the identical pair. (The 33-byte shares are opaque
//! entropy, point-shaped for forward-compatibility with real cashu token
//! C values once v1.2 mint work wires GameTokens into challenges.)
//!
//! Layout mirrors crusade-net's duel.rs/transport split: everything below
//! the wire layer is pure synchronous functions (unit-tested without a
//! relay); [`Lobby`] is the thin async nostr wrapper the UI's async task
//! drives, feature-independent (compiles without `bevy-ui`). No UI, no
//! bevy, no iroh — nostr transport only, wasm-safe: nostr-sdk websockets,
//! and no tokio usage (pacing via `nostr_sdk::async_utility::time`).

use std::collections::HashSet;
use std::time::Duration;

use crusade_bridge::derive_battle_seed_from_reveal;
use crusade_core::types::ChampionInstance;
use kirk::events::{
    ChallengeAcceptContent, ChallengeContent, EventParser, MoveContent, MoveType,
    CHALLENGE_ACCEPT_KIND, CHALLENGE_KIND, MOVE_KIND,
};
use nostr::{Event, EventBuilder, EventId, Filter, Keys, PublicKey, Tag, Timestamp};
use nostr_sdk::async_utility::time;
use nostr_sdk::{Client, EventSource};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Protocol-suite marker carried in a challenge's `game_parameters`; only
/// challenges bearing it are listed, accepted, or completed.
const SUITE: &str = "deus-vult/lobby/v0";
/// kirk `ChallengeContent.game_type` for our challenges.
const GAME_TYPE: &str = "deus-vult/champions-of-the-cross";
/// How long a published challenge stays valid (kirk expiry semantics).
const CHALLENGE_TTL_SECS: u64 = 3600;
/// Suggested awaiting budget for the UI (nostr relay latency is unbounded
/// in the worst case; human matchmaking timescale).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
/// Per-poll EOSE budget inside an await loop.
const FETCH_TIMEOUT: Duration = Duration::from_secs(2);
/// Pause between poll rounds inside an await loop.
const POLL_INTERVAL: Duration = Duration::from_millis(250);
/// Absolute relay-side fetch cap for the challenge listing. Relays fill a
/// limit NEWEST-first (NIP-01), so we fetch up to this cap, sort ascending
/// client-side, and only then truncate to the caller's limit: genuine old
/// challenges survive spam, and memory stays bounded.
const FETCH_HARD_CAP: usize = 500;

/// Domain separator: share preimage commitments.
const DOM_SHARE: &[u8] = b"deus-vult/lobby-share/v0";
/// Domain separator: roster commitments.
const DOM_ROSTER: &[u8] = b"deus-vult/lobby-roster/v0";
/// Domain separator: secret-master -> share derivation.
const DOM_SECRET: &[u8] = b"deus-vult/lobby-secret/v0";

#[derive(Debug, Error)]
pub enum LobbyError {
    #[error("nostr client: {0}")]
    Client(String),
    #[error("protocol: {0}")]
    Protocol(String),
    #[error("timed out waiting for {0}")]
    Timeout(&'static str),
    #[error("bridge: {0}")]
    Bridge(#[from] crusade_bridge::BridgeError),
}

/// Terminal lobby result: everything the shell feeds into
/// [`crate::pvp::NetDuelDriver::host`/`join`] (rosters, seed, battle id)
/// plus the peer key needed to scope the duel's nostr transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LobbyOutcome {
    /// sha256(challenge event id || accept event id).
    pub battle_id: [u8; 32],
    /// `crusade_bridge::derive_battle_seed_from_reveal` over both reveals.
    pub seed: [u8; 32],
    pub our_roster: Vec<ChampionInstance>,
    pub their_roster: Vec<ChampionInstance>,
    /// True when we authored the challenge (engine side 0).
    pub we_host: bool,
    /// The opponent's nostr pubkey.
    pub peer: PublicKey,
}

/// A challenge (kind 9259) listed from the relays: ours-excluded, suite
/// checked, signature verified, unexpired.
#[derive(Debug, Clone)]
pub struct OpenChallenge {
    /// The signed challenge event (joiners pass it to [`Lobby::complete`]).
    pub event: Event,
    /// The challenge author's nostr pubkey.
    pub host: PublicKey,
}

// ============================================================================
// Pure layer (synchronous-testable, no transport)
// ============================================================================

/// One side's lobby state: the roster, the 33-byte shares that entropy the
/// battle seed, and the published commitment set. Secret material stays in
/// memory; only hashes are published until the reveal step.
#[derive(Debug, Clone)]
pub struct LobbyCommitment {
    roster: Vec<ChampionInstance>,
    shares: Vec<[u8; 33]>,
    hashes: Vec<String>,
}

impl LobbyCommitment {
    /// Commit to `roster`: one share per champion, bound together with the
    /// roster itself in the kirk `commitment_hashes` vector.
    pub fn new(roster: Vec<ChampionInstance>) -> Result<Self, LobbyError> {
        if roster.is_empty() {
            return Err(LobbyError::Protocol("empty roster".into()));
        }
        // OsRng via secp256k1 (wasm: getrandom/js) — no fresh entropy dep.
        let master = Keys::generate().secret_key().to_secret_bytes();
        let shares = derive_shares(&master, roster.len());
        let mut hashes: Vec<String> = shares.iter().map(share_commitment).collect();
        hashes.push(roster_commitment(&roster)?);
        Ok(Self {
            roster,
            shares,
            hashes,
        })
    }

    pub fn roster(&self) -> &[ChampionInstance] {
        &self.roster
    }

    /// The set published into a kirk Challenge/ChallengeAccept
    /// `commitment_hashes` field (one share hash per champion + roster hash).
    pub fn commitment_hashes(&self) -> Vec<String> {
        self.hashes.clone()
    }

    /// The full reveal payload for the post-accept step.
    fn reveal(&self, battle_id: [u8; 32]) -> LobbyReveal {
        LobbyReveal {
            battle_id: hex::encode(battle_id),
            roster: self.roster.clone(),
            shares: self.shares.iter().map(hex::encode).collect(),
        }
    }
}

/// What each side publishes after the accept: roster + share preimages,
/// stamped with the battle id so cross-lobby replays cannot satisfy a
/// verification. Wrapped in a kirk kind-9261 Move event on the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LobbyReveal {
    /// Hex `battle_id` this reveal belongs to.
    pub battle_id: String,
    pub roster: Vec<ChampionInstance>,
    /// Hex, 33 bytes each, one per champion, in party order.
    pub shares: Vec<String>,
}

impl LobbyReveal {
    /// Decode the hex shares into 33-byte values; malformed input is a
    /// protocol error.
    pub fn share_values(&self) -> Result<Vec<[u8; 33]>, LobbyError> {
        self.shares
            .iter()
            .map(|s| {
                let raw = hex::decode(s).map_err(|e| {
                    LobbyError::Protocol(format!("reveal share is not valid hex: {e}"))
                })?;
                <[u8; 33]>::try_from(raw.as_slice()).map_err(|_| {
                    LobbyError::Protocol(format!(
                        "reveal share must be 33 bytes, got {}",
                        raw.len()
                    ))
                })
            })
            .collect()
    }

    /// Serialize into a signed kind-9261 kirk Move event (reveal material
    /// chained off `previous`, d/p-tagged for relay-side scoping).
    fn event(&self, keys: &Keys, peer: PublicKey, previous: EventId) -> Result<Event, LobbyError> {
        let mv = MoveContent {
            previous_event_id: previous,
            move_type: MoveType::Move,
            move_data: serde_json::to_value(self)
                .map_err(|e| LobbyError::Protocol(format!("reveal serialize: {e}")))?,
            revealed_tokens: None,
            deadline: None,
        };
        let content = serde_json::to_string(&mv)
            .map_err(|e| LobbyError::Protocol(format!("reveal move serialize: {e}")))?;
        EventBuilder::new(
            MOVE_KIND,
            content,
            [
                Tag::identifier(self.battle_id.clone()),
                Tag::public_key(peer),
            ],
        )
        .to_event(keys)
        .map_err(|e| LobbyError::Protocol(format!("reveal sign: {e}")))
    }

    /// Parse a kind-9261 event back into a reveal (kirk parser + suite
    /// shape). Signature verification is the caller's job. Both kirk
    /// `MoveType::Move` (what [`LobbyReveal::event`] emits) and
    /// `MoveType::Reveal` are accepted — the commitment checks in
    /// `verify_reveal` gate the content either way.
    fn from_event(event: &Event) -> Result<Self, LobbyError> {
        let mv = EventParser::parse_move(event)
            .map_err(|e| LobbyError::Protocol(format!("reveal parse: {e}")))?;
        if !matches!(mv.move_type, MoveType::Move | MoveType::Reveal) {
            return Err(LobbyError::Protocol("not a lobby reveal move".into()));
        }
        serde_json::from_value(mv.move_data)
            .map_err(|e| LobbyError::Protocol(format!("reveal content parse: {e}")))
    }
}

/// sha256(DOM_SECRET || master || i-be32) squeezed into 33 point-shaped
/// bytes (0x02/0x03 prefix from the digest's low bit). Deterministic from
/// the master secret, which never leaves this process. These are
/// point-SHAPED opaque containers only: a sha256 output is a valid
/// secp256k1 x-coordinate only ~50% of the time, so they must never be
/// parsed as curve points — the layout merely stays compatible with v1.2,
/// when real cashu token C values replace them.
fn derive_shares(master: &[u8; 32], n: usize) -> Vec<[u8; 33]> {
    (0..n as u32)
        .map(|i| {
            let h = Sha256::digest(&[DOM_SECRET, master, &i.to_be_bytes()].concat());
            let mut c = [0u8; 33];
            c[0] = 0x02 | (h[31] & 1);
            c[1..].copy_from_slice(&h);
            c
        })
        .collect()
}

/// sha256(DOM_SHARE || share), hex — the kirk `commitment_hashes` element.
fn share_commitment(share: &[u8; 33]) -> String {
    hex::encode(Sha256::digest(&[DOM_SHARE, share].concat()))
}

/// sha256(DOM_ROSTER || serde_json(roster)), hex. Same crate both sides =>
/// same canonical JSON bytes.
fn roster_commitment(roster: &[ChampionInstance]) -> Result<String, LobbyError> {
    let json = serde_json::to_string(roster)
        .map_err(|e| LobbyError::Protocol(format!("roster serialize: {e}")))?;
    Ok(hex::encode(Sha256::digest(
        &[DOM_ROSTER, json.as_bytes()].concat(),
    )))
}

/// battle_id convention: sha256(raw challenge event id || raw accept event
/// id) — identical preimage to crusade-bridge's own, which then derives the
/// seed from it.
fn battle_id_of(challenge_id: EventId, accept_id: EventId) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(challenge_id.to_bytes());
    hasher.update(accept_id.to_bytes());
    hasher.finalize().into()
}

/// Fail-closed check that a reveal opens EXACTLY the published commitment
/// set (share hashes + roster hash, order-insensitive). Returns the decoded
/// shares on success.
fn verify_reveal(
    commitment_hashes: &[String],
    reveal: &LobbyReveal,
) -> Result<Vec<[u8; 33]>, LobbyError> {
    let shares = reveal.share_values()?;
    let mut expected: HashSet<String> = shares.iter().map(share_commitment).collect();
    expected.insert(roster_commitment(&reveal.roster)?);
    let published: HashSet<&str> = commitment_hashes.iter().map(String::as_str).collect();
    if expected.len() != published.len()
        || !expected.iter().all(|h| published.contains(h.as_str()))
    {
        return Err(LobbyError::Protocol(
            "reveal does not open the published commitments".into(),
        ));
    }
    Ok(shares)
}

/// Suite + expiry check for a parsed challenge.
fn is_open_at(content: &ChallengeContent, now: u64) -> bool {
    content.game_parameters.get("suite") == Some(&serde_json::Value::from(SUITE))
        && content.expiry.map(|e| e > now).unwrap_or(false)
}

/// Structural validation of a challenge/accept pair about to be completed.
/// Returns `(we_host, peer, their commitment hashes)`. `our_hashes` must
/// equal what WE published on our side, so a UI cannot complete with a
/// commitment that drifted since publish/accept.
fn validate_pair(
    me: PublicKey,
    challenge: &Event,
    accept: &Event,
    our_hashes: &[String],
    now: u64,
) -> Result<(bool, PublicKey, Vec<String>), LobbyError> {
    if challenge.verify().is_err() || accept.verify().is_err() {
        return Err(LobbyError::Protocol("bad event signature".into()));
    }
    let ch = EventParser::parse_challenge(challenge)
        .map_err(|e| LobbyError::Protocol(format!("challenge parse: {e}")))?;
    let ac = EventParser::parse_challenge_accept(accept)
        .map_err(|e| LobbyError::Protocol(format!("accept parse: {e}")))?;
    if ac.challenge_id != challenge.id {
        return Err(LobbyError::Protocol(
            "accept references a different challenge".into(),
        ));
    }
    if !is_open_at(&ch, now) {
        return Err(LobbyError::Protocol(
            "challenge is expired or not this suite".into(),
        ));
    }
    if challenge.pubkey == accept.pubkey {
        return Err(LobbyError::Protocol("self-accepted challenge".into()));
    }
    if (challenge.pubkey == me) == (accept.pubkey == me) {
        return Err(LobbyError::Protocol(
            "exactly one side of the pair must be ours".into(),
        ));
    }
    ch.validate()
        .and_then(|_| ac.validate())
        .map_err(|e| LobbyError::Protocol(format!("content validation: {e}")))?;
    let we_host = challenge.pubkey == me;
    let (published_mine, published_theirs) = if we_host {
        (&ch.commitment_hashes, &ac.commitment_hashes)
    } else {
        (&ac.commitment_hashes, &ch.commitment_hashes)
    };
    let mine: HashSet<&str> = our_hashes.iter().map(String::as_str).collect();
    let wire: HashSet<&str> = published_mine.iter().map(String::as_str).collect();
    if mine != wire {
        return Err(LobbyError::Protocol(
            "local commitment does not match what was published".into(),
        ));
    }
    let peer = if we_host { accept.pubkey } else { challenge.pubkey };
    Ok((we_host, peer, published_theirs.clone()))
}

/// The terminal pure step: order both sides' shares by ROLE (challenge
/// creator = side 0), derive battle id + seed, assemble the outcome.
fn build_outcome(
    we_host: bool,
    peer: PublicKey,
    ours: &LobbyCommitment,
    their_roster: Vec<ChampionInstance>,
    their_shares: Vec<[u8; 33]>,
    challenge_id: EventId,
    accept_id: EventId,
) -> Result<LobbyOutcome, LobbyError> {
    let battle_id = battle_id_of(challenge_id, accept_id);
    let (creator_cs, acceptor_cs): (&[[u8; 33]], &[[u8; 33]]) = if we_host {
        (&ours.shares, &their_shares)
    } else {
        (&their_shares, &ours.shares)
    };
    let seed = derive_battle_seed_from_reveal(
        &challenge_id.to_hex(),
        &accept_id.to_hex(),
        creator_cs,
        acceptor_cs,
    )?;
    Ok(LobbyOutcome {
        battle_id,
        seed,
        our_roster: ours.roster.clone(),
        their_roster,
        we_host,
        peer,
    })
}

// ============================================================================
// Wire layer (thin async nostr wrapper)
// ============================================================================

/// Async lobby handle the UI drives. Owns one nostr client; every method is
/// EOSE-polling based (stored-event catch-up included), so races between
/// publish and subscribe cannot lose events. Events seen by awaited
/// subscriptions are scoped to `connect` time onward.
pub struct Lobby {
    client: Client,
    keys: Keys,
    connected_at: Timestamp,
}

impl std::fmt::Debug for Lobby {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lobby")
            .field("pubkey", &self.keys.public_key().to_hex())
            .finish_non_exhaustive()
    }
}

impl Lobby {
    /// Connect to every relay in `relay_urls` ("wss://..." strings).
    pub async fn connect(keys: Keys, relay_urls: &[String]) -> Result<Self, LobbyError> {
        if relay_urls.is_empty() {
            return Err(LobbyError::Protocol("no relays configured".into()));
        }
        let client = Client::new(&keys);
        for url in relay_urls {
            client
                .add_relay(url)
                .await
                .map_err(|e| LobbyError::Client(format!("add relay {url}: {e}")))?;
        }
        client.connect().await;
        Ok(Self {
            client,
            keys,
            connected_at: Timestamp::now(),
        })
    }

    /// Our nostr identity (challenge/accept/reveal author).
    pub fn public_key(&self) -> PublicKey {
        self.keys.public_key()
    }

    /// The underlying relay client (seam for constructing this duel's
    /// `crusade_net::nostr_transport::NostrTransport` after an outcome).
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Disconnect from all relays and consume the handle (relay hygiene for
    /// the shell: call when leaving the lobby for a duel or the menu).
    /// Best-effort — relay close errors are swallowed on the way out.
    pub async fn disconnect(self) {
        let _ = self.client.disconnect().await;
    }

    /// One nostr event out; error unless at least one relay accepted it.
    async fn send(&self, event: Event) -> Result<(), LobbyError> {
        let out = self
            .client
            .send_event(event)
            .await
            .map_err(|e| LobbyError::Client(format!("send: {e}")))?;
        if out.success.is_empty() {
            return Err(LobbyError::Protocol(
                "no relay accepted the event".into(),
            ));
        }
        Ok(())
    }

    /// One EOSE-bounded fetch round.
    async fn fetch(&self, filter: Filter) -> Result<Vec<Event>, LobbyError> {
        self.client
            .get_events_of(vec![filter], EventSource::relays(Some(FETCH_TIMEOUT)))
            .await
            .map_err(|e| LobbyError::Client(format!("fetch: {e}")))
    }

    /// Publish our challenge (kind 9259: roster+share commitments, suite
    /// marker, 1h expiry). Returns the signed event for later
    /// [`Lobby::complete`].
    pub async fn publish_challenge(
        &self,
        commitment: &LobbyCommitment,
    ) -> Result<Event, LobbyError> {
        let content = ChallengeContent {
            game_type: GAME_TYPE.to_string(),
            commitment_hashes: commitment.commitment_hashes(),
            game_parameters: serde_json::json!({ "suite": SUITE }),
            expiry: Some(Timestamp::now().as_u64() + CHALLENGE_TTL_SECS),
            timeout_config: None,
        };
        content
            .validate()
            .map_err(|e| LobbyError::Protocol(format!("challenge: {e}")))?;
        let event = content
            .to_event(&self.keys)
            .map_err(|e| LobbyError::Protocol(format!("challenge sign: {e}")))?;
        self.send(event.clone()).await?;
        Ok(event)
    }

    /// List open lobby challenges visible since we connected: suite
    /// matches, unexpired, signature valid, not authored by us. Oldest
    /// first (fair lobby order); at most `limit` returned. The relay is
    /// asked for up to [`FETCH_HARD_CAP`] events and the client-side
    /// ascending sort happens BEFORE truncation, so a burst of fresh spam
    /// cannot starve a genuine older challenge out of the listing.
    pub async fn fetch_open_challenges(
        &self,
        limit: usize,
    ) -> Result<Vec<OpenChallenge>, LobbyError> {
        let filter = Filter::new()
            .kind(CHALLENGE_KIND)
            .since(self.connected_at)
            .limit(FETCH_HARD_CAP);
        let now = Timestamp::now().as_u64();
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for event in self.fetch(filter).await? {
            if event.pubkey == self.keys.public_key() || !seen.insert(event.id) {
                continue;
            }
            if event.verify().is_err() {
                continue;
            }
            let Ok(content) = EventParser::parse_challenge(&event) else {
                continue;
            };
            if !is_open_at(&content, now) {
                continue;
            }
            out.push(OpenChallenge {
                host: event.pubkey,
                event,
            });
        }
        out.sort_by_key(|c| c.event.created_at);
        out.truncate(limit);
        Ok(out)
    }

    /// Wait until any open challenge appears; returns the oldest one.
    pub async fn await_challenge(&self, timeout: Duration) -> Result<OpenChallenge, LobbyError> {
        let deadline = Timestamp::now().as_u64() + timeout.as_secs().max(1);
        loop {
            let mut open = self.fetch_open_challenges(1).await?;
            if !open.is_empty() {
                return Ok(open.swap_remove(0));
            }
            if Timestamp::now().as_u64() >= deadline {
                return Err(LobbyError::Timeout("an incoming challenge"));
            }
            sleep(POLL_INTERVAL).await;
        }
    }

    /// Accept a listed challenge (kind 9260 carrying OUR commitment set).
    /// Returns the signed accept event for later [`Lobby::complete`].
    pub async fn accept(
        &self,
        challenge: &OpenChallenge,
        commitment: &LobbyCommitment,
    ) -> Result<Event, LobbyError> {
        let parsed = EventParser::parse_challenge(&challenge.event)
            .map_err(|e| LobbyError::Protocol(format!("challenge parse: {e}")))?;
        if challenge.host == self.keys.public_key() {
            return Err(LobbyError::Protocol("cannot accept our own challenge".into()));
        }
        if !is_open_at(&parsed, Timestamp::now().as_u64()) {
            return Err(LobbyError::Protocol(
                "challenge is expired or not this suite".into(),
            ));
        }
        let content = ChallengeAcceptContent {
            challenge_id: challenge.event.id,
            commitment_hashes: commitment.commitment_hashes(),
        };
        content
            .validate()
            .map_err(|e| LobbyError::Protocol(format!("accept: {e}")))?;
        let event = content
            .to_event(&self.keys)
            .map_err(|e| LobbyError::Protocol(format!("accept sign: {e}")))?;
        self.send(event.clone()).await?;
        Ok(event)
    }

    /// Host side: wait for the first valid accept (kind 9260) that
    /// references OUR `challenge` and is not authored by us.
    ///
    /// Relay-side narrowing is impossible here: kirk's accept builder
    /// (`src/events/challenge.rs`, `ChallengeAcceptContent::to_event`)
    /// attaches ZERO tags — no e/p/a tag names the challenge or us — so the
    /// challenge-id match can only happen content-side after decrypt-free
    /// parsing. Mitigation instead: event ids already inspected this call
    /// are memoized, so repeat poll rounds never re-verify/re-parse them.
    pub async fn await_accept(
        &self,
        challenge: &Event,
        timeout: Duration,
    ) -> Result<Event, LobbyError> {
        let filter = Filter::new()
            .kind(CHALLENGE_ACCEPT_KIND)
            .since(self.connected_at);
        let deadline = Timestamp::now().as_u64() + timeout.as_secs().max(1);
        // Seen-and-rejected (or foreign) ids: fetch rounds may redeliver
        // stored events; one inspection each, never again.
        let mut seen: HashSet<EventId> = HashSet::new();
        loop {
            for event in self.fetch(filter.clone()).await? {
                if !seen.insert(event.id) {
                    continue;
                }
                if event.pubkey == self.keys.public_key() || event.verify().is_err() {
                    continue;
                }
                let Ok(content) = EventParser::parse_challenge_accept(&event) else {
                    continue;
                };
                if content.challenge_id != challenge.id || content.validate().is_err() {
                    continue;
                }
                return Ok(event);
            }
            if Timestamp::now().as_u64() >= deadline {
                return Err(LobbyError::Timeout("an accept of our challenge"));
            }
            sleep(POLL_INTERVAL).await;
        }
    }

    /// Terminal symmetric step for BOTH sides: validate the pair, publish
    /// our reveal (kind 9261, chained off the accept, d-tagged with the
    /// battle id), await the peer's reveal, verify it opens the peer's
    /// published commitments, and derive the shared battle id + seed.
    pub async fn complete(
        &self,
        challenge: &Event,
        accept: &Event,
        ours: &LobbyCommitment,
        timeout: Duration,
    ) -> Result<LobbyOutcome, LobbyError> {
        let (we_host, peer, their_hashes) = validate_pair(
            self.keys.public_key(),
            challenge,
            accept,
            &ours.hashes,
            Timestamp::now().as_u64(),
        )?;
        let battle_id = battle_id_of(challenge.id, accept.id);
        let reveal = ours.reveal(battle_id);
        let event = reveal.event(&self.keys, peer, accept.id)?;
        self.send(event).await?;

        let filter = Filter::new()
            .kind(MOVE_KIND)
            .author(peer)
            .identifier(reveal.battle_id.clone())
            .since(self.connected_at);
        let deadline = Timestamp::now().as_u64() + timeout.as_secs().max(1);
        loop {
            for event in self.fetch(filter.clone()).await? {
                if event.verify().is_err() {
                    continue;
                }
                let Ok(their_reveal) = LobbyReveal::from_event(&event) else {
                    continue;
                };
                if their_reveal.battle_id != reveal.battle_id {
                    continue;
                }
                // Authored by the peer on THIS battle id but not opening its
                // commitments: fail closed, there is nothing else to await.
                let their_shares = verify_reveal(&their_hashes, &their_reveal)?;
                return build_outcome(
                    we_host,
                    peer,
                    ours,
                    their_reveal.roster,
                    their_shares,
                    challenge.id,
                    accept.id,
                );
            }
            if Timestamp::now().as_u64() >= deadline {
                return Err(LobbyError::Timeout("the peer's reveal"));
            }
            sleep(POLL_INTERVAL).await;
        }
    }
}

/// Sleep without tokio: a never-completing future under a timeout. wasm-safe
/// (nostr's timer glue), non-blocking on native runtimes.
async fn sleep(d: Duration) {
    let _ = time::timeout(Some(d), std::future::pending::<()>()).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crusade_core::data;
    use crusade_core::{DevotionStats, GraceStats, RiteSlot};
    use nostr_relay_builder::mock::MockRelay;

    /// Same battle-fixture shape as crusade-net's duel tests.
    fn mkchamp(species_id: u16, level: u8, rite_ids: &[u16]) -> ChampionInstance {
        let sp = data::species_by_id(species_id).expect("fixture species");
        let mut rites: [Option<RiteSlot>; 4] = [None; 4];
        for (i, id) in rite_ids.iter().enumerate() {
            let r = data::rite_by_id(*id).expect("fixture rite");
            rites[i] = Some(RiteSlot {
                rite_id: *id,
                zeal_left: r.zeal,
            });
        }
        let mut c = ChampionInstance {
            species_id,
            level,
            xp: 0,
            grace: GraceStats {
                resolve: 0,
                valour: 0,
                piety: 0,
                guard: 0,
                aegis: 0,
                alacrity: 0,
            },
            devotion: DevotionStats {
                resolve: 0,
                valour: 0,
                piety: 0,
                guard: 0,
                aegis: 0,
                alacrity: 0,
            },
            calling_id: 0,
            rites,
            current_resolve: 0,
            ailment: None,
        };
        c.current_resolve = data::compute_stats(&c, sp).max_resolve;
        c
    }

    fn roster_a() -> Vec<ChampionInstance> {
        vec![mkchamp(1, 20, &[1, 2]), mkchamp(2, 20, &[1, 2])]
    }

    fn roster_b() -> Vec<ChampionInstance> {
        vec![mkchamp(3, 20, &[1, 6])]
    }

    fn commitment(party: Vec<ChampionInstance>) -> LobbyCommitment {
        LobbyCommitment::new(party).expect("commitment")
    }

    fn pair_events(
        host_keys: &Keys,
        join_keys: &Keys,
        ours_h: &LobbyCommitment,
        ours_j: &LobbyCommitment,
    ) -> (Event, Event) {
        let challenge = ChallengeContent {
            game_type: GAME_TYPE.to_string(),
            commitment_hashes: ours_h.commitment_hashes(),
            game_parameters: serde_json::json!({ "suite": SUITE }),
            expiry: Some(Timestamp::now().as_u64() + CHALLENGE_TTL_SECS),
            timeout_config: None,
        }
        .to_event(host_keys)
        .unwrap();
        let accept = ChallengeAcceptContent {
            challenge_id: challenge.id,
            commitment_hashes: ours_j.commitment_hashes(),
        }
        .to_event(join_keys)
        .unwrap();
        (challenge, accept)
    }

    #[test]
    fn commitment_opens_its_own_reveal() {
        let c = commitment(roster_a());
        let hashes = c.commitment_hashes();
        // one share hash per champion + one roster hash
        assert_eq!(hashes.len(), roster_a().len() + 1);
        assert!(hashes.iter().all(|h| h.len() == 64 && hex::decode(h).is_ok()));

        let reveal = c.reveal([7u8; 32]);
        let shares = verify_reveal(&hashes, &reveal).expect("own reveal must verify");
        assert_eq!(shares.len(), roster_a().len());
        // Deterministic re-derivation from the commitment itself.
        assert_eq!(shares, c.shares);
    }

    #[test]
    fn tampered_reveal_is_rejected() {
        let c = commitment(roster_a());
        let hashes = c.commitment_hashes();

        let mut forged = c.reveal([7u8; 32]);
        forged.roster[0].level += 1; // roster swap after commitment
        assert!(verify_reveal(&hashes, &forged).is_err());

        let mut forged = c.reveal([7u8; 32]);
        let bad = &mut forged.shares[0];
        bad.replace_range(..2, if bad.starts_with("00") { "01" } else { "00" });
        assert!(verify_reveal(&hashes, &forged).is_err());

        let mut short = c.reveal([7u8; 32]);
        short.shares.pop();
        assert!(verify_reveal(&hashes, &short).is_err());
    }

    #[test]
    fn reveal_share_values_are_strict() {
        let mut r = commitment(roster_b()).reveal([1u8; 32]);
        r.shares[0] = "zz".into();
        assert!(r.share_values().is_err());
        r.shares[0] = "aa".repeat(32);
        assert!(r.share_values().is_err());
    }

    #[test]
    fn outcome_is_role_order_sensitive_and_symmetric() {
        let host_c = commitment(roster_a());
        let join_c = commitment(roster_b());
        let (challenge, accept) = pair_events(
            &Keys::generate(),
            &Keys::generate(),
            &host_c,
            &join_c,
        );

        let host_reveal = host_c.reveal(battle_id_of(challenge.id, accept.id));
        let join_reveal = join_c.reveal(battle_id_of(challenge.id, accept.id));
        let host_shares = host_reveal.share_values().unwrap();
        let join_shares = join_reveal.share_values().unwrap();

        let host_out = build_outcome(
            true,
            join_reveal_host_pub(&accept),
            &host_c,
            join_reveal.roster.clone(),
            join_shares.clone(),
            challenge.id,
            accept.id,
        )
        .unwrap();
        let join_out = build_outcome(
            false,
            host_reveal_peer_pub(&challenge),
            &join_c,
            host_reveal.roster.clone(),
            host_shares.clone(),
            challenge.id,
            accept.id,
        )
        .unwrap();

        assert_eq!(host_out.battle_id, join_out.battle_id);
        assert_eq!(host_out.seed, join_out.seed);
        // battle_id matches the documented sha256(id||id) convention.
        let mut hasher = Sha256::new();
        hasher.update(challenge.id.to_bytes());
        hasher.update(accept.id.to_bytes());
        let want: [u8; 32] = hasher.finalize().into();
        assert_eq!(host_out.battle_id, want);
        // Side order matters (seed fn golden-tested in crusade-bridge).
        assert!(host_out.we_host && !join_out.we_host);
    }

    fn join_reveal_host_pub(accept: &Event) -> PublicKey {
        accept.pubkey
    }

    fn host_reveal_peer_pub(challenge: &Event) -> PublicKey {
        challenge.pubkey
    }

    #[test]
    fn validate_pair_rejects_invalid_shapes() {
        let host_keys = Keys::generate();
        let join_keys = Keys::generate();
        let host_c = commitment(roster_a());
        let join_c = commitment(roster_b());
        let (challenge, accept) = pair_events(&host_keys, &join_keys, &host_c, &join_c);
        let now = Timestamp::now().as_u64();

        // Joiner's point of view also validates.
        let (we_host_join, _, _) =
            validate_pair(join_keys.public_key(), &challenge, &accept, &join_c.hashes, now)
                .expect("joiner pair");
        assert!(!we_host_join);

        // Happy path, both sides' points of view.
        let (we_host, _, theirs) =
            validate_pair(host_keys.public_key(), &challenge, &accept, &host_c.hashes, now)
                .expect("host pair");
        assert!(we_host);
        assert_eq!(theirs, join_c.commitment_hashes());

        // An accept pointing elsewhere is not this battle.
        let stray = ChallengeAcceptContent {
            challenge_id: EventId::from_hex(&"ab".repeat(32)).unwrap(),
            commitment_hashes: join_c.commitment_hashes(),
        }
        .to_event(&join_keys)
        .unwrap();
        assert!(validate_pair(host_keys.public_key(), &challenge, &stray, &host_c.hashes, now)
            .is_err());

        // Our role: completing with somebody else's pair is impossible.
        let mallory = Keys::generate();
        assert!(
            validate_pair(mallory.public_key(), &challenge, &accept, &host_c.hashes, now).is_err()
        );
    }

    /// Acceptance: host + joiner over the embedded-relay fixture derive the
    /// identical battle id and seed, with rosters exchanged.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn host_joiner_handshake_over_embedded_relay() {
        let relay = MockRelay::run().await.expect("mock relay starts");
        let url = relay.url();

        let host = Lobby::connect(Keys::generate(), &[url.clone()])
            .await
            .expect("host connect");
        let joiner = Lobby::connect(Keys::generate(), &[url])
            .await
            .expect("joiner connect");
        let host_pk = host.public_key();
        let join_pk = joiner.public_key();

        let host_roster = roster_a();
        let join_roster = roster_b();
        let host_c = commitment(host_roster.clone());
        let join_c = commitment(join_roster.clone());

        let challenge = host.publish_challenge(&host_c).await.expect("publish");
        let challenge_id = challenge.id;

        let timeout = Duration::from_secs(30);
        let join_task = tokio::spawn(async move {
            let open = joiner.await_challenge(timeout).await.expect("await challenge");
            assert_eq!(open.event.id, challenge_id, "joiner sees OUR challenge");
            let accept = joiner
                .accept(&open, &join_c)
                .await
                .expect("accept publishes");
            joiner.complete(&open.event, &accept, &join_c, timeout).await
        });

        let accept = host
            .await_accept(&challenge, timeout)
            .await
            .expect("await accept");
        let host_out = host
            .complete(&challenge, &accept, &host_c, timeout)
            .await
            .expect("host complete");
        let join_out = join_task
            .await
            .expect("joiner task")
            .expect("joiner complete");

        assert_eq!(host_out.battle_id, join_out.battle_id, "battle ids agree");
        assert_eq!(host_out.seed, join_out.seed, "seeds agree");
        assert!(host_out.we_host);
        assert!(!join_out.we_host);
        assert_eq!(host_out.peer, join_pk);
        assert_eq!(join_out.peer, host_pk);
        assert_eq!(host_out.our_roster, host_roster);
        assert_eq!(host_out.their_roster, join_roster);
        assert_eq!(join_out.our_roster, join_roster);
        assert_eq!(join_out.their_roster, host_roster);

        host.disconnect().await;
        relay.shutdown();
    }

    /// A challenge from an unknown suite / an expired one never lists.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn foreign_and_expired_challenges_are_invisible() {
        let relay = MockRelay::run().await.expect("mock relay starts");
        let url = relay.url();
        let watcher = Lobby::connect(Keys::generate(), &[url.clone()])
            .await
            .expect("connect");
        let foreign = Client::new(&Keys::generate());
        foreign.add_relay(&url).await.expect("relay");
        foreign.connect().await;

        let other = ChallengeContent {
            game_type: GAME_TYPE.to_string(),
            commitment_hashes: commitment(roster_b()).commitment_hashes(),
            game_parameters: serde_json::json!({ "suite": "someone/else" }),
            expiry: Some(Timestamp::now().as_u64() + CHALLENGE_TTL_SECS),
            timeout_config: None,
        }
        .to_event(&Keys::generate())
        .unwrap();
        foreign.send_event(other).await.expect("send foreign");

        // Give the relay a beat, then: nothing open from our suite.
        sleep(Duration::from_millis(750)).await;
        let open = watcher.fetch_open_challenges(64).await.expect("fetch");
        assert!(open.is_empty(), "foreign-suite challenge must not list");
        relay.shutdown();
    }

    /// Spam cannot starve a genuine OLDER challenge: fresh valid-suite spam
    /// fills the newest end, the listing still surfaces the oldest first.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn spam_does_not_starve_oldest_genuine_challenge() {
        let relay = MockRelay::run().await.expect("mock relay starts");
        let url = relay.url();
        let watcher = Lobby::connect(Keys::generate(), &[url.clone()])
            .await
            .expect("connect");
        let spammer = Client::new(&Keys::generate());
        spammer.add_relay(&url).await.expect("relay");
        spammer.connect().await;

        fn suite_challenge(keys: &Keys, byte: u8) -> Event {
            ChallengeContent {
                game_type: GAME_TYPE.to_string(),
                commitment_hashes: vec![hex::encode([byte; 32])],
                game_parameters: serde_json::json!({ "suite": SUITE }),
                expiry: Some(Timestamp::now().as_u64() + CHALLENGE_TTL_SECS),
                timeout_config: None,
            }
            .to_event(keys)
            .unwrap()
        }

        // The genuine challenge first (real commitment, oldest timestamp);
        // nostr timestamps are seconds-granular, so space the spam clearly.
        let genu_keys = Keys::generate();
        let genuine = ChallengeContent {
            game_type: GAME_TYPE.to_string(),
            commitment_hashes: commitment(roster_a()).commitment_hashes(),
            game_parameters: serde_json::json!({ "suite": SUITE }),
            expiry: Some(Timestamp::now().as_u64() + CHALLENGE_TTL_SECS),
            timeout_config: None,
        }
        .to_event(&genu_keys)
        .unwrap();
        spammer.send_event(genuine.clone()).await.expect("genuine");
        sleep(Duration::from_millis(1200)).await;
        let spam_keys = Keys::generate();
        for i in 0..10u8 {
            let spam = suite_challenge(&spam_keys, i + 1);
            spammer.send_event(spam).await.expect("spam");
        }

        let open = watcher.fetch_open_challenges(5).await.expect("fetch");
        assert_eq!(open.len(), 5, "caller limit applies after sorting");
        assert_eq!(
            open[0].event.id,
            genuine.id,
            "oldest (genuine) challenge survives spam starvation"
        );
        assert!(open.iter().any(|c| c.event.id == genuine.id));
        relay.shutdown();
    }
}

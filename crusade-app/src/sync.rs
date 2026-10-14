//! Device roster sync (M6): exchange document bytes with a paired peer and
//! merge both ways.
//!
//! The CRDT doc carries champions, inventory, and settings; automerge merge
//! semantics make any two forks converge after a byte exchange in both
//! directions (see `crusade_data::roster`). Pairing/discovery and the wire
//! framing itself are the host app's job (crusade-net's `NetMsg` is a closed
//! duel protocol and stays untouched); crusade-app exposes the two pure
//! primitives a pairing loop needs:
//!
//! ```text
//! device A: bytes_a = encode(store_a)?
//! device B: bytes_b = encode(store_b)?
//!   ...ship the byte blobs over any channel...
//! device A: gained_a = merge_from_peer(store_a, &bytes_b)?
//! device B: gained_b = merge_from_peer(store_b, &bytes_a)?
//! ```
//!
//! Sync is for one's own roster across one's own devices only — never for
//! battle truth (that is commit-reveal duel territory).
//!
//! Both stores must be forks of a common ancestor document (pair a new
//! device by loading the account's saved bytes once — the same precondition
//! `RosterStore::merge_bytes` documents). Automerge identifies nested
//! objects by the operations that created them, so two stores each created
//! fresh with `RosterStore::new()` own disjoint root maps and cannot
//! converge through doc merge alone.

use crusade_data::RosterStore;

use crate::AppError;

/// Serialize this device's document for sending to a paired peer
/// (automerge `.save()` bytes, already bounded on the receive side).
pub fn encode(store: &mut RosterStore) -> Result<Vec<u8>, AppError> {
    Ok(store.save_bytes()?)
}

/// Merge a peer's document bytes into the local store. Returns the number
/// of newly listed champions (post-merge count minus pre-merge count; 0 when
/// the peer had nothing new — merges are idempotent, and CRDT merges never
/// shrink the champions map).
pub fn merge_from_peer(store: &mut RosterStore, bytes: &[u8]) -> Result<usize, AppError> {
    let before = store.list_champions()?.len();
    store.merge_bytes(bytes)?;
    let after = store.list_champions()?.len();
    Ok(after.saturating_sub(before))
}

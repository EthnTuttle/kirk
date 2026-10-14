//! Persistence shim: browser `localStorage` on wasm, no-ops on native.
//!
//! The whole roster document (automerge bytes, hex-encoded) lives under one
//! key so the PWA survives reloads and offline restarts. Native shells keep
//! their existing in-memory behavior; same call sites compile everywhere.

/// localStorage key holding the hex-encoded roster document.
#[cfg(target_arch = "wasm32")]
const STORAGE_KEY: &str = "deusvult.roster.v1";

/// Read the saved roster document, if any. Malformed hex (or a missing
/// window, e.g. a non-browser wasm host) degrades to `None` — the caller
/// then falls back to seeding.
#[cfg(target_arch = "wasm32")]
pub fn web_load_document() -> Option<Vec<u8>> {
    let storage = web_sys::window()?.local_storage().ok()??;
    let encoded = storage.get_item(STORAGE_KEY).ok()??;
    hex::decode(encoded).ok()
}

/// Persist the roster document. Quota errors (private browsing etc.) are
/// swallowed deliberately: the game stays playable, just unsaved.
#[cfg(target_arch = "wasm32")]
pub fn web_save_document(bytes: &[u8]) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(Some(storage)) = window.local_storage() else {
        return;
    };
    let _ = storage.set_item(STORAGE_KEY, &hex::encode(bytes));
}

/// Native stub: there is no shared document store, so nothing to load.
#[cfg(not(target_arch = "wasm32"))]
pub fn web_load_document() -> Option<Vec<u8>> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
pub fn web_save_document(_bytes: &[u8]) {}

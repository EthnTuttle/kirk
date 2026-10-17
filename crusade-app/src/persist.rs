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

// ---------- Trail lane persistence (M8b) ----------
//
// The Pilgrim Road's journey state persists as an EXPLICIT save DTO — not
// a serde derive on `engine::TrailState` (M8A review section 5.2: the
// engine's `FallenEntry.cause` is a `&'static str` from a fixed table and
// cannot be deserialized; the correct shape re-expresses the cause as a
// bounded enum). The DTO rides the roster document as ONE settings entry
// (`trail.save`, JSON), exactly like the codex masks and ladder rating, so
// the trail lane survives reloads and roster sync without any document
// schema change. `trail.seed` (minted once per document at first embark)
// feeds the per-day rng streams, so a mid-journey reload replays identical
// days.
//
// Versioning: `save_version` is 2. Version 1 = a roster document that
// predates the trail lane and simply carries no `trail.save` key (or an
// empty one) — the migration is therefore "v1 save => trail lane absent",
// which is automatic: no key is read, nothing is rewritten. Any other
// version read degrades to `None` (the lane stays absent) rather than
// failing the whole load.

use crusade_core::engine::trail::{FallenEntry, ManEntry, TrailState};
use crusade_core::types::ChampionInstance;
use crusade_data::{RosterStore, StoreError};
use serde::{Deserialize, Serialize};

/// Settings key holding the JSON [`TrailSave`].
pub const TRAIL_SAVE_KEY: &str = "trail.save";
/// Settings key holding the hex-encoded 32-byte pilgrimage seed, minted
/// once per document (client.rs `trail_seed`).
pub const TRAIL_SEED_KEY: &str = "trail.seed";
/// Current save-shape version; v1 documents have no trail lane at all.
pub const TRAIL_SAVE_VERSION: u16 = 2;

/// The bounded cause ledger: every way a man enters the Book of the
/// Fallen, re-expressed from the engine's fixed `&'static str` cause table
/// (so the DTO is deserializable) plus the one app-lane cause (battle
/// deaths on the road are recorded by crusade-app, not the engine).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FallenCause {
    /// The Crossing took him (`the_crossing`).
    LostAtSea,
    /// Trail event 2.
    FeverRoad,
    /// Trail event 3.
    DrownedAtFord,
    /// Trail event 10.
    SlippedAway,
    /// Engine fallback cause for `LoseMen` events without a set string.
    TheRoadTookHim,
    /// `champion_falls`' Reordered losses.
    RoutAfterCaptain,
    /// App-lane: the trail champion fell in battle (crusade-app records
    /// this page itself before `champion_falls` runs — the engine's mantle
    /// deliberately does not, M8A review issue 6).
    SlainInBattle,
}

/// App-lane cause strings, keyed to the engine's literals verbatim.
impl FallenCause {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LostAtSea => "lost at sea in the Crossing",
            Self::FeverRoad => "fever took him on the road",
            Self::DrownedAtFord => "drowned at the ford",
            Self::SlippedAway => "slipped away in the dark, and was seen no more",
            Self::TheRoadTookHim => "the road took him",
            Self::RoutAfterCaptain => "lost in the rout after their captain fell",
            Self::SlainInBattle => "fell in battle upon the pilgrim road",
        }
    }

    /// The inverse of [`Self::as_str`]; `None` for strings outside the
    /// fixed cause table (a corrupt or hand-edited save).
    pub fn from_cause(s: &str) -> Option<Self> {
        Some(match s {
            "lost at sea in the Crossing" => Self::LostAtSea,
            "fever took him on the road" => Self::FeverRoad,
            "drowned at the ford" => Self::DrownedAtFord,
            "slipped away in the dark, and was seen no more" => Self::SlippedAway,
            "the road took him" => Self::TheRoadTookHim,
            "lost in the rout after their captain fell" => Self::RoutAfterCaptain,
            "fell in battle upon the pilgrim road" => Self::SlainInBattle,
            _ => return None,
        })
    }
}

/// DTO shadow of the engine's `FallenEntry` (owned + enum cause).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FallenSave {
    pub name: String,
    pub day: u16,
    pub cause: FallenCause,
    pub where_fell: u16,
}

/// The explicit trail-lane save document (M8A review section 5.2). Every
/// field mirrors `engine::TrailState` plus the app-lane battle party facts
/// the engine deliberately does not own; conversion is total (save ->
/// state -> save is the identity).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrailSave {
    pub save_version: u16,
    pub day: u16,
    pub screen: u16,
    pub supplies: u16,
    pub morale: i8,
    pub region_flags: u8,
    pub men: Vec<ManEntry>,
    pub fallen: Vec<FallenSave>,
    /// The living trail battle party (the champion's file as battle-lane
    /// champion instances; resolve persists between road battles).
    pub party: Vec<ChampionInstance>,
    /// The fallen/current champion's level — the succession banding input
    /// (docs/OVERWORLD.md section 6.2: former champion -5..+2).
    pub champion_level: u8,
    /// The pilgrimage ended (whole company gone). Terminal rows persist so
    /// the Book of the Fallen remains openable after the run.
    pub over: bool,
}

impl TrailSave {
    /// Snapshot a live trail campaign row. Fails only if the engine's
    /// cause table grew past [`FallenCause`] — loud, never a silent drop.
    pub fn capture(
        state: &TrailState,
        party: &[ChampionInstance],
        champion_level: u8,
        over: bool,
    ) -> Result<Self, String> {
        let mut fallen = Vec::with_capacity(state.fallen.len());
        for f in &state.fallen {
            fallen.push(FallenSave {
                name: f.name.clone(),
                day: f.day,
                cause: FallenCause::from_cause(f.cause).ok_or_else(|| {
                    format!("fallen cause {:?} is outside the bounded ledger", f.cause)
                })?,
                where_fell: f.where_fell,
            });
        }
        Ok(Self {
            save_version: TRAIL_SAVE_VERSION,
            day: state.day,
            screen: state.screen,
            supplies: state.supplies,
            morale: state.morale,
            region_flags: state.region_flags,
            men: state.men.clone(),
            fallen,
            party: party.to_vec(),
            champion_level,
            over,
        })
    }

    /// Rebuild the engine journey state row for row.
    pub fn to_state(&self) -> TrailState {
        TrailState {
            day: self.day,
            screen: self.screen,
            supplies: self.supplies,
            morale: self.morale,
            men: self.men.clone(),
            fallen: self
                .fallen
                .iter()
                .map(|f| FallenEntry {
                    name: f.name.clone(),
                    day: f.day,
                    cause: f.cause.as_str(),
                    where_fell: f.where_fell,
                })
                .collect(),
            region_flags: self.region_flags,
        }
    }
}

/// Persist the trail lane into the roster document (one settings write).
pub fn save_trail(store: &mut RosterStore, save: &TrailSave) -> Result<(), StoreError> {
    let json = serde_json::to_string(save)
        .map_err(|e| StoreError::Json(format!("trail save: {e}")))?;
    store.set_setting(TRAIL_SAVE_KEY, &json)
}

/// Read back the trail lane, if any. A missing/empty key (a v1 document),
/// an unknown `save_version`, or malformed JSON all degrade to `None` —
/// the trail lane is absent, never a load failure; mirroring the client's
/// sealed-mask degrade discipline.
pub fn load_trail(store: &RosterStore) -> Result<Option<TrailSave>, StoreError> {
    let Some(raw) = store.setting(TRAIL_SAVE_KEY)? else {
        return Ok(None);
    };
    if raw.is_empty() {
        return Ok(None);
    }
    let Ok(save) = serde_json::from_str::<TrailSave>(&raw) else {
        return Ok(None);
    };
    if save.save_version != TRAIL_SAVE_VERSION {
        return Ok(None);
    }
    Ok(Some(save))
}

/// Clear the trail lane (abandoned runs). The seed key is kept: the
/// pilgrimage seed is the document's salt by design.
pub fn clear_trail(store: &mut RosterStore) -> Result<(), StoreError> {
    store.set_setting(TRAIL_SAVE_KEY, "")
}

#[cfg(test)]
mod trail_save_tests {
    use super::*;
    use crusade_core::engine::trail;

    fn fixture_state() -> TrailState {
        let mut rng = crusade_core::rng::BattleRng::from_seed([5u8; 32]);
        let mut out = trail::the_crossing(120, 250, &mut rng);
        // March a day so the Book may have gained a road page, then leave
        // the state as-is (whatever the seed rolled is valid fixture data).
        let mut day_rng = crusade_core::rng::BattleRng::for_turn([5u8; 32], 3);
        let _ = trail::travel_day(&mut out.state, 6, &mut day_rng);
        out.state
    }

    /// The save DTO round-trips through JSON losslessly, and the engine
    /// state rebuilds row for row (Book of the Fallen causes keyed through
    /// the bounded ledger both directions).
    #[test]
    fn trail_save_round_trip_is_lossless() {
        let state = fixture_state();
        let save = TrailSave::capture(&state, &[], 20, false).expect("capture");
        assert_eq!(save.save_version, TRAIL_SAVE_VERSION);
        let json = serde_json::to_string(&save).expect("serialize");
        let back: TrailSave = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, save);
        let rebuilt = back.to_state();
        // The engine state is Serialize-only; compare via JSON.
        assert_eq!(
            serde_json::to_value(&rebuilt).expect("state serializes"),
            serde_json::to_value(&fixture_state()).expect("state serializes"),
        );
    }

    /// Every engine cause string maps into the bounded ledger and back —
    /// the mapping can never silently grow stale.
    #[test]
    fn the_cause_ledger_is_total() {
        for c in [
            FallenCause::LostAtSea,
            FallenCause::FeverRoad,
            FallenCause::DrownedAtFord,
            FallenCause::SlippedAway,
            FallenCause::TheRoadTookHim,
            FallenCause::RoutAfterCaptain,
            FallenCause::SlainInBattle,
        ] {
            assert_eq!(FallenCause::from_cause(c.as_str()), Some(c));
        }
        assert_eq!(FallenCause::from_cause("some novel cause"), None);
    }

    /// Store-level round trip + the v1 -> v2 migration: a document with no
    /// trail key (v1) loads with the lane ABSENT and stays valid; only a
    /// version-2 row is ever written or read.
    #[test]
    fn store_round_trip_and_v1_migration() {
        let mut store = RosterStore::new();
        // v1: no key at all.
        assert_eq!(load_trail(&store).expect("load"), None);

        let save = TrailSave::capture(&fixture_state(), &[], 20, false).expect("capture");
        save_trail(&mut store, &save).expect("save");
        let back = load_trail(&store).expect("load").expect("lane present");
        assert_eq!(back, save);

        // Hand-rolled v1-ish rows: unknown version or malformed JSON read
        // as absent (never a failure), so old or damaged saves stay openable.
        store
            .set_setting(TRAIL_SAVE_KEY, r#"{"save_version":1,"day":2}"#)
            .expect("write");
        assert_eq!(load_trail(&store).expect("load"), None);
        store.set_setting(TRAIL_SAVE_KEY, "not json").expect("write");
        assert_eq!(load_trail(&store).expect("load"), None);
        clear_trail(&mut store).expect("clear");
        assert_eq!(load_trail(&store).expect("load"), None);

        // And the lane survives a document save/load (bytes) round trip.
        let save = TrailSave::capture(&fixture_state(), &[], 20, false).expect("capture");
        save_trail(&mut store, &save).expect("save");
        let bytes = store.save_bytes().expect("document saves");
        let reloaded = RosterStore::load_bytes(&bytes).expect("document loads");
        assert_eq!(
            load_trail(&reloaded).expect("load").expect("lane present"),
            save
        );
    }
}

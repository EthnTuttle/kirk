//! RosterStore: typed facade over the automerge doc.
//!
//! Schema (mirrors the crate-root contract): the automerge root always
//! carries the four maps `champions`, `inventory`, `settings`, `battles`.
//! Handles to the three maps with a typed facade are cached in the struct;
//! `battles` is materialized so saved docs are schema-complete, but has no
//! facade methods yet.
//!
//! Merge semantics: automerge map keys are last-writer-wins; concurrent
//! writes to the same key (e.g. the same champion edited on two devices)
//! resolve deterministically to the same winner on every replica. Conflicts
//! at *different* keys always merge cleanly.
//!
//! Inventory values are plain `u64` scalars (LWW), *not* automerge
//! counters: counters merge by summing deltas, which cannot be clamped at
//! zero and does not match absolute-balance semantics. The trade-off is
//! that concurrent independent `add_item` adjustments to the *same* item
//! resolve to one side's absolute total rather than the sum of both.
//!
//! Actor ids: `AutoCommit::new` picks a random actor, and `AutoCommit::load`
//! also assigns a fresh random actor on load, so forked docs never share an
//! actor and `merge` needs no manual actor management.

use automerge::transaction::Transactable;
use automerge::{AutoCommit, AutomergeError, ObjId, ObjType, ReadDoc, ROOT};
use crusade_core::types::ChampionInstance;

use crate::StoreError;

fn am_err(e: AutomergeError) -> StoreError {
    StoreError::Automerge(e.to_string())
}

fn json_err(e: serde_json::Error) -> StoreError {
    StoreError::Json(e.to_string())
}

/// Input cap for `load_bytes`/`merge_bytes`: anything larger is rejected
/// before automerge ever sees it (a hostile doc would otherwise burn memory
/// and CPU inside `AutoCommit::load`).
const MAX_DOC_BYTES: usize = 8 * 1024 * 1024;

/// Cap on entries returned by `list_champions`.
const MAX_LISTED: usize = 10_000;

fn reject_oversized(bytes: &[u8]) -> Result<(), StoreError> {
    if bytes.len() > MAX_DOC_BYTES {
        return Err(StoreError::Automerge("doc too large".to_string()));
    }
    Ok(())
}

#[derive(Debug)]
pub struct RosterStore {
    doc: AutoCommit,
    champions: ObjId,
    inventory: ObjId,
    settings: ObjId,
}

impl RosterStore {
    pub fn new() -> Self {
        // Creating maps at the root of a fresh document cannot fail.
        Self::bootstrap(AutoCommit::new())
            .expect("RosterStore::new: root map creation on fresh doc")
    }

    /// Materialize the four root maps (creating any that are missing) and
    /// cache the handles this facade uses. Common to `new` and `load_bytes`.
    fn bootstrap(mut doc: AutoCommit) -> Result<Self, StoreError> {
        let champions = ensure_root_map(&mut doc, "champions")?;
        let inventory = ensure_root_map(&mut doc, "inventory")?;
        let settings = ensure_root_map(&mut doc, "settings")?;
        // No facade yet; materialized for schema completeness (schema notes
        // at crate root).
        let _ = ensure_root_map(&mut doc, "battles")?;
        Ok(Self {
            doc,
            champions,
            inventory,
            settings,
        })
    }

    /// Insert a champion, returning its new id (uuid v4 string).
    pub fn add_champion(&mut self, c: &ChampionInstance) -> Result<String, StoreError> {
        let id = uuid::Uuid::new_v4().to_string(); // hyphenated form
        let json = serde_json::to_string(c).map_err(json_err)?;
        self.doc
            .put(&self.champions, id.clone(), json)
            .map_err(am_err)?;
        Ok(id)
    }

    pub fn get_champion(&self, id: &str) -> Result<ChampionInstance, StoreError> {
        let (val, _) = self
            .doc
            .get(&self.champions, id)
            .map_err(am_err)?
            .ok_or_else(|| StoreError::UnknownChampion(id.to_string()))?;
        let json = val
            .as_str()
            .ok_or_else(|| corrupt("champions", id, "string"))?;
        serde_json::from_str(json).map_err(json_err)
    }

    /// Last-writer-wins replace of one champion entry. Acts as an upsert:
    /// an absent id is stored rather than rejected, which is the
    /// CRDT-friendly choice when a device receives an id it has not seen.
    pub fn set_champion(&mut self, id: &str, c: &ChampionInstance) -> Result<(), StoreError> {
        let json = serde_json::to_string(c).map_err(json_err)?;
        self.doc.put(&self.champions, id, json).map_err(am_err)
    }

    pub fn remove_champion(&mut self, id: &str) -> Result<(), StoreError> {
        if self.doc.get(&self.champions, id).map_err(am_err)?.is_none() {
            return Err(StoreError::UnknownChampion(id.to_string()));
        }
        self.doc.delete(&self.champions, id).map_err(am_err)
    }

    pub fn list_champions(&self) -> Result<Vec<(String, ChampionInstance)>, StoreError> {
        // Poisoned entries (bad JSON, wrong value type) are omitted rather
        // than failing the whole listing; direct `get_champion` stays strict.
        // The list itself is capped so an inflated map can't exhaust memory.
        let mut out = Vec::with_capacity(self.doc.length(&self.champions).min(MAX_LISTED));
        for id in self.doc.keys(&self.champions) {
            let Ok(c) = self.get_champion(&id) else {
                continue;
            };
            out.push((id, c));
            if out.len() >= MAX_LISTED {
                break;
            }
        }
        Ok(out)
    }

    /// Adjust one inventory counter by `delta`, clamped at 0 (the total
    /// never goes negative). Returns the new total.
    pub fn add_item(&mut self, item: &str, delta: i64) -> Result<u64, StoreError> {
        let current = self.item_total(item)?;
        let next = if delta >= 0 {
            current.saturating_add(delta as u64)
        } else {
            current.saturating_sub(delta.unsigned_abs())
        };
        self.doc.put(&self.inventory, item, next).map_err(am_err)?;
        Ok(next)
    }

    pub fn inventory(&self) -> Result<Vec<(String, u64)>, StoreError> {
        let mut out = Vec::with_capacity(self.doc.length(&self.inventory));
        for name in self.doc.keys(&self.inventory) {
            let n = self.item_total(&name)?;
            out.push((name, n));
        }
        Ok(out)
    }

    fn item_total(&self, item: &str) -> Result<u64, StoreError> {
        match self.doc.get(&self.inventory, item).map_err(am_err)? {
            Some((val, _)) => val
                .as_u64()
                .ok_or_else(|| corrupt("inventory", item, "uint")),
            None => Ok(0),
        }
    }

    pub fn set_setting(&mut self, key: &str, value: &str) -> Result<(), StoreError> {
        self.doc.put(&self.settings, key, value).map_err(am_err)
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>, StoreError> {
        match self.doc.get(&self.settings, key).map_err(am_err)? {
            Some((val, _)) => Ok(Some(
                val.as_str()
                    .ok_or_else(|| corrupt("settings", key, "string"))?
                    .to_string(),
            )),
            None => Ok(None),
        }
    }

    /// Serialize the whole document (`.save()` bytes).
    pub fn save_bytes(&mut self) -> Result<Vec<u8>, StoreError> {
        Ok(self.doc.save())
    }

    /// Load a document previously produced by `save_bytes`.
    pub fn load_bytes(bytes: &[u8]) -> Result<Self, StoreError> {
        reject_oversized(bytes)?;
        let doc = AutoCommit::load(bytes).map_err(am_err)?;
        Self::bootstrap(doc)
    }

    /// Merge a peer's document bytes (both forks then compare equal).
    pub fn merge_bytes(&mut self, peer: &[u8]) -> Result<(), StoreError> {
        reject_oversized(peer)?;
        let mut peer_doc = AutoCommit::load(peer).map_err(am_err)?;
        self.doc.merge(&mut peer_doc).map_err(am_err)?;
        Ok(())
    }

    /// Test-only backdoor: write an arbitrary string into the champions map
    /// (bypassing JSON validation) so tests can craft poisoned entries.
    #[cfg(test)]
    #[doc(hidden)]
    pub fn __put_raw_champion(&mut self, id: &str, json: &str) {
        self.doc
            .put(&self.champions, id, json)
            .expect("test-only raw put");
    }
}

/// Fetch the object id of a root map, creating it if absent. A pre-existing
/// non-map value at `key` is a schema violation.
fn ensure_root_map(doc: &mut AutoCommit, key: &'static str) -> Result<ObjId, StoreError> {
    // Copy out owned data so the read borrow ends before any `put_object`.
    let existing = doc
        .get(ROOT, key)
        .map_err(am_err)?
        .map(|(v, id)| (v.as_objtype(), id));
    match existing {
        Some((Some(ObjType::Map), id)) => Ok(id),
        Some(_) => Err(StoreError::Automerge(format!("root[{key}] is not a map"))),
        None => doc.put_object(ROOT, key, ObjType::Map).map_err(am_err),
    }
}

/// Corruption guard for values only this facade writes; unreachable through
/// the public API.
fn corrupt(map: &str, key: &str, want: &str) -> StoreError {
    StoreError::Automerge(format!("{map}[{key}] is not a {want}"))
}

impl Default for RosterStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crusade_core::data;
    use crusade_core::types::{DevotionStats, GraceStats, RiteSlot};

    /// Same construction as crusade-core's engine tests: species `species_id`
    /// at `level` with a learnset-legal rite selection, zero grace/devotion,
    /// full zeal per slot, resolve set to the computed maximum.
    fn mkchamp(species_id: u16, level: u8, rite_ids: &[u16]) -> ChampionInstance {
        let sp = data::species_by_id(species_id).expect("fixture species");
        let mut rites: [Option<RiteSlot>; 4] = [None; 4];
        for (i, id) in rite_ids.iter().enumerate() {
            let r = data::rite_by_id(*id).expect("fixture rite");
            assert!(
                sp.learnset
                    .iter()
                    .any(|e| e.rite_id == *id && e.level <= level),
                "fixture rite {id} not learnable by species {species_id} at {level}"
            );
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
            converted: false,
            converted_from: None,
        };
        c.current_resolve = data::compute_stats(&c, sp).max_resolve;
        c
    }

    fn sorted(mut v: Vec<(String, ChampionInstance)>) -> Vec<(String, ChampionInstance)> {
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }

    #[test]
    fn add_get_list_remove_round_trip() {
        let mut s = RosterStore::new();
        assert!(matches!(
            s.get_champion("nope"),
            Err(StoreError::UnknownChampion(_))
        ));

        let c = mkchamp(1, 10, &[1]);
        let id = s.add_champion(&c).unwrap();
        assert_eq!(id.len(), 36, "hyphenated uuid");
        assert_eq!(s.get_champion(&id).unwrap(), c);
        assert_eq!(s.list_champions().unwrap(), vec![(id.clone(), c.clone())]);

        let mut leveled = c.clone();
        leveled.level = 20;
        s.set_champion(&id, &leveled).unwrap();
        assert_eq!(s.get_champion(&id).unwrap().level, 20);

        s.remove_champion(&id).unwrap();
        assert!(matches!(
            s.get_champion(&id),
            Err(StoreError::UnknownChampion(_))
        ));
        assert!(s.list_champions().unwrap().is_empty());
        assert!(matches!(
            s.remove_champion(&id),
            Err(StoreError::UnknownChampion(_))
        ));
    }

    #[test]
    fn save_load_byte_round_trip() {
        let mut a = RosterStore::new();
        let c1 = mkchamp(1, 10, &[1]);
        let c2 = mkchamp(1, 25, &[1]);
        let id1 = a.add_champion(&c1).unwrap();
        let id2 = a.add_champion(&c2).unwrap();
        assert_eq!(a.add_item("shards", 7).unwrap(), 7);
        assert_eq!(a.add_item("shards", -2).unwrap(), 5);
        assert_eq!(a.add_item("incense", 1).unwrap(), 1);
        a.set_setting("difficulty", "penitent").unwrap();
        a.set_setting("locale", "la").unwrap();

        let bytes = a.save_bytes().unwrap();
        let b = RosterStore::load_bytes(&bytes).unwrap();

        let la = sorted(a.list_champions().unwrap());
        let lb = sorted(b.list_champions().unwrap());
        assert_eq!(la, lb);
        assert_eq!(la.len(), 2);
        assert!(la.iter().any(|(id, _)| id == &id1));
        assert!(la.iter().any(|(id, _)| id == &id2));

        let mut ia = a.inventory().unwrap();
        let mut ib = b.inventory().unwrap();
        ia.sort();
        ib.sort();
        assert_eq!(ia, ib);
        assert_eq!(
            ib,
            vec![("incense".to_string(), 1u64), ("shards".to_string(), 5u64)]
        );

        assert_eq!(
            b.setting("difficulty").unwrap().as_deref(),
            Some("penitent")
        );
        assert_eq!(b.setting("locale").unwrap().as_deref(), Some("la"));
        assert_eq!(b.setting("missing").unwrap(), None);
    }

    #[test]
    fn concurrent_merge_both_ways_converges() {
        let mut base = RosterStore::new();
        let base_bytes = base.save_bytes().unwrap();
        let mut a = RosterStore::load_bytes(&base_bytes).unwrap();
        let mut b = RosterStore::load_bytes(&base_bytes).unwrap();

        // Fork A: champion X + inventory.
        let cx = mkchamp(1, 10, &[1]);
        let idx = a.add_champion(&cx).unwrap();
        assert_eq!(a.add_item("shards", 3).unwrap(), 3);

        // Fork B (same base): champion Y + a setting.
        let cy = mkchamp(1, 40, &[1]);
        let idy = b.add_champion(&cy).unwrap();
        b.set_setting("difficulty", "penitent").unwrap();

        let a_bytes = a.save_bytes().unwrap();
        let b_bytes = b.save_bytes().unwrap();
        a.merge_bytes(&b_bytes).unwrap();
        b.merge_bytes(&a_bytes).unwrap();

        for s in [&a, &b] {
            assert_eq!(s.get_champion(&idx).unwrap(), cx);
            assert_eq!(s.get_champion(&idy).unwrap(), cy);
            assert_eq!(s.inventory().unwrap(), vec![("shards".to_string(), 3)]);
            assert_eq!(
                s.setting("difficulty").unwrap().as_deref(),
                Some("penitent")
            );
        }

        let la = sorted(a.list_champions().unwrap());
        let lb = sorted(b.list_champions().unwrap());
        assert_eq!(la, lb, "both forks must list the same champions");
        assert_eq!(la.len(), 2);
    }

    #[test]
    fn same_champion_concurrent_edit_is_deterministic() {
        let mut base = RosterStore::new();
        let c = mkchamp(1, 10, &[1]);
        let id = base.add_champion(&c).unwrap();
        let base_bytes = base.save_bytes().unwrap();

        let mut a = RosterStore::load_bytes(&base_bytes).unwrap();
        let mut b = RosterStore::load_bytes(&base_bytes).unwrap();

        let mut a_edit = c.clone();
        a_edit.level = 20;
        let mut b_edit = c.clone();
        b_edit.level = 30;
        a.set_champion(&id, &a_edit).unwrap();
        b.set_champion(&id, &b_edit).unwrap();

        let a_bytes = a.save_bytes().unwrap();
        let b_bytes = b.save_bytes().unwrap();
        a.merge_bytes(&b_bytes).unwrap();
        b.merge_bytes(&a_bytes).unwrap();

        let va = a.get_champion(&id).unwrap();
        let vb = b.get_champion(&id).unwrap();
        assert_eq!(va, vb, "LWW winner must be identical on both peers");
        assert!(va == a_edit || va == b_edit, "winner must be a real edit");
    }

    #[test]
    fn inventory_clamps_at_zero() {
        let mut s = RosterStore::new();
        assert_eq!(s.add_item("shards", -99).unwrap(), 0);
        assert_eq!(s.add_item("shards", 5).unwrap(), 5);
        assert_eq!(s.add_item("shards", -3).unwrap(), 2);
        assert_eq!(s.add_item("shards", -10).unwrap(), 0);
        assert_eq!(s.inventory().unwrap(), vec![("shards".to_string(), 0u64)]);
        assert_eq!(s.add_item("shards", i64::MIN).unwrap(), 0);
    }

    /// 9 MiB of (pseudo-random) bytes must be refused before automerge
    /// parsing, from both the load and merge entry points.
    #[test]
    fn oversized_docs_rejected() {
        let mut big = vec![0u8; 9 * 1024 * 1024];
        // xorshift32 fill — content is irrelevant, size gate fires first.
        let mut x = 0x9E3779B9u32;
        for b in big.iter_mut() {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *b = x as u8;
        }
        let want = StoreError::Automerge("doc too large".to_string());
        assert_eq!(RosterStore::load_bytes(&big).unwrap_err(), want);
        let mut s = RosterStore::new();
        assert_eq!(s.merge_bytes(&big).unwrap_err(), want);
    }

    /// A map holding both valid champions and poisoned entries must list the
    /// good ones; direct get on a poisoned id still errors.
    #[test]
    fn poisoned_champion_entries_are_skipped() {
        let mut s = RosterStore::new();
        let good = mkchamp(1, 10, &[1]);
        let good_id = s.add_champion(&good).unwrap();
        s.__put_raw_champion("bad-json", "{not a champion");
        s.__put_raw_champion("bad-shape", "12345");

        let listed = s.list_champions().unwrap();
        assert_eq!(listed, vec![(good_id, good)]);
        assert!(s.get_champion("bad-json").is_err());
        assert!(s.get_champion("bad-shape").is_err());
    }
}

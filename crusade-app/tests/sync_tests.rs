//! Device roster sync: two devices exchange `sync::encode` byte blobs and
//! `sync::merge_from_peer` both ways; both stores converge to the union.

use crusade_app::sync;
use crusade_core::data;
use crusade_core::types::{ChampionInstance, DevotionStats, GraceStats, RiteSlot};
use crusade_data::RosterStore;

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
    };
    c.current_resolve = data::compute_stats(&c, sp).max_resolve;
    c
}

fn sorted_ids(store: &RosterStore) -> Vec<String> {
    let mut ids: Vec<String> = store
        .list_champions()
        .unwrap()
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    ids.sort();
    ids
}

#[test]
fn cross_sync_converges_and_reports_gained_champions() {
    // Both devices fork the same account genesis document (the store's CRDT
    // merge is defined between forks of a common ancestor — see roster.rs);
    // afterwards each records a different champion.
    let mut genesis = RosterStore::new();
    let genesis_bytes = genesis.save_bytes().unwrap();
    let mut a = RosterStore::load_bytes(&genesis_bytes).unwrap();
    let mut b = RosterStore::load_bytes(&genesis_bytes).unwrap();

    let audric = mkchamp(1, 20, &[1, 2]);
    let id_audric = a.add_champion(&audric).unwrap();
    a.add_item("contrition-shard", 3).unwrap();

    let hrothwyn = mkchamp(6, 30, &[12, 13]);
    let id_hrothwyn = b.add_champion(&hrothwyn).unwrap();

    // The pairing host ships each side's blob to the other (framing is the
    // host's job; plain bytes here).
    let bytes_a = sync::encode(&mut a).unwrap();
    let bytes_b = sync::encode(&mut b).unwrap();

    let gained_b = sync::merge_from_peer(&mut b, &bytes_a).unwrap();
    let gained_a = sync::merge_from_peer(&mut a, &bytes_b).unwrap();
    assert_eq!((gained_a, gained_b), (1, 1), "each side gains one champion");

    // Both devices list both champions, same payload.
    let want_ids = {
        let mut v = vec![id_audric.clone(), id_hrothwyn.clone()];
        v.sort();
        v
    };
    assert_eq!(sorted_ids(&a), want_ids);
    assert_eq!(sorted_ids(&b), want_ids);
    assert_eq!(a.get_champion(&id_hrothwyn).unwrap(), hrothwyn);
    assert_eq!(b.get_champion(&id_audric).unwrap(), audric);

    // Inventory rides along (different keys merge cleanly).
    assert_eq!(
        b.inventory().unwrap(),
        vec![("contrition-shard".to_string(), 3u64)]
    );

    // Idempotent: merging the same blob again reports nothing new.
    assert_eq!(sync::merge_from_peer(&mut a, &bytes_b).unwrap(), 0);
    assert_eq!(sorted_ids(&a), want_ids);
}

#[test]
fn merge_rejects_garbage_and_oversized_blobs() {
    let mut a = RosterStore::new();
    a.add_champion(&mkchamp(1, 10, &[1])).unwrap();

    assert!(sync::merge_from_peer(&mut a, b"definitely not a doc").is_err());
    let big = vec![0u8; 9 * 1024 * 1024];
    assert!(sync::merge_from_peer(&mut a, &big).is_err());

    // The failed merges left the store untouched.
    assert_eq!(a.list_champions().unwrap().len(), 1);
}

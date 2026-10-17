//! Unit tests for the greedy practice-opponent policy.

use crusade_app::policy::greedy_action;
use crusade_core::data;
use crusade_core::engine::{Action, Battle};
use crusade_core::types::{ChampionInstance, DevotionStats, GraceStats, RiteSlot};

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

fn battle(party_a: Vec<ChampionInstance>, party_b: Vec<ChampionInstance>) -> Battle {
    Battle::new(party_a, party_b, [7u8; 32]).expect("legal fixture battle")
}

#[test]
fn greedy_picks_highest_power_times_effectiveness() {
    // Squire Audric knows Measured Blow (40p) and Humble Rebuke (55p), both
    // Humility-typed. Prince of Pride is Sin(Pride), which Humility opposes,
    // so effectiveness is 200 either way and raw power breaks the tie.
    let b = battle(vec![mkchamp(1, 15, &[1, 3])], vec![mkchamp(101, 15, &[8])]);
    assert_eq!(
        greedy_action(&b, 0),
        Action::Rite { slot: 1 },
        "55-power Humble Rebuke beats 40-power Measured Blow at equal effectiveness"
    );
}

#[test]
fn greedy_tie_breaks_to_lowest_slot() {
    // Two Champion-typed Humility rites vs a neutral defender (Patience):
    // effectiveness 100 for both; powers 40 vs 55 -> slot 1 wins on value.
    // Against a Humility defender both drop to 50, still slot 1 (higher
    // power). To see the pure tie-break, use two identical-power options:
    // rite 1 in slots 0 and 1.
    let b = battle(vec![mkchamp(1, 15, &[1, 1])], vec![mkchamp(3, 15, &[1])]);
    assert_eq!(
        greedy_action(&b, 0),
        Action::Rite { slot: 0 },
        "equal value rites resolve to the lowest slot"
    );
}

#[test]
fn greedy_status_only_party_plays_first_usable() {
    // Steady Ward (rite 2) is a Status rite: no damaging rite exists, so the
    // policy plays the first usable rite rather than yielding.
    let b = battle(vec![mkchamp(1, 15, &[2])], vec![mkchamp(3, 15, &[1])]);
    assert_eq!(greedy_action(&b, 0), Action::Rite { slot: 0 });
}

#[test]
fn greedy_no_zeal_switches_or_yields() {
    // Zeal 0 fails Battle::new legality (rites enter with full zeal), so we
    // spend it after construction to model an exhausted late-battle state.
    let mut b = battle(
        vec![mkchamp(1, 15, &[1]), mkchamp(2, 15, &[1])],
        vec![mkchamp(3, 15, &[1])],
    );
    b.sides[0].party[0].rites[0].as_mut().unwrap().zeal_left = 0;
    assert_eq!(
        greedy_action(&b, 0),
        Action::Switch { party_idx: 1 },
        "with no usable rite the first alive reserve is the pick"
    );

    let mut solo = battle(vec![mkchamp(1, 15, &[1])], vec![mkchamp(3, 15, &[1])]);
    solo.sides[0].party[0].rites[0].as_mut().unwrap().zeal_left = 0;
    assert_eq!(
        greedy_action(&solo, 0),
        Action::Yield,
        "no rite and no reserve leaves only Yield"
    );
}

//! Engine tests: determinism, termination, chart integration, ailments,
//! stages, forced switches, yield, drain, zeal accounting.

use super::*;
use crate::data;
use crate::types::*;

fn mkchamp(species_id: u16, level: u8, rite_ids: &[u16]) -> ChampionInstance {
    let sp = data::species_by_id(species_id).expect("fixture species");
    let mut rites: [Option<RiteSlot>; 4] = [None; 4];
    for (i, id) in rite_ids.iter().enumerate() {
        let r = data::rite_by_id(*id).expect("fixture rite");
        // must be legal per learnset
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

fn audric(lvl: u8, extra: &[u16]) -> ChampionInstance {
    let mut r = vec![1u16];
    r.extend_from_slice(extra);
    mkchamp(1, lvl, &r)
}

fn battle(pa: Vec<ChampionInstance>, pb: Vec<ChampionInstance>, seed: u8) -> Battle {
    Battle::new(pa, pb, [seed; 32]).expect("legal fixture battle")
}

fn rite(slot: u8) -> Action {
    Action::Rite { slot }
}

// Calibration constants for the scripted tests below: seeds chosen (via a
// one-off scan over 0..=40) so the seeded rolls land as the test requires.
const RECOIL_SEED: u8 = 21;
const SLEEP_SWITCH_SEED: u8 = 5;

#[test]
fn determinism_same_seed_same_log() {
    let script = |seed: u8| {
        let mut b = battle(vec![audric(20, &[2])], vec![mkchamp(3, 20, &[1, 6])], seed);
        for _ in 0..8 {
            let _ = b.apply_turn(rite(0), rite(0));
        }
        b.events().to_vec()
    };
    assert_eq!(script(42), script(42));
    assert_ne!(script(42), script(43), "different seeds should diverge");
}

#[test]
fn terminates_with_winner_under_500_turns() {
    for seed in [1u8, 7, 99, 200] {
        let mut b = battle(
            vec![audric(25, &[2]), audric(25, &[2])],
            vec![mkchamp(3, 25, &[1, 6]), mkchamp(3, 25, &[1, 6])],
            seed,
        );
        // tiny deterministic xorshift for action selection
        let mut x = seed as u32 + 1;
        let mut turns = 0;
        let outcome = loop {
            if let Some(w) = b.winner {
                break Some(w);
            }
            if turns >= 500 {
                break None;
            }
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            for pending_side in 0..2u8 {
                if b.legal_actions(pending_side)
                    .iter()
                    .all(|a| matches!(a, Action::Switch { .. }))
                {
                    // forced switch pending
                    let target = b.legal_actions(pending_side)[0];
                    if let Action::Switch { party_idx } = target {
                        b.apply_forced_switch(pending_side, party_idx).unwrap();
                    }
                }
            }
            let la = b.legal_actions(0);
            let lb = b.legal_actions(1);
            let a = la[(x as usize) % la.len()];
            let act_b = lb[((x >> 8) as usize) % lb.len()];
            if b.apply_turn(a, act_b).is_err() {
                break None;
            }
            turns += 1;
        };
        assert_eq!(outcome.is_some(), true, "seed {seed} should terminate");
    }
}

#[test]
fn effectiveness_events_match_chart() {
    // Humility rite vs Pride sin-spawn: 200. vs Patience champion: 100.
    let mut b1 = battle(vec![audric(20, &[])], vec![mkchamp(101, 20, &[8])], 5);
    let _ = b1.apply_turn(rite(0), rite(0));
    let d = b1
        .events()
        .iter()
        .find(|e| matches!(e, BattleEvent::Damage { side: 1, .. }));
    match d {
        Some(BattleEvent::Damage {
            effectiveness_pct, ..
        }) => assert_eq!(*effectiveness_pct, 200),
        other => panic!("expected damage event vs sin, got {other:?}"),
    }
}

#[test]
fn effectiveness_neutral_vs_patience() {
    let mut b = battle(vec![audric(20, &[])], vec![mkchamp(3, 20, &[1, 6])], 5);
    let _ = b.apply_turn(rite(0), rite(0));
    let effs: Vec<u8> = b
        .events()
        .iter()
        .filter_map(|e| match e {
            BattleEvent::Damage {
                side: 1,
                effectiveness_pct,
                ..
            } => Some(*effectiveness_pct),
            _ => None,
        })
        .collect();
    assert!(!effs.is_empty());
    assert!(effs.iter().all(|&e| e == 100));
}

#[test]
fn zeal_exhaustion_errors_nozeal() {
    // rite 6 zeal = 10 (Bastien learns at 9)
    let mut b = battle(vec![mkchamp(3, 20, &[6])], vec![mkchamp(3, 20, &[6])], 9);
    for _ in 0..10 {
        b.apply_turn(rite(0), rite(0)).expect("first ten turns ok");
    }
    let err = b.apply_turn(rite(0), rite(0)).unwrap_err();
    assert_eq!(err, BattleError::NoZeal);
}

#[test]
fn slumber_blocks_actions() {
    // Caster: Mabel (learns rite 5 at 7) at 35 so she can tank detection-turn
    // hits; sleeper: Audric. During the watch window the casting side spends
    // its action switching between two Mabels (pure pass), so Litany is never
    // re-applied and wake-up is observable.
    let mut b = battle(
        vec![mkchamp(2, 35, &[5]), mkchamp(2, 35, &[5])],
        vec![audric(20, &[])],
        3,
    );
    let mut applied = false;
    for _ in 0..6 {
        let ev = b.apply_turn(rite(0), rite(0)).unwrap();
        applied |= ev.iter().any(|e| {
            matches!(
                e,
                BattleEvent::AilmentApplied {
                    side: 1,
                    ailment: Ailment::Slumber
                }
            )
        });
        if applied {
            break;
        }
    }
    assert!(applied, "slumber should land within 6 turns (acc 90)");
    // watch window: side 0 passes by swapping its two Mabels
    let mut woke = false;
    let mut toggle = 1u8;
    for _ in 0..4 {
        if b.sides[1].active().ailment != Some(Ailment::Slumber) {
            woke = true;
            break;
        }
        let ev = b
            .apply_turn(Action::Switch { party_idx: toggle }, rite(0))
            .unwrap();
        toggle = 1 - toggle;
        assert!(
            !ev.iter()
                .any(|e| matches!(e, BattleEvent::RiteUsed { side: 1, .. })),
            "slumbering side must not act"
        );
    }
    woke |= b.sides[1].active().ailment.is_none();
    assert!(woke, "slumber must wear off within the watch window");
}

#[test]
fn stages_reset_on_switch() {
    let mut b = battle(
        vec![audric(20, &[2]), audric(20, &[])],
        vec![mkchamp(3, 20, &[1])],
        11,
    );
    // slot 1 = rite 2 (Steady Ward: +Guard self)
    let _ = b.apply_turn(rite(1), rite(0)).unwrap();
    assert!(b.sides[0].active_state.stages[stat_idx(BattleStat::Guard)] > 0);
    let _ = b
        .apply_turn(Action::Switch { party_idx: 1 }, rite(0))
        .unwrap();
    assert!(b.sides[0].active_state.stages.iter().all(|&s| s == 0));
}

#[test]
fn forced_switch_flow() {
    // level 50 Pride sin-spawn vs two level 5 Audrics -> quick KO
    let mut b = battle(
        vec![audric(5, &[]), audric(5, &[])],
        vec![mkchamp(101, 50, &[8])],
        13,
    );
    let mut saw_forced = false;
    for _ in 0..10 {
        if b.winner.is_some() {
            break;
        }
        let ev = b.apply_turn(rite(0), rite(0)).unwrap();
        if ev
            .iter()
            .any(|e| matches!(e, BattleEvent::ForcedSwitchNeeded { side: 0 }))
        {
            saw_forced = true;
            break;
        }
    }
    assert!(saw_forced);
    assert_eq!(b.apply_forced_switch(0, 0), Err(BattleError::BadSwitch));
    b.apply_forced_switch(0, 1).expect("reserve alive");
    assert_eq!(b.sides[0].active_idx, 1);
}

#[test]
fn yield_ends_battle() {
    let mut b = battle(vec![audric(20, &[])], vec![mkchamp(3, 20, &[1])], 17);
    let ev = b.apply_turn(Action::Yield, rite(0)).unwrap();
    assert!(ev
        .iter()
        .any(|e| matches!(e, BattleEvent::BattleEnded { winner: 1 })));
    assert_eq!(b.winner, Some(1));
    assert_eq!(b.apply_turn(rite(0), rite(0)), Err(BattleError::BattleOver));
}

#[test]
fn drain_heals_attacker_capped() {
    // Mabel (side1) takes a hit, then drains with rite 4 (learns at 1).
    let mut b = battle(vec![audric(20, &[])], vec![mkchamp(2, 20, &[4])], 19);
    let _ = b.apply_turn(rite(0), rite(0)).unwrap();
    let hurt = b.sides[1].active().current_resolve;
    let max = b.sides[1].computed().max_resolve;
    assert!(hurt < max, "mabel should have taken damage first");
    let ev = b.apply_turn(rite(0), rite(0)).unwrap();
    let healed = ev
        .iter()
        .any(|e| matches!(e, BattleEvent::Healed { side: 1, amount } if *amount > 0));
    assert!(healed, "drain should heal: {ev:?}");
    assert!(b.sides[1].active().current_resolve <= max);
}

#[test]
fn no_panic_across_random_actions_many_seeds() {
    for seed in 0..16u8 {
        let mut b = battle(
            vec![mkchamp(2, 15, &[1, 5]), audric(15, &[2])],
            vec![mkchamp(3, 15, &[1, 6]), mkchamp(101, 15, &[8])],
            seed,
        );
        let mut x = seed as u32 + 7;
        for _ in 0..200 {
            if b.winner.is_some() {
                break;
            }
            for s in 0..2u8 {
                let la = b.legal_actions(s);
                if !la.is_empty() && la.iter().all(|a| matches!(a, Action::Switch { .. })) {
                    if let Action::Switch { party_idx } = la[(x as usize) % la.len()] {
                        let _ = b.apply_forced_switch(s, party_idx);
                    }
                }
            }
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            let la = b.legal_actions(0);
            let lb = b.legal_actions(1);
            if la.is_empty() || lb.is_empty() {
                break;
            }
            let _ = b.apply_turn(
                la[(x as usize) % la.len()],
                lb[((x >> 8) as usize) % lb.len()],
            );
        }
    }
}

#[test]
fn recoil_double_ko_keeps_first_decided_winner() {
    // Side 1 KOs side 0's last champion with Glorious Burden (rite 19,
    // Recoil 33%); the attacker sits at 1 hp so its own recoil would finish
    // it too. The first faint must stand: exactly one BattleEnded in the
    // whole log, winner == 1 (the defender fainted first).
    let mut romilda = mkchamp(10, 20, &[19]); // learns Glorious Burden at 16
    romilda.current_resolve = 1;
    let mut b = Battle::new(vec![audric(1, &[])], vec![romilda], [RECOIL_SEED; 32])
        .expect("legal fixture battle");
    let ev = b.apply_turn(rite(0), rite(0)).expect("turn resolves");
    let battle_ended: Vec<&BattleEvent> = b
        .events()
        .iter()
        .filter(|e| matches!(e, BattleEvent::BattleEnded { .. }))
        .collect();
    assert_eq!(
        battle_ended.len(),
        1,
        "exactly one BattleEnded expected: {:?}",
        b.events()
    );
    assert!(
        matches!(battle_ended[0], BattleEvent::BattleEnded { winner: 1 }),
        "winner must be the side whose foe fainted first: {:?}",
        b.events()
    );
    assert!(ev
        .iter()
        .any(|e| matches!(e, BattleEvent::BattleEnded { winner: 1 })));
    assert_eq!(b.winner, Some(1));
}

#[test]
fn slumber_wears_off_when_champion_leaves_field() {
    // Side 0 Mabel casts Litany of Stillness (rite 5) on side 1's Audric;
    // next turn side 1 switches to its reserve. The outgoing champion's
    // Slumber must be cleared and AilmentEnded emitted.
    let mut b = battle(
        vec![mkchamp(2, 20, &[5])],
        vec![audric(20, &[]), audric(20, &[])],
        SLEEP_SWITCH_SEED,
    );
    let ev1 = b.apply_turn(rite(0), rite(0)).unwrap();
    assert!(
        ev1.iter().any(|e| matches!(
            e,
            BattleEvent::AilmentApplied {
                side: 1,
                ailment: Ailment::Slumber
            }
        )),
        "litany must land on turn 1 with this seed: {ev1:?}"
    );
    assert_eq!(b.sides[1].party[0].ailment, Some(Ailment::Slumber));
    let ev2 = b
        .apply_turn(rite(0), Action::Switch { party_idx: 1 })
        .unwrap();
    assert_eq!(
        b.sides[1].party[0].ailment, None,
        "outgoing sleeper must wake on leaving the field"
    );
    assert!(
        ev2.iter().any(|e| matches!(
            e,
            BattleEvent::AilmentEnded {
                side: 1,
                ailment: Ailment::Slumber
            }
        )),
        "AilmentEnded(Slumber) expected on switch-out: {ev2:?}"
    );
}

#[test]
fn battle_rejects_champion_with_zero_resolve() {
    let mut dead = audric(20, &[]);
    dead.current_resolve = 0;
    let err = Battle::new(
        vec![audric(20, &[]), dead],
        vec![audric(20, &[])],
        [1u8; 32],
    )
    .err()
    .expect("zero-resolve champion must not enter battle");
    assert_eq!(
        err,
        BattleError::IllegalChampion(1, InstanceError::NotBattleReady)
    );
}

/// Fixed mixed script (rites + switches) run twice with the same seed; the
/// FULL serialized battle state must be identical after every turn, not
/// just the emitted events.
#[test]
fn replay_full_state_identical() {
    let run = || {
        let mut b = battle(
            vec![audric(20, &[2]), audric(20, &[])],
            vec![mkchamp(3, 20, &[1, 6]), mkchamp(2, 20, &[1, 4])],
            42,
        );
        let script: [(Action, Action); 8] = [
            (rite(0), rite(0)),
            (rite(1), rite(0)),
            (Action::Switch { party_idx: 1 }, rite(1)),
            (rite(0), rite(0)),
            (Action::Switch { party_idx: 0 }, rite(0)),
            (rite(0), rite(1)),
            (rite(1), rite(0)),
            (rite(0), rite(0)),
        ];
        let mut snaps = Vec::with_capacity(script.len());
        for (a, act) in script {
            b.apply_turn(a, act).expect("scripted turn must resolve");
            snaps.push(serde_json::to_string(&b).expect("battle serializes"));
        }
        snaps
    };
    assert_eq!(run(), run());
}

/// FNV-1a 64-bit over bytes; test-local helper for the golden digest.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Golden digest of a fixed 12-turn script under seed [7;32] (includes a
/// slumber attempt and stage changes). INTENTIONAL logic changes must
/// recalibrate the constant (run with --nocapture to see the new value).
#[test]
fn golden_digest() {
    const GOLDEN_DIGEST: u64 = 0xcc09aa9f2b276900;
    // eprintln calibration line; keep it so recalibration stays easy.
    let mut b = Battle::new(
        vec![mkchamp(12, 30, &[5, 1, 26]), audric(20, &[2])],
        vec![mkchamp(3, 12, &[1, 6]), mkchamp(3, 12, &[1, 6])],
        [7u8; 32],
    )
    .expect("legal fixture battle");
    let script: [(Action, Action); 12] = [
        (rite(0), rite(1)), // slumber attempt; Bastien Oath of Patience
        (rite(2), rite(0)),
        (rite(0), rite(0)),
        (Action::Switch { party_idx: 1 }, rite(1)),
        (rite(1), rite(0)), // Audric: Steady Ward stage change
        (rite(0), rite(1)),
        (rite(0), rite(0)),
        (Action::Switch { party_idx: 0 }, rite(0)),
        (rite(0), rite(0)),
        (rite(1), rite(0)),
        (rite(0), rite(0)),
        (rite(2), rite(1)),
    ];
    for (a, act) in script {
        b.apply_turn(a, act).expect("scripted turn must resolve");
    }
    let mut blob = serde_json::to_string(&b).expect("battle serializes");
    blob.push_str(&serde_json::to_string(b.events()).expect("events serialize"));
    let h = fnv1a64(blob.as_bytes());
    eprintln!("golden_digest = {h:#018x}");
    assert_eq!(
        h, GOLDEN_DIGEST,
        "battle logic changed; if intentional, recalibrate GOLDEN_DIGEST"
    );
}

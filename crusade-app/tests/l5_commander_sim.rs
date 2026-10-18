//! L5 COMMANDER LEARNSET SIM (docs/L5-SIM.md): measures the documented-
//! but-unapplied proposal from docs/FUZZ-ADVERSARIES.md § Residuals —
//! pushing the named commanders' strong L1-lane rites to learn level 6
//! (the same lever the archetypes received on 2026-10-17) — without
//! touching any data table. The override learnsets below are a COPY of
//! the committed ones with the pushed entries re-leveled in memory.
//!
//! MEASUREMENT LANE ONLY: every test is `#[ignore]`d; run by hand with:
//!
//! ```text
//! cargo test -p crusade-app --test l5_commander_sim --release -- --ignored --nocapture
//! ```
//!
//! Two lanes:
//!
//! - LANE A (pool fuzz mirror, the lane the proposal was written for):
//!   crusade-sim `fuzz adversaries` semantics at L5 — random 3-man
//!   teams, champions vs the full committed adversary pool
//!   (108..=117 + 120..=125; crusade-sim pins 120..=123, this lane adds
//!   the Hattin wings so the whole named tier is measured) — current
//!   learnsets vs the APPLIED overrides. Champions-side aggregate and
//!   per-commander win rates are the accept/reject signal.
//! - LANE B (gate matchlets, the "gates are the wall" check
//!   (docs/TRAIL-BATTLE-DEPTH.md: 1.9% measured, ≤35% band)): each
//!   landed gate of docs/GATE-CONTENT.md §0 — escorts at captain-2
//!   (clamped 5..=50), boss at captain+offset — vs a lone captain on
//!   the soak's veteran recipe, swept over captain levels
//!   {3, 15, 25, 40, 50}. The L3 row is the diagnostic: the only cell
//!   where a 6+ learn level can still bite a boss (captain <= 3 ⇒ boss
//!   <= 5/6). Records captain win% and battle length, before vs after.

use crusade_app::policy::greedy_action;
use crusade_core::data;
use crusade_core::engine::{Action, Battle};
use crusade_core::rng::BattleRng;
use crusade_core::types::{
    BattleStat, ChampionInstance, DevotionStats, GraceStats, LearnEntry, RiteSlot, Secondary,
    MAX_LEVEL,
};

fn draw_id(rng: &mut BattleRng, champions: bool) -> u16 {
    if champions {
        rng.range_u32(1, 12) as u16 // species 1..=12 (range is inclusive)
    } else {
        ADVERSARY_POOL[rng.range_u32(0, ADVERSARY_POOL.len() as u32 - 1) as usize]
    }
}

const MY_SIDE: u8 = 0;
const FOE_SIDE: u8 = 1;
const TURN_CAP: u32 = 4000;

/// The named commander tier (docs/HISTORICAL-FOES.md section 5).
const COMMANDER_IDS: &[u16] = &[120, 121, 122, 123, 124, 125];

/// Gates of record (docs/GATE-CONTENT.md §0; landed in
/// crusade-core/src/data/overworld.rs): (battle_ref, escorts, boss,
/// boss level offset).
const GATES: &[(u16, [u16; 2], u16, u8)] = &[
    (101, [108, 109], 120, 2),
    (102, [114, 116], 121, 2),
    (103, [110, 116], 122, 2),
    (104, [116, 114], 124, 2),
    (105, [116, 117], 125, 3),
    (108, [116, 117], 123, 3),
];

/// Captain levels swept in lane B. 3 is the diagnostic floor (the only
/// level at which a boss can stand at 5 with the pushed lanes still
/// off his kit); 15/25/40/50 span the plausible captain states at the
/// screens 13..=25 gates.
const CAPTAIN_LEVELS: &[u8] = &[3, 15, 25, 40, 50];

/// The proposal's applied learnsets (in-memory override; data tables
/// untouched). Rule: the pushed strong early rites land at level 6 and
/// Measured Blow (rite 1) holds the level-1 slot where a commander had
/// no rite 1 of his own — the exact treatment the archetypes received
/// (FUZZ-ADVERSARIES.md, every archetype "gaining Measured Blow at
/// level 1"), and the only way to leave no L1..=5 kit riteless.
/// Commanders 120..=123 are as documented; 124 (Harass and Fade 51) and
/// 125 (Couched Lance 45) ride the same L1 lanes, so the lever is
/// extended to them by analogy — flagged in docs/L5-SIM.md.
fn applied_learnset(id: u16) -> Option<&'static [LearnEntry]> {
    const KILIJ: &[LearnEntry] = &[
        LearnEntry { level: 1, rite_id: 1 },
        LearnEntry { level: 6, rite_id: 43 },
        LearnEntry { level: 6, rite_id: 51 },
        LearnEntry { level: 10, rite_id: 44 },
    ];
    const NUR: &[LearnEntry] = &[
        LearnEntry { level: 1, rite_id: 1 },
        LearnEntry { level: 5, rite_id: 47 },
        LearnEntry { level: 6, rite_id: 45 },
        LearnEntry { level: 10, rite_id: 53 },
        LearnEntry { level: 14, rite_id: 56 },
    ];
    const SHIRKUH: &[LearnEntry] = &[
        LearnEntry { level: 1, rite_id: 1 },
        LearnEntry { level: 6, rite_id: 49 },
        LearnEntry { level: 6, rite_id: 51 },
        LearnEntry { level: 8, rite_id: 53 },
        LearnEntry { level: 12, rite_id: 57 },
    ];
    const SALADIN: &[LearnEntry] = &[
        LearnEntry { level: 1, rite_id: 1 },
        LearnEntry { level: 6, rite_id: 47 },
        LearnEntry { level: 6, rite_id: 57 },
        LearnEntry { level: 10, rite_id: 56 },
        LearnEntry { level: 14, rite_id: 18 },
    ];
    const GOKBORI: &[LearnEntry] = &[
        LearnEntry { level: 1, rite_id: 1 },
        LearnEntry { level: 6, rite_id: 51 },
        LearnEntry { level: 6, rite_id: 43 },
        LearnEntry { level: 10, rite_id: 57 },
        LearnEntry { level: 14, rite_id: 56 },
    ];
    const TAQI: &[LearnEntry] = &[
        LearnEntry { level: 1, rite_id: 1 },
        LearnEntry { level: 6, rite_id: 45 },
        LearnEntry { level: 6, rite_id: 53 },
        LearnEntry { level: 10, rite_id: 47 },
        LearnEntry { level: 14, rite_id: 57 },
    ];
    match id {
        120 => Some(KILIJ),
        121 => Some(NUR),
        122 => Some(SHIRKUH),
        123 => Some(SALADIN),
        124 => Some(GOKBORI), // extended by analogy (51 was his L1 lane)
        125 => Some(TAQI),    // extended by analogy (45 was his L1 lane)
        _ => None,
    }
}

// ---------- Fixture construction ----------

/// Kit for `species_id` at `level` under `learnset_override` (None =
/// the committed table), crusade-sim/crusade-app practice semantics:
/// up to 4 rites, most recent learnset entries first.
fn kit(species_id: u16, level: u8, learnset_override: Option<&[LearnEntry]>) -> Vec<u16> {
    let sp = data::species_by_id(species_id).expect("species table");
    let learnset: &[LearnEntry] = learnset_override.unwrap_or(sp.learnset);
    let mut picked: Vec<u16> = Vec::new();
    for rite_id in learnset
        .iter()
        .filter(|e| e.level <= level)
        .map(|e| e.rite_id)
        .rev()
    {
        if picked.len() == 4 {
            break;
        }
        if !picked.contains(&rite_id) {
            picked.push(rite_id);
        }
    }
    picked.reverse();
    picked
}

fn fill_rites(c: &mut ChampionInstance, rite_ids: &[u16]) {
    for (i, id) in rite_ids.iter().enumerate().take(4) {
        let r = data::rite_by_id(*id).expect("rite table");
        c.rites[i] = Some(RiteSlot {
            rite_id: *id,
            zeal_left: r.zeal,
        });
    }
}

/// The fuzz/practice foe body: zero grace/devotion, learnset kit.
fn practice_foe(species_id: u16, level: u8, applied: bool) -> ChampionInstance {
    let sp = data::species_by_id(species_id).expect("species table");
    let rite_ids = kit(
        species_id,
        level,
        if applied {
            applied_learnset(species_id)
        } else {
            None
        },
    );
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
        rites: [None; 4],
        current_resolve: 0,
        ailment: None,
        converted: false,
        converted_from: None,
        typing_override: None,
    };
    fill_rites(&mut c, &rite_ids);
    c.current_resolve = data::compute_stats(&c, sp).max_resolve;
    c
}

/// The pilgrim captain on the soak's veteran recipe (grace 15,
/// devotion 22/21 lane) with his own species learnset kit at `level`.
fn veteran_captain(species_id: u16, level: u8) -> ChampionInstance {
    let sp = data::species_by_id(species_id).expect("species table");
    let rite_ids = kit(species_id, level, None);
    let mut c = ChampionInstance {
        species_id,
        level,
        xp: 0,
        grace: GraceStats {
            resolve: 15,
            valour: 15,
            piety: 15,
            guard: 15,
            aegis: 15,
            alacrity: 15,
        },
        devotion: DevotionStats {
            resolve: 22,
            valour: 22,
            piety: 22,
            guard: 22,
            aegis: 21,
            alacrity: 21,
        },
        calling_id: 0,
        rites: [None; 4],
        current_resolve: 0,
        ailment: None,
        converted: false,
        converted_from: None,
        typing_override: None,
    };
    fill_rites(&mut c, &rite_ids);
    c.current_resolve = data::compute_stats(&c, sp).max_resolve;
    c
}

// ---------- Pilgrim-side fight policy (the storage soak's recipe) ----------

fn best_attack(b: &Battle) -> Option<u8> {
    let mut best: Option<(u8, u8)> = None;
    for (i, slot) in b.sides[usize::from(MY_SIDE)].party[b.sides[usize::from(MY_SIDE)].active_idx]
        .rites
        .iter()
        .enumerate()
    {
        let Some(slot) = slot else { continue };
        if slot.zeal_left == 0 {
            continue;
        }
        let Some(r) = data::rite_by_id(slot.rite_id) else {
            continue;
        };
        if r.power == 0 {
            continue;
        }
        if best.is_none_or(|(_, p)| r.power > p) {
            best = Some((i as u8, r.power));
        }
    }
    best.map(|(slot, _)| slot)
}

fn valour_opener(b: &Battle) -> Option<u8> {
    b.sides[usize::from(MY_SIDE)].party[b.sides[usize::from(MY_SIDE)].active_idx]
        .rites
        .iter()
        .enumerate()
        .find(|(_, slot)| {
            slot.as_ref().is_some_and(|s| {
                s.zeal_left > 0
                    && data::rite_by_id(s.rite_id).is_some_and(|r| {
                        r.power == 0
                            && r.secondaries.iter().any(|s| {
                                matches!(
                                    s,
                                    Secondary::StageOnSelf {
                                        stat: BattleStat::Valour,
                                        ..
                                    }
                                )
                            })
                    })
            })
        })
        .map(|(i, _)| i as u8)
}

/// Kit patch for hypothetical learnsets: `Battle::new` validates rites
/// against the COMMITTED learnset, which an applied-only kit cannot
/// pass. Build the battle legal, then rewrite the one member's `rites`
/// array through the serde round-trip (Battle is Serialize; the fields
/// are private but present in the JSON). Sim-only; data untouched.
fn patch_kit_json(b: &Battle, side: usize, idx: usize, kit: &[u16]) -> Battle {
    let mut v = serde_json::to_value(b).expect("battle serializes");
    let rites = &mut v["sides"][side]["party"][idx]["rites"];
    for (i, slot) in rites
        .as_array_mut()
        .expect("rites array")
        .iter_mut()
        .enumerate()
    {
        *slot = match kit.get(i) {
            Some(id) => {
                let r = data::rite_by_id(*id).expect("rite table");
                serde_json::json!({"rite_id": id, "zeal_left": r.zeal})
            }
            None => serde_json::Value::Null,
        };
    }
    serde_json::from_value(v).expect("battle deserializes")
}

/// One battle to its end; (side0 won, turns). Forced switches resolved
/// on both sides exactly as the trail soak does.
/// `foe_kit_overrides`: (party index, applied kit) patches applied
/// AFTER construction (see patch_kit_json).
fn fight(
    party: Vec<ChampionInstance>,
    foes: Vec<ChampionInstance>,
    bseed: [u8; 32],
    foe_kit_overrides: &[(usize, Vec<u16>)],
) -> (bool, u32) {
    let Ok(mut b) = Battle::new(party, foes, bseed) else {
        // Latent wart guard: refusal must never read as a side-0 win.
        panic!("battle construction refused by the engine");
    };
    for &(idx, ref kit) in foe_kit_overrides {
        b = patch_kit_json(&b, 1, idx, kit);
    }
    let mut turns = 0u32;
    while b.winner.is_none() {
        for side in [MY_SIDE, FOE_SIDE] {
            loop {
                let acts = b.legal_actions(side);
                if acts.is_empty() || !acts.iter().all(|a| matches!(a, Action::Switch { .. })) {
                    break;
                }
                let Action::Switch { party_idx } = acts[0] else { break };
                b.apply_forced_switch(side, party_idx).expect("legal");
                if b.winner.is_some() {
                    break;
                }
            }
        }
        if b.winner.is_some() {
            break;
        }
        let me = if turns < 2 {
            valour_opener(&b).map(|slot| Action::Rite { slot })
        } else {
            None
        };
        let mine = me
            .or_else(|| best_attack(&b).map(|slot| Action::Rite { slot }))
            .unwrap_or(Action::Yield);
        let acts = b.legal_actions(MY_SIDE);
        let mine = if acts.contains(&mine) { mine } else { acts[0] };
        let foe = greedy_action(&b, FOE_SIDE);
        b.apply_turn(mine, foe).expect("legal actions");
        turns += 1;
        assert!(turns < TURN_CAP, "a sim battle did not terminate");
    }
    (b.winner == Some(MY_SIDE), turns)
}

fn cell_seed(cell: u8, battle: u32) -> [u8; 32] {
    let mut s = [0u8; 32];
    s[0] = cell ^ 0x5A;
    s[1..5].copy_from_slice(&battle.to_le_bytes());
    s
}

// ---------- Lane A: L5 pool fuzz mirror ----------

/// crusade-sim adversary pool, extended with the Hattin wings so the
/// whole named tier is measured.
const ADVERSARY_POOL: &[u16] = &[
    108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 120, 121, 122, 123, 124, 125,
];

fn gen_team(rng: &mut BattleRng, level: u8, champions: bool) -> Vec<ChampionInstance> {
    (0..3)
        .map(|_| practice_foe(draw_id(rng, champions), level, false))
        .collect()
}

/// Applied-kit overrides for team members whose kit at `level` changes
/// under the proposal (commanders on their applied learnset).
fn applied_overrides(team: &[ChampionInstance], level: u8) -> Vec<(usize, Vec<u16>)> {
    team.iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let applied = kit(c.species_id, level, applied_learnset(c.species_id));
            let current = kit(c.species_id, level, None);
            (applied != current).then_some((i, applied))
        })
        .collect()
}

fn lane_a(n: usize, level: u8, applied: bool) {
    let mut side_wins = [0usize; 2];
    let mut total_turns = 0u64;
    // per-commander (appearances, wins)
    let mut per_id: std::collections::HashMap<u16, (usize, usize)> =
        COMMANDER_IDS.iter().map(|&id| (id, (0, 0))).collect();
    for s in 0..n {
        let seed = [s as u8; 32];
        let mut rng = BattleRng::from_seed(seed);
        let ta = gen_team(&mut rng, level, true);
        let tb = gen_team(&mut rng, level, false);
        let overrides = if applied {
            applied_overrides(&tb, level)
        } else {
            Vec::new()
        };
        for c in &tb {
            if let Some(e) = per_id.get_mut(&c.species_id) {
                e.0 += 1;
            }
        }
        let (won0, turns) = fight(ta.clone(), tb.clone(), seed, &overrides);
        if won0 {
            side_wins[0] += 1;
        } else {
            side_wins[1] += 1;
            for c in &tb {
                if let Some(e) = per_id.get_mut(&c.species_id) {
                    e.1 += 1;
                }
            }
        }
        total_turns += u64::from(turns);
    }
    println!(
        "  applied={} | champions-side {} / {} ({:.1}%) | avg turns {:.1}",
        applied,
        side_wins[0],
        n,
        side_wins[0] as f64 / n as f64 * 100.0,
        total_turns as f64 / n.max(1) as f64
    );
    for &id in COMMANDER_IDS {
        let (app, wins) = per_id[&id];
        let name = data::species_by_id(id).map_or("?", |sp| sp.name);
        println!(
            "    {id:>3} {name:<18} app {app:>4} wins {wins:>4} {:>5.1}%",
            if app > 0 { wins as f64 / app as f64 * 100.0 } else { f64::NAN }
        );
    }
}

#[test]
#[ignore = "lane A: L5 pool fuzz mirror, current vs applied commander learnsets (docs/L5-SIM.md)"]
fn l5_commander_pool_fuzz() {
    const N: usize = 2000;
    println!("== Lane A: champions vs adversary pool, level 5, n={} ==", N);
    println!("(kits that change at L5 under the applied learnsets:)");
    for &id in COMMANDER_IDS {
        let name = data::species_by_id(id).map_or("?", |sp| sp.name);
        println!(
            "    {id:>3} {name:<18} today {:?} -> applied {:?}",
            kit(id, 5, None),
            kit(id, 5, applied_learnset(id))
        );
    }
    lane_a(N, 5, false);
    lane_a(N, 5, true);
}

// ---------- Lane B: gate matchlets ----------

#[test]
#[ignore = "lane B: gate matchlets captain win% before/after (docs/L5-SIM.md)"]
fn l5_commander_gate_matchlets() {
    const N: u32 = 1024;
    println!("== Lane B: gate matchlets, lone veteran captain, n={} per cell ==", N);
    println!(
        "{:>5} {:>22} {:>4} {:>4} | {:>7} {:>7} | {:>6} {:>6}",
        "ref", "boss", "cap", "bosslv", "before%", "after%", "turns-", "turns+"
    );
    let mut cell = 1u8;
    for &(_bref, escorts, boss, offset) in GATES {
        for &cap in CAPTAIN_LEVELS {
            let boss_lv = cap.saturating_add(offset).min(MAX_LEVEL);
            let escort_lv = cap.saturating_sub(2).clamp(5, MAX_LEVEL);
            let mut before = (0u32, 0u64);
            let mut after = (0u32, 0u64);
            // Applied-kit patch for the boss (party index 2), if his kit
            // at boss_lv changes under the proposal.
            let applied = kit(boss, boss_lv, applied_learnset(boss));
            let current = kit(boss, boss_lv, None);
            let boss_override: &[(usize, Vec<u16>)] = if applied != current {
                &[(2, applied)]
            } else {
                &[]
            };
            for i in 0..N {
                let captain = vec![veteran_captain(7, cap)];
                let foes: Vec<ChampionInstance> = escorts
                    .iter()
                    .map(|&e| practice_foe(e, escort_lv, false))
                    .chain(std::iter::once(practice_foe(boss, boss_lv, false)))
                    .collect();
                let (w0, t0) = fight(captain.clone(), foes.clone(), cell_seed(cell, i), &[]);
                if w0 {
                    before.0 += 1;
                }
                before.1 += u64::from(t0);
                let (w1, t1) = fight(captain, foes, cell_seed(cell, i), boss_override);
                if w1 {
                    after.0 += 1;
                }
                after.1 += u64::from(t1);
            }
            let name = data::species_by_id(boss).map_or("?", |sp| sp.name);
            println!(
                "{:>5} {:>22} {:>4} {:>4} | {:>6.1}% {:>6.1}% | {:>6.1} {:>6.1}",
                _bref,
                name,
                cap,
                boss_lv,
                before.0 as f64 / N as f64 * 100.0,
                after.0 as f64 / N as f64 * 100.0,
                before.1 as f64 / N as f64,
                after.1 as f64 / N as f64,
            );
            cell += 1;
        }
    }
}

//! MAKE-CAMP policy soak (docs/RECOVERY.md, docs/RECOVERY-SIM.md): a
//! journey-math simulator for the Pilgrim Road with REAL battles (the
//! crusade-core engine, the client's own foe mixes and level rules,
//! the same fight recipe `tests/storage_soak.rs` uses), run under three
//! camping policies. This is a MEASUREMENT LANE ONLY: every test is
//! `#[ignore]`d; run by hand with:
//!
//! ```text
//! cargo test -p crusade-app --test camp_policy_sim -- --ignored --nocapture
//! ```
//!
//! Unlike the storage soak this lane does not touch `ClientGame` or the
//! document — camp is modeled at the engine level (`trail::make_camp`
//! semantics inline over a `TrailState`, dial-parameterized so the design
//! doc's dials can be swept before they are locked). What is measured is
//! the JOURNEY: days survived, supplies, battles entered/won, arrivals at
//! the screen-25 terminus by day 40, living strength at arrival, and Book
//! of the Fallen pages. The pilgrimage seed is varied per run; the fight
//! policy (two Valour-stack openers, then best damaging rite) is the
//! fixed "best Known play" recipe.

use crusade_app::policy::greedy_action;
use crusade_core::data;
use crusade_core::engine::trail::{self as trail_engine, SuccessionOutcome, TrailState};
use crusade_core::engine::{Action, Battle};
use crusade_core::rng::BattleRng;
use crusade_core::types::{
    BattleStat, ChampionInstance, DevotionStats, GraceStats, RiteSlot, Secondary, MAX_LEVEL,
};

// ---------- Journey constants (mirroring crusade-app/src/client.rs) ----------

const TRAIL_WILD_ROLL_PCT: u8 = 8;
const TRAIL_VICTORY_MORALE: i8 = 5;
const MY_SIDE: u8 = 0;
const FOE_SIDE: u8 = 1;

/// ROADMAP's worst-case crossing figure; the season clock.
const DAY_CEILING: u16 = 40;
/// Pilgrimages per configuration (the seed set).
const RUNS: u32 = 48;

/// Screens carrying a Camp prop: the shipped screen 3 plus the
/// pre-gate-cluster placements docs/RECOVERY.md section 3A directs the
/// data pass to add (before the gates at 9, 13, 16, 19/20, and 25).
const CAMP_SCREENS: [u16; 6] = [3, 8, 12, 15, 18, 21];

fn trail_day_rng(seed: [u8; 32], day: u16) -> BattleRng {
    BattleRng::for_turn(seed, u32::from(day) + 1)
}

fn trail_battle_seed(seed: [u8; 32], day: u16) -> [u8; 32] {
    let mut s = seed;
    s[0] ^= 0xB8;
    s[2..4].copy_from_slice(&day.to_le_bytes());
    s
}

// ---------- Fixtures (the storage soak's veteran recipe) ----------

fn mkveteran(species_id: u16, level: u8, rite_ids: &[u16]) -> ChampionInstance {
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
        rites,
        current_resolve: 0,
        ailment: None,
        converted: false,
        converted_from: None,
        typing_override: None,
    };
    c.current_resolve = data::compute_stats(&c, sp).max_resolve;
    c
}

/// The client's practice_champion recipe: last-4 learnset rites at level,
/// no training investment, full resolve.
fn practice_champion(species_id: u16, level: u8) -> ChampionInstance {
    let sp = data::species_by_id(species_id).expect("species table");
    let mut picked: Vec<u16> = Vec::new();
    for rite_id in sp
        .learnset
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
    let mut rites: [Option<RiteSlot>; 4] = [None; 4];
    for (i, id) in picked.iter().enumerate() {
        let r = data::rite_by_id(*id).expect("rite table");
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
        typing_override: None,
    };
    c.current_resolve = data::compute_stats(&c, sp).max_resolve;
    c
}

/// The client's wild/ambush mix tables (client.rs `trail_foe_mix`).
fn trail_foe_mix(battle_ref: u16, zone: Option<u8>) -> &'static [u16] {
    match battle_ref {
        201 => &[112, 113],
        203 => &[108, 109, 111],
        _ => match zone {
            Some(1) => &[112, 113],
            Some(3) => &[108, 109, 111],
            Some(4) => &[111, 114, 116],
            Some(5) => &[113, 110, 114],
            Some(6) => &[114, 116, 117],
            Some(7) => &[108, 109, 115],
            _ => &[112],
        },
    }
}

/// The client's gate escort tables (client.rs `gate_escort_mix`).
fn gate_escort_mix(battle_ref: u16) -> &'static [u16] {
    match battle_ref {
        101 => &[108, 109],
        102 => &[114, 116],
        103 => &[110, 116],
        104 => &[116, 114],
        105 => &[116, 117],
        108 => &[116, 117],
        _ => &[112],
    }
}

// ---------- Battle driver (the soak's best-play recipe) ----------

/// Highest-power usable damaging rite slot, lowest slot on ties.
fn best_attack(b: &Battle) -> Option<u8> {
    let mut best: Option<(u8, u8)> = None;
    for (i, slot) in b.sides[usize::from(MY_SIDE)]
        .party[b.sides[usize::from(MY_SIDE)].active_idx]
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

/// A Valour stage-stacking status rite (Zealous-Cry-style opener).
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

/// Fight one battle to its end. Returns (we_won, my party after).
fn fight(party: Vec<ChampionInstance>, foes: Vec<ChampionInstance>, bseed: [u8; 32]) -> (bool, Vec<ChampionInstance>) {
    let Ok(mut b) = Battle::new(party.clone(), foes, bseed) else {
        // Entry refused (e.g. spent zeal — the road's v1 carryover bug):
        // the battle does not build; treat as skipped, party unchanged.
        return (true, party);
    };
    let mut turns = 0u32;
    while b.winner.is_none() {
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
        // Foe's forced switches: greedy first-alive, as the client does.
        while b.winner.is_none() {
            let acts = b.legal_actions(FOE_SIDE);
            if acts.is_empty() || !acts.iter().all(|a| matches!(a, Action::Switch { .. })) {
                break;
            }
            let Action::Switch { party_idx } = acts[0] else { break };
            b.apply_forced_switch(FOE_SIDE, party_idx).expect("legal");
        }
        turns += 1;
        assert!(turns < 2000, "a road battle did not terminate");
    }
    let won = b.winner == Some(MY_SIDE);
    if !won {
        return (false, party);
    }
    // Victory routing (client.rs on_trail_battle_end): survivors march on
    // UNRESTED, zeal field-refilled so the next entry validates.
    let survivors: Vec<ChampionInstance> = b.sides[usize::from(MY_SIDE)]
        .party
        .iter()
        .filter(|c| c.current_resolve > 0)
        .cloned()
        .map(|mut c| {
            for slot in c.rites.iter_mut().flatten() {
                if let Some(r) = data::rite_by_id(slot.rite_id) {
                    slot.zeal_left = r.zeal;
                }
            }
            c
        })
        .collect();
    (true, survivors)
}

// ---------- The dial pack under test ----------

#[derive(Clone, Copy)]
struct Dials {
    heal_num: u16,
    heal_den: u16,
    cap_num: u16,
    cap_den: u16,
    morale_gain: i8,
    ambush_pct: u8,
    /// Hypothetical roguelite meta knob: bonus starting supplies, percent.
    supplies_bonus_pct: u16,
}

/// The design doc's Candidate A spec as shipped to phase 1.
const SPEC_DIALS: Dials = Dials {
    heal_num: trail_engine::CAMP_RESOLVE_PCT_NUM,
    heal_den: trail_engine::CAMP_RESOLVE_PCT_DEN,
    cap_num: trail_engine::CAMP_RESOLVE_CAP_NUM,
    cap_den: trail_engine::CAMP_RESOLVE_CAP_DEN,
    morale_gain: trail_engine::CAMP_MORALE_GAIN,
    ambush_pct: trail_engine::CAMP_AMBUSH_PCT,
    supplies_bonus_pct: 0,
};

#[derive(Clone, Copy, PartialEq)]
enum Policy {
    NeverCamp,
    AlwaysCamp,
    /// Camp only when the mantle is under 40% resolve and the stores hold
    /// more than two days' upkeep.
    Prudent,
}

struct SimConfig {
    name: &'static str,
    policy: Policy,
    dials: Dials,
    /// The storage soak's aggressive posture: engage every elective
    /// choice-encounter prop up to 3 battles/day and concede every 8th
    /// battle. The never-camp baseline keeps it so the extinction-day
    /// regression target compares like for like with docs/STORAGE-SOAK.md.
    engage_electives: bool,
}

// ---------- The journey ----------

#[derive(Default)]
struct Run {
    end_day: u16,
    extinct: bool,
    never_camped: bool,
    camps: u16,
    battles: u16,
    wins: u16,
    reached_25: bool,
    /// Living strength and Book pages at first arrival at screen 25.
    arrival_living: Option<u16>,
    arrival_book: Option<u16>,
    end_living: u16,
    end_supplies: u16,
    end_book: u16,
}

struct Jour {
    state: TrailState,
    party: Vec<ChampionInstance>,
    champion_level: u8,
    over: bool,
    seed: [u8; 32],
    run: Run,
    dials: Dials,
    aggressive: bool,
}

impl Jour {
    fn mantle_max(&self) -> u16 {
        let c = &self.party[0];
        let sp = data::species_by_id(c.species_id).expect("species table");
        data::compute_stats(c, sp).max_resolve
    }

    /// Route a finished road battle exactly as the client does
    /// (client.rs on_trail_battle_end), less the conversion queue. `self.
    /// party` already holds the victory-confirmed survivors when won.
    fn settle_battle(&mut self, won: bool) {
        if won {
            self.run.wins += 1;
            self.state.morale = (self.state.morale + TRAIL_VICTORY_MORALE).min(trail_engine::MORALE_MAX);
            return;
        }
        // Defeat: the mantle falls where the company broke — the
        // champion's own page is written first, then THE MANTLE resolves
        // over the file (client.rs on_trail_battle_end order).
        if let Some(ch) = self
            .state
            .men
            .iter_mut()
            .find(|m| m.alive && m.champion)
        {
            ch.alive = false;
            ch.champion = false;
            let name = ch.name.clone();
            self.state.fallen.push(trail_engine::FallenEntry {
                name,
                day: self.state.day,
                cause: "fell in battle upon the pilgrim road",
                where_fell: self.state.screen,
            });
        }
        let former_level = self.champion_level;
        let former_species = self.party.first().map(|c| c.species_id).unwrap_or(1);
        let mut rng = trail_day_rng(self.seed, self.state.day);
        match trail_engine::champion_falls(&mut self.state, &mut rng) {
            SuccessionOutcome::RisesInRighteousness(_) => {
                let level = former_level
                    .saturating_sub(5)
                    .saturating_add(rng.range_u8(0, 7))
                    .clamp(1, MAX_LEVEL);
                self.champion_level = level;
                self.party = vec![practice_champion(former_species, level)];
            }
            SuccessionOutcome::Reordered { .. } => {
                if self.state.living_count() == 0 {
                    self.over = true;
                    self.party.clear();
                } else {
                    let level = former_level.saturating_sub(5).max(1);
                    self.champion_level = level;
                    if let Some(m) = self.state.living_mut().next() {
                        m.champion = true;
                    }
                    self.party = vec![practice_champion(former_species, level)];
                }
            }
        }
    }

    fn battle_mix(&mut self, mix: &[u16]) {
        self.run.battles += 1;
        if self.aggressive && self.run.battles % 8 == 0 {
            // The soak's fixed cadence: a conceded fall, routed through
            // the real defeat path (FALL_EVERY_N_BATTLES = 8).
            self.settle_battle(false);
            return;
        }
        let level = self.champion_level.clamp(5, MAX_LEVEL);
        let foes: Vec<ChampionInstance> =
            mix.iter().map(|&id| practice_champion(id, level)).collect();
        let bseed = trail_battle_seed(self.seed, self.state.day);
        let (won, party) = fight(std::mem::take(&mut self.party), foes, bseed);
        self.party = party;
        self.settle_battle(won);
    }

    /// One camp day (engine `make_camp` semantics, dial-parameterized so
    /// variants can be swept without touching the shipped consts).
    fn camp(&mut self) {
        self.run.camps += 1;
        self.run.never_camped = false;
        let pre_day = self.state.day;
        let mut rng = trail_day_rng(self.seed, pre_day);
        self.state.day += 1;
        let cost = trail_engine::daily_supply_cost(self.state.living_count()) * 2;
        let affordable = self.state.supplies >= cost; // checked by caller
        debug_assert!(affordable);
        self.state.supplies = self.state.supplies.saturating_sub(cost);
        if self.state.supplies == 0 {
            self.state.morale = (self.state.morale - trail_engine::STARVATION_MORALE_COST)
                .max(trail_engine::MORALE_MIN);
        }
        self.state.morale = (self.state.morale + self.dials.morale_gain).min(trail_engine::MORALE_MAX);
        let max = self.mantle_max();
        let mend = max * self.dials.heal_num / self.dials.heal_den;
        let cap = max * self.dials.cap_num / self.dials.cap_den;
        let resolve_after = self.party[0].current_resolve.saturating_add(mend).min(cap);
        if rng.pct() < self.dials.ambush_pct {
            // The watch cries: fought from the UNMENDED resolve; the
            // balms go on only if the night is kept. The ambush fights
            // the screen's zone mix (client.rs `trail_make_camp`; a camp
            // off the zone table falls back to brigands).
            let zone = crusade_core::data::overworld::screen_by_id(self.state.screen)
                .and_then(|s| s.encounter_zone);
            let mix = match zone {
                Some(z) => trail_foe_mix(0, Some(z)),
                None => trail_foe_mix(201, None),
            };
            self.run.battles += 1;
            let level = self.champion_level.clamp(5, MAX_LEVEL);
            let foes: Vec<ChampionInstance> =
                mix.iter().map(|&id| practice_champion(id, level)).collect();
            let bseed = trail_battle_seed(self.seed, self.state.day);
            let (won, party) = fight(std::mem::take(&mut self.party), foes, bseed);
            self.party = party;
            self.settle_battle(won);
            if !won || self.over {
                return;
            }
        }
        let m = &mut self.party[0];
        m.current_resolve = resolve_after;
        m.ailment = None;
    }
}

/// One pilgrimage under one configuration.
fn run_pilgrimage(cfg: &SimConfig, seed_byte: u8) -> Run {
    let mut seed = [0u8; 32];
    for b in seed.iter_mut() {
        *b = seed_byte;
    }
    let mut crossing_rng = trail_day_rng(seed, 0);
    let out = trail_engine::the_crossing(255, 1000, &mut crossing_rng);
    let mut state = out.state;
    if cfg.dials.supplies_bonus_pct > 0 {
        state.supplies = state
            .supplies
            .saturating_add(out.supplies_carried * cfg.dials.supplies_bonus_pct / 100);
    }
    if let Some(m) = state.living_mut().next() {
        m.champion = true;
    }
    let mut j = Jour {
        state,
        party: vec![mkveteran(7, 50, &[16, 17, 15, 7])],
        champion_level: 50,
        over: false,
        seed,
        run: Run {
            never_camped: true,
            ..Run::default()
        },
        dials: cfg.dials,
        aggressive: cfg.engage_electives,
    };

    while !j.over && j.state.day < DAY_CEILING {
        if j.run.reached_25 {
            break; // the terminus is reached; the journey's purpose is met
        }
        let living = j.state.living_count();
        let cost2 = trail_engine::daily_supply_cost(living) * 2;
        let on_camp = CAMP_SCREENS.contains(&j.state.screen);
        let max = j.mantle_max();
        let resolve = j.party.first().map(|c| c.current_resolve).unwrap_or(0);
        let wants_camp = on_camp
            && j.state.supplies >= cost2
            && match cfg.policy {
                Policy::NeverCamp => false,
                Policy::AlwaysCamp => resolve < max * cfg.dials.cap_num / cfg.dials.cap_den,
                Policy::Prudent => {
                    resolve * 100 < max * 40
                        && j.state.supplies > 2 * trail_engine::daily_supply_cost(living)
                }
            };
        if wants_camp {
            j.camp();
            continue;
        }
        // March: easternmost exit; at the terminus the map bounces 24<->25.
        let cur = crusade_core::data::overworld::screen_by_id(j.state.screen).expect("screen table");
        let next = cur.edges.iter().flatten().copied().max().expect("exit");
        let pre_day = j.state.day;
        let mut rng = trail_day_rng(seed, pre_day);
        let out = trail_engine::travel_day(&mut j.state, next, &mut rng).expect("spine edge");
        let dest = crusade_core::data::overworld::screen_by_id(next).expect("screen table");
        if j.state.screen == 25 && !j.run.reached_25 {
            j.run.reached_25 = true;
            j.run.arrival_living = Some(j.state.living_count());
            j.run.arrival_book = Some(j.state.fallen.len() as u16);
        }
        if let Some(gate) = dest.scripted_gate {
            // The battle of record: two escorts at mantle-2, commander at
            // mantle+offset capped 50 (client.rs travel()).
            let lv = j.champion_level;
            let boss_lv = lv.saturating_add(gate.boss_level_offset).min(MAX_LEVEL);
            let escort_lv = lv.saturating_sub(2).clamp(5, MAX_LEVEL);
            j.run.battles += 1;
            let mut foes: Vec<ChampionInstance> = gate_escort_mix(gate.battle_ref)
                .iter()
                .take(2)
                .map(|&id| practice_champion(id, escort_lv))
                .collect();
            foes.push(practice_champion(
                gate.commander_id.expect("named"),
                boss_lv,
            ));
            let bseed = trail_battle_seed(seed, j.state.day);
            let (won, party) = fight(std::mem::take(&mut j.party), foes, bseed);
            j.party = party;
            j.settle_battle(won);
        } else if let Some(b) = out.battle_ref {
            j.battle_mix(trail_foe_mix(b, dest.encounter_zone));
        } else if let Some(zone) = dest.encounter_zone {
            if rng.pct() < TRAIL_WILD_ROLL_PCT {
                j.battle_mix(trail_foe_mix(0, Some(zone)));
            }
        }
        if j.over {
            break;
        }
        // Elective top-up under the aggressive posture (baseline only):
        // re-engage the screen's choice props to 3 battles on the day,
        // exactly the storage soak's BATTLES_PER_DAY_MAX churn.
        if cfg.engage_electives {
            let day_start_battles = j.run.battles;
            for p in dest.props {
                if let crusade_core::data::overworld::PropRef::ChoiceEncounter(_) = p {
                    while !j.over && j.run.battles < day_start_battles + 3 {
                        j.battle_mix(trail_foe_mix(0, dest.encounter_zone));
                    }
                }
            }
        }
    }
    j.run.end_day = j.state.day;
    j.run.extinct = j.over;
    j.run.end_living = j.state.living_count();
    j.run.end_supplies = j.state.supplies;
    j.run.end_book = j.state.fallen.len() as u16;
    j.run
}

// ---------- Aggregation ----------

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if v.is_empty() {
        return 0.0;
    }
    v[v.len() / 2]
}

fn report(cfg: &SimConfig, runs: &[Run]) {
    let n = runs.len() as f64;
    let extinct_days: Vec<f64> = runs
        .iter()
        .filter(|r| r.extinct)
        .map(|r| f64::from(r.end_day))
        .collect();
    let pct_any_win = runs.iter().filter(|r| r.wins > 0).count() as f64 / n * 100.0;
    let pct_reached = runs.iter().filter(|r| r.reached_25).count() as f64 / n * 100.0;
    let arrival: Vec<&Run> = runs.iter().filter(|r| r.reached_25).collect();
    let med_arr_living = median(
        arrival
            .iter()
            .filter_map(|r| r.arrival_living)
            .map(f64::from)
            .collect(),
    );
    let pct_arr_book5 = if arrival.is_empty() {
        0.0
    } else {
        arrival
            .iter()
            .filter(|r| r.arrival_book.unwrap_or(0) >= 5)
            .count() as f64
            / arrival.len() as f64
            * 100.0
    };
    let never = runs.iter().filter(|r| r.never_camped).count();
    let never_extinct_15 = runs
        .iter()
        .filter(|r| r.never_camped)
        .filter(|r| r.end_day <= 15)
        .count();
    let total_camps: u32 = runs.iter().map(|r| u32::from(r.camps)).sum();
    let total_battles: u32 = runs.iter().map(|r| u32::from(r.battles)).sum();
    let total_wins: u32 = runs.iter().map(|r| u32::from(r.wins)).sum();
    println!("\n=== {} ===", cfg.name);
    println!(
        "runs {} | median extinction day {:.1} (n_extinct {}) | >=1 win: {:.0}% | reach 25 by {}: {:.0}%",
        runs.len(),
        if extinct_days.is_empty() { f64::NAN } else { median(extinct_days.clone()) },
        extinct_days.len(),
        pct_any_win,
        DAY_CEILING,
        pct_reached
    );
    println!(
        "arrivals {} | median living at arrival {:.1} | arrivals with >=5 Book pages {:.0}%",
        arrival.len(),
        med_arr_living,
        pct_arr_book5
    );
    println!(
        "camps {} (never-camped runs {}) | battles {} | wins {} | never-camp runs extinct by day 15: {}/{}",
        total_camps,
        never,
        total_battles,
        total_wins,
        if never == never_extinct_15 { 100 } else { never_extinct_15 * 100 / never.max(1) },
        never
    );
    let med_end_supplies = median(runs.iter().map(|r| f64::from(r.end_supplies)).collect());
    println!("median end-day supplies {med_end_supplies:.0}");
}

fn sweep(cfgs: &[SimConfig]) {
    for cfg in cfgs {
        let runs: Vec<Run> = (0..RUNS).map(|s| run_pilgrimage(cfg, s as u8)).collect();
        report(cfg, &runs);
    }
}

/// The four policy lanes from the phase-1 brief.
#[test]
#[ignore = "camp policy soak: measurement lane (see docs/RECOVERY-SIM.md)"]
fn camp_policy_sweep() {
    sweep(&[
        SimConfig {
            name: "never-camp (soak posture: fights everything, electives on)",
            policy: Policy::NeverCamp,
            dials: SPEC_DIALS,
            engage_electives: true,
        },
        SimConfig {
            name: "never-camp, rational posture (no electives, no seeded falls)",
            policy: Policy::NeverCamp,
            dials: SPEC_DIALS,
            engage_electives: false,
        },
        SimConfig {
            name: "camp-whenever-at-camp-screen",
            policy: Policy::AlwaysCamp,
            dials: SPEC_DIALS,
            engage_electives: false,
        },
        SimConfig {
            name: "prudent-camp (resolve < 40%, supplies > 2 days)",
            policy: Policy::Prudent,
            dials: SPEC_DIALS,
            engage_electives: false,
        },
        SimConfig {
            name: "prudent-camp + ROGUELITE KNOB (+10% starting supplies)",
            policy: Policy::Prudent,
            dials: Dials {
                supplies_bonus_pct: 10,
                ..SPEC_DIALS
            },
            engage_electives: false,
        },
        // Sensitivity probes: even a much stronger mend (50% heal, FULL
        // cap) or a risk-free camp should not unlock the win target if
        // the battles themselves are the wall.
        SimConfig {
            name: "SENSITIVITY prudent, 50% heal to 100% cap, 10% ambush",
            policy: Policy::Prudent,
            dials: Dials {
                heal_num: 1,
                heal_den: 2,
                cap_num: 1,
                cap_den: 1,
                ..SPEC_DIALS
            },
            engage_electives: false,
        },
        SimConfig {
            name: "SENSITIVITY prudent, spec heal, 0% ambush",
            policy: Policy::Prudent,
            dials: Dials {
                ambush_pct: 0,
                ..SPEC_DIALS
            },
            engage_electives: false,
        },
    ]);
}

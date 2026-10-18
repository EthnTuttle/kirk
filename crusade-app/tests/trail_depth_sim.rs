//! TRAIL-BATTLE-DEPTH candidates (docs/TRAIL-BATTLE-DEPTH.md): a
//! journey-math simulator measuring the SHAPE of the fighting line,
//! with REAL battles (the crusade-core engine, the client's own foe
//! mixes and gate level rules, the storage soak's fight recipe). This
//! is a MEASUREMENT LANE ONLY: every test is `#[ignore]`d; run by hand
//! with:
//!
//! ```text
//! cargo test -p crusade-app --test trail_depth_sim -- --ignored --nocapture
//! ```
//!
//! Candidates (owner brief 2026-10-17, battle-depth-first sequenced
//! ahead of Make Camp wiring):
//!
//! - CURRENT: the shipped single-file line (one mantle instance vs
//!   2-3 near-peer foes) — the regression baseline.
//! - COMPANY (A): the van fights — the mantle plus up to two named
//!   vanguardsmen from the living roster, perma-dead at 0 resolve,
//!   each a page of the Book. `COMPANY_SWOON` is the rejected variant
//!   where fallen vanguardsmen are only left for dead (unconscious),
//!   measured to show what it costs the Book.
//! - ASYMMETRIC (B): single-file champions kept; wild/ambush foe
//!   tables rebalanced by region (1 foe early, 2 mid, 3 only at
//!   gates). Gates unchanged.
//! - SKIRMISH (C): wild/ambush battles become a 1v1 duel of the van
//!   against the enemy picket; the loser withdraws (road defeat costs
//!   resolve and morale, not the captain). Gates unchanged.
//!
//! COMPANY_CAMP pairs the winning shape with the spec'd Make Camp
//! dials (prudent policy) to check the waiting recovery layer engages
//! once a wound economy exists.

use crusade_app::policy::greedy_action;
use crusade_core::data;
use crusade_core::engine::trail::{self as trail_engine, SuccessionOutcome, TrailState};
use crusade_core::engine::{Action, Battle};
use crusade_core::rng::BattleRng;
use crusade_core::types::{
    BattleStat, ChampionInstance, DevotionStats, GraceStats, RiteSlot, Secondary, MAX_LEVEL,
};

const TRAIL_WILD_ROLL_PCT: u8 = 8;
const TRAIL_VICTORY_MORALE: i8 = 5;
const MY_SIDE: u8 = 0;
const FOE_SIDE: u8 = 1;
const DAY_CEILING: u16 = 40;
const RUNS: u32 = 48;
/// Camp screens per docs/RECOVERY.md section 3A (only the COMPANY_CAMP
/// lane pitches tents; it is the interaction check, nothing more).
const CAMP_SCREENS: [u16; 6] = [3, 8, 12, 15, 18, 21];
/// The van: at most this many named men stand beside the captain
/// (the design default; the sim sweeps it).
const _: usize = 2;
/// A skirmish lost: the van withdraws at this fraction of max resolve.
const SKIRMISH_WITHDRAW_NUM: u16 = 3;
const SKIRMISH_WITHDRAW_DEN: u16 = 20;
/// ..and the company counts the shame of it in morale.
const SKIRMISH_MORALE_COST: i8 = 3;

fn trail_day_rng(seed: [u8; 32], day: u16) -> BattleRng {
    BattleRng::for_turn(seed, u32::from(day) + 1)
}

fn trail_battle_seed(seed: [u8; 32], day: u16) -> [u8; 32] {
    let mut s = seed;
    s[0] ^= 0xB8;
    s[2..4].copy_from_slice(&day.to_le_bytes());
    s
}

// ---------- Fixtures (the storage soak's recipes, verbatim) ----------

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

// ---------- The client's tables ----------

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

/// Candidate B's rebalanced wild/ambush tables: one foe in the home
/// marches and upon the open road, two in the deep marches, three only
/// at the gates (gate tables themselves are untouched).
fn trail_foe_mix_asym(battle_ref: u16, zone: Option<u8>) -> &'static [u16] {
    let full = trail_foe_mix(battle_ref, zone);
    let shallow = &full[..1];
    let mid = &full[..full.len().min(2)];
    match battle_ref {
        0 => match zone {
            Some(1 | 2) => shallow,
            _ => mid,
        },
        201 => shallow,
        _ => mid,
    }
}

// ---------- Battle driver (soak recipe, switching handled both ways) ----------

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

/// Fight one battle to its end, any party depth. Returns (we_won, our
/// WHOLE post-battle party — felled members still present at 0 resolve
/// so the caller can write their pages; the living carry their wounds
/// forward, zeal field-refilled so the next entry validates).
fn fight_full(
    party: Vec<ChampionInstance>,
    foes: Vec<ChampionInstance>,
    bseed: [u8; 32],
) -> (bool, Vec<ChampionInstance>) {
    let Ok(mut b) = Battle::new(party.clone(), foes, bseed) else {
        // Entry refused (e.g. spent zeal): the battle does not build;
        // treat as skipped, party unchanged.
        return (true, party);
    };
    let mut turns = 0u32;
    while b.winner.is_none() {
        // Our forced switches when the active man has fallen.
        loop {
            let acts = b.legal_actions(MY_SIDE);
            if acts.is_empty() || !acts.iter().all(|a| matches!(a, Action::Switch { .. })) {
                break;
            }
            let Action::Switch { party_idx } = acts[0] else { break };
            b.apply_forced_switch(MY_SIDE, party_idx).expect("legal");
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
        while b.winner.is_none() {
            let acts = b.legal_actions(FOE_SIDE);
            if acts.is_empty() || !acts.iter().all(|a| matches!(a, Action::Switch { .. })) {
                break;
            }
            let Action::Switch { party_idx } = acts[0] else { break };
            b.apply_forced_switch(FOE_SIDE, party_idx).expect("legal");
        }
        turns += 1;
        assert!(turns < 4000, "a road battle did not terminate");
    }
    let won = b.winner == Some(MY_SIDE);
    let post: Vec<ChampionInstance> = b.sides[usize::from(MY_SIDE)]
        .party
        .iter()
        .cloned()
        .map(|mut c| {
            if c.current_resolve > 0 {
                for slot in c.rites.iter_mut().flatten() {
                    if let Some(r) = data::rite_by_id(slot.rite_id) {
                        slot.zeal_left = r.zeal;
                    }
                }
            }
            c
        })
        .collect();
    (won, post)
}

// ---------- The journey ----------

#[derive(Clone, Copy, PartialEq)]
enum Depth {
    /// The shipped single-file line.
    Current,
    /// Candidate A: the van fights (perma-dead vanguardsmen).
    Company,
    /// Candidate A rejected variant: felled vanguardsmen are left for
    /// dead but recover (no Book page) — measured to show the cost.
    CompanySwoon,
    /// Candidate B: single file kept, foe tables thinned off the gates.
    Asymmetric,
    /// Candidate C: wild/ambush battles are 1v1 picket duels; the loser
    /// withdraws wounded, nobody dies off the gates.
    Skirmish,
}

struct SimConfig {
    name: &'static str,
    depth: Depth,
    /// Prudent Make Camp, spec dials (the interaction check lane).
    prudent_camp: bool,
    /// Vanguardsmen mint at the captain's level less this.
    van_off: u8,
    /// false: gates of record are the captain's own fight (the van
    /// holds the road-screens only); true: the van stands at gates too.
    van_at_gates: bool,
    /// How many named men stand beside the captain on the road.
    van_slots: usize,
}

/// A named man of the van: his roster identity and his battle body.
struct Vanguard {
    name: String,
    champ: ChampionInstance,
}

#[derive(Default)]
struct Run {
    end_day: u16,
    extinct: bool,
    road_battles: u16,
    road_wins: u16,
    gate_battles: u16,
    gate_wins: u16,
    reached_25: bool,
    arrival_living: Option<u16>,
    arrival_book: Option<u16>,
    end_living: u16,
    end_book: u16,
    vanguard_pages: u16,
    camps: u16,
}

struct Jour {
    state: TrailState,
    mantle: ChampionInstance,
    champion_level: u8,
    vanguard: Vec<Vanguard>,
    over: bool,
    seed: [u8; 32],
    run: Run,
    depth: Depth,
    van_off: u8,
    van_at_gates: bool,
    van_slots: usize,
}

impl Jour {
    fn mantle_max(&self) -> u16 {
        let sp = data::species_by_id(self.mantle.species_id).expect("species table");
        data::compute_stats(&self.mantle, sp).max_resolve
    }

    fn book_page(&mut self, name: String, cause: &'static str) {
        let day = self.state.day;
        let where_fell = self.state.screen;
        self.state.fallen.push(trail_engine::FallenEntry {
            name,
            day,
            cause,
            where_fell,
        });
        self.run.end_book = self.state.fallen.len() as u16;
    }

    /// Defeat routing exactly as client.rs on_trail_battle_end writes
    /// it: the captain's page first, then THE MANTLE over the file.
    fn settle_mantle_fallen(&mut self) {
        if let Some(ch) = self
            .state
            .men
            .iter_mut()
            .find(|m| m.alive && m.champion)
        {
            ch.alive = false;
            ch.champion = false;
            let name = ch.name.clone();
            self.book_page(name, "fell in battle upon the pilgrim road");
        }
        let former_level = self.champion_level;
        let former_species = self.mantle.species_id;
        let mut rng = trail_day_rng(self.seed, self.state.day);
        match trail_engine::champion_falls(&mut self.state, &mut rng) {
            SuccessionOutcome::RisesInRighteousness(_) => {
                let level = former_level
                    .saturating_sub(5)
                    .saturating_add(rng.range_u8(0, 7))
                    .clamp(1, MAX_LEVEL);
                self.champion_level = level;
                self.mantle = practice_champion(former_species, level);
            }
            SuccessionOutcome::Reordered { .. } => {
                if self.state.living_count() == 0 {
                    self.over = true;
                } else {
                    let level = former_level.saturating_sub(5).max(1);
                    self.champion_level = level;
                    if let Some(m) = self.state.living_mut().next() {
                        m.champion = true;
                    }
                    self.mantle = practice_champion(former_species, level);
                }
            }
        }
    }

    /// The van is drawn from the living roster (Candidate A): dead men
    /// are struck off; vacant places are filled by living men of the
    /// file, minted at the captain's level less VANGUARD_LEVEL_OFF.
    /// A depleted file simply fights shorter; a lone captain fights
    /// alone (the mercy rule is arithmetic, not a switch).
    fn sync_vanguard(&mut self) {
        self.vanguard.retain(|v| {
            self.state
                .men
                .iter()
                .any(|m| m.alive && !m.champion && m.name == v.name)
        });
        let level = self.champion_level
            .saturating_sub(self.van_off)
            .clamp(5, MAX_LEVEL);
        let species = self.mantle.species_id;
        while self.vanguard.len() < self.van_slots {
            let taken: Vec<&str> = self.vanguard.iter().map(|v| v.name.as_str()).collect();
            let next = self
                .state
                .living()
                .find(|m| !m.champion && !taken.contains(&m.name.as_str()));
            let Some(m) = next else { break };
            self.vanguard.push(Vanguard {
                name: m.name.clone(),
                champ: practice_champion(species, level),
            });
        }
    }

    /// One battle against already-leveled foes (species, level).
    /// `gate` marks a scripted battle of record; otherwise it is an
    /// ordinary road fight (wild, event-raised, or camp watch).
    /// Returns true when the fight is WON and the mantle still stands.
    fn battle(&mut self, foe_specs: &[(u16, u8)], gate: bool) -> bool {
        if gate {
            self.run.gate_battles += 1;
        } else {
            self.run.road_battles += 1;
        }
        let lv = self.champion_level.clamp(5, MAX_LEVEL);
        let bseed = trail_battle_seed(self.seed, self.state.day);
        let build_foes = |specs: &[(u16, u8)]| -> Vec<ChampionInstance> {
            specs.iter().map(|&(id, l)| practice_champion(id, l)).collect()
        };

        match self.depth {
            Depth::Current | Depth::Asymmetric => {
                let foes = build_foes(foe_specs);
                let party = vec![std::mem::replace(&mut self.mantle, practice_champion(7, 5))];
                let (won, post) = fight_full(party, foes, bseed);
                if won {
                    self.mantle = post.into_iter().next().expect("one file");
                    if gate {
                        self.run.gate_wins += 1;
                    } else {
                        self.run.road_wins += 1;
                    }
                    self.state.morale =
                        (self.state.morale + TRAIL_VICTORY_MORALE).min(trail_engine::MORALE_MAX);
                    true
                } else {
                    self.settle_mantle_fallen();
                    false
                }
            }
            Depth::Skirmish if !gate => {
                // The van fights the picket: one duel at even level, the
                // loser's side withdraws. No Book page off the gates.
                let foes = build_foes(&[(foe_specs[0].0, lv)]);
                let party = vec![std::mem::replace(&mut self.mantle, practice_champion(7, 5))];
                let (won, post) = fight_full(party, foes, bseed);
                if won {
                    self.mantle = post.into_iter().next().expect("one file");
                    self.run.road_wins += 1;
                    self.state.morale =
                        (self.state.morale + TRAIL_VICTORY_MORALE).min(trail_engine::MORALE_MAX);
                    true
                } else {
                    let max = self.mantle_max();
                    let wounded = (max * SKIRMISH_WITHDRAW_NUM / SKIRMISH_WITHDRAW_DEN).max(1);
                    self.mantle.current_resolve = self.mantle.current_resolve.min(wounded);
                    self.state.morale = (self.state.morale - SKIRMISH_MORALE_COST)
                        .max(trail_engine::MORALE_MIN);
                    false
                }
            }
            Depth::Skirmish | Depth::Company | Depth::CompanySwoon => {
                // The van fights under the company candidates; whether
                // it stands at the gates of record is the dial.
                let with_van = matches!(self.depth, Depth::Company | Depth::CompanySwoon)
                    && (!gate || self.van_at_gates);
                if with_van {
                    self.sync_vanguard();
                }
                let mut party = Vec::new();
                party.push(std::mem::replace(&mut self.mantle, practice_champion(7, 5)));
                let names: Vec<String> = if with_van {
                    self.vanguard.iter().map(|v| v.name.clone()).collect()
                } else {
                    Vec::new()
                };
                if with_van {
                    for v in self.vanguard.drain(..) {
                        party.push(v.champ);
                    }
                }
                let foes = build_foes(foe_specs);
                let (won, post) = fight_full(party, foes, bseed);
                // First: read the dead off the post-battle line.
                let mantle_stands = !post.is_empty() && post[0].current_resolve > 0;
                if mantle_stands {
                    self.mantle = post[0].clone();
                }
                for (i, name) in names.iter().enumerate() {
                    let body = &post[i + 1];
                    if body.current_resolve > 0 {
                        self.vanguard.push(Vanguard {
                            name: name.clone(),
                            champ: body.clone(),
                        });
                    } else if self.depth == Depth::CompanySwoon {
                        // Left for dead upon the field; the balms find
                        // him by morning. The Book keeps no page.
                        let mut c = body.clone();
                        let sp = data::species_by_id(c.species_id).expect("species table");
                        let max = data::compute_stats(&c, sp).max_resolve;
                        c.current_resolve = (max / 4).max(1);
                        self.vanguard.push(Vanguard {
                            name: name.clone(),
                            champ: c,
                        });
                    } else {
                        // Dead where the line stood: his page is written
                        // and the roster marked.
                        if let Some(m) = self
                            .state
                            .men
                            .iter_mut()
                            .find(|m| m.alive && &m.name == name)
                        {
                            m.alive = false;
                        }
                        self.run.vanguard_pages += 1;
                        self.book_page(
                            name.clone(),
                            "fell in battle upon the pilgrim road, shoulder to shoulder in the van",
                        );
                    }
                }
                if won {
                    if gate {
                        self.run.gate_wins += 1;
                    } else {
                        self.run.road_wins += 1;
                    }
                    self.state.morale =
                        (self.state.morale + TRAIL_VICTORY_MORALE).min(trail_engine::MORALE_MAX);
                }
                if !mantle_stands {
                    self.settle_mantle_fallen();
                }
                won && mantle_stands
            }
        }
    }

    /// A camp day (spec dials; the interaction lane only).
    fn camp_day(&mut self) {
        self.run.camps += 1;
        // The day stream keyed on the PRE-increment day, mirroring the
        // client (a camp and a march on "day N" never alias).
        let mut rng = trail_day_rng(self.seed, self.state.day);
        self.state.day += 1;
        let cost = trail_engine::daily_supply_cost(self.state.living_count()) * 2;
        self.state.supplies = self.state.supplies.saturating_sub(cost);
        if self.state.supplies == 0 {
            self.state.morale = (self.state.morale - trail_engine::STARVATION_MORALE_COST)
                .max(trail_engine::MORALE_MIN);
        }
        self.state.morale =
            (self.state.morale + trail_engine::CAMP_MORALE_GAIN).min(trail_engine::MORALE_MAX);
        let max = self.mantle_max();
        let mend = max * trail_engine::CAMP_RESOLVE_PCT_NUM / trail_engine::CAMP_RESOLVE_PCT_DEN;
        let cap = max * trail_engine::CAMP_RESOLVE_CAP_NUM / trail_engine::CAMP_RESOLVE_CAP_DEN;
        let resolve_after = self.mantle.current_resolve.saturating_add(mend).min(cap);
        if rng.pct() < trail_engine::CAMP_AMBUSH_PCT {
            // The watch cries: fought unmended, by the company. The ambush
            // fights the screen's zone mix (client.rs `trail_make_camp`;
            // a camp off the zone table falls back to brigands).
            let zone = crusade_core::data::overworld::screen_by_id(self.state.screen)
                .and_then(|s| s.encounter_zone);
            let mix = match zone {
                Some(z) => trail_foe_mix(0, Some(z)),
                None => trail_foe_mix(201, None),
            };
            let lv = self.champion_level.clamp(5, MAX_LEVEL);
            let specs: Vec<(u16, u8)> = mix.iter().map(|&id| (id, lv)).collect();
            let kept = self.battle(&specs, false);
            if self.over || !kept {
                return; // a lost night fight is an ordinary road defeat
            }
        }
        self.mantle.current_resolve = resolve_after;
        self.mantle.ailment = None;
    }
}

fn run_pilgrimage(cfg: &SimConfig, seed_byte: u8) -> Run {
    let mut seed = [0u8; 32];
    for b in seed.iter_mut() {
        *b = seed_byte;
    }
    let mut crossing_rng = trail_day_rng(seed, 0);
    let out = trail_engine::the_crossing(255, 1000, &mut crossing_rng);
    let mut state = out.state;
    if let Some(m) = state.living_mut().next() {
        m.champion = true;
    }
    let mut j = Jour {
        state,
        mantle: mkveteran(7, 50, &[16, 17, 15, 7]),
        champion_level: 50,
        vanguard: Vec::new(),
        over: false,
        seed,
        run: Run::default(),
        depth: cfg.depth,
        van_off: cfg.van_off,
        van_at_gates: cfg.van_at_gates,
        van_slots: cfg.van_slots,
    };

    while !j.over && j.state.day < DAY_CEILING {
        if j.run.reached_25 {
            break;
        }
        // The prudent camp trigger (interaction lane only).
        if cfg.prudent_camp && CAMP_SCREENS.contains(&j.state.screen) {
            let living = j.state.living_count();
            let daily = trail_engine::daily_supply_cost(living);
            let max = j.mantle_max();
            let resolve = j.mantle.current_resolve;
            if j.state.supplies > 2 * daily && resolve * 100 < max * 40 {
                j.camp_day();
                continue;
            }
        }
        let cur =
            crusade_core::data::overworld::screen_by_id(j.state.screen).expect("screen table");
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
            // The battle of record: two escorts at mantle-2, the
            // commander at mantle+offset capped 50 (client.rs travel()).
            let lv = j.champion_level;
            let boss_lv = lv.saturating_add(gate.boss_level_offset).min(MAX_LEVEL);
            let escort_lv = lv.saturating_sub(2).clamp(5, MAX_LEVEL);
            let mut specs: Vec<(u16, u8)> = gate_escort_mix(gate.battle_ref)
                .iter()
                .take(2)
                .map(|&id| (id, escort_lv))
                .collect();
            specs.push((gate.commander_id.expect("named"), boss_lv));
            j.battle(&specs, true);
        } else if let Some(b) = out.battle_ref {
            let mix = match cfg.depth {
                Depth::Asymmetric => trail_foe_mix_asym(b, dest.encounter_zone),
                _ => trail_foe_mix(b, dest.encounter_zone),
            };
            let lv = j.champion_level.clamp(5, MAX_LEVEL);
            let specs: Vec<(u16, u8)> = mix.iter().map(|&id| (id, lv)).collect();
            j.battle(&specs, false);
        } else if let Some(zone) = dest.encounter_zone {
            if rng.pct() < TRAIL_WILD_ROLL_PCT {
                let mix = match cfg.depth {
                    Depth::Asymmetric => trail_foe_mix_asym(0, Some(zone)),
                    _ => trail_foe_mix(0, Some(zone)),
                };
                let lv = j.champion_level.clamp(5, MAX_LEVEL);
                let specs: Vec<(u16, u8)> = mix.iter().map(|&id| (id, lv)).collect();
                j.battle(&specs, false);
            }
        }
        j.run.end_book = j.state.fallen.len() as u16;
    }
    j.run.end_day = j.state.day;
    j.run.extinct = j.over;
    j.run.end_living = j.state.living_count();
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

const CONFIGS: &[SimConfig] = &[
    SimConfig {
        name: "CURRENT (single file; shipped tables)",
        depth: Depth::Current,
        prudent_camp: false,
        van_off: 2,
        van_at_gates: true,
        van_slots: 2,
    },
    SimConfig {
        name: "A. COMPANY van@captain-2, van at gates (first pass)",
        depth: Depth::Company,
        prudent_camp: false,
        van_off: 2,
        van_at_gates: true,
        van_slots: 2,
    },
    SimConfig {
        name: "A2. COMPANY van@captain-0, van at gates",
        depth: Depth::Company,
        prudent_camp: false,
        van_off: 0,
        van_at_gates: true,
        van_slots: 2,
    },
    SimConfig {
        name: "A3. COMPANY van@captain-2, gates captain-alone",
        depth: Depth::Company,
        prudent_camp: false,
        van_off: 2,
        van_at_gates: false,
        van_slots: 2,
    },
    SimConfig {
        name: "A4. COMPANY van@captain-0, gates captain-alone",
        depth: Depth::Company,
        prudent_camp: false,
        van_off: 0,
        van_at_gates: false,
        van_slots: 2,
    },
    SimConfig {
        name: "A-rej. COMPANY-SWOON (felled van left for dead, no pages)",
        depth: Depth::CompanySwoon,
        prudent_camp: false,
        van_off: 2,
        van_at_gates: true,
        van_slots: 2,
    },
    SimConfig {
        name: "B. ASYMMETRIC TABLES (1 foe early, 2 mid, 3 only at gates)",
        depth: Depth::Asymmetric,
        prudent_camp: false,
        van_off: 2,
        van_at_gates: true,
        van_slots: 2,
    },
    SimConfig {
        name: "C. SKIRMISH (1v1 picket duels off the gates; loser withdraws)",
        depth: Depth::Skirmish,
        prudent_camp: false,
        van_off: 2,
        van_at_gates: true,
        van_slots: 2,
    },
    SimConfig {
        name: "A5. COMPANY van of ONE @captain-2, gates captain-alone",
        depth: Depth::Company,
        prudent_camp: false,
        van_off: 2,
        van_at_gates: false,
        van_slots: 1,
    },
    SimConfig {
        name: "A3+Camp. COMPANY (A3 dials) + prudent Make Camp, spec dials",
        depth: Depth::Company,
        prudent_camp: true,
        van_off: 2,
        van_at_gates: false,
        van_slots: 2,
    },
];

fn report(cfg: &SimConfig, runs: &[Run]) {
    let n = runs.len() as f64;
    let road_b: u32 = runs.iter().map(|r| u32::from(r.road_battles)).sum();
    let road_w: u32 = runs.iter().map(|r| u32::from(r.road_wins)).sum();
    let gate_b: u32 = runs.iter().map(|r| u32::from(r.gate_battles)).sum();
    let gate_w: u32 = runs.iter().map(|r| u32::from(r.gate_wins)).sum();
    let extinct_days: Vec<f64> = runs
        .iter()
        .filter(|r| r.extinct)
        .map(|r| f64::from(r.end_day))
        .collect();
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
    let van_pages: u32 = runs.iter().map(|r| u32::from(r.vanguard_pages)).sum();
    let camps: u32 = runs.iter().map(|r| u32::from(r.camps)).sum();
    println!("\n=== {} ===", cfg.name);
    println!(
        "road battles {} won {:.1}% | gate battles {} won {:.1}%",
        road_b,
        road_w as f64 / road_b.max(1) as f64 * 100.0,
        gate_b,
        gate_w as f64 / gate_b.max(1) as f64 * 100.0,
    );
    println!(
        "median extinction day {:.1} (extinct {}/{}) | reach 25 by {}: {:.0}%",
        if extinct_days.is_empty() {
            f64::NAN
        } else {
            median(extinct_days)
        },
        runs.iter().filter(|r| r.extinct).count(),
        runs.len(),
        DAY_CEILING,
        pct_reached,
    );
    println!(
        "arrivals {} | median living at arrival {:.1} | arrivals with >=5 Book pages {:.0}% | vanguard pages {} | camps {}",
        arrival.len(),
        med_arr_living,
        pct_arr_book5,
        van_pages,
        camps,
    );
}

fn sweep(cfgs: &[SimConfig]) {
    for cfg in cfgs {
        let runs: Vec<Run> = (0..RUNS).map(|s| run_pilgrimage(cfg, s as u8)).collect();
        report(cfg, &runs);
    }
}

#[test]
#[ignore = "trail battle-depth soak: measurement lane (docs/TRAIL-BATTLE-DEPTH.md)"]
fn trail_depth_sweep() {
    sweep(CONFIGS);
}



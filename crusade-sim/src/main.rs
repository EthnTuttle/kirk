//! crusade-sim: headless AI-vs-AI battle harness over crusade-core.
//!
//! Subcommands (plain std::env::args parsing):
//!   crusade-sim fuzz <n> <level>  — n seeded battles (seeds 0..n as
//!                                   [s as u8; 32]), random 3-champion teams
//!                                   from species 1..=12, balance report.
//!   crusade-sim duel <ids>v<ids>  — one verbose battle at level 25,
//!                                   seed [42;32]. Example: 1,2,3v4,5,6

use std::process::exit;

use crusade_core::data;
use crusade_core::engine::{Action, Battle};
use crusade_core::types::{
    ChampionInstance, DevotionStats, GraceStats, RiteCategory, RiteSlot, MAX_LEVEL,
};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

const TURN_CAP: u32 = 500;
const TEAM_SIZE: usize = 3;
const SPECIES_MIN: u16 = 1;
const SPECIES_MAX: u16 = 12;

// ---------- Fixture construction ----------

/// Build a legal champion: up to 4 rites taken from the *most recent*
/// learnset entries with level <= `level`, full zeal, no ailment,
/// current_resolve = computed max.
fn build_champion(species_id: u16, level: u8) -> Result<ChampionInstance, String> {
    let sp = data::species_by_id(species_id)
        .ok_or_else(|| format!("unknown species id {species_id}"))?;
    let mut picked: Vec<u16> = Vec::new();
    for id in sp
        .learnset
        .iter()
        .filter(|e| e.level <= level)
        .map(|e| e.rite_id)
        .rev()
    {
        if picked.len() == 4 {
            break;
        }
        if !picked.contains(&id) {
            picked.push(id);
        }
    }
    picked.reverse();
    let mut rites: [Option<RiteSlot>; 4] = [None; 4];
    for (i, id) in picked.iter().enumerate() {
        let r = data::rite_by_id(*id).ok_or_else(|| format!("unknown rite id {id}"))?;
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
    Ok(c)
}

/// Deterministic random team from species ids SPECIES_MIN..=SPECIES_MAX,
/// drawn from the per-battle seeded rng.
fn gen_team(rng: &mut ChaCha8Rng, level: u8) -> Result<Vec<ChampionInstance>, String> {
    (0..TEAM_SIZE)
        .map(|_| {
            let id = rng.gen_range(SPECIES_MIN..=SPECIES_MAX);
            build_champion(id, level)
        })
        .collect()
}

// ---------- Policy AI (identical for both sides) ----------

/// Compute power * effectiveness vs the opponent's active typing for every
/// usable damaging rite; play the max (ties -> lowest slot). If every usable
/// rite is status/zero-power, play the first usable rite. Never switches
/// voluntarily; yields only when no rite has zeal left.
fn policy_action(b: &Battle, side: u8) -> Action {
    let s = side as usize;
    let me = b.sides[s].active();
    let opp_typing = data::species_by_id(b.sides[1 - s].active().species_id).map(|sp| sp.typing);
    let mut first_usable: Option<u8> = None;
    let mut best: Option<(u8, u32)> = None;
    for (i, slot) in me.rites.iter().enumerate() {
        let Some(rs) = slot else { continue };
        if rs.zeal_left == 0 {
            continue;
        }
        if first_usable.is_none() {
            first_usable = Some(i as u8);
        }
        let Some(rite) = data::rite_by_id(rs.rite_id) else {
            continue;
        };
        if rite.category == RiteCategory::Status || rite.power == 0 {
            continue;
        }
        let eff = opp_typing.map_or(100, |t| data::effectiveness(rite.rite_type, t));
        let score = rite.power as u32 * eff as u32;
        if best.is_none_or(|(_, sc)| score > sc) {
            best = Some((i as u8, score));
        }
    }
    match (best, first_usable) {
        (Some((slot, _)), _) | (None, Some(slot)) => Action::Rite { slot },
        (None, None) => Action::Yield,
    }
}

// ---------- Battle loop ----------

/// Loop apply_turn until a winner emerges, resolving ForcedSwitchNeeded with
/// the first alive party member. Hard cap TURN_CAP turns.
fn run_battle(mut b: Battle, verbose: bool) -> Result<(u8, u32), String> {
    let mut turns: u32 = 0;
    loop {
        if let Some(w) = b.winner {
            return Ok((w, turns));
        }
        if turns >= TURN_CAP {
            return Err(format!("hit {TURN_CAP}-turn cap"));
        }
        // Forced switches, side 0 first. When a switch is pending the engine
        // reports only Switch actions via legal_actions.
        for s in 0..2u8 {
            if b.winner.is_some() {
                break;
            }
            let la = b.legal_actions(s);
            let forced = !la.is_empty() && la.iter().all(|a| matches!(a, Action::Switch { .. }));
            if forced {
                let from = b.events().len();
                let Action::Switch { party_idx } = la[0] else {
                    unreachable!()
                };
                b.apply_forced_switch(s, party_idx)
                    .map_err(|e| format!("forced switch side {s}: {e}"))?;
                if verbose {
                    for e in &b.events()[from..] {
                        println!("  (forced) {e:?}");
                    }
                }
            }
        }
        if b.winner.is_some() {
            continue;
        }
        let a0 = policy_action(&b, 0);
        let a1 = policy_action(&b, 1);
        let ev = b
            .apply_turn(a0, a1)
            .map_err(|e| format!("apply_turn: {e}"))?;
        turns += 1;
        if verbose {
            println!("turn {turns}:");
            for e in &ev {
                println!("  {e:?}");
            }
        }
    }
}

// ---------- Subcommands ----------

fn cmd_fuzz(n: usize, level: u8) -> i32 {
    if level == 0 || level > MAX_LEVEL {
        eprintln!("fuzz: level must be 1..={MAX_LEVEL}");
        return 2;
    }
    let mut side_wins = [0usize; 2];
    let mut total_turns: u64 = 0;
    let mut stats = [(0usize, 0usize); (SPECIES_MAX + 1) as usize]; // id -> (appearances, wins)
    let mut failures = 0usize;

    for s in 0..n {
        let seed = [s as u8; 32];
        let mut rng = ChaCha8Rng::from_seed(seed);
        let result =
            (|| -> Result<(Vec<ChampionInstance>, Vec<ChampionInstance>, u8, u32), String> {
                let ta = gen_team(&mut rng, level)?;
                let tb = gen_team(&mut rng, level)?;
                let b =
                    Battle::new(ta.clone(), tb.clone(), seed).map_err(|e| format!("setup: {e}"))?;
                let (w, t) = run_battle(b, false)?;
                Ok((ta, tb, w, t))
            })();
        match result {
            Ok((ta, tb, w, t)) => {
                side_wins[w as usize] += 1;
                total_turns += u64::from(t);
                let winners = if w == 0 { &ta } else { &tb };
                for c in ta.iter().chain(tb.iter()) {
                    stats[c.species_id as usize].0 += 1;
                }
                for c in winners.iter() {
                    stats[c.species_id as usize].1 += 1;
                }
            }
            Err(e) => {
                failures += 1;
                println!("FAIL seed {s}: {e}");
            }
        }
    }

    println!("== crusade-sim fuzz (level {level}) ==");
    println!("battles: {n}");
    println!("side0 wins: {}", side_wins[0]);
    println!("side1 wins: {}", side_wins[1]);
    println!(
        "avg turns: {}",
        if n > 0 { total_turns / n as u64 } else { 0 }
    );
    println!();
    println!("{:<22} {:>4} {:>4} {:>5}", "species", "app", "wins", "win%");
    let mut rows: Vec<(u16, usize, usize)> = (SPECIES_MIN..=SPECIES_MAX)
        .filter(|&id| stats[id as usize].0 > 0)
        .map(|id| (id, stats[id as usize].0, stats[id as usize].1))
        .collect();
    // win-rate desc (integer permille), then appearances desc, then id asc
    rows.sort_by(|&(ia, aa, wa), &(ib, ab, wb)| {
        (wb * 1000 / ab)
            .cmp(&(wa * 1000 / aa))
            .then(ab.cmp(&aa))
            .then(ia.cmp(&ib))
    });
    for (id, app, wins) in rows {
        let name = data::species_by_id(id).map_or("?", |sp| sp.name);
        println!(
            "{id:>3} {name:<18} {app:>4} {wins:>4} {:>4}%",
            wins * 100 / app
        );
    }
    println!("failures: {failures}");

    i32::from(failures > 0)
}

fn cmd_duel(desc: &str) -> i32 {
    const LEVEL: u8 = 25;
    let Some((sa, sb)) = desc.split_once('v') else {
        eprintln!("duel: description must look like 1,2,3v4,5,6");
        return 2;
    };
    let parse_side = |s: &str| -> Result<Vec<ChampionInstance>, String> {
        s.split(',')
            .map(str::trim)
            .map(|t| {
                t.parse::<u16>()
                    .map_err(|_| format!("bad species id '{t}'"))
                    .and_then(|id| build_champion(id, LEVEL))
            })
            .collect()
    };
    let (ta, tb) = match (parse_side(sa), parse_side(sb)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("duel: {e}");
            return 2;
        }
    };
    let roster = |party: &[ChampionInstance]| {
        party
            .iter()
            .map(|c| {
                let sp = data::species_by_id(c.species_id).expect("validated species");
                format!(
                    "{} (id {}, {} rites)",
                    sp.name,
                    c.species_id,
                    c.rites.iter().flatten().count()
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    println!("side 0: {}", roster(&ta));
    println!("side 1: {}", roster(&tb));
    println!("level {LEVEL}, seed [42;32]");
    println!();
    let b = match Battle::new(ta, tb, [42u8; 32]) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("duel: setup: {e}");
            return 1;
        }
    };
    match run_battle(b, true) {
        Ok((w, t)) => {
            println!();
            println!("winner: side {w} in {t} turns");
            0
        }
        Err(e) => {
            println!();
            println!("FAIL: {e}");
            1
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let code = match args.get(1).map(String::as_str) {
        Some("fuzz") => {
            let n = args.get(2).and_then(|s| s.parse::<usize>().ok());
            let level = args.get(3).and_then(|s| s.parse::<u8>().ok());
            match (n, level) {
                (Some(n), Some(level)) => cmd_fuzz(n, level),
                _ => {
                    eprintln!("usage: crusade-sim fuzz <n> <level>");
                    2
                }
            }
        }
        Some("duel") => match args.get(2) {
            Some(desc) => cmd_duel(desc),
            None => {
                eprintln!("usage: crusade-sim duel <a,b,cvX,Y,Z>  (e.g. 1,2,3v4,5,6)");
                2
            }
        },
        _ => {
            eprintln!("usage:");
            eprintln!("  crusade-sim fuzz <n> <level>");
            eprintln!("  crusade-sim duel <a,b,cvX,Y,Z>  (e.g. 1,2,3v4,5,6)");
            2
        }
    };
    exit(code);
}

//! Static game data tables + stat math (Worker A: glm/data).
//! API is fixed here; table contents and tests are the worker's job.

use crate::types::*;

pub mod callings;
pub mod chart;
pub mod rites;
pub mod species;

/// All champion species, sorted by id.
pub fn all_species() -> &'static [SpeciesBase] {
    species::SPECIES
}

pub fn species_by_id(id: u16) -> Option<&'static SpeciesBase> {
    species::SPECIES.iter().find(|s| s.id == id)
}

pub fn rite_by_id(id: u16) -> Option<&'static Rite> {
    rites::RITES.iter().find(|r| r.id == id)
}

pub fn calling_by_id(id: u8) -> Option<&'static Calling> {
    callings::CALLINGS.iter().find(|c| c.id == id)
}

/// Effectiveness percent (25..=400 via dual-type product; see chart rules).
///
/// Wheel order W = [Humility, Charity, Chastity, Diligence, Patience,
/// Temperance, Kindness]. For Virtue(V) at index i vs virtue defender D:
///   D==V -> 50; D==W[(i+1)%7] -> 200; D==W[(i+6)%7] -> 50; else 100.
/// Dual defenders multiply (p1*p2/100).
/// Virtue(V) vs Sin(s): 200 iff V.opposed_sin()==s else 100.
/// Sin(s) vs Virtues: 200 iff any virtue V has V.opposed_sin()==s else 100.
/// Sin(s) vs Sin(d): 50 if same else 100. Typeless: always 100.
pub fn effectiveness(att: RiteType, def: Typing) -> u8 {
    chart::effectiveness(att, def)
}

/// XP required to *reach* `level` (2..=50) for the given curve.
pub fn xp_for_level(slope: u32, div: u32, level: u8) -> u32 {
    let n = level as u32;
    slope * n * n * n / div
}

/// Compute derived stats for an instance (see formula doc on ComputedStats).
pub fn compute_stats(c: &ChampionInstance, sp: &SpeciesBase) -> ComputedStats {
    let lvl = c.level as u32;
    let core = |base: u16, grace: u8, dev: u8| -> u32 {
        (2 * base as u32 + grace as u32 + dev as u32 / 4) * lvl / 50
    };
    let calling = callings::CALLINGS
        .iter()
        .find(|x| x.id == c.calling_id)
        .copied()
        .unwrap_or(callings::NEUTRAL);
    let apply_calling = |flat: u32, stat: BattleStat| -> u16 {
        let pct = match (calling.inc, calling.dec) {
            (Some(i), _) if i == stat => 110,
            (_, Some(d)) if d == stat => 90,
            _ => 100,
        };
        ((flat + 5) * pct / 100) as u16
    };
    let g = &c.grace;
    let d = &c.devotion;
    ComputedStats {
        max_resolve: (core(sp.base.resolve, g.resolve, d.resolve) + lvl + 10) as u16,
        valour: apply_calling(core(sp.base.valour, g.valour, d.valour), BattleStat::Valour),
        piety: apply_calling(core(sp.base.piety, g.piety, d.piety), BattleStat::Piety),
        guard: apply_calling(core(sp.base.guard, g.guard, d.guard), BattleStat::Guard),
        aegis: apply_calling(core(sp.base.aegis, g.aegis, d.aegis), BattleStat::Aegis),
        alacrity: apply_calling(core(sp.base.alacrity, g.alacrity, d.alacrity), BattleStat::Alacrity),
    }
}

/// Stage multiplier (-6..=+6) -> percent (Pokemon: 2/(2-s) or (2+s)/2).
pub fn stage_pct(stage: i8) -> u32 {
    let s = stage.clamp(-6, 6) as i32;
    if s >= 0 {
        (2 + s) as u32 * 50 // (2+s)/2 * 100
    } else {
        100 * 2 / (2 - s) as u32 // floor
    }
}

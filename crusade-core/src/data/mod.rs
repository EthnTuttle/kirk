//! Static game data tables + stat math (Worker A: glm/data).
//! API is fixed here; table contents and tests are the worker's job.

use crate::types::*;

pub mod adversaries;
pub mod overworld;
pub mod callings;
pub mod chart;
pub mod rites;
pub mod species;
pub mod witnesses;

/// All champion species, sorted by id.
pub fn all_species() -> &'static [SpeciesBase] {
    species::SPECIES
}

/// Species lookup: SPECIES first (ids < 108 unchanged), then the historical
/// adversary tables (docs/HISTORICAL-FOES.md section 7 seam).
pub fn species_by_id(id: u16) -> Option<&'static SpeciesBase> {
    species::SPECIES
        .iter()
        .find(|s| s.id == id)
        .or_else(|| adversaries::ADVERSARIES.iter().find(|s| s.id == id))
}

pub fn rite_by_id(id: u16) -> Option<&'static Rite> {
    rites::RITES
        .iter()
        .find(|r| r.id == id)
        .or_else(|| adversaries::ADVERSARY_RITES.iter().find(|r| r.id == id))
}

pub fn calling_by_id(id: u8) -> Option<&'static Calling> {
    callings::CALLINGS.iter().find(|c| c.id == id)
}

/// All revered witnesses of the Hagiography's witness roll (lore-only,
/// non-combatant), sorted by id.
pub fn all_witnesses() -> &'static [witnesses::Witness] {
    witnesses::WITNESSES
}

pub fn witness_by_id(id: u16) -> Option<&'static witnesses::Witness> {
    witnesses::WITNESSES.iter().find(|w| w.id == id)
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
        alacrity: apply_calling(
            core(sp.base.alacrity, g.alacrity, d.alacrity),
            BattleStat::Alacrity,
        ),
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

/// Chance (percent, 3..=85) that a Call to Repentance succeeds against a
/// defeated foe — docs/CONVERSION.md section 3, implemented exactly:
///
/// - None when the foe is not convertible (commanders and sin-spawn carry
///   `obstinacy: None`; champions likewise).
/// - hardness = obstinacy + 15 if the foe out-levels the caller, plus
///   sin_depth * 10.
/// - conviction = caller piety (post-level stat) + 20 if the felling blow
///   was a virtue rite, plus min(fervor_bonus, 15).
/// - chance = clamp(conviction / (conviction + hardness), 3%, 85%).
///
/// Pure integer math (percent = conviction*100/(conviction+hardness), then
/// clamped); no I/O, wasm-safe. The actual draw path reuses the RngFacade
/// and lives outside this fn.
pub fn conversion_chance(
    caller_piety: u16,
    caller_level: u8,
    caller_used_virtue_kill: bool,
    fervor_bonus: u8,
    foe: &SpeciesBase,
    foe_level: u8,
) -> Option<u8> {
    let obstinacy = foe.obstinacy?;
    let sin_depth = foe.sin_depth.unwrap_or(0);
    let mut hardness = obstinacy as u32;
    if foe_level > caller_level {
        hardness += 15;
    }
    hardness += sin_depth as u32 * 10;
    let mut conviction = caller_piety as u32;
    if caller_used_virtue_kill {
        conviction += 20;
    }
    conviction += fervor_bonus.min(15) as u32;
    let total = conviction + hardness;
    // obstinacy >= 1 for every convertible foe, so total == 0 is unreachable
    // with table data; guard anyway rather than divide by zero.
    let pct = if total == 0 { 100 } else { conviction * 100 / total };
    Some(pct.clamp(3, 85) as u8)
}

#[cfg(test)]
mod serde_tests {
    //! CONVERSION.md section 7: the instance fields `converted` /
    //! `converted_from` are additive with serde defaults — old saves
    //! deserialize unchanged.
    use crate::types::*;

    /// Pre-M7 shape: no `converted` / `converted_from` keys at all.
    const OLD_SAVE: &str = r#"{
        "species_id": 1,
        "level": 10,
        "xp": 0,
        "grace": {"resolve":0,"valour":0,"piety":0,"guard":0,"aegis":0,"alacrity":0},
        "devotion": {"resolve":0,"valour":0,"piety":0,"guard":0,"aegis":0,"alacrity":0},
        "calling_id": 0,
        "rites": [{"rite_id":1,"zeal_left":35},null,null,null],
        "current_resolve": 1,
        "ailment": null
    }"#;

    #[test]
    fn old_instance_save_deserializes_with_defaults() {
        let c: ChampionInstance = serde_json::from_str(OLD_SAVE).expect("old save shape loads");
        assert_eq!(c.species_id, 1);
        assert!(!c.converted);
        assert_eq!(c.converted_from, None);
    }

    #[test]
    fn instance_round_trips_and_omits_none_origin() {
        let c: ChampionInstance = serde_json::from_str(OLD_SAVE).unwrap();
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"converted\":false"));
        assert!(!json.contains("converted_from"), "None origin stays absent");
        let back: ChampionInstance = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c);
        // A baptized convert round-trips with its origin.
        let mut conv = c.clone();
        conv.converted = true;
        conv.converted_from = Some(108);
        let json = serde_json::to_string(&conv).unwrap();
        assert!(json.contains("\"converted_from\":108"));
        assert_eq!(serde_json::from_str::<ChampionInstance>(&json).unwrap(), conv);
    }
}

#[cfg(test)]
mod conversion_tests {
    use super::*;
    use crate::data::species_by_id;

    fn arch(id: u16) -> &'static SpeciesBase {
        crate::data::adversaries::ADVERSARIES
            .iter()
            .find(|s| s.id == id)
            .unwrap()
    }

    /// CONVERSION.md section 3 worked example shape: piety 100, virtue kill,
    /// fervor 10 vs tier-1 obstinacy 40 at equal level.
    #[test]
    fn contested_roll_math() {
        // conviction = 100 + 20 + 10 = 130; hardness = 40 + 10 = 50.
        // 130 / 180 = 72%.
        let foe = arch(110); // obstinacy 40, sin_depth 1
        assert_eq!(conversion_chance(100, 20, true, 10, foe, 20), Some(72));
        // Without the virtue kill or fervor: 100 / 150 = 66%.
        assert_eq!(conversion_chance(100, 20, false, 0, foe, 20), Some(66));
    }

    #[test]
    fn seniority_penalty_adds_15_hardness() {
        let foe = arch(113); // obstinacy 22, sin_depth 0
        let peer = conversion_chance(80, 20, false, 0, foe, 20).unwrap();
        let senior = conversion_chance(80, 20, false, 0, foe, 21).unwrap();
        // 80/102 = 78 vs 80/117 = 68.
        assert_eq!((peer, senior), (78, 68));
    }

    #[test]
    fn fervor_bonus_caps_at_15() {
        let foe = arch(108); // obstinacy 28, sin_depth 0
        let capped = conversion_chance(57, 10, false, 15, foe, 10).unwrap();
        let overflowing = conversion_chance(57, 10, false, 99, foe, 10).unwrap();
        assert_eq!(capped, overflowing);
        // 72 / 100 = 72%.
        assert_eq!(capped, 72);
    }

    #[test]
    fn floor_is_three_percent() {
        // Weakest possible caller against the hardest common foe, out-leveled:
        // conviction 1, hardness 75 + 15 + 30 = 120 -> 0% clamped to 3%.
        let muqaddam = arch(117);
        assert_eq!(conversion_chance(1, 1, false, 0, muqaddam, 50), Some(3));
        // Zero piety still honors the Romans 10:13 floor.
        assert_eq!(conversion_chance(0, 50, false, 0, muqaddam, 50), Some(3));
    }

    #[test]
    fn ceiling_is_85_percent() {
        // A saint against a willing tribesman: 435 / 457 ~= 95% -> clamped 85%.
        let bedouin = arch(113);
        assert_eq!(conversion_chance(400, 50, true, 15, bedouin, 1), Some(85));
        // Tier extremes: the easiest tier-0 still cannot exceed the ceiling.
        assert_eq!(conversion_chance(600, 50, true, 15, bedouin, 1), Some(85));
    }

    #[test]
    fn tier_extremes_order_hardness() {
        // Same caller against tier 0 vs tier 3: the deeper heart converts less.
        let tier0 = arch(111); // obstinacy 25, sin_depth 0
        let tier3 = arch(117); // obstinacy 75, sin_depth 3
        let easy = conversion_chance(100, 30, true, 15, tier0, 30).unwrap();
        let hard = conversion_chance(100, 30, true, 15, tier3, 30).unwrap();
        // 135/160 = 84 vs 135/240 = 56.
        assert_eq!((easy, hard), (84, 56));
        assert!(easy > hard);
    }

    #[test]
    fn non_convertible_foes_return_none() {
        // Commanders never convert.
        for id in [120u16, 121, 122, 123] {
            let foe = arch(id);
            assert_eq!(conversion_chance(200, 50, true, 15, foe, 1), None);
        }
        // Sin-spawn and champions carry no obstinacy either.
        for id in [1u16, 2, 101, 107] {
            let foe = species_by_id(id).unwrap();
            assert_eq!(foe.obstinacy, None);
            assert_eq!(conversion_chance(200, 50, true, 15, foe, 1), None);
        }
    }
}

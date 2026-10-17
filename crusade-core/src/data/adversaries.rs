//! Historical adversary tables (M7, docs/HISTORICAL-FOES.md sections 4-6):
//! field archetypes ids 108..=117 and named commanders 120..=125 (the
//! Hattin wing commanders Gokbori and Taki al-Din Umar join at 124/125),
//! plus the add-only adversary rites 43..=62 cited `[NEW]` in the section
//! 4 kit sketches (58..=62 are the 2026-10-17 balance-tune strokes — see
//! the post-tune note in docs/FUZZ-ADVERSARIES.md). `data::species_by_id`
//! / `data::rite_by_id` scan these tables after SPECIES / RITES; ids
//! below 108 resolve exactly as before.
//!
//! Register (section 6, chronicle voice): these are honorable men who fight
//! for a false god — redeemable, never caricatured. `Typing::Valor` keeps
//! them off the virtue/sin chart; the lane is balanced by stats and kits.
//! `obstinacy` / `sin_depth` drive the Call to Repentance
//! (docs/CONVERSION.md sections 3 and 6); commanders never convert.
//!
//! Base stat order: resolve, valour, piety, guard, aegis, alacrity.
//! Archetype totals 300-380 (117 rides the commander band by design);
//! commanders 390-420.

use crate::types::*;

/// Adversary rites, add-only alongside `rites::RITES` (which stays LOCKED at
/// ids 1..=42). All Typeless: flat 1x on the chart, never STAB — armed honor
/// claims neither virtue nor sin. Each entry cites its archetype.
pub static ADVERSARY_RITES: &[Rite] = &[
    Rite {
        // Turcoman Bowrider (108): shots loosed at the gallop, wheeling past.
        id: 43,
        name: "Wheeling Shots",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Martial,
        power: 45,
        accuracy: 95,
        zeal: 25,
        priority: 0,
        secondaries: &[Secondary::MultiHit],
    },
    Rite {
        // Turcoman Bowrider (108): dust of the plain screening the retreat.
        id: 44,
        name: "Dust of the Plain",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Status,
        power: 0,
        accuracy: 101,
        zeal: 15,
        priority: 0,
        secondaries: &[Secondary::StageOnSelf {
            stat: BattleStat::Alacrity,
            delta: 1,
            chance_pct: 100,
        }],
    },
    Rite {
        // Ghulam Lancer (109, 117): the couched two-handed lance at the charge.
        id: 45,
        name: "Couched Lance",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Martial,
        power: 80,
        accuracy: 90,
        zeal: 10,
        priority: 0,
        secondaries: &[Secondary::HighCrit],
    },
    Rite {
        // Sudani Shieldbearer (110): the full-body hide shield locked in line.
        id: 46,
        name: "Testudo of Hides",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Status,
        power: 0,
        accuracy: 101,
        zeal: 15,
        priority: 0,
        secondaries: &[
            Secondary::StageOnSelf {
                stat: BattleStat::Guard,
                delta: 1,
                chance_pct: 100,
            },
            Secondary::StageOnSelf {
                stat: BattleStat::Aegis,
                delta: 1,
                chance_pct: 100,
            },
        ],
    },
    Rite {
        // Armenian Bowman (111, 123): massed volleys blackening the sky.
        id: 47,
        name: "Storm of Arrows",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Martial,
        power: 30,
        accuracy: 95,
        zeal: 25,
        priority: 0,
        secondaries: &[Secondary::MultiHit],
    },
    Rite {
        // Armenian Bowman (111): files shooting in steady rotation.
        id: 48,
        name: "Volley in Turn",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Martial,
        power: 65,
        accuracy: 100,
        zeal: 20,
        priority: 0,
        secondaries: &[],
    },
    Rite {
        // Daylamite Axeman (112, 122): two tabar axes swung as one.
        id: 49,
        name: "Cleave of Axes",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Martial,
        power: 80,
        accuracy: 90,
        zeal: 15,
        priority: 0,
        secondaries: &[Secondary::HighCrit],
    },
    Rite {
        // Daylamite Axeman (112): the zhupin javelin thrown first and fast.
        id: 50,
        name: "Zhupin Cast",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Martial,
        power: 45,
        accuracy: 100,
        zeal: 30,
        priority: 1,
        secondaries: &[],
    },
    Rite {
        // Bedouin Outrider (113, 120, 122): strike, wheel, and be gone.
        id: 51,
        name: "Harass and Fade",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Martial,
        power: 45,
        accuracy: 100,
        zeal: 25,
        priority: 0,
        secondaries: &[Secondary::Volatile {
            volatile_: Volatile::Flinch,
            chance_pct: 30,
        }],
    },
    Rite {
        // Bedouin Outrider (113): the mirage veiling the rider's line.
        id: 52,
        name: "Mirage Veil",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Status,
        power: 0,
        accuracy: 101,
        zeal: 10,
        priority: 0,
        secondaries: &[Secondary::StageOnSelf {
            stat: BattleStat::Aegis,
            delta: 1,
            chance_pct: 100,
        }],
    },
    Rite {
        // Kurdish Ghazw Rider (114, 121, 122): the spear coming down from the hills.
        id: 53,
        name: "Spear of the Hills",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Martial,
        power: 70,
        accuracy: 95,
        zeal: 15,
        priority: 0,
        secondaries: &[],
    },
    Rite {
        // Naffatun Sapper (115): naphtha in clay, a comet of fire (Brand).
        id: 54,
        name: "Comet of Naphtha",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Spiritual,
        power: 80,
        accuracy: 90,
        zeal: 15,
        priority: 0,
        secondaries: &[Secondary::Ailment {
            ailment: Ailment::Brand,
            chance_pct: 30,
        }],
    },
    Rite {
        // Naffatun Sapper (115): wet rawhide against his own fire.
        id: 55,
        name: "Wet Hide Ward",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Status,
        power: 0,
        accuracy: 101,
        zeal: 15,
        priority: 0,
        secondaries: &[Secondary::StageOnSelf {
            stat: BattleStat::Guard,
            delta: 1,
            chance_pct: 100,
        }],
    },
    Rite {
        // Askari Alamdar (116, 117, 121, 123): the banner planted, the tulb rallies.
        id: 56,
        name: "Hold the Banner",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Status,
        power: 0,
        accuracy: 101,
        zeal: 20,
        priority: 0,
        secondaries: &[Secondary::StageOnSelf {
            stat: BattleStat::Valour,
            delta: 1,
            chance_pct: 100,
        }],
    },
    Rite {
        // Muqaddam of the Tulb (117, 122, 123): wheeling the line onto the foe's flank.
        id: 57,
        name: "Wheel the Line",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Martial,
        power: 75,
        accuracy: 90,
        zeal: 10,
        priority: 0,
        secondaries: &[Secondary::StageOnTarget {
            stat: BattleStat::Alacrity,
            delta: -1,
            chance_pct: 100,
        }],
    },
    Rite {
        // Sudani Shieldbearer (110): the short thrusting spear taking a
        // horseman out of the saddle; the shield never leaves the line.
        id: 58,
        name: "Unhorse the Rider",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Martial,
        power: 65,
        accuracy: 90,
        zeal: 15,
        priority: 0,
        secondaries: &[Secondary::StageOnSelf {
            stat: BattleStat::Guard,
            delta: 1,
            chance_pct: 100,
        }],
    },
    Rite {
        // Armenian Bowman (111): the last sheaf, a shaft finding the eye-slit.
        id: 59,
        name: "Arrow through the Visor",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Martial,
        power: 85,
        accuracy: 90,
        zeal: 12,
        priority: 0,
        secondaries: &[Secondary::HighCrit],
    },
    Rite {
        // Daylamite Axeman (112): the falling stroke hewing harness and shield.
        id: 60,
        name: "The Falling Stroke",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Martial,
        power: 85,
        accuracy: 90,
        zeal: 12,
        priority: 0,
        secondaries: &[Secondary::StageOnTarget {
            stat: BattleStat::Guard,
            delta: -1,
            chance_pct: 100,
        }],
    },
    Rite {
        // Bedouin Outrider (113): the one true charge, the lance taken at speed.
        id: 61,
        name: "Lance at the Gallop",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Martial,
        power: 80,
        accuracy: 95,
        zeal: 15,
        priority: 0,
        secondaries: &[Secondary::Volatile {
            volatile_: Volatile::Flinch,
            chance_pct: 30,
        }],
    },
    Rite {
        // Askari Alamdar (116): the banner going forward, the line behind it.
        id: 62,
        name: "The Banner Advances",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Martial,
        power: 85,
        accuracy: 90,
        zeal: 12,
        priority: 0,
        secondaries: &[],
    },
];

/// Historical field adversaries: archetypes 108..=117 (section 4 table)
/// and named commanders 120..=125 (section 5 — the Hattin wing commanders
/// ride the same band and never convert). Sorted by id.
pub static ADVERSARIES: &[SpeciesBase] = &[
    SpeciesBase {
        id: 108,
        name: "Turcoman Bowrider",
        typing: Typing::Valor,
        base: Stats { resolve: 40, valour: 55, piety: 35, guard: 40, aegis: 45, alacrity: 85 },
        learnset: &[
            // Balance-tune 2026-10-17: Wheeling Shots pushed off the L1 lane
            // (L5 pool overtuning, docs/FUZZ-ADVERSARIES.md) — at L5 the
            // Bowrider fields Measured Blow and dust, not the multi-hit.
            LearnEntry { level: 1, rite_id: 1 },
            LearnEntry { level: 5, rite_id: 44 },
            LearnEntry { level: 6, rite_id: 43 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "He loosed from the saddle and was gone before the shaft fell; the plain itself seemed to ride against us.",
        obstinacy: Some(28),
        sin_depth: Some(0),
    },
    SpeciesBase {
        id: 109,
        name: "Ghulam Lancer",
        typing: Typing::Valor,
        base: Stats { resolve: 65, valour: 85, piety: 30, guard: 70, aegis: 50, alacrity: 45 },
        learnset: &[
            // Balance-tune 2026-10-17: Sundering Blow pushed off the L1 lane
            // (L5 pool overtuning); Measured Blow holds the early charge.
            LearnEntry { level: 1, rite_id: 1 },
            LearnEntry { level: 6, rite_id: 15 },
            LearnEntry { level: 7, rite_id: 18 },
            LearnEntry { level: 12, rite_id: 45 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "The household lancer came on at the couch, unhurried as a man walking to evening prayer; our line bent beneath him.",
        obstinacy: Some(42),
        sin_depth: Some(1),
    },
    SpeciesBase {
        id: 110,
        name: "Sudani Shieldbearer",
        typing: Typing::Valor,
        base: Stats { resolve: 80, valour: 55, piety: 25, guard: 85, aegis: 45, alacrity: 30 },
        learnset: &[
            // Balance-tune 2026-10-17 (owner decision, docs/FUZZ-ADVERSARIES.md
            // post-tune note): the wall that punishes. Measured Blow alone
            // could not answer the champions' harness, so the short spear
            // joins at 6 — a real stroke that still locks the shield line
            // (Guard rising with each hit). Testudo of Hides moves to 10.
            LearnEntry { level: 1, rite_id: 1 },
            LearnEntry { level: 1, rite_id: 2 },
            LearnEntry { level: 6, rite_id: 58 },
            LearnEntry { level: 10, rite_id: 46 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "Behind a shield taller than a man he stood his ground all day; we broke three spears upon his patience.",
        obstinacy: Some(40),
        sin_depth: Some(1),
    },
    SpeciesBase {
        id: 111,
        name: "Armenian Bowman",
        typing: Typing::Valor,
        base: Stats { resolve: 55, valour: 60, piety: 35, guard: 50, aegis: 55, alacrity: 65 },
        learnset: &[
            // Balance-tune 2026-10-17: Storm of Arrows pushed off the L1 lane
            // (L5 pool overtuning). Measured Blow holds the L1 slot so a
            // level 1-4 baptized convert is never riteless (bricked); under
            // the max-power policy it is dormant once Volley in Turn joins
            // at 5. Arrow through the Visor (59) answers the chronic L20/L40
            // sag with a real sheaf-ender at 12.
            LearnEntry { level: 1, rite_id: 1 },
            LearnEntry { level: 5, rite_id: 48 },
            LearnEntry { level: 6, rite_id: 47 },
            LearnEntry { level: 9, rite_id: 2 },
            LearnEntry { level: 12, rite_id: 59 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "His arrows fell among us in turns, as regular as rain; by the ninth hour our shields bristled like hedgerows.",
        obstinacy: Some(25),
        sin_depth: Some(0),
    },
    SpeciesBase {
        id: 112,
        name: "Daylamite Axeman",
        typing: Typing::Valor,
        base: Stats { resolve: 70, valour: 80, piety: 25, guard: 65, aegis: 40, alacrity: 40 },
        learnset: &[
            // Balance-tune 2026-10-17: Cleave of Axes pushed off the L4 lane
            // (L5 pool overtuning); The Falling Stroke (60) answers the L20/L40
            // sag — the harness-hewing stroke the axes were made for.
            LearnEntry { level: 1, rite_id: 1 },
            LearnEntry { level: 6, rite_id: 49 },
            LearnEntry { level: 8, rite_id: 50 },
            LearnEntry { level: 12, rite_id: 60 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "He swung two axes as other men swing one, and the din of his coming carried over the whole battle.",
        obstinacy: Some(45),
        sin_depth: Some(1),
    },
    SpeciesBase {
        id: 113,
        name: "Bedouin Outrider",
        typing: Typing::Valor,
        base: Stats { resolve: 45, valour: 65, piety: 30, guard: 40, aegis: 40, alacrity: 85 },
        learnset: &[
            // Balance-tune 2026-10-17: Harass and Fade pushed off the L3 lane
            // (L5 pool overtuning); Lance at the Gallop (61) answers the L20/L40
            // sag — the one true charge of a man never at sword's length.
            LearnEntry { level: 1, rite_id: 1 },
            LearnEntry { level: 6, rite_id: 51 },
            LearnEntry { level: 7, rite_id: 52 },
            LearnEntry { level: 10, rite_id: 61 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "We saw him at dawn and again at dusk and never once at sword's length; the desert seemed to walk with him.",
        obstinacy: Some(22),
        sin_depth: Some(0),
    },
    SpeciesBase {
        id: 114,
        name: "Kurdish Ghazw Rider",
        typing: Typing::Valor,
        base: Stats { resolve: 60, valour: 75, piety: 35, guard: 60, aegis: 50, alacrity: 55 },
        learnset: &[
            // Balance-tune 2026-10-17: Sundering Blow pushed off the L1 lane
            // (L5 pool overtuning); Measured Blow holds the early ride.
            LearnEntry { level: 1, rite_id: 1 },
            LearnEntry { level: 6, rite_id: 15 },
            LearnEntry { level: 7, rite_id: 53 },
            LearnEntry { level: 10, rite_id: 7 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "A rider of the hills, spear over-arm, who came down at us like weather out of the mountains.",
        obstinacy: Some(55),
        sin_depth: Some(2),
    },
    SpeciesBase {
        id: 115,
        name: "Naffatun Sapper",
        typing: Typing::Valor,
        base: Stats { resolve: 50, valour: 45, piety: 70, guard: 55, aegis: 60, alacrity: 40 },
        learnset: &[
            // Balance-tune 2026-10-17: Comet of Naphtha pushed off the L1 lane
            // (L5 pool overtuning) and its power trimmed up 60 -> 80 to answer
            // the chronic L20/L40 sag — the naphtha pot is his whole trade.
            LearnEntry { level: 1, rite_id: 1 },
            LearnEntry { level: 6, rite_id: 54 },
            LearnEntry { level: 6, rite_id: 55 },
            LearnEntry { level: 9, rite_id: 5 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "He carried fire in clay and walked through arrows as a man walks through rain; burnt, we hold him no less in honor.",
        obstinacy: Some(40),
        sin_depth: Some(1),
    },
    SpeciesBase {
        id: 116,
        name: "Askari Alamdar",
        typing: Typing::Valor,
        base: Stats { resolve: 75, valour: 60, piety: 40, guard: 70, aegis: 65, alacrity: 45 },
        learnset: &[
            // Balance-tune 2026-10-17: Bulwark's Stand pushed off the L5 lane
            // (L5 pool overtuning); The Banner Advances (62) answers the L20/L40
            // sag — the banner going forward with the line behind it.
            LearnEntry { level: 1, rite_id: 1 },
            LearnEntry { level: 6, rite_id: 18 },
            LearnEntry { level: 9, rite_id: 56 },
            LearnEntry { level: 12, rite_id: 62 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "Where the banner stood the line stood; while he lived, that host would not be turned.",
        obstinacy: Some(58),
        sin_depth: Some(2),
    },
    SpeciesBase {
        // Mini-boss tier: 405 total rides the commander band by design.
        id: 117,
        name: "Muqaddam of the Tulb",
        typing: Typing::Valor,
        base: Stats { resolve: 85, valour: 85, piety: 45, guard: 80, aegis: 60, alacrity: 50 },
        learnset: &[
            LearnEntry { level: 1, rite_id: 15 },
            LearnEntry { level: 6, rite_id: 45 },
            LearnEntry { level: 10, rite_id: 57 },
            LearnEntry { level: 14, rite_id: 56 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "He wheeled his tulb as a shepherd turns one flock; our left horn was spent before the ninth hour.",
        obstinacy: Some(75),
        sin_depth: Some(3),
    },
    // ---------- Commanders (section 5): never convert ----------
    SpeciesBase {
        id: 120,
        name: "Kilij Arslan I",
        typing: Typing::Valor,
        base: Stats { resolve: 70, valour: 75, piety: 45, guard: 65, aegis: 60, alacrity: 85 },
        learnset: &[
            LearnEntry { level: 1, rite_id: 1 },
            LearnEntry { level: 1, rite_id: 43 },
            LearnEntry { level: 6, rite_id: 51 },
            LearnEntry { level: 10, rite_id: 44 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "The Sultan of Rum harried our van from ambush at Dorylaeum; his horsemen ringed the plain, and the day hung in doubt until relief came.",
        obstinacy: None,
        sin_depth: Some(3),
    },
    SpeciesBase {
        id: 121,
        name: "Nur ad-Din",
        typing: Typing::Valor,
        base: Stats { resolve: 80, valour: 85, piety: 50, guard: 75, aegis: 65, alacrity: 60 },
        learnset: &[
            LearnEntry { level: 1, rite_id: 45 },
            LearnEntry { level: 5, rite_id: 47 },
            LearnEntry { level: 10, rite_id: 53 },
            LearnEntry { level: 14, rite_id: 56 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "The atabeg's host came on in good order, horse and foot as one machine; his justice and his night prayers were reported even among our own.",
        obstinacy: None,
        sin_depth: Some(3),
    },
    SpeciesBase {
        id: 122,
        name: "Asad al-Din Shirkuh",
        typing: Typing::Valor,
        base: Stats { resolve: 90, valour: 95, piety: 40, guard: 80, aegis: 55, alacrity: 55 },
        learnset: &[
            LearnEntry { level: 1, rite_id: 49 },
            LearnEntry { level: 1, rite_id: 51 },
            LearnEntry { level: 8, rite_id: 53 },
            LearnEntry { level: 12, rite_id: 57 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "The old mountain lion marched us into Egypt and out again thrice; he counted miles as other commanders count swords.",
        obstinacy: None,
        sin_depth: Some(3),
    },
    SpeciesBase {
        id: 123,
        name: "Saladin",
        typing: Typing::Valor,
        base: Stats { resolve: 85, valour: 85, piety: 55, guard: 80, aegis: 70, alacrity: 45 },
        learnset: &[
            LearnEntry { level: 1, rite_id: 47 },
            LearnEntry { level: 6, rite_id: 57 },
            LearnEntry { level: 10, rite_id: 56 },
            LearnEntry { level: 14, rite_id: 18 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "The Sultan denied us water at Hattin and broke the kingdom's host; yet he kept his safe-conducts, and at Jaffa his courtesy crossed the lines.",
        obstinacy: None,
        sin_depth: Some(3),
    },
    SpeciesBase {
        // Hattin wing (section 5 sub-boss texture): Saladin's shrewd
        // right hand; rides the commander band at 405 and never converts.
        id: 124,
        name: "Gokbori",
        typing: Typing::Valor,
        base: Stats { resolve: 80, valour: 80, piety: 45, guard: 70, aegis: 60, alacrity: 70 },
        learnset: &[
            LearnEntry { level: 1, rite_id: 51 },
            LearnEntry { level: 6, rite_id: 43 },
            LearnEntry { level: 10, rite_id: 57 },
            LearnEntry { level: 14, rite_id: 56 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "He rode the Sultan's left wing at Hattin, folding our flank in all day long; shrewd in the spending of men, and grave with it.",
        obstinacy: None,
        sin_depth: Some(3),
    },
    SpeciesBase {
        // Hattin wing (section 5 sub-boss texture): the Sultan's left
        // hand; rides the commander band at 400 and never converts.
        id: 125,
        name: "Taki al-Din Umar",
        typing: Typing::Valor,
        base: Stats { resolve: 85, valour: 80, piety: 50, guard: 75, aegis: 65, alacrity: 45 },
        learnset: &[
            LearnEntry { level: 1, rite_id: 45 },
            LearnEntry { level: 6, rite_id: 53 },
            LearnEntry { level: 10, rite_id: 47 },
            LearnEntry { level: 14, rite_id: 57 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "The Sultan's right wing barred the springs of Hattin from us; from the ninth hour the thirst did his work, and the day went with the water.",
        obstinacy: None,
        sin_depth: Some(3),
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{rite_by_id, species_by_id};

    #[test]
    fn adversaries_sorted_unique_ids_and_count() {
        assert_eq!(ADVERSARIES.len(), 16);
        for (i, w) in ADVERSARIES.windows(2).enumerate() {
            assert!(w[0].id < w[1].id, "ids out of order at {}", i);
        }
        let ids: Vec<u16> = ADVERSARIES.iter().map(|s| s.id).collect();
        let mut expected: Vec<u16> = (108..=117).collect();
        expected.extend(120..=125);
        assert_eq!(ids, expected);
    }

    #[test]
    fn adversary_rites_sorted_unique_ids_43_to_62() {
        assert_eq!(ADVERSARY_RITES.len(), 20);
        for (i, r) in ADVERSARY_RITES.iter().enumerate() {
            assert_eq!(r.id, i as u16 + 43);
        }
        // The champion table stays locked at 1..=42.
        assert_eq!(crate::data::rites::RITES.len(), 42);
    }

    #[test]
    fn adversary_rite_names_nonempty_ascii_and_bounds() {
        for r in ADVERSARY_RITES {
            assert!(!r.name.is_empty());
            assert!(r.name.is_ascii(), "{}", r.name);
            if r.category == RiteCategory::Status {
                assert_eq!(r.power, 0, "status rite {} has power", r.name);
            } else {
                assert!((20..=120).contains(&r.power), "power {}", r.name);
            }
            assert!((70..=101).contains(&r.accuracy), "accuracy {}", r.name);
            assert!((5..=40).contains(&r.zeal), "zeal {}", r.name);
            assert!((-2..=2).contains(&r.priority), "priority {}", r.name);
            // The Valor lane is off the chart: flat 1x, never STAB.
            assert_eq!(r.rite_type, RiteType::Typeless, "{}", r.name);
        }
    }

    #[test]
    fn names_and_lore_nonempty_ascii() {
        for sp in ADVERSARIES {
            assert!(!sp.name.is_empty());
            assert!(sp.name.is_ascii(), "{}", sp.name);
            assert!(!sp.lore.is_empty(), "{} has empty lore", sp.name);
            assert!(sp.lore.is_ascii(), "{}", sp.name);
        }
    }

    #[test]
    fn learnset_rites_exist_levels_ascending_in_range() {
        for sp in ADVERSARIES {
            assert!(!sp.learnset.is_empty(), "{} has empty learnset", sp.name);
            for w in sp.learnset.windows(2) {
                assert!(
                    w[0].level <= w[1].level,
                    "{} learnset levels not ascending",
                    sp.name
                );
            }
            for e in sp.learnset {
                assert!(
                    (1..=crate::types::MAX_LEVEL).contains(&e.level),
                    "{} learnset level {} out of range",
                    sp.name,
                    e.level
                );
                assert!(
                    rite_by_id(e.rite_id).is_some(),
                    "{} references unknown rite {}",
                    sp.name,
                    e.rite_id
                );
            }
        }
    }

    #[test]
    fn every_adversary_has_damaging_rite_at_level_5_or_below() {
        for sp in ADVERSARIES {
            let early_damage = sp.learnset.iter().any(|e| {
                e.level <= 5
                    && rite_by_id(e.rite_id)
                        .map(|r| r.category != RiteCategory::Status && r.power > 0)
                        .unwrap_or(false)
            });
            assert!(early_damage, "{} lacks early damaging rite", sp.name);
        }
    }

    #[test]
    fn all_adversaries_are_valor_typed() {
        for sp in ADVERSARIES {
            assert_eq!(sp.typing, Typing::Valor, "{}", sp.name);
        }
    }

    #[test]
    fn base_stats_in_range_and_totals_in_band() {
        for sp in ADVERSARIES {
            for stat in [
                sp.base.resolve,
                sp.base.valour,
                sp.base.piety,
                sp.base.guard,
                sp.base.aegis,
                sp.base.alacrity,
            ] {
                assert!(
                    (20..=130).contains(&stat),
                    "{} base stat {stat} out of 20..=130",
                    sp.name
                );
            }
            let total = sp.base.resolve
                + sp.base.valour
                + sp.base.piety
                + sp.base.guard
                + sp.base.aegis
                + sp.base.alacrity;
            // 117 rides the commander band by design (section 4 note).
            if (108..=116).contains(&sp.id) {
                assert!((300..=380).contains(&total), "{} total {total}", sp.name);
            } else {
                assert!((390..=420).contains(&total), "{} total {total}", sp.name);
            }
        }
    }

    /// CONVERSION.md section 6 tier bands: tier 0 20-35, tier 1 35-50,
    /// tier 2 50-70, tier 3 70-90. Commanders never convert (obstinacy
    /// None, sin_depth 3).
    #[test]
    fn obstinacy_and_sin_depth_follow_tier_table() {
        let tiers: [(u16, u8, u8); 10] = [
            (108, 28, 0),
            (109, 42, 1),
            (110, 40, 1),
            (111, 25, 0),
            (112, 45, 1),
            (113, 22, 0),
            (114, 55, 2),
            (115, 40, 1),
            (116, 58, 2),
            (117, 75, 3),
        ];
        for (id, obst, depth) in tiers {
            let sp = ADVERSARIES.iter().find(|s| s.id == id).unwrap();
            assert_eq!(sp.obstinacy, Some(obst), "{}", sp.name);
            assert_eq!(sp.sin_depth, Some(depth), "{}", sp.name);
            let (lo, hi) = match depth {
                0 => (20, 35),
                1 => (35, 50),
                2 => (50, 70),
                _ => (70, 90),
            };
            assert!(
                (lo..=hi).contains(&obst),
                "{} obstinacy {obst} outside tier {depth} band",
                sp.name
            );
        }
        for sp in ADVERSARIES.iter().filter(|s| s.id >= 120) {
            assert_eq!(sp.obstinacy, None, "{} must never convert", sp.name);
            assert_eq!(sp.sin_depth, Some(3), "{}", sp.name);
        }
    }

    /// Section 5 sub-boss texture: Saladin's Hattin wings — the shrewd
    /// left (Gokbori, 124) rides the commander band at 405, the right
    /// (Taki al-Din Umar, 125) at 400; both Valor-typed, never convertible,
    /// kits drawn from the adversary rites 43..=57.
    #[test]
    fn hattin_wing_commanders_match_spec() {
        let right = ADVERSARIES.iter().find(|s| s.id == 124).unwrap();
        let left = ADVERSARIES.iter().find(|s| s.id == 125).unwrap();
        assert_eq!(right.name, "Gokbori");
        assert_eq!(left.name, "Taki al-Din Umar");
        for (sp, want) in [(right, 405), (left, 400)] {
            let total = sp.base.resolve
                + sp.base.valour
                + sp.base.piety
                + sp.base.guard
                + sp.base.aegis
                + sp.base.alacrity;
            assert_eq!(total, want, "{} total {total}", sp.name);
            assert_eq!(sp.typing, Typing::Valor, "{}", sp.name);
            assert_eq!(sp.obstinacy, None, "{} must never convert", sp.name);
            assert_eq!(sp.sin_depth, Some(3), "{}", sp.name);
            assert!(
                sp.learnset.iter().all(|e| (43..=57).contains(&e.rite_id)),
                "{} kit draws on adversary rites",
                sp.name
            );
        }
    }

    #[test]
    fn species_by_id_scans_adversaries_after_species() {
        // ids < 108 resolve exactly as before.
        for id in 1..=12u16 {
            assert_eq!(species_by_id(id).map(|s| s.id), Some(id));
        }
        for id in 101..=107u16 {
            assert_eq!(species_by_id(id).map(|s| s.id), Some(id));
        }
        for sp in ADVERSARIES {
            assert_eq!(species_by_id(sp.id).map(|s| s.id), Some(sp.id));
        }
        // The archetype/commander gap stays empty.
        assert!(species_by_id(118).is_none());
        assert!(species_by_id(119).is_none());
        assert!(species_by_id(126).is_none());
    }
}

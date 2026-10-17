//! Species table. Worker A fills: 12 launch Champions (ids 1..=12) + 7
//! sin-spawn bosses (ids 101..=107). Sorted by id.
//!
//! Base stat order: resolve, valour, piety, guard, aegis, alacrity.
//! Champions bear 1-2 virtues; every virtue appears at least twice across the
//! roster. Sin-spawn bear one sin each and know two Sin-typed rites.

use crate::types::*;

pub static SPECIES: &[SpeciesBase] = &[
    SpeciesBase {
        id: 1,
        name: "Squire Audric",
        typing: Typing::Virtues([Some(Virtue::Humility), None]),
        base: Stats { resolve: 45, valour: 60, piety: 35, guard: 55, aegis: 50, alacrity: 65 },
        learnset: &[
            LearnEntry { level: 1, rite_id: 1 },
            LearnEntry { level: 1, rite_id: 2 },
            LearnEntry { level: 5, rite_id: 3 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "A squire who kneels before every dawn and counts his master's errands as prayers.",
        obstinacy: None,
        sin_depth: None,
    },
    SpeciesBase {
        id: 2,
        name: "Cleric Mabel",
        typing: Typing::Virtues([Some(Virtue::Charity), None]),
        base: Stats { resolve: 55, valour: 30, piety: 75, guard: 45, aegis: 80, alacrity: 25 },
        learnset: &[
            LearnEntry { level: 1, rite_id: 1 },
            LearnEntry { level: 1, rite_id: 4 },
            LearnEntry { level: 7, rite_id: 5 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "A gentle cleric whose alms purse empties as fast as it fills, for she cannot pass a beggar by.",
        obstinacy: None,
        sin_depth: None,
    },
    SpeciesBase {
        id: 3,
        name: "Serjeant Bastien",
        typing: Typing::Virtues([Some(Virtue::Patience), None]),
        base: Stats { resolve: 60, valour: 70, piety: 40, guard: 70, aegis: 45, alacrity: 40 },
        learnset: &[
            LearnEntry { level: 1, rite_id: 1 },
            LearnEntry { level: 1, rite_id: 2 },
            LearnEntry { level: 9, rite_id: 6 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "A serjeant who holds the line in silence long after bolder hearts have fled the field.",
        obstinacy: None,
        sin_depth: None,
    },
    SpeciesBase {
        id: 4,
        name: "Hospitaller Ysolt",
        typing: Typing::Virtues([Some(Virtue::Charity), None]),
        base: Stats { resolve: 80, valour: 30, piety: 70, guard: 55, aegis: 75, alacrity: 30 },
        learnset: &[
            LearnEntry { level: 1, rite_id: 1 },
            LearnEntry { level: 3, rite_id: 4 },
            LearnEntry { level: 6, rite_id: 10 },
            LearnEntry { level: 9, rite_id: 27 },
            LearnEntry { level: 13, rite_id: 11 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "A hospitaller sister who binds wounds under a hail of arrows and asks after the soul before the flesh.",
        obstinacy: None,
        sin_depth: None,
    },
    SpeciesBase {
        id: 5,
        name: "Wayfarer Aelis",
        typing: Typing::Virtues([Some(Virtue::Kindness), None]),
        base: Stats { resolve: 50, valour: 55, piety: 40, guard: 45, aegis: 50, alacrity: 100 },
        learnset: &[
            LearnEntry { level: 1, rite_id: 28 },
            LearnEntry { level: 3, rite_id: 23 },
            LearnEntry { level: 6, rite_id: 25 },
            LearnEntry { level: 10, rite_id: 8 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "A pilgrim scout who has walked every road to the holy city thrice and gladly guides strangers along them.",
        obstinacy: None,
        sin_depth: None,
    },
    SpeciesBase {
        id: 6,
        name: "Canoness Hrothwyn",
        typing: Typing::Virtues([Some(Virtue::Chastity), None]),
        base: Stats { resolve: 60, valour: 40, piety: 90, guard: 50, aegis: 70, alacrity: 40 },
        learnset: &[
            LearnEntry { level: 1, rite_id: 1 },
            LearnEntry { level: 4, rite_id: 12 },
            LearnEntry { level: 7, rite_id: 14 },
            LearnEntry { level: 11, rite_id: 13 },
            LearnEntry { level: 15, rite_id: 21 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "A canoness whose clear voice in the choir settles even the most restless heart.",
        obstinacy: None,
        sin_depth: None,
    },
    SpeciesBase {
        id: 7,
        name: "Vexillor Anchetil",
        typing: Typing::Virtues([Some(Virtue::Diligence), None]),
        base: Stats { resolve: 65, valour: 80, piety: 35, guard: 65, aegis: 55, alacrity: 45 },
        learnset: &[
            LearnEntry { level: 1, rite_id: 1 },
            LearnEntry { level: 1, rite_id: 7 },
            LearnEntry { level: 5, rite_id: 15 },
            LearnEntry { level: 9, rite_id: 17 },
            LearnEntry { level: 14, rite_id: 16 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "A standard-bearer who plants the pilgrim star's banner and would sooner break than let it fall.",
        obstinacy: None,
        sin_depth: None,
    },
    SpeciesBase {
        id: 8,
        name: "Brother Aldwyn",
        typing: Typing::Virtues([Some(Virtue::Humility), None]),
        base: Stats { resolve: 75, valour: 60, piety: 45, guard: 70, aegis: 70, alacrity: 35 },
        learnset: &[
            LearnEntry { level: 1, rite_id: 1 },
            LearnEntry { level: 1, rite_id: 2 },
            LearnEntry { level: 5, rite_id: 3 },
            LearnEntry { level: 10, rite_id: 9 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "A humble brother who takes the lowest bench, the heaviest pack, and the last crust without a murmur.",
        obstinacy: None,
        sin_depth: None,
    },
    SpeciesBase {
        id: 9,
        name: "Cantor Sabina",
        typing: Typing::Virtues([Some(Virtue::Temperance), None]),
        base: Stats { resolve: 55, valour: 55, piety: 80, guard: 50, aegis: 65, alacrity: 55 },
        learnset: &[
            LearnEntry { level: 1, rite_id: 1 },
            LearnEntry { level: 4, rite_id: 21 },
            LearnEntry { level: 8, rite_id: 14 },
            LearnEntry { level: 12, rite_id: 22 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "A cantor whose measured psalms keep the whole company in even step and even temper.",
        obstinacy: None,
        sin_depth: None,
    },
    SpeciesBase {
        id: 10,
        name: "Chevalier Raimbaud",
        typing: Typing::Virtues([Some(Virtue::Patience), Some(Virtue::Kindness)]),
        base: Stats { resolve: 70, valour: 75, piety: 40, guard: 75, aegis: 60, alacrity: 40 },
        learnset: &[
            LearnEntry { level: 1, rite_id: 1 },
            LearnEntry { level: 3, rite_id: 7 },
            LearnEntry { level: 6, rite_id: 25 },
            LearnEntry { level: 10, rite_id: 18 },
            LearnEntry { level: 16, rite_id: 19 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "A chevalier who bears every blow in patience and answers cruelty only with mercy.",
        obstinacy: None,
        sin_depth: None,
    },
    SpeciesBase {
        id: 11,
        name: "Vigil-Knight Wymar",
        typing: Typing::Virtues([Some(Virtue::Diligence), Some(Virtue::Temperance)]),
        base: Stats { resolve: 70, valour: 70, piety: 50, guard: 70, aegis: 65, alacrity: 45 },
        learnset: &[
            LearnEntry { level: 1, rite_id: 1 },
            LearnEntry { level: 4, rite_id: 22 },
            LearnEntry { level: 8, rite_id: 17 },
            LearnEntry { level: 13, rite_id: 16 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "A knight of the long vigil who counts wakeful nights as prayers and duties as blessings.",
        obstinacy: None,
        sin_depth: None,
    },
    SpeciesBase {
        id: 12,
        name: "Friar Osbert",
        typing: Typing::Virtues([Some(Virtue::Chastity), None]),
        base: Stats { resolve: 60, valour: 45, piety: 75, guard: 60, aegis: 60, alacrity: 50 },
        learnset: &[
            LearnEntry { level: 1, rite_id: 1 },
            LearnEntry { level: 3, rite_id: 26 },
            LearnEntry { level: 6, rite_id: 14 },
            LearnEntry { level: 10, rite_id: 13 },
            LearnEntry { level: 14, rite_id: 5 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "A friar who preaches purity of heart in the market square and fears no scorn.",
        obstinacy: None,
        sin_depth: None,
    },
    SpeciesBase {
        id: 101,
        name: "Prince of Pride",
        typing: Typing::Sin(Sin::Pride),
        base: Stats { resolve: 60, valour: 55, piety: 55, guard: 50, aegis: 50, alacrity: 45 },
        learnset: &[
            LearnEntry { level: 1, rite_id: 8 },
            LearnEntry { level: 1, rite_id: 41 },
            LearnEntry { level: 8, rite_id: 42 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "A crowned shadow that whispers to every champion that he alone deserves the victory.",
        obstinacy: None,
        sin_depth: None,
    },
    SpeciesBase {
        id: 102,
        name: "Wretch of Envy",
        typing: Typing::Sin(Sin::Envy),
        base: Stats { resolve: 55, valour: 50, piety: 60, guard: 45, aegis: 50, alacrity: 60 },
        learnset: &[
            LearnEntry { level: 1, rite_id: 1 },
            LearnEntry { level: 3, rite_id: 29 },
            LearnEntry { level: 8, rite_id: 30 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "A hollow-eyed wretch that covets every virtue it sees and poisons what it cannot have.",
        obstinacy: None,
        sin_depth: None,
    },
    SpeciesBase {
        id: 103,
        name: "Fiend of Lust",
        typing: Typing::Sin(Sin::Lust),
        base: Stats { resolve: 50, valour: 70, piety: 65, guard: 40, aegis: 45, alacrity: 55 },
        learnset: &[
            LearnEntry { level: 1, rite_id: 8 },
            LearnEntry { level: 4, rite_id: 31 },
            LearnEntry { level: 8, rite_id: 32 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "A sweet-voiced fiend whose burning haze unmakes every vow it touches.",
        obstinacy: None,
        sin_depth: None,
    },
    SpeciesBase {
        id: 104,
        name: "Sleeper of Sloth",
        typing: Typing::Sin(Sin::Sloth),
        base: Stats { resolve: 90, valour: 55, piety: 40, guard: 60, aegis: 55, alacrity: 20 },
        learnset: &[
            LearnEntry { level: 1, rite_id: 1 },
            LearnEntry { level: 3, rite_id: 33 },
            LearnEntry { level: 8, rite_id: 34 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "A vast sleeper whose dreams seep outward as a grey mire of idleness.",
        obstinacy: None,
        sin_depth: None,
    },
    SpeciesBase {
        id: 105,
        name: "Brute of Wrath",
        typing: Typing::Sin(Sin::Wrath),
        base: Stats { resolve: 65, valour: 85, piety: 35, guard: 50, aegis: 45, alacrity: 55 },
        learnset: &[
            LearnEntry { level: 1, rite_id: 1 },
            LearnEntry { level: 4, rite_id: 35 },
            LearnEntry { level: 8, rite_id: 36 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "A frothing brute whose rage feeds on every wound it gives and every wound it takes.",
        obstinacy: None,
        sin_depth: None,
    },
    SpeciesBase {
        id: 106,
        name: "Glutton Maw",
        typing: Typing::Sin(Sin::Gluttony),
        base: Stats { resolve: 85, valour: 65, piety: 45, guard: 55, aegis: 45, alacrity: 25 },
        learnset: &[
            LearnEntry { level: 1, rite_id: 8 },
            LearnEntry { level: 3, rite_id: 37 },
            LearnEntry { level: 8, rite_id: 38 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "A walking maw that devours harvests whole and yet starves the more it eats.",
        obstinacy: None,
        sin_depth: None,
    },
    SpeciesBase {
        id: 107,
        name: "Grasper of Greed",
        typing: Typing::Sin(Sin::Greed),
        base: Stats { resolve: 60, valour: 70, piety: 60, guard: 50, aegis: 50, alacrity: 50 },
        learnset: &[
            LearnEntry { level: 1, rite_id: 1 },
            LearnEntry { level: 3, rite_id: 39 },
            LearnEntry { level: 8, rite_id: 40 },
        ],
        xp_slope: 4,
        xp_div: 5,
        lore: "A grasping miser-fiend that hoards even the light and drowns pilgrims in its golden tide.",
        obstinacy: None,
        sin_depth: None,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{compute_stats, rite_by_id, species_by_id};

    #[test]
    fn species_sorted_unique_ids_and_count() {
        assert_eq!(SPECIES.len(), 19);
        for (i, w) in SPECIES.windows(2).enumerate() {
            assert!(w[0].id < w[1].id, "ids out of order at {}", i);
        }
        let ids: Vec<u16> = SPECIES.iter().map(|s| s.id).collect();
        let mut expected: Vec<u16> = (1..=12).collect();
        expected.extend(101..=107);
        assert_eq!(ids, expected);
    }

    #[test]
    fn names_and_lore_nonempty_ascii() {
        for sp in SPECIES {
            assert!(!sp.name.is_empty());
            assert!(sp.name.is_ascii(), "{}", sp.name);
            assert!(!sp.lore.is_empty(), "{} has empty lore", sp.name);
            assert!(sp.lore.is_ascii(), "{}", sp.name);
        }
    }

    #[test]
    fn learnset_rites_exist_levels_ascending_in_range() {
        for sp in SPECIES {
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
    fn every_species_has_damaging_rite_at_level_5_or_below() {
        for sp in SPECIES {
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
    fn champion_constraints_ids_4_to_12() {
        for sp in SPECIES.iter().filter(|s| (4..=12).contains(&s.id)) {
            let total = sp.base.resolve
                + sp.base.valour
                + sp.base.piety
                + sp.base.guard
                + sp.base.aegis
                + sp.base.alacrity;
            assert!((300..=420).contains(&total), "{} total {total}", sp.name);
            let n = sp.learnset.len();
            assert!((3..=7).contains(&n), "{} learnset len {n}", sp.name);
        }
    }

    #[test]
    fn roster_covers_every_virtue_twice_and_has_two_duals() {
        let champions: Vec<&SpeciesBase> = SPECIES.iter().filter(|s| s.id < 100).collect();
        assert_eq!(champions.len(), 12);
        for v in Virtue::ALL {
            let n = champions
                .iter()
                .filter(|s| match s.typing {
                    Typing::Virtues(vs) => vs.iter().flatten().any(|x| *x == v),
                    Typing::Sin(_) | Typing::Valor => false,
                })
                .count();
            assert!(n >= 2, "virtue {v:?} covered only {n} times");
        }
        let duals = champions
            .iter()
            .filter(|s| matches!(s.typing, Typing::Virtues([Some(_), Some(_)])))
            .count();
        assert!(duals >= 2, "need >= 2 dual-virtue champions, got {duals}");
    }

    #[test]
    fn sin_spawn_cover_each_sin_and_have_two_sin_rites() {
        let sins = [
            Sin::Pride,
            Sin::Envy,
            Sin::Lust,
            Sin::Sloth,
            Sin::Wrath,
            Sin::Gluttony,
            Sin::Greed,
        ];
        for s in sins {
            assert!(
                SPECIES.iter().any(|sp| sp.typing == Typing::Sin(s)),
                "no species for sin {s:?}"
            );
        }
        for sp in SPECIES.iter().filter(|s| s.id >= 101) {
            let Typing::Sin(own) = sp.typing else {
                panic!("{} is not Sin-typed", sp.name);
            };
            let sin_rites: Vec<&Rite> = sp
                .learnset
                .iter()
                .filter_map(|e| rite_by_id(e.rite_id))
                .filter(|r| matches!(r.rite_type, RiteType::Sin(_)))
                .collect();
            assert!(
                sin_rites.len() >= 2,
                "{} has fewer than two Sin-typed rites",
                sp.name
            );
            for r in sin_rites {
                assert_eq!(r.rite_type, RiteType::Sin(own), "{}: {}", sp.name, r.name);
            }
        }
    }

    #[test]
    fn base_stats_in_range_and_sin_spawn_have_two_sin_rites() {
        for sp in SPECIES {
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
        }
        for sp in SPECIES.iter().filter(|s| (101..=107).contains(&s.id)) {
            let n = sp
                .learnset
                .iter()
                .filter_map(|e| rite_by_id(e.rite_id))
                .filter(|r| matches!(r.rite_type, RiteType::Sin(_)))
                .count();
            assert!(n >= 2, "{} has {n} Sin-typed rites, need >= 2", sp.name);
        }
    }

    // ---------- compute_stats sanity ----------

    fn zero_instance(species_id: u16, level: u8, calling_id: u8) -> ChampionInstance {
        ChampionInstance {
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
            calling_id,
            rites: [None, None, None, None],
            current_resolve: 1,
            ailment: None,
            converted: false,
            converted_from: None,
        }
    }

    #[test]
    fn audric_resolve_grows_with_level() {
        let sp = species_by_id(1).expect("Audric exists");
        let lvl1 = compute_stats(&zero_instance(1, 1, 0), sp);
        let lvl50 = compute_stats(&zero_instance(1, 50, 0), sp);
        assert!(lvl50.max_resolve > lvl1.max_resolve);
    }

    #[test]
    fn fervent_mabel_has_more_piety_than_steadfast_mabel() {
        let sp = species_by_id(2).expect("Mabel exists");
        let steady = compute_stats(&zero_instance(2, 20, 0), sp);
        let fervent = compute_stats(&zero_instance(2, 20, 1), sp);
        assert!(fervent.piety > steady.piety);
    }

    // ---------- validate_instance ----------

    fn legal_audric() -> ChampionInstance {
        ChampionInstance {
            species_id: 1,
            level: 10,
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
            // Rites 1 and 2 are in Audric's learnset at level 1; zeal matches
            // the rites' max zeal (35 and 20).
            rites: [
                Some(RiteSlot {
                    rite_id: 1,
                    zeal_left: 35,
                }),
                Some(RiteSlot {
                    rite_id: 2,
                    zeal_left: 20,
                }),
                None,
                None,
            ],
            current_resolve: 1,
            ailment: None,
            converted: false,
            converted_from: None,
        }
    }

    fn audric_base() -> &'static SpeciesBase {
        species_by_id(1).expect("Audric exists")
    }

    #[test]
    fn validate_accepts_legal_instance() {
        assert_eq!(validate_instance(&legal_audric(), audric_base()), Ok(()));
    }

    #[test]
    fn validate_rejects_bad_level() {
        let mut c = legal_audric();
        c.level = 0;
        assert_eq!(
            validate_instance(&c, audric_base()),
            Err(InstanceError::BadLevel(0))
        );
        c.level = 51;
        assert_eq!(
            validate_instance(&c, audric_base()),
            Err(InstanceError::BadLevel(51))
        );
    }

    #[test]
    fn validate_rejects_bad_grace() {
        let mut c = legal_audric();
        c.grace.piety = 16;
        assert_eq!(
            validate_instance(&c, audric_base()),
            Err(InstanceError::BadGrace(16))
        );
    }

    #[test]
    fn validate_rejects_bad_devotion() {
        let mut c = legal_audric();
        c.devotion.guard = 64;
        assert_eq!(
            validate_instance(&c, audric_base()),
            Err(InstanceError::BadDevotion(64))
        );
    }

    #[test]
    fn validate_rejects_bad_devotion_total() {
        let mut c = legal_audric();
        c.devotion = DevotionStats {
            resolve: 30,
            valour: 30,
            piety: 30,
            guard: 30,
            aegis: 5,
            alacrity: 6,
        };
        assert_eq!(
            validate_instance(&c, audric_base()),
            Err(InstanceError::BadDevotionTotal(131))
        );
    }

    #[test]
    fn validate_rejects_illegal_rite() {
        // Rite not in the species' learnset at all.
        let mut c = legal_audric();
        c.rites[2] = Some(RiteSlot {
            rite_id: 40,
            zeal_left: 25,
        });
        assert_eq!(
            validate_instance(&c, audric_base()),
            Err(InstanceError::IllegalRite(40))
        );
        // Rite in learnset but at a level above the instance.
        let mut c = legal_audric();
        c.level = 4;
        c.rites[1] = Some(RiteSlot {
            rite_id: 3,
            zeal_left: 15,
        });
        assert_eq!(
            validate_instance(&c, audric_base()),
            Err(InstanceError::IllegalRite(3))
        );
        // Rite legal but zeal_left differs from the rite's max zeal.
        let mut c = legal_audric();
        c.rites[0] = Some(RiteSlot {
            rite_id: 1,
            zeal_left: 34,
        });
        assert_eq!(
            validate_instance(&c, audric_base()),
            Err(InstanceError::IllegalRite(1))
        );
    }

    #[test]
    fn validate_rejects_resolve_overflow() {
        let sp = audric_base();
        let mut c = legal_audric();
        let max = compute_stats(&c, sp).max_resolve;
        c.current_resolve = max + 1;
        assert_eq!(
            validate_instance(&c, sp),
            Err(InstanceError::ResolveOverflow)
        );
        c.current_resolve = max;
        assert_eq!(validate_instance(&c, sp), Ok(()));
    }
}

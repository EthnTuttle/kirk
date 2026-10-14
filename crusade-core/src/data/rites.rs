//! Rite table. Worker A fills (42 rites, ids 1..=42).
//!
//! Ids 1..=8 are contract-fixed. Ids 9..=28 round out the champion arsenal
//! (three rites per virtue, more Typeless utility); ids 29..=42 are Sin-typed
//! rites for sin-spawn, two for each of the seven sins.

use crate::types::*;

pub static RITES: &[Rite] = &[
    Rite {
        id: 1,
        name: "Measured Blow",
        rite_type: RiteType::Virtue(Virtue::Humility),
        category: RiteCategory::Martial,
        power: 40,
        accuracy: 100,
        zeal: 35,
        priority: 0,
        secondaries: &[],
    },
    Rite {
        id: 2,
        name: "Steady Ward",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Status,
        power: 0,
        accuracy: 101,
        zeal: 20,
        priority: 1,
        secondaries: &[Secondary::StageOnSelf {
            stat: BattleStat::Guard,
            delta: 1,
            chance_pct: 100,
        }],
    },
    Rite {
        id: 3,
        name: "Humble Rebuke",
        rite_type: RiteType::Virtue(Virtue::Humility),
        category: RiteCategory::Spiritual,
        power: 55,
        accuracy: 95,
        zeal: 15,
        priority: 0,
        secondaries: &[Secondary::Ailment {
            ailment: Ailment::Brand,
            chance_pct: 10,
        }],
    },
    Rite {
        id: 4,
        name: "Almsgiving",
        rite_type: RiteType::Virtue(Virtue::Charity),
        category: RiteCategory::Spiritual,
        power: 45,
        accuracy: 100,
        zeal: 20,
        priority: 0,
        secondaries: &[Secondary::Drain { pct: 50 }],
    },
    Rite {
        id: 5,
        name: "Litany of Stillness",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Status,
        power: 0,
        accuracy: 90,
        zeal: 15,
        priority: 0,
        secondaries: &[Secondary::Ailment {
            ailment: Ailment::Slumber,
            chance_pct: 100,
        }],
    },
    Rite {
        id: 6,
        name: "Oath of Patience",
        rite_type: RiteType::Virtue(Virtue::Patience),
        category: RiteCategory::Martial,
        power: 60,
        accuracy: 100,
        zeal: 10,
        priority: -1,
        secondaries: &[Secondary::StageOnSelf {
            stat: BattleStat::Guard,
            delta: 1,
            chance_pct: 100,
        }],
    },
    Rite {
        id: 7,
        name: "Zealous Cry",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Status,
        power: 0,
        accuracy: 101,
        zeal: 25,
        priority: 0,
        secondaries: &[Secondary::StageOnSelf {
            stat: BattleStat::Valour,
            delta: 1,
            chance_pct: 100,
        }],
    },
    Rite {
        id: 8,
        name: "Flail of Penance",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Martial,
        power: 50,
        accuracy: 90,
        zeal: 25,
        priority: 0,
        secondaries: &[Secondary::MultiHit],
    },
    Rite {
        id: 9,
        name: "Humbling Light",
        rite_type: RiteType::Virtue(Virtue::Humility),
        category: RiteCategory::Spiritual,
        power: 50,
        accuracy: 100,
        zeal: 20,
        priority: 0,
        secondaries: &[Secondary::StageOnTarget {
            stat: BattleStat::Valour,
            delta: -1,
            chance_pct: 100,
        }],
    },
    Rite {
        id: 10,
        name: "Vigil Light",
        rite_type: RiteType::Virtue(Virtue::Charity),
        category: RiteCategory::Status,
        power: 0,
        accuracy: 101,
        zeal: 10,
        priority: 0,
        secondaries: &[Secondary::StageOnSelf {
            stat: BattleStat::Piety,
            delta: 1,
            chance_pct: 100,
        }],
    },
    Rite {
        id: 11,
        name: "Radiance of Mercy",
        rite_type: RiteType::Virtue(Virtue::Charity),
        category: RiteCategory::Spiritual,
        power: 60,
        accuracy: 95,
        zeal: 15,
        priority: 0,
        secondaries: &[],
    },
    Rite {
        id: 12,
        name: "Rite of Censure",
        rite_type: RiteType::Virtue(Virtue::Chastity),
        category: RiteCategory::Status,
        power: 0,
        accuracy: 95,
        zeal: 15,
        priority: 0,
        secondaries: &[Secondary::StageOnTarget {
            stat: BattleStat::Piety,
            delta: -1,
            chance_pct: 100,
        }],
    },
    Rite {
        id: 13,
        name: "Frost of Contrition",
        rite_type: RiteType::Virtue(Virtue::Chastity),
        category: RiteCategory::Spiritual,
        power: 55,
        accuracy: 95,
        zeal: 15,
        priority: 0,
        secondaries: &[Secondary::Ailment {
            ailment: Ailment::Frost,
            chance_pct: 10,
        }],
    },
    Rite {
        id: 14,
        name: "Argent Verdict",
        rite_type: RiteType::Virtue(Virtue::Chastity),
        category: RiteCategory::Spiritual,
        power: 70,
        accuracy: 90,
        zeal: 10,
        priority: 0,
        secondaries: &[Secondary::Volatile {
            volatile_: Volatile::Flinch,
            chance_pct: 20,
        }],
    },
    Rite {
        id: 15,
        name: "Sundering Blow",
        rite_type: RiteType::Virtue(Virtue::Diligence),
        category: RiteCategory::Martial,
        power: 45,
        accuracy: 100,
        zeal: 25,
        priority: 0,
        secondaries: &[Secondary::StageOnTarget {
            stat: BattleStat::Guard,
            delta: -1,
            chance_pct: 100,
        }],
    },
    Rite {
        id: 16,
        name: "Sword of the Watch",
        rite_type: RiteType::Virtue(Virtue::Diligence),
        category: RiteCategory::Martial,
        power: 75,
        accuracy: 95,
        zeal: 15,
        priority: 0,
        secondaries: &[Secondary::HighCrit],
    },
    Rite {
        id: 17,
        name: "Faithful Toil",
        rite_type: RiteType::Virtue(Virtue::Diligence),
        category: RiteCategory::Martial,
        power: 65,
        accuracy: 100,
        zeal: 15,
        priority: 0,
        secondaries: &[],
    },
    Rite {
        id: 18,
        name: "Bulwark's Stand",
        rite_type: RiteType::Virtue(Virtue::Patience),
        category: RiteCategory::Martial,
        power: 80,
        accuracy: 90,
        zeal: 10,
        priority: 0,
        secondaries: &[],
    },
    Rite {
        id: 19,
        name: "Glorious Burden",
        rite_type: RiteType::Virtue(Virtue::Patience),
        category: RiteCategory::Martial,
        power: 120,
        accuracy: 90,
        zeal: 5,
        priority: 0,
        secondaries: &[Secondary::Recoil { pct: 33 }],
    },
    Rite {
        id: 20,
        name: "Warding Mantle",
        rite_type: RiteType::Virtue(Virtue::Temperance),
        category: RiteCategory::Status,
        power: 0,
        accuracy: 101,
        zeal: 15,
        priority: 0,
        secondaries: &[Secondary::StageOnSelf {
            stat: BattleStat::Aegis,
            delta: 1,
            chance_pct: 100,
        }],
    },
    Rite {
        id: 21,
        name: "Piercing Hymn",
        rite_type: RiteType::Virtue(Virtue::Temperance),
        category: RiteCategory::Spiritual,
        power: 50,
        accuracy: 95,
        zeal: 15,
        priority: 0,
        secondaries: &[Secondary::StageOnTarget {
            stat: BattleStat::Aegis,
            delta: -1,
            chance_pct: 100,
        }],
    },
    Rite {
        id: 22,
        name: "Tempered Blade",
        rite_type: RiteType::Virtue(Virtue::Temperance),
        category: RiteCategory::Martial,
        power: 65,
        accuracy: 100,
        zeal: 15,
        priority: 0,
        secondaries: &[],
    },
    Rite {
        id: 23,
        name: "Soaring Hymn",
        rite_type: RiteType::Virtue(Virtue::Kindness),
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
        id: 24,
        name: "Psalm of Warding",
        rite_type: RiteType::Virtue(Virtue::Kindness),
        category: RiteCategory::Status,
        power: 0,
        accuracy: 101,
        zeal: 20,
        priority: 0,
        secondaries: &[Secondary::StageOnSelf {
            stat: BattleStat::Guard,
            delta: 1,
            chance_pct: 100,
        }],
    },
    Rite {
        id: 25,
        name: "Shepherd's Crook",
        rite_type: RiteType::Virtue(Virtue::Kindness),
        category: RiteCategory::Martial,
        power: 60,
        accuracy: 95,
        zeal: 20,
        priority: 0,
        secondaries: &[],
    },
    Rite {
        id: 26,
        name: "Chains of Conscience",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Status,
        power: 0,
        accuracy: 90,
        zeal: 15,
        priority: 0,
        secondaries: &[Secondary::Ailment {
            ailment: Ailment::Shackled,
            chance_pct: 100,
        }],
    },
    Rite {
        id: 27,
        name: "Balm of Gilead",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Status,
        power: 0,
        accuracy: 101,
        zeal: 5,
        priority: 0,
        secondaries: &[Secondary::HealSelf { pct: 50 }],
    },
    Rite {
        id: 28,
        name: "Pilgrim's Advance",
        rite_type: RiteType::Typeless,
        category: RiteCategory::Martial,
        power: 40,
        accuracy: 100,
        zeal: 30,
        priority: 1,
        secondaries: &[],
    },
    Rite {
        id: 29,
        name: "Whisper of Envy",
        rite_type: RiteType::Sin(Sin::Envy),
        category: RiteCategory::Spiritual,
        power: 40,
        accuracy: 100,
        zeal: 25,
        priority: 0,
        secondaries: &[Secondary::Ailment {
            ailment: Ailment::Venom,
            chance_pct: 20,
        }],
    },
    Rite {
        id: 30,
        name: "Covetous Fang",
        rite_type: RiteType::Sin(Sin::Envy),
        category: RiteCategory::Martial,
        power: 65,
        accuracy: 95,
        zeal: 15,
        priority: 0,
        secondaries: &[],
    },
    Rite {
        id: 31,
        name: "Alluring Haze",
        rite_type: RiteType::Sin(Sin::Lust),
        category: RiteCategory::Spiritual,
        power: 50,
        accuracy: 95,
        zeal: 20,
        priority: 0,
        secondaries: &[Secondary::Volatile {
            volatile_: Volatile::Confusion,
            chance_pct: 25,
        }],
    },
    Rite {
        id: 32,
        name: "Scarlet Lash",
        rite_type: RiteType::Sin(Sin::Lust),
        category: RiteCategory::Martial,
        power: 70,
        accuracy: 90,
        zeal: 10,
        priority: 0,
        secondaries: &[Secondary::Ailment {
            ailment: Ailment::Brand,
            chance_pct: 20,
        }],
    },
    Rite {
        id: 33,
        name: "Mire of Torpor",
        rite_type: RiteType::Sin(Sin::Sloth),
        category: RiteCategory::Status,
        power: 0,
        accuracy: 100,
        zeal: 15,
        priority: 0,
        secondaries: &[Secondary::StageOnTarget {
            stat: BattleStat::Alacrity,
            delta: -1,
            chance_pct: 100,
        }],
    },
    Rite {
        id: 34,
        name: "Slothful Embrace",
        rite_type: RiteType::Sin(Sin::Sloth),
        category: RiteCategory::Martial,
        power: 55,
        accuracy: 95,
        zeal: 20,
        priority: 0,
        secondaries: &[Secondary::Ailment {
            ailment: Ailment::Slumber,
            chance_pct: 15,
        }],
    },
    Rite {
        id: 35,
        name: "Scourge of Wrath",
        rite_type: RiteType::Sin(Sin::Wrath),
        category: RiteCategory::Martial,
        power: 80,
        accuracy: 90,
        zeal: 10,
        priority: 0,
        secondaries: &[],
    },
    Rite {
        id: 36,
        name: "Fury Unbound",
        rite_type: RiteType::Sin(Sin::Wrath),
        category: RiteCategory::Martial,
        power: 50,
        accuracy: 100,
        zeal: 15,
        priority: 0,
        secondaries: &[Secondary::StageOnSelf {
            stat: BattleStat::Valour,
            delta: 1,
            chance_pct: 100,
        }],
    },
    Rite {
        id: 37,
        name: "Maw of Gluttony",
        rite_type: RiteType::Sin(Sin::Gluttony),
        category: RiteCategory::Martial,
        power: 60,
        accuracy: 100,
        zeal: 15,
        priority: 0,
        secondaries: &[Secondary::Drain { pct: 50 }],
    },
    Rite {
        id: 38,
        name: "Ravenous Gale",
        rite_type: RiteType::Sin(Sin::Gluttony),
        category: RiteCategory::Spiritual,
        power: 45,
        accuracy: 95,
        zeal: 20,
        priority: 0,
        secondaries: &[],
    },
    Rite {
        id: 39,
        name: "Maw of Avarice",
        rite_type: RiteType::Sin(Sin::Greed),
        category: RiteCategory::Martial,
        power: 75,
        accuracy: 90,
        zeal: 10,
        priority: 0,
        secondaries: &[],
    },
    Rite {
        id: 40,
        name: "Grasping Tide",
        rite_type: RiteType::Sin(Sin::Greed),
        category: RiteCategory::Spiritual,
        power: 40,
        accuracy: 100,
        zeal: 25,
        priority: 0,
        secondaries: &[Secondary::Volatile {
            volatile_: Volatile::SeedLeech,
            chance_pct: 100,
        }],
    },
    Rite {
        id: 41,
        name: "Blade of Vainglory",
        rite_type: RiteType::Sin(Sin::Pride),
        category: RiteCategory::Martial,
        power: 60,
        accuracy: 95,
        zeal: 15,
        priority: 0,
        secondaries: &[],
    },
    Rite {
        id: 42,
        name: "Tirade of Scorn",
        rite_type: RiteType::Sin(Sin::Pride),
        category: RiteCategory::Spiritual,
        power: 50,
        accuracy: 100,
        zeal: 20,
        priority: 0,
        secondaries: &[],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rites_sorted_unique_ids_1_to_42() {
        assert_eq!(RITES.len(), 42);
        for (i, r) in RITES.iter().enumerate() {
            assert_eq!(r.id, i as u16 + 1);
        }
    }

    #[test]
    fn names_nonempty_ascii() {
        for r in RITES {
            assert!(!r.name.is_empty());
            assert!(r.name.is_ascii(), "{}", r.name);
        }
    }

    #[test]
    fn field_bounds() {
        for r in RITES {
            if r.category == RiteCategory::Status {
                assert_eq!(r.power, 0, "status rite {} has power", r.name);
            } else {
                assert!((20..=120).contains(&r.power), "power {}", r.name);
            }
            assert!((70..=101).contains(&r.accuracy), "accuracy {}", r.name);
            assert!((5..=40).contains(&r.zeal), "zeal {}", r.name);
            assert!((-2..=2).contains(&r.priority), "priority {}", r.name);
            for s in r.secondaries {
                match s {
                    Secondary::Ailment { chance_pct, .. }
                    | Secondary::Volatile { chance_pct, .. } => {
                        assert!(*chance_pct <= 100, "chance {}", r.name)
                    }
                    Secondary::StageOnSelf {
                        delta, chance_pct, ..
                    }
                    | Secondary::StageOnTarget {
                        delta, chance_pct, ..
                    } => {
                        assert!(*chance_pct <= 100, "chance {}", r.name);
                        assert!((-6..=6).contains(delta), "stage {}", r.name);
                    }
                    Secondary::HealSelf { pct }
                    | Secondary::Drain { pct }
                    | Secondary::Recoil { pct } => {
                        assert!(*pct <= 100, "pct {}", r.name)
                    }
                    Secondary::MultiHit | Secondary::HighCrit => {}
                }
            }
        }
    }

    #[test]
    fn required_special_rites_exist() {
        // One pure-heal status rite: Typeless, never misses, heals half.
        assert!(RITES.iter().any(|r| r.rite_type == RiteType::Typeless
            && r.category == RiteCategory::Status
            && r.accuracy == 101
            && r.secondaries
                .iter()
                .any(|s| matches!(s, Secondary::HealSelf { pct: 50 }))));
        // One HighCrit martial rite.
        assert!(RITES.iter().any(|r| r.category == RiteCategory::Martial
            && r.power > 0
            && r.secondaries
                .iter()
                .any(|s| matches!(s, Secondary::HighCrit))));
        // One power-120 recoil rite at 33%.
        assert!(RITES.iter().any(|r| r.power == 120
            && r.secondaries
                .iter()
                .any(|s| matches!(s, Secondary::Recoil { pct: 33 }))));
        // One priority +1 weak finisher.
        assert!(RITES
            .iter()
            .any(|r| r.priority == 1 && r.power > 0 && r.power <= 40));
    }

    #[test]
    fn every_ailment_kind_is_inflicted_by_some_rite() {
        let kinds = [
            Ailment::Brand,
            Ailment::Shackled,
            Ailment::Venom,
            Ailment::Slumber,
            Ailment::Frost,
        ];
        for a in kinds {
            assert!(
                RITES.iter().any(|r| r
                    .secondaries
                    .iter()
                    .any(|s| matches!(s, Secondary::Ailment { ailment, .. } if *ailment == a))),
                "no rite inflicts {a:?}"
            );
        }
    }

    #[test]
    fn every_battle_stat_has_self_buff_and_target_debuff() {
        let stats = [
            BattleStat::Valour,
            BattleStat::Piety,
            BattleStat::Guard,
            BattleStat::Aegis,
            BattleStat::Alacrity,
        ];
        for st in stats {
            assert!(
                RITES.iter().any(|r| r.secondaries.iter().any(
                    |s| matches!(s, Secondary::StageOnSelf { stat, delta, .. } if *stat == st && *delta > 0)
                )),
                "no self-buff for {st:?}"
            );
            assert!(
                RITES.iter().any(|r| r.secondaries.iter().any(
                    |s| matches!(s, Secondary::StageOnTarget { stat, delta, .. } if *stat == st && *delta < 0)
                )),
                "no target-debuff for {st:?}"
            );
        }
    }

    #[test]
    fn type_balance() {
        for v in Virtue::ALL {
            let n = RITES
                .iter()
                .filter(|r| r.rite_type == RiteType::Virtue(v))
                .count();
            assert!(n >= 3, "only {n} rites for {v:?}");
        }
        let typeless = RITES
            .iter()
            .filter(|r| r.rite_type == RiteType::Typeless)
            .count();
        assert_eq!(typeless, 7);
        let sin_typed = RITES
            .iter()
            .filter(|r| matches!(r.rite_type, RiteType::Sin(_)))
            .count();
        assert!(sin_typed >= 6, "need >= 6 Sin-typed rites, got {sin_typed}");
    }
}

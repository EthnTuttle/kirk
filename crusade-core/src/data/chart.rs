//! Effectiveness wheel. Worker A implements the rules documented on
//! `data::effectiveness` (mod.rs). Keep the wheel as data, not match arms.

use crate::types::*;

/// The virtue wheel W = [Humility, Charity, Chastity, Diligence, Patience,
/// Temperance, Kindness]. Each virtue strikes the next wheel entry for 200%
/// and is resisted (50%) by itself and its wheel predecessor.
const WHEEL: [Virtue; 7] = Virtue::ALL;

/// Wheel index of a virtue. `Virtue::ALL` is total over the enum, so the
/// fallback is unreachable.
fn wheel_index(v: Virtue) -> usize {
    WHEEL.iter().position(|&w| w == v).unwrap_or(0)
}

/// Percent for an attacking virtue vs a single defender virtue:
/// self -> 50, wheel successor -> 200, wheel predecessor -> 50, else 100.
fn virtue_vs_virtue(att: Virtue, def: Virtue) -> u32 {
    let i = wheel_index(att);
    if def == att {
        50
    } else if def == WHEEL[(i + 1) % 7] {
        200
    } else if def == WHEEL[(i + 6) % 7] {
        50
    } else {
        100
    }
}

/// Effectiveness percent of a rite type against a creature typing.
///
/// Dual-virtue defenders multiply the two percents and divide by 100, with a
/// missing second virtue counting as 100. The product is computed in u32 and
/// saturated to u8::MAX: the 400% case is only reachable if a dual typing
/// lists the same virtue twice, which SPECIES never does.
pub fn effectiveness(att: RiteType, def: Typing) -> u8 {
    match att {
        RiteType::Typeless => 100,
        RiteType::Virtue(v) => match def {
            Typing::Sin(s) => {
                if v.opposed_sin() == s {
                    200
                } else {
                    100
                }
            }
            Typing::Virtues([first, second]) => {
                let p1 = first.map(|d| virtue_vs_virtue(v, d)).unwrap_or(100);
                let p2 = second.map(|d| virtue_vs_virtue(v, d)).unwrap_or(100);
                (p1 * p2 / 100).min(u8::MAX as u32) as u8
            }
        },
        RiteType::Sin(s) => match def {
            Typing::Sin(d) => {
                if s == d {
                    50
                } else {
                    100
                }
            }
            Typing::Virtues(virtues) => {
                if virtues.iter().flatten().any(|v| v.opposed_sin() == s) {
                    200
                } else {
                    100
                }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SINS: [Sin; 7] = [
        Sin::Pride,
        Sin::Envy,
        Sin::Lust,
        Sin::Sloth,
        Sin::Wrath,
        Sin::Gluttony,
        Sin::Greed,
    ];

    fn single(d: Virtue) -> Typing {
        Typing::Virtues([Some(d), None])
    }

    #[test]
    fn virtue_hits_opposed_sin_200_and_other_sins_100() {
        for v in Virtue::ALL {
            for s in SINS {
                let want = if v.opposed_sin() == s { 200 } else { 100 };
                assert_eq!(
                    effectiveness(RiteType::Virtue(v), Typing::Sin(s)),
                    want,
                    "{v:?} vs {s:?}"
                );
            }
        }
    }

    #[test]
    fn each_virtue_has_one_200_and_two_50_among_virtue_defenders() {
        for v in Virtue::ALL {
            let (mut n200, mut n50, mut n100) = (0, 0, 0);
            for d in Virtue::ALL {
                match effectiveness(RiteType::Virtue(v), single(d)) {
                    200 => n200 += 1,
                    50 => n50 += 1,
                    100 => n100 += 1,
                    other => panic!("unexpected pct {other} for {v:?} vs {d:?}"),
                }
            }
            assert_eq!((n200, n50, n100), (1, 2, 4), "attacker {v:?}");
            // The resistances are self and the wheel predecessor.
            let i = wheel_index(v);
            assert_eq!(effectiveness(RiteType::Virtue(v), single(v)), 50);
            assert_eq!(
                effectiveness(RiteType::Virtue(v), single(WHEEL[(i + 6) % 7])),
                50
            );
            assert_eq!(
                effectiveness(RiteType::Virtue(v), single(WHEEL[(i + 1) % 7])),
                200
            );
        }
    }

    #[test]
    fn typeless_always_100() {
        for d in Virtue::ALL {
            assert_eq!(effectiveness(RiteType::Typeless, single(d)), 100);
        }
        for s in SINS {
            assert_eq!(effectiveness(RiteType::Typeless, Typing::Sin(s)), 100);
        }
        assert_eq!(
            effectiveness(
                RiteType::Typeless,
                Typing::Virtues([Some(Virtue::Humility), Some(Virtue::Charity)])
            ),
            100
        );
    }

    #[test]
    fn sin_vs_same_sin_50_else_100() {
        for s in SINS {
            for d in SINS {
                let want = if s == d { 50 } else { 100 };
                assert_eq!(effectiveness(RiteType::Sin(s), Typing::Sin(d)), want);
            }
        }
    }

    #[test]
    fn sin_hits_opposing_virtue_200() {
        for v in Virtue::ALL {
            let s = v.opposed_sin();
            assert_eq!(effectiveness(RiteType::Sin(s), single(v)), 200);
            for d in Virtue::ALL {
                if d != v {
                    assert_eq!(effectiveness(RiteType::Sin(s), single(d)), 100);
                }
            }
        }
    }

    #[test]
    fn dual_defender_multiplies_and_missing_second_counts_as_100() {
        // Humility (wheel 0): successor Charity -> 200, self -> 50.
        let dual = Typing::Virtues([Some(Virtue::Charity), Some(Virtue::Humility)]);
        assert_eq!(effectiveness(RiteType::Virtue(Virtue::Humility), dual), 100);
        // Double resist: self (50) and predecessor Kindness (50) -> 25.
        let resisting = Typing::Virtues([Some(Virtue::Humility), Some(Virtue::Kindness)]);
        assert_eq!(
            effectiveness(RiteType::Virtue(Virtue::Humility), resisting),
            25
        );
        // Single virtue through the dual constructor equals the plain single.
        for v in Virtue::ALL {
            for d in Virtue::ALL {
                assert_eq!(
                    effectiveness(RiteType::Virtue(v), Typing::Virtues([Some(d), None])),
                    effectiveness(RiteType::Virtue(v), single(d))
                );
            }
        }
    }
}

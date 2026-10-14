//! Callings table. Worker A fills all 25 (5 stats choose +,- incl. 5 neutral).
//!
//! Ids 0..=4 are contract-fixed; ids 5..=20 cover the remaining 16
//! (+inc, -dec) ordered pairs with inc != dec; ids 21..=24 are extra neutral
//! callings with distinct reverent names.

use crate::types::{BattleStat, Calling};

pub const NEUTRAL: Calling = Calling {
    id: 0,
    name: "Steadfast",
    inc: None,
    dec: None,
};

pub static CALLINGS: &[Calling] = &[
    NEUTRAL,
    Calling {
        id: 1,
        name: "Fervent",
        inc: Some(BattleStat::Piety),
        dec: Some(BattleStat::Alacrity),
    },
    Calling {
        id: 2,
        name: "Stalwart",
        inc: Some(BattleStat::Guard),
        dec: Some(BattleStat::Alacrity),
    },
    Calling {
        id: 3,
        name: "Swift",
        inc: Some(BattleStat::Alacrity),
        dec: Some(BattleStat::Aegis),
    },
    Calling {
        id: 4,
        name: "Devout",
        inc: Some(BattleStat::Piety),
        dec: Some(BattleStat::Valour),
    },
    Calling {
        id: 5,
        name: "Valiant",
        inc: Some(BattleStat::Valour),
        dec: Some(BattleStat::Piety),
    },
    Calling {
        id: 6,
        name: "Lionheart",
        inc: Some(BattleStat::Valour),
        dec: Some(BattleStat::Guard),
    },
    Calling {
        id: 7,
        name: "Fearless",
        inc: Some(BattleStat::Valour),
        dec: Some(BattleStat::Aegis),
    },
    Calling {
        id: 8,
        name: "Resolute",
        inc: Some(BattleStat::Valour),
        dec: Some(BattleStat::Alacrity),
    },
    Calling {
        id: 9,
        name: "Pious",
        inc: Some(BattleStat::Piety),
        dec: Some(BattleStat::Guard),
    },
    Calling {
        id: 10,
        name: "Zealous",
        inc: Some(BattleStat::Piety),
        dec: Some(BattleStat::Aegis),
    },
    Calling {
        id: 11,
        name: "Prudent",
        inc: Some(BattleStat::Guard),
        dec: Some(BattleStat::Valour),
    },
    Calling {
        id: 12,
        name: "Watchful",
        inc: Some(BattleStat::Guard),
        dec: Some(BattleStat::Piety),
    },
    Calling {
        id: 13,
        name: "Stoic",
        inc: Some(BattleStat::Guard),
        dec: Some(BattleStat::Aegis),
    },
    Calling {
        id: 14,
        name: "Serene",
        inc: Some(BattleStat::Aegis),
        dec: Some(BattleStat::Valour),
    },
    Calling {
        id: 15,
        name: "Faithful",
        inc: Some(BattleStat::Aegis),
        dec: Some(BattleStat::Piety),
    },
    Calling {
        id: 16,
        name: "Reverent",
        inc: Some(BattleStat::Aegis),
        dec: Some(BattleStat::Guard),
    },
    Calling {
        id: 17,
        name: "Solemn",
        inc: Some(BattleStat::Aegis),
        dec: Some(BattleStat::Alacrity),
    },
    Calling {
        id: 18,
        name: "Keen",
        inc: Some(BattleStat::Alacrity),
        dec: Some(BattleStat::Valour),
    },
    Calling {
        id: 19,
        name: "Brisk",
        inc: Some(BattleStat::Alacrity),
        dec: Some(BattleStat::Piety),
    },
    Calling {
        id: 20,
        name: "Eager",
        inc: Some(BattleStat::Alacrity),
        dec: Some(BattleStat::Guard),
    },
    Calling {
        id: 21,
        name: "Meek",
        inc: None,
        dec: None,
    },
    Calling {
        id: 22,
        name: "Sober",
        inc: None,
        dec: None,
    },
    Calling {
        id: 23,
        name: "Gentle",
        inc: None,
        dec: None,
    },
    Calling {
        id: 24,
        name: "Grave",
        inc: None,
        dec: None,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exactly_25_callings_with_ids_0_to_24_sorted_unique() {
        assert_eq!(CALLINGS.len(), 25);
        for (i, c) in CALLINGS.iter().enumerate() {
            assert_eq!(c.id, i as u8);
        }
    }

    #[test]
    fn fixed_ids_0_to_4_match_contract() {
        assert_eq!(CALLINGS[0], NEUTRAL);
        let want = [
            ("Steadfast", None, None),
            (
                "Fervent",
                Some(BattleStat::Piety),
                Some(BattleStat::Alacrity),
            ),
            (
                "Stalwart",
                Some(BattleStat::Guard),
                Some(BattleStat::Alacrity),
            ),
            ("Swift", Some(BattleStat::Alacrity), Some(BattleStat::Aegis)),
            ("Devout", Some(BattleStat::Piety), Some(BattleStat::Valour)),
        ];
        for (i, &(name, inc, dec)) in want.iter().enumerate() {
            let c = &CALLINGS[i];
            assert_eq!((c.name, c.inc, c.dec), (name, inc, dec), "calling {i}");
        }
    }

    #[test]
    fn names_nonempty_ascii_and_distinct() {
        let mut names: Vec<&str> = CALLINGS.iter().map(|c| c.name).collect();
        for n in &names {
            assert!(!n.is_empty());
            assert!(n.is_ascii());
        }
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), CALLINGS.len());
    }

    #[test]
    fn twenty_ordered_pairs_all_distinct_and_inc_ne_dec() {
        let mut pairs: Vec<(Option<BattleStat>, Option<BattleStat>)> = Vec::new();
        for c in CALLINGS
            .iter()
            .filter(|c| c.inc.is_some() || c.dec.is_some())
        {
            assert!(
                c.inc.is_some() && c.dec.is_some(),
                "partial calling {}",
                c.name
            );
            assert_ne!(c.inc, c.dec, "{} has inc == dec", c.name);
            let pair = (c.inc, c.dec);
            assert!(!pairs.contains(&pair), "duplicate pair for {}", c.name);
            pairs.push(pair);
        }
        assert_eq!(pairs.len(), 20);
    }

    #[test]
    fn exactly_five_neutral_callings() {
        assert_eq!(
            CALLINGS
                .iter()
                .filter(|c| c.inc.is_none() && c.dec.is_none())
                .count(),
            5
        );
    }
}

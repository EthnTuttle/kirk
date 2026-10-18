//! Baptismal names (docs/CONVERSION.md section 4, card G6). When a
//! defeated adversary answers the Call, he takes a Christian name at the
//! font: a table of attested period names, Latin/Greek/scriptural in
//! chronicle form, per convertible archetype (ids 108..=117 only — the
//! named commanders 120..=125 never convert, so they carry no entry). On
//! the successful Call the client picks one candidate deterministically,
//! keyed off the Call's sealed salt (CONVERSION 3's no-new-rng rule); the
//! man is renamable thereafter through the roster setting. Each candidate
//! carries a one-line namesake note in the chronicle register
//! (docs/HISTORICAL-FOES.md section 6): the saint or Scripture-name the
//! Order connects him with, never a mockery of his own people. Sorted by
//! archetype id.

/// One baptismal-name candidate. Pure table text: the name itself rides
/// into roster docs as a plain string, so no serde derives here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BaptismalName {
    /// The Christian name taken at the font (chronicle/Latin form).
    pub name: &'static str,
    /// One line, chronicle voice: the saint or Scripture-name the Order
    /// connects the man with at his baptism.
    pub namesake: &'static str,
}

/// One archetype's baptismal-name candidates (two or three per office).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BaptismalArchetype {
    pub id: u16,
    pub candidates: &'static [BaptismalName],
}

pub static BAPTISMAL_NAMES: &[BaptismalArchetype] = &[
    // 108 Turcoman Bowrider — the nomad of the Anatolian plain; Greek
    // martyrs of the marches, and the Apostle whose doubt became faith.
    BaptismalArchetype {
        id: 108,
        candidates: &[
            BaptismalName {
                name: "George",
                namesake: "George of Lydda, the martyr rider of Cappadocia.",
            },
            BaptismalName {
                name: "Thomas",
                namesake: "Thomas the Apostle, who doubted and believed.",
            },
            BaptismalName {
                name: "Nicetas",
                namesake: "Nicetas the Goth, a martyr among the horsemen.",
            },
        ],
    },
    // 109 Ghulam Lancer — a man bought young into the lance; Scripture's
    // own slave made brother, and soldiers who chose the martyr's crown.
    BaptismalArchetype {
        id: 109,
        candidates: &[
            BaptismalName {
                name: "John",
                namesake: "John the Forerunner, who cried repentance beside the river.",
            },
            BaptismalName {
                name: "Maurice",
                namesake: "Maurice of the Theban Legion, the soldier who chose the martyr's crown.",
            },
            BaptismalName {
                name: "Onesimus",
                namesake: "Onesimus of Philemon's letter, the runaway slave made brother.",
            },
        ],
    },
    // 110 Sudani Shieldbearer — the Nubian shield line; the desert father
    // of the strong arm, and Philae's soldier-martyr of his own marches.
    BaptismalArchetype {
        id: 110,
        candidates: &[
            BaptismalName {
                name: "Moses",
                namesake: "Moses the Black, the desert father whose strong arm learned mercy.",
            },
            BaptismalName {
                name: "Mercurius",
                namesake: "Mercurius of Philae, the soldier-martyr of the Nubian marches.",
            },
            BaptismalName {
                name: "Andrew",
                namesake: "Andrew the Apostle, who brought his own brother to the Lord.",
            },
        ],
    },
    // 111 Armenian Bowman — his people's saints already; his own kin were
    // in the hill churches (the testimony says so).
    BaptismalArchetype {
        id: 111,
        candidates: &[
            BaptismalName {
                name: "Gregory",
                namesake: "Gregory the Illuminator, who carried Christ to Armenia.",
            },
            BaptismalName {
                name: "James",
                namesake: "James of Nisibis, bishop of the hill churches.",
            },
        ],
    },
    // 112 Daylamite Axeman — strength and loudness turned steadfast; the
    // Apostle martyred in the highlands he came down from.
    BaptismalArchetype {
        id: 112,
        candidates: &[
            BaptismalName {
                name: "Bartholomew",
                namesake: "Bartholomew the Apostle, martyred in the Armenian highlands.",
            },
            BaptismalName {
                name: "Peter",
                namesake: "Peter the Rock, a loud man made steadfast.",
            },
        ],
    },
    // 113 Bedouin Outrider — the desert rider; the baptism on the Gaza
    // road, and the apostle of the Syrian East.
    BaptismalArchetype {
        id: 113,
        candidates: &[
            BaptismalName {
                name: "Philip",
                namesake: "Philip the Deacon, who baptized on the desert road to Gaza.",
            },
            BaptismalName {
                name: "Thaddeus",
                namesake: "Thaddeus of Edessa, apostle of the Syrians and the East.",
            },
        ],
    },
    // 114 Kurdish Ghazw Rider — honour and the feud; zeal that finds a
    // better Kingdom.
    BaptismalArchetype {
        id: 114,
        candidates: &[
            BaptismalName {
                name: "David",
                namesake: "David the psalmist-king, whom the Lord turned from the feud.",
            },
            BaptismalName {
                name: "Simon",
                namesake: "Simon the Zealot, whose zeal found a better Kingdom.",
            },
        ],
    },
    // 115 Naffatun Sapper — the scorned fire-worker; the martyr-bishop,
    // and the man who laid hands on Saul and called him brother.
    BaptismalArchetype {
        id: 115,
        candidates: &[
            BaptismalName {
                name: "Ignatius",
                namesake: "Ignatius of Antioch, the martyr-bishop.",
            },
            BaptismalName {
                name: "Ananias",
                namesake: "Ananias of Damascus, who laid hands on Saul and called him brother.",
            },
        ],
    },
    // 116 Askari Alamdar — the banner man; the cross raised over a
    // standard, and the encourager men rally to.
    BaptismalArchetype {
        id: 116,
        candidates: &[
            BaptismalName {
                name: "Constantine",
                namesake: "Constantine the basileus, who raised the Labarum at the Milvian bridge.",
            },
            BaptismalName {
                name: "Barnabas",
                namesake: "Barnabas the son of consolation, who vouched for Saul.",
            },
        ],
    },
    // 117 Muqaddam of the Tulb — the zealot elite; CONVERSION 1's own
    // archetype of the converted persecutor.
    BaptismalArchetype {
        id: 117,
        candidates: &[
            BaptismalName {
                name: "Paul",
                namesake: "Paul of Tarsus; as Saul the persecutor was made an apostle.",
            },
            BaptismalName {
                name: "Nicodemus",
                namesake: "Nicodemus the ruler, who came by night and stood at last for the Lord.",
            },
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{baptismal_name_for, baptismal_names};

    /// Card G6: the table covers every convertible archetype with two or
    /// three candidates each; namesakes are one terse line; all ASCII
    /// (witnesses.rs convention); ids sorted and unique.
    #[test]
    fn table_shape_sorted_two_or_three_candidates_ascii() {
        assert_eq!(BAPTISMAL_NAMES.len(), 10, "archetypes 108..=117 only");
        for (i, pair) in BAPTISMAL_NAMES.windows(2).enumerate() {
            assert!(pair[0].id < pair[1].id, "archetype ids out of order at {}", i);
        }
        for a in BAPTISMAL_NAMES {
            assert!((2..=3).contains(&a.candidates.len()), "archetype {} candidate count", a.id);
            for c in a.candidates {
                assert!(!c.name.is_empty(), "empty name on {}", a.id);
                assert!(c.name.is_ascii() && c.namesake.is_ascii(), "non-ascii on {}", a.id);
                let beats = c.namesake.matches(['.', ';']).count();
                assert!(beats <= 2, "namesake on {} must be one terse line", a.id);
            }
        }
        assert!(baptismal_names(108).expect("108 covered")[0].name == "George");
    }

    /// The fallback: lookup and selection return None outside the
    /// convertible lane — commanders 120..=125, champions, sin-spawn —
    /// so callers keep their tested expect at the baptism seam.
    #[test]
    fn commanders_and_outsiders_have_no_names() {
        for id in [1u16, 12, 101, 107, 118, 119, 120, 121, 122, 123, 124, 125, 126, u16::MAX] {
            assert!(baptismal_names(id).is_none(), "id {id} carries no candidates");
            assert!(baptismal_name_for(id, 0).is_none(), "id {id} selects nothing");
        }
    }

    /// Deterministic selection keyed off the sealed salt: same (id, salt)
    /// always yields the same name; the pick always lands inside the
    /// declared candidates for that archetype.
    #[test]
    fn selection_is_deterministic_and_in_table() {
        for a in BAPTISMAL_NAMES {
            for salt in 0..64u64 {
                let pick = baptismal_name_for(a.id, salt).expect("covered archetype");
                assert_eq!(pick, baptismal_name_for(a.id, salt).unwrap());
                assert!(a.candidates.contains(&pick), "pick leaves the table on {}", a.id);
            }
        }
    }
}

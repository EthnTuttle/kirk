//! Witness roll (lore-only). Revered figures of the faith whom the Order
//! remembers in the Hagiography alongside the species' `lore` lines. A
//! witness is never a combatant: no typing, no stats, no learnset, no battle
//! or roster presence. Adding entries here must not disturb the locked
//! roster counts (12 champions, 42 rites, 7 sin-spawn) asserted by the
//! `species.rs` and `rites.rs` table tests. Sorted by id.

/// One entry of the witness roll. Pure lore: never serialized (battle
/// transcripts and roster docs carry ids only), so no serde derives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Witness {
    pub id: u16,
    pub name: &'static str,
    /// The style by which pilgrims remember the witness.
    pub epithet: &'static str,
    /// Short memorial reading for the Hagiography's witness roll.
    pub vignette: &'static str,
}

pub static WITNESSES: &[Witness] = &[
    Witness {
        id: 1,
        name: "Kirk",
        epithet: "the Witness of the Open Square",
        vignette: "He took his name from the little stone kirk where he first heard the Word, and \
though he swore no vows and answered to no chapter house, the open square was his parish. Each \
market day he set out a plain bench, sat down in the dust, and bade any passer-by, friend or \
scoffer, sit across from him and test his arguments. He met derision with patience and hard \
questions with Scripture, and every disputation ended where it began: be courageous in speech, \
devout in faith, and true to your hearth, a good husband and a steadfast father. Threats never \
kept him from the bench, and when he was struck down mid-answer for his witness, his widow knelt \
and forgave before the whole city. The bench still stands where he left it, and pilgrims keep it \
warm, that no scoffer may ever find it empty.",
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{all_witnesses, witness_by_id};

    #[test]
    fn witnesses_sorted_unique_ids_and_fields_nonempty_ascii() {
        assert!(!WITNESSES.is_empty(), "witness roll must not be empty");
        for (i, w) in WITNESSES.windows(2).enumerate() {
            assert!(w[0].id < w[1].id, "witness ids out of order at {}", i);
        }
        for w in WITNESSES {
            for s in [w.name, w.epithet, w.vignette] {
                assert!(!s.is_empty(), "witness {} has an empty field", w.id);
                assert!(s.is_ascii(), "witness {} has non-ascii text", w.id);
            }
        }
    }

    /// Pins the homage: Kirk, lay preacher of the open square, whose Scots
    /// namesake ("kirk", the church) anchors him in-world; the bench is his
    /// debating table, the widow's pardon his testament.
    #[test]
    fn kirk_is_first_witness_and_keeps_the_anchor_images() {
        assert_eq!(all_witnesses(), WITNESSES);
        let kirk = witness_by_id(1).expect("witness id 1 exists");
        assert_eq!(kirk.name, "Kirk");
        assert!(!kirk.epithet.is_empty());
        for image in ["kirk", "bench", "widow", "forgav"] {
            assert!(
                kirk.vignette.contains(image),
                "vignette lost its '{image}' image"
            );
        }
    }
}

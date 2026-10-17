//! Conversion (M7b): the Call to Repentance's client-side data and pure
//! roll — docs/CONVERSION.md sections 3-5, docs/HISTORICAL-FOES.md
//! section 6.5 (the codex-of-record bitmasks). No I/O beyond the caller's
//! store handles; wasm-safe.
//!
//! What's here:
//! - the shard item ids and bounties (CONVERSION 4/5: Firstfruits is the
//!   bigger grant; Mercy is smaller but nonzero),
//! - the `campaign.foes_defeated` / `campaign.converts_baptized` u32
//!   settings keys and their bit packing,
//! - the baptismal-name table (per CONVERSION 4, a 12th-c. Christian name
//!   per convertible archetype),
//! - the Testimonies of the Baptized texts (first-person, chronicle
//!   register per HF section 6),
//! - the sealed post-battle draw (`conversion_roll` + `decide_call`) —
//!   a deterministic 1..=100 taken against the chance
//!   `crusade_core::data::conversion_chance` returns.
//!
//! Register gate (HF 6.1-6.3, CONVERSION 8): no slurs, no mock-Arabic,
//! "Saracen" only inside attributed quotation (never used here), the
//! fallen yield/stand/hold, and conversion is never coerced. The tests at
//! the bottom of this file pin the banned terms against every table.

use std::ops::RangeInclusive;

/// Roster-store settings key: u32 bitmask of historical adversaries whose
/// yield the company has witnessed (HF 6.5). Bit `id - 108` for
/// archetypes 108..=117; bit `id - 120 + 12` for commanders 120..=123.
/// Same persisted-settings discipline as `client.rs::SINS_DEFEATED_KEY`.
pub const FOES_DEFEATED_KEY: &str = "campaign.foes_defeated";

/// Roster-store settings key: u32 bitmask of archetypes baptized into the
/// roster (HF 6.5's Testimonies unlock). Bits `id - 108` only —
/// commanders are never called, so bits 12..=15 stay clear forever.
pub const CONVERTS_BAPTIZED_KEY: &str = "campaign.converts_baptized";

/// Roster-store settings key: running count of Calls issued (CONVERSION
/// 5's hagiography statistic). Also the draw salt's moving part.
pub const CALLS_ISSUED_KEY: &str = "campaign.calls_issued";

/// Settings-key PREFIX for a convert's baptismal name: the full key is
/// `roster.baptismal.<roster id>` (the champion's store uuid). Written at
/// baptism; the future rename path rides this same key — absent reads as
/// "no name of his own yet".
pub const BAPTISMAL_KEY_PREFIX: &str = "roster.baptismal.";

/// Inventory item paid for a baptism (CONVERSION 4). Bigger than the
/// Contrition bounty band: the economy pays for mercy, not slaughter.
pub const FIRSTFRUITS_SHARD_ITEM: &str = "firstfruits-shard";
/// Inventory item paid for a release or a refusal (CONVERSION 5): nonzero
/// — releasing is not failure.
pub const MERCY_SHARD_ITEM: &str = "mercy-shard";

/// Per-call shard bounties (CONVERSION 4/5: Firstfruits > Mercy, both
/// fixed per call — the price of a soul is not a sliding scale).
pub const FIRSTFRUITS_BOUNTY: i64 = 8;
pub const MERCY_BOUNTY: i64 = 3;

/// Archetype ids that may answer the Call (ADVERSARIES, convertible lane).
pub const CONVERTIBLE_IDS: RangeInclusive<u16> = 108..=117;
/// Named commanders: recorded on the Roll, never called (CONVERSION 2).
pub const COMMANDER_IDS: RangeInclusive<u16> = 120..=123;

/// The `campaign.foes_defeated` bit for a species id, if it earns one:
/// archetypes 108..=117 -> bits 0..=9, commanders 120..=123 -> bits
/// 12..=15 (HF 6.5; 14 foe ids exceed a u8, hence u32). Every other id
/// (champions, sin-spawn) earns no bit.
pub fn foe_bit(species_id: u16) -> Option<u8> {
    if CONVERTIBLE_IDS.contains(&species_id) {
        Some((species_id - 108) as u8)
    } else if COMMANDER_IDS.contains(&species_id) {
        Some((species_id - 120 + 12) as u8)
    } else {
        None
    }
}

/// The Call succeeds iff the sealed draw clears the chance (percent).
/// `roll` comes from [`conversion_roll`]; pure decision so tests can pin
/// the extremes directly (CONVERSION 3's floor/ceiling: chance in 3..=85,
/// roll in 1..=100).
pub fn decide_call(chance_pct: u8, roll: u8) -> bool {
    roll <= chance_pct
}

/// The sealed post-battle draw: a deterministic 1..=100 from the caller's
/// salt (defeat-and-call history), splitmix64-mixed. This is the client's
/// deterministic-RNG discipline for the one extra sub-seed draw after
/// battle (CONVERSION 3): same input, same answer, on every replica.
pub fn conversion_roll(salt: u64) -> u8 {
    let mut x = salt.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x % 100) as u8 + 1
}

/// A 12th-c. Christian name for a baptized archetype (CONVERSION 4: "a
/// Christian name form of his office"), one per convertible id. Apostolic
/// and commonly-attested Frankish baptismal names; the man's own people
/// are never mocked by it. Renaming (when a rename path lands) edits the
/// `roster.baptismal.<id>` setting this seed value is written to.
pub fn baptismal_name(archetype_id: u16) -> Option<&'static str> {
    Some(match archetype_id {
        108 => "Peter",
        109 => "John",
        110 => "Andrew",
        111 => "Philip",
        112 => "Thomas",
        113 => "Stephen",
        114 => "James",
        115 => "Barnabas",
        116 => "Mark",
        117 => "Paul", // as Saul the persecutor became Paul (CONVERSION 1)
        _ => return None,
    })
}

/// The Testimonies of the Baptized (CONVERSION 4's hagiography unlock):
/// each archetype's own account of hearing Christ and turning, told plain
/// — grief-sober, no melodrama, no coercion (HF 6.1-6.3; the register
/// gate test below pins the banned terms). Unsealed per archetype by the
/// `campaign.converts_baptized` bitmask; texts ride verbatim like the
/// species lore they sit beside.
pub fn testimony(archetype_id: u16) -> Option<&'static str> {
    Some(match archetype_id {
        108 => "I loosed my first bow before I could write my name, and the plain fed us. \
                The Franks stood over me and spoke of a Christ who died praying for the men \
                who killed him, and I had never heard of such a God. I asked for the water \
                myself; I remember my fathers kindly, and I pray for them.",
        109 => "I was bought young and taught the couch and the charge, and I was proud of \
                never having known a home. When he who bested me spoke of the Son of God, \
                it was as one soldier honouring another. I could not answer him, so I \
                listened; and listening, I believed.",
        110 => "I held the shield line because men behind me trusted my arm, and I counted \
                that the whole of righteousness. They read to me where Thomas doubted and \
                the Lord did not cast him off. I yielded to Him as I never yielded in the \
                line, and I do not count it shame.",
        111 => "We shot in turns as we were drilled, and thought of our pay and the march \
                home. A priest among them wept over the men his own side had slain, and \
                that Water-priest's grief taught me more than his argument. I was baptized \
                at springtime; my mother's kin were Christians already and had never dared to tell me.",
        112 => "My strength was my two axes and my loudness, and men feared both. The \
                chaplain said: the Lord was bound and opened not His mouth. I had stood over \
                beaten men enough to know what silence costs. Now I split wood for the \
                Order's fires, and I am quieter.",
        113 => "The desert was my wall and my table, and I served whoever bought water and \
                dates. They showed me where the Lord went into the wilderness and was \
                tempted as I am tempted. I had met nothing there but wind before. Now I \
                meet Him there also when I ride.",
        114 => "I came down from the hills for honour and for the feud, as my brothers did. \
                He who bested me cursed me not, though I had killed his friend that day, and \
                spoke of a Kingdom no hill bounds. I carried that word home in my chest like \
                a coal. God grant my brothers hear it too.",
        115 => "Fire was my trade; I carried it in clay and was scorned as unclean even by \
                our own. They told me of tongues of flame resting on the apostles, and that \
                my craft's fire is a shadow of His Spirit's. I wept. I was baptized the same \
                night, and the burn-scars did not shame me after.",
        116 => "I carried the banner because I was steady, and men rallied to me and died \
                around me. I asked the chaplain what his Christ offers a man with that many \
                dead; he said: resurrection, and truth confessed. I confessed. The banner I \
                carry now is a cross atop a pilgrimage staff.",
        117 => "I wheeled a thousand men and thought wisdom was mine. In chains I heard the \
                Gospel where Christ bids His followers love their enemies, and I saw that my \
                victory-lists were ash. Saul the persecutor found the Lord on a road; so did \
                I. I follow, less wise and more glad.",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crusade_core::data;

    /// HF 6.5 bit packing, pinned.
    #[test]
    fn foe_bit_packs_archetypes_then_commanders() {
        assert_eq!(foe_bit(108), Some(0));
        assert_eq!(foe_bit(117), Some(9));
        assert_eq!(foe_bit(120), Some(12));
        assert_eq!(foe_bit(123), Some(15));
        for id in [1u16, 12, 101, 107, 118, 119, 124] {
            assert_eq!(foe_bit(id), None, "id {id} earns no codex bit");
        }
    }

    /// CONVERSION 3: the draw is deterministic (same salt, same roll, on
    /// every replica) and lands in 1..=100.
    #[test]
    fn conversion_roll_is_deterministic_and_in_range() {
        for salt in 0..256u64 {
            let roll = conversion_roll(salt);
            assert!((1..=100).contains(&roll), "salt {salt}");
            assert_eq!(roll, conversion_roll(salt));
        }
    }

    /// The pure decision at the clamp bounds (CONVERSION 3): a roll of 1
    /// clears even the 3% floor (Romans 10:13); a roll of 100 defeats even
    /// the 85% ceiling (no champion converts at will).
    #[test]
    fn decide_call_holds_the_floor_and_ceiling() {
        assert!(decide_call(3, 1), "the floor is live even against the hardest heart");
        assert!(!decide_call(85, 100), "the ceiling keeps zeal meaningful");
        assert!(decide_call(85, 85) && !decide_call(85, 86));
    }

    /// Every convertible archetype has exactly one baptismal name and one
    /// testimony; commanders and non-adversaries have neither.
    #[test]
    fn tables_cover_exactly_the_convertible_lane() {
        for id in 108u16..=117 {
            assert!(baptismal_name(id).is_some(), "name for {id}");
            let text = testimony(id).expect("testimony for {id}");
            assert!(text.is_ascii(), "testimony {id}");
            let sentences = text.matches('.').count();
            assert!((2..=4).contains(&sentences), "testimony {id}: 2-3 beats, not a homily");
        }
        for id in [1u16, 101, 118, 120, 123] {
            assert_eq!(baptismal_name(id), None);
            assert_eq!(testimony(id), None);
        }
    }

    /// The register gate (HF 6.2/6.3, CONVERSION 8): banned terms never
    /// appear in the new string tables, in any case. ("Saracen" is
    /// permitted only inside attributed quotation; the simple gate outlaws
    /// it outright, which is stricter and safe.)
    #[test]
    fn register_gate_banned_terms_absent() {
        const BANNED: [&str; 5] = ["saracen", "infidel", "heathen", "gibber", "forced convert"];
        let mut corpus: Vec<String> = Vec::new();
        for id in 108u16..=117 {
            corpus.push(baptismal_name(id).unwrap().to_string());
            corpus.push(testimony(id).unwrap().to_string());
        }
        for text in &corpus {
            let lower = text.to_lowercase();
            for term in BANNED {
                assert!(!lower.contains(term), "register violation: '{term}' in {text:?}");
            }
        }
        // And the adversary lore these pages unlock passes the same gate.
        for id in CONVERTIBLE_IDS.chain(COMMANDER_IDS) {
            let sp = data::species_by_id(id).expect("adversary in the static table");
            let lower = sp.lore.to_lowercase();
            for term in BANNED {
                assert!(!lower.contains(term), "register violation in lore of {id}");
            }
        }
    }
}

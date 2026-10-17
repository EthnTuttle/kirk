//! Static overworld data: the Pilgrim Road (docs/OVERWORLD.md).
//!
//! Two layers: a WORLD layer of discrete screens (Zelda-fixed, edge-gated)
//! and a JOURNEY layer of trail events (Oregon-Trail economics). This is the
//! M8a hand-built spine: ~3-5 screens per region so the engine and guard
//! tests have real data; densification to the 40x7 budget is data-only.
//!
//! Register: the chronicle voice. No mockery.

use serde::{Deserialize, Serialize};

// ---------- Regions ----------

/// One region of the pilgrimage (docs/OVERWORLD.md section 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegionDef {
    pub id: u8,
    pub slug: &'static str,
    pub name: &'static str,
}

/// The seven regions, in march order. Ids are 1..=7 and contiguous.
pub static REGIONS: &[RegionDef] = &[
    RegionDef { id: 1, slug: "marches", name: "The Marches" },
    RegionDef { id: 2, slug: "crossing", name: "The Crossing" },
    RegionDef { id: 3, slug: "anatolia", name: "Anatolia" },
    RegionDef { id: 4, slug: "coasts", name: "The Coasts" },
    RegionDef { id: 5, slug: "roads-of-egypt", name: "The Roads of Egypt" },
    RegionDef { id: 6, slug: "horns-of-hattin", name: "The Horns of Hattin" },
    RegionDef { id: 7, slug: "holy-land", name: "The Holy Land" },
];

/// Region lookup by id.
pub fn region_by_id(id: u8) -> Option<&'static RegionDef> {
    REGIONS.iter().find(|r| r.id == id)
}

// ---------- Screens ----------

/// A scripted funnel battle on a gate screen (`battle_ref` points at battle
/// content owned by crusade-app; `commander_id` references a named adversary
/// in `data::adversaries` when the gate is led).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BattleGate {
    pub battle_ref: u16,
    pub commander_id: Option<u16>,
}

/// An elective prop on a screen: the player walks up and engages, or walks
/// away (docs/OVERWORLD.md section 2, choice encounters).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PropRef {
    ChoiceEncounter(u16),
    Shrine,
    Camp,
    Caravan,
}

/// One fixed tile screen. Edges are [N, E, S, W] neighbor screen ids.
/// Static table entry; like Rite/SpeciesBase it is never serialized (the
/// journey state stores screen ids only), so no serde derives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenDef {
    pub id: u16,
    pub region: u8,
    /// Stub tileset/layout ref; real layouts arrive with the art pass.
    pub tiles_ref: u16,
    /// Neighbor screens [N, E, S, W]. `None` = no transition that way.
    pub edges: [Option<u16>; 4],
    /// Wild-encounter table id (archetypes 108..=117 by region), if any.
    pub encounter_zone: Option<u8>,
    /// Scripted funnel battle fired on entering this screen.
    pub scripted_gate: Option<BattleGate>,
    pub props: &'static [PropRef],
}

/// Screen lookup by id.
pub fn screen_by_id(id: u16) -> Option<&'static ScreenDef> {
    SCREENS.iter().find(|s| s.id == id)
}

// The screens are written out region by region as linear spines; the region
// macros below keep the edge wiring (both directions, region-boundary links)
// uniform and auditable.

/// Hand-built v1 spine: 25 screens, ids 1..=25, linear eastward march.
/// Edge discipline: E of screen n is n+1, W of screen n+1 is n; N/S are
/// unused in the spine. Guards below assert uniqueness, contiguity,
/// symmetry, and reachability.
pub static SCREENS: &[ScreenDef] = &[
    // ---- 1. The Marches (home; tutorial; low wild density) ----
    ScreenDef { id: 1, region: 1, tiles_ref: 1, edges: [None, Some(2), None, None], encounter_zone: None, scripted_gate: None, props: &[] },
    ScreenDef { id: 2, region: 1, tiles_ref: 1, edges: [None, Some(3), None, Some(1)], encounter_zone: Some(1), scripted_gate: None, props: &[PropRef::Shrine] },
    ScreenDef { id: 3, region: 1, tiles_ref: 1, edges: [None, Some(4), None, Some(2)], encounter_zone: None, scripted_gate: None, props: &[PropRef::Camp] },
    // ---- 2. The Crossing (compressed gauntlet; supply pressure) ----
    ScreenDef { id: 4, region: 2, tiles_ref: 2, edges: [None, Some(5), None, Some(3)], encounter_zone: None, scripted_gate: None, props: &[] },
    ScreenDef { id: 5, region: 2, tiles_ref: 2, edges: [None, Some(6), None, Some(4)], encounter_zone: None, scripted_gate: None, props: &[PropRef::Caravan] },
    // ---- 3. Anatolia (Rum Seljuk table; Kilij Arslan gate) ----
    ScreenDef { id: 6, region: 3, tiles_ref: 3, edges: [None, Some(7), None, Some(5)], encounter_zone: Some(3), scripted_gate: None, props: &[] },
    ScreenDef { id: 7, region: 3, tiles_ref: 3, edges: [None, Some(8), None, Some(6)], encounter_zone: Some(3), scripted_gate: None, props: &[PropRef::ChoiceEncounter(301)] },
    ScreenDef { id: 8, region: 3, tiles_ref: 3, edges: [None, Some(9), None, Some(7)], encounter_zone: Some(3), scripted_gate: None, props: &[] },
    ScreenDef { id: 9, region: 3, tiles_ref: 3, edges: [None, Some(10), None, Some(8)], encounter_zone: None, scripted_gate: Some(BattleGate { battle_ref: 101, commander_id: Some(120) }), props: &[] },
    // ---- 4. The Coasts (mixed tables; the Harim gate) ----
    ScreenDef { id: 10, region: 4, tiles_ref: 4, edges: [None, Some(11), None, Some(9)], encounter_zone: Some(4), scripted_gate: None, props: &[PropRef::Shrine] },
    ScreenDef { id: 11, region: 4, tiles_ref: 4, edges: [None, Some(12), None, Some(10)], encounter_zone: Some(4), scripted_gate: None, props: &[] },
    ScreenDef { id: 12, region: 4, tiles_ref: 4, edges: [None, Some(13), None, Some(11)], encounter_zone: Some(4), scripted_gate: None, props: &[PropRef::ChoiceEncounter(302)] },
    ScreenDef { id: 13, region: 4, tiles_ref: 4, edges: [None, Some(14), None, Some(12)], encounter_zone: None, scripted_gate: Some(BattleGate { battle_ref: 102, commander_id: Some(121) }), props: &[] },
    // ---- 5. The Roads of Egypt (Shirkuh gate; heaviest supply drain) ----
    ScreenDef { id: 14, region: 5, tiles_ref: 5, edges: [None, Some(15), None, Some(13)], encounter_zone: Some(5), scripted_gate: None, props: &[] },
    ScreenDef { id: 15, region: 5, tiles_ref: 5, edges: [None, Some(16), None, Some(14)], encounter_zone: Some(5), scripted_gate: None, props: &[PropRef::Caravan] },
    ScreenDef { id: 16, region: 5, tiles_ref: 5, edges: [None, Some(17), None, Some(15)], encounter_zone: None, scripted_gate: Some(BattleGate { battle_ref: 103, commander_id: Some(122) }), props: &[] },
    // ---- 6. The Horns of Hattin (scripted gauntlet; Gokbori / Taki al-Din wings) ----
    ScreenDef { id: 17, region: 6, tiles_ref: 6, edges: [None, Some(18), None, Some(16)], encounter_zone: Some(6), scripted_gate: None, props: &[] },
    ScreenDef { id: 18, region: 6, tiles_ref: 6, edges: [None, Some(19), None, Some(17)], encounter_zone: Some(6), scripted_gate: None, props: &[PropRef::ChoiceEncounter(303)] },
    ScreenDef { id: 19, region: 6, tiles_ref: 6, edges: [None, Some(20), None, Some(18)], encounter_zone: None, scripted_gate: Some(BattleGate { battle_ref: 104, commander_id: Some(124) }), props: &[] },
    ScreenDef { id: 20, region: 6, tiles_ref: 6, edges: [None, Some(21), None, Some(19)], encounter_zone: None, scripted_gate: Some(BattleGate { battle_ref: 105, commander_id: Some(123) }), props: &[] },
    // ---- 7. The Holy Land (Arsuf, Jaffa; the finale gates) ----
    ScreenDef { id: 21, region: 7, tiles_ref: 7, edges: [None, Some(22), None, Some(20)], encounter_zone: Some(7), scripted_gate: None, props: &[PropRef::Shrine] },
    ScreenDef { id: 22, region: 7, tiles_ref: 7, edges: [None, Some(23), None, Some(21)], encounter_zone: Some(7), scripted_gate: None, props: &[] },
    ScreenDef { id: 23, region: 7, tiles_ref: 7, edges: [None, Some(24), None, Some(22)], encounter_zone: None, scripted_gate: Some(BattleGate { battle_ref: 106, commander_id: Some(125) }), props: &[] },
    ScreenDef { id: 24, region: 7, tiles_ref: 7, edges: [None, Some(25), None, Some(23)], encounter_zone: None, scripted_gate: Some(BattleGate { battle_ref: 107, commander_id: Some(122) }), props: &[] },
    ScreenDef { id: 25, region: 7, tiles_ref: 7, edges: [None, None, None, Some(24)], encounter_zone: None, scripted_gate: Some(BattleGate { battle_ref: 108, commander_id: Some(123) }), props: &[] },
];

// ---------- Trail events ----------

/// A fixed engine verb (docs/OVERWORLD.md section 5: "effects are
/// data-selected from a fixed engine verb set").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrailVerb {
    /// Lose this many supplies.
    LoseSupplies(u16),
    /// Lose this many named men.
    LoseMen(u8),
    /// Shift party morale (clamped -100..=100 by the engine).
    MoraleDelta(i8),
    /// Enter a battle (wild/ambush ref).
    Battle(u16),
    /// A willing hearer on the road; surfaced to the caller, which runs
    /// the conversion flow of docs/CONVERSION.md. Payload = adversary
    /// archetype id (108..=117).
    ConvertOpportunity(u16),
    /// Gain this many supplies (forage, alms).
    Forage(u16),
    /// A day's rest: morale restored by the engine's rest rule.
    Rest,
    /// The road is quiet.
    Nothing,
}

/// A trail-event table entry. `region: None` = any region; `min_day` gates
/// late-road events. `weight` feeds the engine's weighted pick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrailEvent {
    pub id: u16,
    pub region: Option<u8>,
    pub weight: u8,
    pub min_day: u8,
    pub verb: TrailVerb,
}

/// The trail-event table, chronicle register: the road takes its tithe and
/// gives its alms. Battle refs 2xx = ambush/wild tables by region.
pub static TRAIL_EVENTS: &[TrailEvent] = &[
    TrailEvent { id: 1, region: None, weight: 10, min_day: 1, verb: TrailVerb::LoseSupplies(20) },   // rations spoiled in the barrels
    TrailEvent { id: 2, region: None, weight: 8, min_day: 3, verb: TrailVerb::LoseMen(1) },         // fever in the ranks
    TrailEvent { id: 3, region: None, weight: 6, min_day: 2, verb: TrailVerb::LoseMen(1) },         // a man drowned at the ford
    TrailEvent { id: 4, region: None, weight: 8, min_day: 2, verb: TrailVerb::Battle(201) },        // brigands upon the road
    TrailEvent { id: 5, region: None, weight: 8, min_day: 1, verb: TrailVerb::Rest },               // respite at a wayside shrine
    TrailEvent { id: 6, region: None, weight: 10, min_day: 1, verb: TrailVerb::Forage(15) },        // a generous villager shared his store
    TrailEvent { id: 7, region: None, weight: 12, min_day: 1, verb: TrailVerb::Forage(10) },        // good forage by the way
    TrailEvent { id: 8, region: None, weight: 8, min_day: 1, verb: TrailVerb::LoseSupplies(8) },    // a cart axle broke in the ruts
    TrailEvent { id: 9, region: None, weight: 6, min_day: 5, verb: TrailVerb::LoseSupplies(12) },   // thieves in the night watch
    TrailEvent { id: 10, region: None, weight: 6, min_day: 8, verb: TrailVerb::LoseMen(1) },        // a man slipped away in the dark
    TrailEvent { id: 11, region: None, weight: 10, min_day: 1, verb: TrailVerb::MoraleDelta(8) },   // the psalms were sung in good voice
    TrailEvent { id: 12, region: None, weight: 8, min_day: 1, verb: TrailVerb::MoraleDelta(-10) },  // ill rumor out of the east
    TrailEvent { id: 13, region: None, weight: 6, min_day: 1, verb: TrailVerb::LoseSupplies(15) },  // a storm scattered the train
    TrailEvent { id: 14, region: None, weight: 15, min_day: 1, verb: TrailVerb::Nothing },          // the day passed quietly
    TrailEvent { id: 15, region: Some(3), weight: 10, min_day: 1, verb: TrailVerb::Battle(203) },   // Seljuk riders shadowing the column
    TrailEvent { id: 16, region: Some(5), weight: 10, min_day: 1, verb: TrailVerb::LoseSupplies(25) }, // the sun drank the waterskins
    TrailEvent { id: 17, region: None, weight: 5, min_day: 4, verb: TrailVerb::ConvertOpportunity(113) }, // a captive outrider heard the Word gladly
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_ids_unique_and_contiguous() {
        assert_eq!(REGIONS.len(), 7);
        for (i, r) in REGIONS.iter().enumerate() {
            assert_eq!(r.id as usize, i + 1);
        }
    }

    #[test]
    fn screen_ids_unique_and_contiguous() {
        for (i, s) in SCREENS.iter().enumerate() {
            assert_eq!(s.id as usize, i + 1);
        }
        // Every screen's region exists.
        for s in SCREENS {
            assert!(region_by_id(s.region).is_some());
        }
    }

    #[test]
    fn edges_are_symmetric() {
        for s in SCREENS {
            for (dir, edge) in s.edges.iter().enumerate() {
                if let Some(n) = edge {
                    let nb = screen_by_id(*n).expect("edge target exists");
                    let back = nb.edges[(dir + 2) % 4];
                    assert_eq!(
                        back,
                        Some(s.id),
                        "screen {} dir {} -> {} lacks reverse edge",
                        s.id,
                        dir,
                        n
                    );
                }
            }
        }
    }

    #[test]
    fn every_region_reachable_from_screen_1() {
        // BFS over the edge graph from screen 1.
        let mut seen = vec![1u16];
        let mut frontier = vec![1u16];
        while let Some(cur) = frontier.pop() {
            let s = screen_by_id(cur).unwrap();
            for e in s.edges.iter().flatten() {
                if !seen.contains(e) {
                    seen.push(*e);
                    frontier.push(*e);
                }
            }
        }
        assert_eq!(seen.len(), SCREENS.len(), "every screen reachable");
        for r in REGIONS {
            assert!(
                SCREENS.iter().any(|s| s.region == r.id && seen.contains(&s.id)),
                "region {} reachable",
                r.id
            );
        }
    }

    #[test]
    fn gates_reference_valid_commanders() {
        for s in SCREENS {
            if let Some(g) = s.scripted_gate {
                if let Some(cid) = g.commander_id {
                    assert!(
                        crate::data::adversaries::ADVERSARIES
                            .iter()
                            .any(|a| a.id == cid && a.id >= 120),
                        "gate commander {} must be a named adversary (120..=125)",
                        cid
                    );
                }
            }
        }
    }

    /// docs/OVERWORLD.md region 6: the Hattin gauntlet is flanked by
    /// Saladin's wing commanders — Gokbori gates screen 19, Taki al-Din
    /// Umar screen 23 (screens 20/24/25 keep the worker's existing
    /// 123/122/123 finale mapping).
    #[test]
    fn hattin_wing_gates_reference_the_wing_commanders() {
        let gate = |id: u16| screen_by_id(id).unwrap().scripted_gate.unwrap().commander_id;
        assert_eq!(gate(19), Some(124));
        assert_eq!(gate(23), Some(125));
    }

    #[test]
    fn trail_event_ids_unique_and_regions_valid() {
        for (i, e) in TRAIL_EVENTS.iter().enumerate() {
            assert!(TRAIL_EVENTS[..i].iter().all(|o| o.id != e.id));
            if let Some(r) = e.region {
                assert!(region_by_id(r).is_some());
            }
            assert!(e.weight > 0);
        }
        // The classic spread: at least one of each load-bearing verb.
        use TrailVerb::*;
        for v in [
            LoseSupplies(1),
            LoseMen(1),
            MoraleDelta(1),
            MoraleDelta(-1),
            Battle(0),
            Forage(1),
            Rest,
            Nothing,
        ] {
            let kind = std::mem::discriminant(&v);
            let signed = matches!(v, MoraleDelta(d) if d < 0);
            let hit = TRAIL_EVENTS.iter().any(|e| {
                std::mem::discriminant(&e.verb) == kind
                    && (!matches!(e.verb, MoraleDelta(_))
                        || (matches!(e.verb, MoraleDelta(d) if d < 0)) == signed)
            });
            assert!(hit, "verb kind {:?} present", v);
        }
    }
}

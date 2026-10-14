//! View snapshots: everything a UI needs to draw one frame. All serde.

use crusade_core::engine::BattleEvent;
use crusade_core::types::Ailment;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Menu,
    Roster,
    BattleSetup,
    InBattle,
    BattleOver,
    ForcedSwitchSelect, // modal: we owe a switch
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChampionCard {
    /// Roster id (crusade-data key).
    pub id: String,
    pub species_id: u16,
    pub species_name: String,
    pub level: u8,
    pub current_resolve: u16,
    pub max_resolve: u16,
    pub ailment: Option<Ailment>,
    pub types_display: Vec<String>,
    pub rites: Vec<RiteCard>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiteCard {
    pub slot: u8,
    pub rite_id: u16,
    pub name: String,
    pub type_display: String,
    pub category_display: String,
    pub power: u8,
    pub accuracy: u8,
    pub zeal_left: u8,
    pub zeal_max: u8,
    pub usable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BattleChampionView {
    /// "me" or "foe".
    pub ours: bool,
    pub species_name: String,
    pub level: u8,
    pub current_resolve: u16,
    pub max_resolve: u16,
    pub ailment: Option<Ailment>,
    pub types_display: Vec<String>,
    /// stage deltas -6..=6 as (stat name, stage)
    pub stages: Vec<(String, i8)>,
    pub alive_reserves: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewSnapshot {
    pub phase: Phase,
    pub roster_count: usize,
    /// In-battle:
    pub me: Option<BattleChampionView>,
    pub foe: Option<BattleChampionView>,
    pub turn: u32,
    pub my_rites: Vec<RiteCard>,
    pub can_switch_to: Vec<String>, // roster-party indices we may switch to
    pub log_tail: Vec<String>,      // human-readable renderings of recent events
    pub battle_result: Option<u8>,  // Some(winner) when over
    /// New events produced by the last input (for animation queues).
    pub fresh_events: Vec<BattleEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientInput {
    OpenRoster,
    CloseToMenu,
    /// Start practice duel vs built-in policy AI with the first `party`
    /// roster champions at a given level.
    StartPractice {
        level: u8,
    },
    PickRite {
        slot: u8,
    },
    PickSwitch {
        party_idx: u8,
    },
    Concede,
    /// After BattleOver: clear back to menu.
    AcknowledgeResult,
}

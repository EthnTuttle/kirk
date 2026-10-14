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
    /// Campaign: between gauntlet nodes. The party has just rested (the log
    /// tail carries the heal preview); ContinueCampaign advances to the next
    /// boss, AbandonCampaign walks away keeping earned shards.
    CampaignRest,
}

/// Campaign (PvE gauntlet) progress view: `Some` from StartCampaign until the
/// run is acknowledged or abandoned. A full clear reads `defeated == 7`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CampaignView {
    /// 0-based index of the node being fought / next to fight; 7 after a
    /// full gauntlet clear.
    pub node: u8,
    /// Boss nodes defeated so far (0..=7).
    pub defeated: u8,
    /// Contrition shards earned this run (bounty per node is its 1-based
    /// index, so a full clear pays 1+2+..+7 = 28).
    pub shards_earned: u64,
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
    /// Campaign progress; `Some` while a gauntlet run is live (including its
    /// final BattleOver frame, where `defeated == 7` marks the full clear).
    pub campaign: Option<CampaignView>,
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
    /// Begin the seven-node sin-spawn gauntlet with the (first six) roster
    /// champions at their stored levels (capped at MAX_LEVEL).
    StartCampaign,
    /// CampaignRest: advance to the next boss node.
    ContinueCampaign,
    /// Walk away from a running campaign; contrition shards already earned
    /// are kept and the roster store was never touched mid-run.
    AbandonCampaign,
}

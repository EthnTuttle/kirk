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
    /// The Tournament of Virtue: browsing open challenges on the relays.
    /// The async lobby session (shared handles, nostr identity) lives OUT of
    /// the synchronous client — the bridge reports progress through
    /// [`ClientInput::LobbyNet`] while [`ViewSnapshot::lobby`] carries the
    /// renderable state.
    Lobby,
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

/// One open challenge listed from the relays (lobby screen row).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChallengeCard {
    /// Full challenge event id, hex — the [`ClientInput::AcceptChallenge`]
    /// handle and the stable button identity.
    pub event_id: String,
    /// Short display label for the host (npub prefix).
    pub host_label: String,
    /// Champions the host committed to fielding.
    pub party_size: u8,
}

/// Lobby progress latch. The async bridge reports stage moves through
/// [`LobbyNet::Notice`]; the sync client only renders what it is told.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LobbyStage {
    /// Reaching the relays (connect in flight).
    Connecting,
    /// A challenge-listing fetch is in flight.
    Listing,
    /// At rest; every lobby button is live.
    Idle,
    /// Our challenge is being published.
    Posting,
    /// Challenge issued; awaiting a champion's answer.
    AwaitingAccept,
    /// We are answering a listed challenge.
    Accepting,
}

/// Render state of the Tournament of Virtue lobby. Pure data: built by
/// `ClientGame` from bridge-reported facts; the nostr session itself lives
/// in the shell's async task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbyView {
    pub stage: LobbyStage,
    /// Relays this session is connected to (filled on [`LobbyNet::Ready`]).
    pub relays: Vec<String>,
    /// Short display label of our nostr identity (npub prefix).
    pub key_label: String,
    /// Open challenges seen on the relays (others' only), oldest first.
    pub challenges: Vec<ChallengeCard>,
    /// In-theme status line; also carries failure flavor text.
    pub notice: String,
}

impl LobbyView {
    /// A fresh lobby that has not reached the relays yet.
    pub fn connecting() -> Self {
        Self {
            stage: LobbyStage::Connecting,
            relays: Vec::new(),
            key_label: String::new(),
            challenges: Vec::new(),
            notice: "Seeking the heralds...".to_string(),
        }
    }
}

/// Bridge -> client lobby/duel reports. These travel as
/// [`ClientInput::LobbyNet`] so every UI (bevy shell, tests, future
/// bindings) drives the identical synchronous transition table; all nostr
/// handles stay in the shell's async task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LobbyNet {
    /// Relay connections established; the lobby is usable.
    Ready {
        relays: Vec<String>,
        key_label: String,
    },
    /// Stage move plus an in-theme status line.
    Notice { stage: LobbyStage, text: String },
    /// A fresh open-challenge listing.
    ChallengeList { challenges: Vec<ChallengeCard> },
    /// The covenant is sealed (lobby commit-reveal completed): the bridge
    /// now owns the net duel and renders it; the client steps out of the
    /// lobby into net-battle space.
    DuelReady,
    /// The duel settled: digest exchanged, attestation signed by the
    /// driver. The client applies the ladder result to its own store.
    DuelFinished { won: bool },
    /// The duel died out of band (peer fled, link timed out mid-turn).
    DuelAborted { text: String },
    /// A lobby-stage failure (connect/list/post/accept/complete); the
    /// lobby stays open with the flavor text on display.
    Failed { text: String },
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
    /// Lobby render data; `Some` while in [`Phase::Lobby`]. Net duels
    /// render from the bridge-held [`crate::pvp::NetDuelDriver`] snapshot
    /// instead, so this stays `None` outside the lobby.
    #[serde(default)]
    pub lobby: Option<LobbyView>,
    /// New events produced by the last input (for animation queues).
    pub fresh_events: Vec<BattleEvent>,
}

/// The honor-system ladder Elo line for the menu and lobby screens
/// (SHIPPING.md limitation 4: self-scored until mint arbitration ships —
/// the "(self-kept)" honesty suffix stays). `None` (no rating in the store
/// yet) reads as unranked. One helper so both screens agree verbatim.
pub fn ladder_line(rating: Option<u64>) -> String {
    match rating {
        Some(r) => format!("Ladder: {r} (self-kept)"),
        None => "Ladder: unranked".to_string(),
    }
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
    /// Menu -> Tournament of Virtue: enter the lobby. The client mints or
    /// loads our nostr seal from the store; the bridge then connects.
    EnterLobby,
    /// Lobby: fetch the open-challenge listing again.
    RefreshChallenges,
    /// Lobby: publish our company as an open challenge and await an answer.
    PostChallenge,
    /// Lobby: answer one listed challenge (its event id, hex).
    AcceptChallenge {
        event_id: String,
    },
    /// Lobby -> Menu (any in-flight lobby work withdraws).
    LeaveLobby,
    /// Async bridge report (lobby session / duel lifecycle). Drives the
    /// synchronous lobby view-state; net handles never enter the client.
    LobbyNet(LobbyNet),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ladder_line_marks_a_rating_self_kept_and_absent_as_unranked() {
        assert_eq!(ladder_line(Some(1200)), "Ladder: 1200 (self-kept)");
        assert_eq!(ladder_line(None), "Ladder: unranked");
    }
}

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
    /// The Hagiography: the witness roll of the revered non-combatant dead
    /// (pure lore) plus the Manuscript of Sins bestiary. A read-only
    /// memorial off the menu, like the roster minus every mechanic;
    /// [`ViewSnapshot::witnesses`], [`ViewSnapshot::sins`],
    /// [`ViewSnapshot::foes`] and [`ViewSnapshot::testimonies`] carry the
    /// books only while this phase is up.
    Hagiography,
    /// Post-battle Call to Repentance (M7b, docs/CONVERSION.md): a won
    /// battle left a convertible adversary (ids 108..=117) yielded; the
    /// player owes one choice per pending call — [`ClientInput::CallToRepentance`]
    /// or [`ClientInput::ReleaseSoul`]. The queue drains into BattleOver
    /// (or back to Trail mid-pilgrimage).
    ConversionChoice,
    /// The Pilgrim Road (M8b, docs/OVERWORLD.md): the journey layer,
    /// between screens. [`ViewSnapshot::trail`] carries the renderable
    /// state; [`ClientInput::Travel`] marches a day, road battles reuse
    /// the InBattle/BattleOver flow and route back here.
    Trail,
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
    /// Company-page (gallery) card: the six per-instance Devotion stats
    /// pre-rendered in class order (resolve, valour, piety, guard, aegis,
    /// alacrity) as "R0 V0 P0 G0 Ae0 Al0". Serde tail discipline: appended
    /// after every pre-existing field with a default, so documents written
    /// before the gallery page still load (the stats row then reads all
    /// zeros — rebuilt verbatim on the next save by any new client).
    #[serde(default)]
    pub devotion_display: String,
    /// Same recipe for the six Grace stats (IV-analogue, 0..=15 each).
    #[serde(default)]
    pub grace_display: String,
    /// Gallery vesture pick: which painted plate the Company card shows —
    /// 0 the canonical base, 1 the `alt-a` wardrobe read, 2 `alt-b`
    /// (crusade-app client settings `roster.variant.<species_id>`; the
    /// shell's plate path keys off it). Battle sprites are untouched: the
    /// alt sets ship portraits only, never animation families. Same tail
    /// discipline as the displays above: pre-variant documents load with 0.
    #[serde(default)]
    pub variant: u8,
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

/// One entry of the Hagiography's witness roll, rendered (the core's
/// `Witness` is pure lore and never serde; this card is its serde-safe
/// shadow). A witness has no id here: the roll is read, never targeted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitnessCard {
    pub name: String,
    /// The style by which pilgrims remember the witness.
    pub epithet: String,
    /// The short memorial reading.
    pub vignette: String,
}

/// One entry of the Hagiography's Manuscript of Sins: the bestiary row of
/// one sin-boss of the campaign gauntlet (`species_id` 101..=107, in
/// gauntlet order — Lust first, Pride last). The row always names the
/// boss; once the boss's node has been cleared (`defeated`), `sin` and
/// `lore` are revealed. A sealed row carries EMPTY `sin`/`lore` strings, so no UI
/// binding can leak the reward text before it is earned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SinCard {
    /// Static-table species id (101..=107); also the 32px icon key
    /// `sprites/icons/{id:03}.png`.
    pub species_id: u16,
    /// The boss's name (always shown — dimmed while sealed).
    pub name: String,
    /// The cardinal sin ("Lust", ...); empty until `defeated`.
    pub sin: String,
    /// The grotesque allegory reading; empty until `defeated`.
    pub lore: String,
    /// The boss's gauntlet node has been cleared (the entry is revealed).
    pub defeated: bool,
}

/// One row of the Hagiography's third book, the Roll of Worthy
/// Adversaries (docs/HISTORICAL-FOES.md section 6.5): one entry per field
/// archetype (108..=117) and named commander (120..=123), in id order. The
/// row always names the man (dimmed while unmet); once his defeat has been
/// witnessed in any battle on this document (`defeated`), the chronicle
/// `lore` reading is revealed. A sealed row carries EMPTY lore, so no UI
/// binding can leak the reward text before it is earned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoeCard {
    /// Static-table species id (108..=117, 120..=123).
    pub species_id: u16,
    /// The adversary's name (always shown — dimmed while sealed).
    pub name: String,
    /// The chronicle reading from the species table; empty until `defeated`.
    pub lore: String,
    /// The adversary's yield has been witnessed (the entry is revealed).
    pub defeated: bool,
}

/// One row of the Hagiography's Testimonies of the Baptized: the converted
/// archetype's own account of repentance (docs/CONVERSION.md section 4).
/// Rows exist for the ten convertible archetypes in id order; a sealed row
/// carries EMPTY `testimony` text until its `campaign.converts_baptized`
/// bit is set. Commanders are never called, so they have no rows here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestimonyCard {
    /// Static-table archetype id (108..=117).
    pub species_id: u16,
    /// The archetype's office (always shown — dimmed while sealed).
    pub name: String,
    /// The man's testimony, verbatim from `conversion::testimony`; empty
    /// until `baptized`.
    pub testimony: String,
    /// An instance of this archetype has been baptized on this document.
    pub baptized: bool,
}

/// The live post-battle Call (M7b): which yielded adversary the choice
/// concerns, at what level, and the chance the Call succeeds (percent,
/// pre-computed off `crusade_core::data::conversion_chance` — shown so the
/// player reads the same odds the roll uses). `pending` counts the calls
/// still owed, head included.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversionView {
    pub species_id: u16,
    pub species_name: String,
    pub foe_level: u8,
    pub chance_pct: u8,
    pub pending: u8,
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
    /// The Hagiography's witness roll; populated only while in
    /// [`Phase::Hagiography`] (one tiny lore table — battle snapshots stay
    /// lean). Read-only: the roll is rendered, never targeted.
    #[serde(default)]
    pub witnesses: Vec<WitnessCard>,
    /// The Hagiography's Manuscript of Sins: the 7-row sin-boss bestiary,
    /// gated exactly like the witness roll above. Reveal state comes from
    /// the persisted defeat mask (client.rs `SINS_DEFEATED_KEY`), never
    /// from the live run — sealed rows carry empty `sin`/`lore`.
    /// The Hagiography's Manuscript of Sins: the 7-row sin-boss bestiary,
    /// gated exactly like the witness roll above. Reveal state comes from
    /// the persisted defeat mask (client.rs `SINS_DEFEATED_KEY`), never
    /// from the live run — sealed rows carry empty `sin`/`lore`.
    #[serde(default)]
    pub sins: Vec<SinCard>,
    /// The Hagiography's Roll of Worthy Adversaries (HF 6.5): the 14-row
    /// historical-foe codex, gated like the manuscript above; reveal state
    /// is the persisted `campaign.foes_defeated` u32 mask. Sealed rows
    /// carry empty `lore`.
    #[serde(default)]
    pub foes: Vec<FoeCard>,
    /// The Hagiography's Testimonies of the Baptized (CONVERSION 4): one
    /// row per convertible archetype, gated on the persisted
    /// `campaign.converts_baptized` mask; sealed rows carry empty text.
    #[serde(default)]
    pub testimonies: Vec<TestimonyCard>,
    /// The live Call to Repentance, if any: `Some` only while
    /// [`Phase::ConversionChoice`] is up (one yielded convertible adversary
    /// per decision; the queue drains into BattleOver).
    #[serde(default)]
    pub conversion: Option<ConversionView>,
    /// The Pilgrim Road journey state (M8b): `Some` while a trail campaign
    /// is on the document (including its battles and its terminal
    /// Book-of-the-Fallen frame). Serde tail discipline: pre-M8b snapshots
    /// load with `None`.
    #[serde(default)]
    pub trail: Option<TrailView>,
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

/// The Pilgrim Road's renderable journey row (docs/OVERWORLD.md sections
/// 3-6), populated while a trail campaign lives on the document. Names,
/// never numbers, for the men: `champion_name` is who carries the mantle,
/// `last_fallen` the Book of the Fallen's latest page pre-rendered as
/// "Name, day N, where — cause" (empty before any loss).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrailView {
    /// Current screen id (the world-layer position).
    pub screen: u16,
    /// Display name of the region the company marches through.
    pub region_name: String,
    /// Day of the pilgrimage (the Crossing consumed days 0-2).
    pub day: u16,
    pub supplies: u16,
    pub morale: i8,
    /// Souls still marching (named men, living).
    pub living: u16,
    /// The man bearing the mantle (empty when the file stands unled after
    /// a rout until the mantle resolves — v1 always names someone while
    /// any live).
    pub champion_name: String,
    /// Pages in the Book of the Fallen so far.
    pub fallen_count: u16,
    /// Pre-rendered latest page of the Book; empty if none.
    pub last_fallen: String,
    /// Adjacent screens the company may march to (Travel targets).
    pub exits: Vec<u16>,
    /// Elective choice-encounter refs on this screen (EngageChoice
    /// targets; encounter props only — shrines etc. are scenery in v1).
    pub choice_encounters: Vec<u16>,
    /// The pilgrimage is over (the whole company is gone). Terminal.
    pub over: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientInput {
    OpenRoster,
    /// Roster/Hagiography -> Menu: the back-path for the read-only screens.
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
    /// Menu -> Hagiography: open the witness roll of the revered dead.
    /// Read-only; [`ClientInput::CloseToMenu`] is the way back, mirroring
    /// the roster.
    ViewHagiography,
    /// Roster gallery: cycle one champion's vesture pick
    /// (base -> alt-a -> alt-b -> base), persisted under
    /// `roster.variant.<species_id>`. Gallery chrome only — battle art is
    /// always the canonical base, so this mutates a setting, never a
    /// battle or the roster body.
    CycleVariant {
        species_id: u16,
    },
    /// ConversionChoice (M7b): issue the Call to Repentance to the yielded
    /// adversary at the head of the queue (docs/CONVERSION.md). One
    /// contested roll; success baptizes him into the roster, failure is the
    /// Mercy release.
    CallToRepentance,
    /// ConversionChoice: release the yielded adversary without the Call —
    /// his soul commended unto God (Mercy outcome, no roll).
    ReleaseSoul,
    /// Menu -> the Pilgrim Road (M8b): embark on a new trail campaign.
    /// `charter_quality` (0..=255, banded poor/fair/best by the engine)
    /// and `provisioning_spend` (same banding) are the Crossing's two
    /// UI-facing dials (docs/OVERWORLD.md section 6.4).
    StartTrail {
        charter_quality: u8,
        provisioning_spend: u16,
    },
    /// Trail: march one day to an adjacent screen (engine `travel_day`;
    /// the day's fortunes, upkeep, and any gate/ambush/wild roll resolve
    /// inside).
    Travel { to_screen: u16 },
    /// Trail: engage a ChoiceEncounter prop on the current screen (its
    /// encounter ref from [`TrailView::choice_encounters`]).
    EngageChoice { encounter_ref: u16 },
    /// Trail -> Menu: abandon the pilgrimage; the lane is cleared from the
    /// document (an abandoned run's Book pages vanish with it).
    AbandonTrail,
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

    /// Serde tail discipline: a ChampionCard document written BEFORE the
    /// gallery-page fields existed must still load, with the new displays
    /// defaulting to empty strings (never a parse failure, never a field
    /// reordered — the wire tail only ever grows).
    #[test]
    fn champion_card_pre_gallery_json_still_loads() {
        let legacy = r#"{
            "id": "00000000-0000-4000-8000-000000000001",
            "species_id": 1,
            "species_name": "Squire Audric",
            "level": 25,
            "current_resolve": 61,
            "max_resolve": 61,
            "ailment": null,
            "types_display": ["Humility"],
            "rites": []
        }"#;
        let card: ChampionCard =
            serde_json::from_str(legacy).expect("a pre-gallery card document loads");
        assert_eq!(card.devotion_display, "");
        assert_eq!(card.grace_display, "");
        assert_eq!(card.variant, 0, "pre-vesture documents read the base plate");
        // ...and a fully-populated card round-trips with the tail intact.
        let mut full = card;
        full.devotion_display = "R12 V8 P0 G3 Ae0 Al0".to_string();
        full.grace_display = "R15 V15 P15 G15 Ae15 Al15".to_string();
        full.variant = 2;
        let json = serde_json::to_string(&full).expect("card serializes");
        let back: ChampionCard = serde_json::from_str(&json).expect("card reloads");
        assert_eq!(back, full);
    }
}

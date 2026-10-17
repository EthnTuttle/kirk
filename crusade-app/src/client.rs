//! ClientGame: the state machine a UI drives. Worker H implements.
//!
//! Owns a RosterStore + an optional active battle (practice mode for M5a;
//! net duels plug in via crusade-net later without view changes).
//! `handle(input)` mutates state; `snapshot()` renders.
//!
//! Practice battle flow: StartPractice -> ForcedSwitchSelect/InBattle...
//! The AI opponent uses [`crate::policy::greedy_action`]. Events append to
//! an internal log (rendered human-readable lines; `ViewSnapshot::log_tail`
//! shows the last few) and to `fresh_events` (raw, take-drain: each
//! `snapshot()` returns the events produced since the previous snapshot and
//! clears the buffer).

use std::cell::RefCell;

use crusade_core::data;
use crusade_core::engine::{Action, Battle, BattleEvent};
use crusade_core::types::{
    Ailment, BattleStat, ChampionInstance, DevotionStats, GraceStats, RiteCategory, RiteSlot,
    RiteType, Sin, Typing, Virtue, Volatile, MAX_LEVEL,
};
use crusade_data::RosterStore;
use nostr::Keys;

use crate::policy::greedy_action;
use crate::pvp::{record_ladder_result, LADDER_RATING_KEY};
use crate::view::{
    BattleChampionView, CampaignView, ChampionCard, ClientInput, LobbyNet, LobbyStage, LobbyView,
    Phase, RiteCard, SinCard, ViewSnapshot, WitnessCard,
};
use crate::AppError;

/// Side 0 is always the local player in M5a practice battles.
const MY_SIDE: u8 = 0;
const FOE_SIDE: u8 = 1;
/// Ring buffer bound for the human-readable log.
pub(crate) const LOG_CAP: usize = 200;
/// How many log lines a snapshot exposes.
pub(crate) const LOG_TAIL: usize = 8;
/// Cap for the raw event queue a snapshot drains (UI animations only).
const FRESH_EVENTS_CAP: usize = 512;
/// Practice battles use a fixed seed; real duels arrive via commit-reveal.
const PRACTICE_SEED: [u8; 32] = [0x50; 32];
/// Deterministic practice theme team (ids into the species table).
const PRACTICE_FOE_SPECIES: [u16; 3] = [3, 8, 10];
/// A battle party fields at most 6 champions.
const PARTY_CAP: usize = 6;

/// Campaign (PvE gauntlet): seven sin-spawn boss nodes ordered by drama —
/// Lust, Gluttony, Greed, Sloth, Envy, Wrath, and Pride last. Species ids
/// follow each spawn's `Typing::Sin` in `crusade_core::data::species`.
///
/// SINGLE SOURCE for the boss order: the mechanics below index it by node,
/// bit i of the [`SINS_DEFEATED_KEY`] mask means "node i's boss has
/// yielded", and the bevy shell imports this very const for its rest-screen
/// portraits (the old `screens.rs` mirror was hoisted here — card D3
/// documented exactly this seam as the cleaner end-state).
pub const CAMPAIGN_NODE_SPECIES: [u16; 7] = [103, 106, 107, 104, 102, 105, 101];
/// Base seed for campaign node battles; the node index is folded in so each
/// node's rng stream is fixed but distinct (same recipe as PRACTICE_SEED).
const CAMPAIGN_SEED: [u8; 32] = [0xC9; 32];
/// PvE rewards paid to the shared inventory (writes to the store, unlike
/// anything else mid-battle).
const SHARD_ITEM: &str = "contrition-shard";
/// Settings key for the persisted sin-boss defeat mask: bit i (node i of
/// [`CAMPAIGN_NODE_SPECIES`]) is set the moment that boss's node is
/// cleared, beside the shard bounty. This is the Hagiography's Manuscript
/// of Sins reveal record — durable through the roster document like the
/// ladder rating, so revealed entries survive reload and roster sync.
pub const SINS_DEFEATED_KEY: &str = "campaign.sins_defeated";

/// Settings-key PREFIX for a champion's gallery vesture pick: the full key
/// is `roster.variant.<species_id>` and the value a decimal `u8` — 0 the
/// canonical base plate, 1/2 the `alt-a`/`alt-b` wardrobe reads staged
/// beside it (`sprites/gallery/{id:03}-{slug}--alt-{a,b}.png`). Pure
/// gallery chrome persisted in the roster document's settings map exactly
/// like the defeat mask and ladder rating (durable through save/sync);
/// battle art never reads it. Absent or unparseable values read as 0.
pub const VARIANT_KEY_PREFIX: &str = "roster.variant.";
/// How many plates sit on the cycle ring: base + alt-a + alt-b.
const VARIANT_COUNT: u8 = 3;

/// Settings key for the player's persisted nostr secret (hex) — the
/// Tournament of Virtue identity ("seal"). Minted on first lobby entry.
pub const NET_SECRET_KEY: &str = "net.nostr.secret";
/// Settings key for a relay-list override: comma/whitespace-separated
/// `wss://` URLs. Absent/empty/unusable values fall back to
/// [`DEFAULT_LOBBY_RELAYS`].
pub const NET_RELAYS_KEY: &str = "net.relays";
/// Default public relays for the Tournament of Virtue lobby.
pub const DEFAULT_LOBBY_RELAYS: [&str; 3] = [
    "wss://relay.damus.io",
    "wss://nos.lol",
    "wss://relay.nostr.band",
];

/// Boss level for a gauntlet node: 15 at node 0, +5 per node, capped at 40.
fn boss_level(node: u8) -> u8 {
    (node * 5 + 15).min(40)
}

/// Each boss fields two same-sin minions five levels under it (floored at 5).
fn minion_level(node: u8) -> u8 {
    boss_level(node).saturating_sub(5).max(5)
}

/// Live gauntlet run. `party` holds the campaign clones of the player's
/// roster: `current_resolve` persists between nodes (battle legality demands
/// full zeal at entry, so zeal is restored at each rest while resolve is
/// not). The roster store itself is never mutated by campaign battles.
#[derive(Debug, Clone)]
struct CampaignState {
    /// 0-based index of the node being fought / next to fight; 7 once the
    /// gauntlet is cleared.
    node: u8,
    /// Living party clones (fallen champions leave the gauntlet for good).
    party: Vec<ChampionInstance>,
    /// Boss nodes defeated.
    defeated: u8,
    /// Contrition shards paid out so far this run.
    shards_earned: u64,
}

#[derive(Debug)]
pub struct ClientGame {
    store: RosterStore,
    battle: Option<Battle>,
    phase: Phase,
    /// Active gauntlet run, if any.
    campaign: Option<CampaignState>,
    /// Lobby render state while in [`Phase::Lobby`]. The async nostr
    /// session it mirrors lives in the UI's bridge task; everything here is
    /// plain data arrived via [`ClientInput::LobbyNet`].
    lobby: Option<LobbyView>,
    /// Our nostr seal for the tournament, minted on first
    /// [`ClientInput::EnterLobby`] and persisted into the store. Watched by
    /// the bridge, which spawns the lobby session when it sees the phase
    /// flip; the client itself never touches an async handle.
    nostr_keys: Option<Keys>,
    /// Human-readable log lines, capped at `LOG_CAP`.
    log: Vec<String>,
    /// Raw events since the last snapshot. `RefCell` so `snapshot(&self)`
    /// can drain them (take-drain semantics for animation queues).
    fresh_events: RefCell<Vec<BattleEvent>>,
}

impl ClientGame {
    pub fn new(store: RosterStore) -> Self {
        Self {
            store,
            battle: None,
            phase: Phase::Menu,
            campaign: None,
            lobby: None,
            nostr_keys: None,
            log: Vec::new(),
            fresh_events: RefCell::new(Vec::new()),
        }
    }

    pub fn store(&mut self) -> &mut RosterStore {
        &mut self.store
    }

    /// Current phase, without `snapshot()`'s take-drain side effect on
    /// `fresh_events` (the persistence guard needs a side-effect-free read).
    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn snapshot(&self) -> ViewSnapshot {
        let roster_count = self.store.list_champions().map(|v| v.len()).unwrap_or(0);
        let tail_start = self.log.len().saturating_sub(LOG_TAIL);
        let log_tail = self.log[tail_start..].to_vec();
        let fresh_events = self.fresh_events.take();
        let mut snap = ViewSnapshot {
            phase: self.phase,
            roster_count,
            me: None,
            foe: None,
            turn: 0,
            my_rites: Vec::new(),
            can_switch_to: Vec::new(),
            log_tail,
            battle_result: None,
            campaign: self.campaign.as_ref().map(|c| CampaignView {
                node: c.node,
                defeated: c.defeated,
                shards_earned: c.shards_earned,
            }),
            lobby: self.lobby.clone(),
            // The witness roll rides along only while its screen is up
            // (one tiny lore table; every other snapshot stays lean).
            witnesses: if self.phase == Phase::Hagiography {
                witness_cards()
            } else {
                Vec::new()
            },
            // The Manuscript of Sins rides with the witness roll — the
            // same phase-gated discipline. Its reveal state is the
            // persisted defeat mask, not the live run: Hagiography is a
            // Menu-neighbour phase, where `campaign` is always `None`.
            sins: if self.phase == Phase::Hagiography {
                sin_cards(self.sins_defeated_mask())
            } else {
                Vec::new()
            },
            fresh_events,
        };
        let Some(b) = self.battle.as_ref() else {
            return snap;
        };
        if matches!(
            self.phase,
            Phase::InBattle | Phase::ForcedSwitchSelect | Phase::BattleOver
        ) {
            snap.me = Some(champion_view(b, MY_SIDE, MY_SIDE));
            snap.foe = Some(champion_view(b, FOE_SIDE, MY_SIDE));
            snap.turn = b.turn;
            snap.my_rites = rite_cards(b.sides[usize::from(MY_SIDE)].active());
            snap.can_switch_to = b
                .legal_actions(MY_SIDE)
                .iter()
                .filter_map(|a| match a {
                    Action::Switch { party_idx } => b.sides[usize::from(MY_SIDE)]
                        .party
                        .get(usize::from(*party_idx))
                        .map(|c| format!("{party_idx}:{}", species_name(c))),
                    _ => None,
                })
                .collect();
            if self.phase == Phase::BattleOver {
                snap.battle_result = b.winner;
            }
        }
        snap
    }

    /// Roster phase viewmodel: one card per stored champion (sorted by id
    /// for a stable UI order). Kept out of [`ViewSnapshot`] so battle
    /// snapshots stay small and serde-cheap.
    pub fn roster_cards(&self) -> Vec<ChampionCard> {
        let mut all = self.store.list_champions().unwrap_or_default();
        all.sort_by(|a, b| a.0.cmp(&b.0));
        all.into_iter()
            .map(|(id, c)| {
                let sp = data::species_by_id(c.species_id);
                ChampionCard {
                    id,
                    species_id: c.species_id,
                    species_name: sp.map_or_else(
                        || format!("Champion #{}", c.species_id),
                        |s| s.name.to_string(),
                    ),
                    level: c.level,
                    current_resolve: c.current_resolve,
                    max_resolve: sp.map_or(c.current_resolve, |s| {
                        data::compute_stats(&c, s).max_resolve
                    }),
                    ailment: c.ailment,
                    types_display: sp.map_or_else(Vec::new, |s| typing_display(s.typing)),
                    rites: rite_cards(&c),
                    // The gallery page's fuller stats: per-instance Devotion
                    // (training) and Grace (temperament) blocks, pre-rendered
                    // in the class's canonical stat order.
                    devotion_display: training_display([
                        c.devotion.resolve,
                        c.devotion.valour,
                        c.devotion.piety,
                        c.devotion.guard,
                        c.devotion.aegis,
                        c.devotion.alacrity,
                    ]),
                    grace_display: training_display([
                        c.grace.resolve,
                        c.grace.valour,
                        c.grace.piety,
                        c.grace.guard,
                        c.grace.aegis,
                        c.grace.alacrity,
                    ]),
                    // The gallery's per-champion vesture pick, read from
                    // the persisted settings ring each rebuild (single
                    // source of truth: the store — no client-side shadow).
                    variant: self.variant_of(c.species_id),
                }
            })
            .collect()
    }

    /// Our nostr seal, once the lobby has been entered (the bridge spawns
    /// the lobby session from this + [`ClientGame::lobby_relays`]).
    pub fn net_identity(&self) -> Option<Keys> {
        self.nostr_keys.clone()
    }

    /// The honor-system ladder Elo, read from the store key
    /// [`record_ladder_result`] writes (SHIPPING.md limitation 4). `None`
    /// (key absent or unparseable) is the unranked state the menu/lobby
    /// render. Exposed as an accessor like [`Self::roster_cards`] rather
    /// than a [`ViewSnapshot`] field: the net-duel driver builds its own
    /// snapshot literal, and the rating is menu/lobby chrome, not battle
    /// data.
    pub fn ladder_rating(&self) -> Option<u64> {
        self.store
            .setting(LADDER_RATING_KEY)
            .ok()
            .flatten()
            .and_then(|s| s.parse::<u64>().ok())
    }

    /// A champion's persisted gallery vesture pick (0 base, 1 alt-a,
    /// 2 alt-b) under [`VARIANT_KEY_PREFIX`] + the species id. An absent
    /// or unparseable value degrades to the base plate — never an error —
    /// mirroring [`Self::ladder_rating`]'s unranked degrade.
    pub fn variant_of(&self, species_id: u16) -> u8 {
        self.store
            .setting(&format!("{VARIANT_KEY_PREFIX}{species_id}"))
            .ok()
            .flatten()
            .and_then(|s| s.parse::<u8>().ok())
            .unwrap_or(0)
            .min(VARIANT_COUNT - 1)
    }

    /// Cycle the champion's gallery vesture pick base -> alt-a -> alt-b ->
    /// base and persist the new ring position (the shell's input pump
    /// bumps the snapshot on every accepted input, so the caller just
    /// re-renders). Champions only: the alt sets are the twelve battle
    /// champions' wardrobe reads; a sin-boss or unknown id has no alt
    /// plates at all, so the ring refuses them rather than pointing a
    /// future shell at files that will never exist.
    pub fn cycle_variant(&mut self, species_id: u16) -> Result<(), AppError> {
        let sp = data::species_by_id(species_id)
            .ok_or_else(|| illegal(format!("unknown species {species_id}")))?;
        if sp.id >= 100 {
            return Err(illegal(format!(
                "sin-spawn keep one vesture (species {species_id})"
            )));
        }
        let next = (self.variant_of(species_id) + 1) % VARIANT_COUNT;
        self.store
            .set_setting(&format!("{VARIANT_KEY_PREFIX}{species_id}"), &next.to_string())?;
        Ok(())
    }

    /// The persisted sin-boss defeat mask (bit i: the node-i boss of
    /// [`CAMPAIGN_NODE_SPECIES`] has yielded, at least once, in any run on
    /// this document). An absent or unparseable value degrades to 0 — a
    /// sealed manuscript — never to an error, mirroring how
    /// [`Self::ladder_rating`] degrades to unranked.
    fn sins_defeated_mask(&self) -> u8 {
        self.store
            .setting(SINS_DEFEATED_KEY)
            .ok()
            .flatten()
            .and_then(|s| s.parse::<u8>().ok())
            .unwrap_or(0)
    }

    /// The relay list the bridge connects the lobby to: the store's
    /// `net.relays` override (comma/whitespace-separated `wss://` URLs), or
    /// [`DEFAULT_LOBBY_RELAYS`] when unset or unusable.
    pub fn lobby_relays(&self) -> Vec<String> {
        let custom = self
            .store
            .setting(NET_RELAYS_KEY)
            .ok()
            .flatten()
            .map(|raw| parse_relay_list(&raw))
            .unwrap_or_default();
        if custom.is_empty() {
            DEFAULT_LOBBY_RELAYS.iter().map(|s| s.to_string()).collect()
        } else {
            custom
        }
    }

    /// The company our lobby commitments bind: first six roster champions,
    /// field-reset at their stored levels (campaign rules — stored levels,
    /// clamped legal). Empty roster is an error; the shell must not offer
    /// challenge buttons then.
    pub fn ladder_party(&self) -> Result<Vec<ChampionInstance>, AppError> {
        let mut roster = self.store.list_champions()?;
        if roster.is_empty() {
            return Err(illegal("cannot enter the lists with an empty company"));
        }
        roster.sort_by(|a, b| a.0.cmp(&b.0));
        let mut party = Vec::new();
        for (_, c) in roster.into_iter().take(PARTY_CAP) {
            let level = c.level.clamp(1, MAX_LEVEL);
            party.push(field_reset(c, level)?);
        }
        Ok(party)
    }

    pub fn handle(&mut self, input: ClientInput) -> Result<(), AppError> {
        match input {
            ClientInput::OpenRoster => match self.phase {
                Phase::Menu => {
                    self.phase = Phase::Roster;
                    Ok(())
                }
                p => Err(illegal(format!("cannot open the roster from {p:?}"))),
            },
            ClientInput::CloseToMenu => match self.phase {
                Phase::Roster | Phase::Hagiography => {
                    self.phase = Phase::Menu;
                    Ok(())
                }
                p => Err(illegal(format!("cannot close to menu from {p:?}"))),
            },
            ClientInput::ViewHagiography => match self.phase {
                Phase::Menu => {
                    self.phase = Phase::Hagiography;
                    Ok(())
                }
                p => Err(illegal(format!("cannot open the hagiography from {p:?}"))),
            },
            ClientInput::CycleVariant { species_id } => match self.phase {
                // Gallery chrome: only while the gallery is on screen. The
                // store write rides the same persist hook as every accepted
                // input (input.rs::persist_store), no migration needed.
                Phase::Roster => self.cycle_variant(species_id),
                p => Err(illegal(format!("cannot reconsider vesture from {p:?}"))),
            },
            ClientInput::StartPractice { level } => self.start_practice(level),
            ClientInput::StartCampaign => self.start_campaign(),
            ClientInput::ContinueCampaign => match self.phase {
                Phase::CampaignRest => self.continue_campaign(),
                p => Err(illegal(format!("no campaign rest to continue from {p:?}"))),
            },
            ClientInput::AbandonCampaign => self.abandon_campaign(),
            ClientInput::PickRite { slot } => match self.phase {
                Phase::InBattle => self.play_turn(Action::Rite { slot }),
                p => Err(illegal(format!("cannot pick a rite from {p:?}"))),
            },
            ClientInput::PickSwitch { party_idx } => match self.phase {
                Phase::InBattle => self.play_turn(Action::Switch { party_idx }),
                Phase::ForcedSwitchSelect => self.forced_switch(party_idx),
                p => Err(illegal(format!("cannot switch from {p:?}"))),
            },
            ClientInput::Concede => match self.phase {
                Phase::InBattle => self.play_turn(Action::Yield),
                p => Err(illegal(format!("cannot concede from {p:?}"))),
            },
            ClientInput::AcknowledgeResult => match self.phase {
                Phase::BattleOver => {
                    self.battle = None;
                    self.campaign = None;
                    self.fresh_events.borrow_mut().clear();
                    self.phase = Phase::Menu;
                    Ok(())
                }
                p => Err(illegal(format!("no result to acknowledge from {p:?}"))),
            },
            ClientInput::EnterLobby => match self.phase {
                Phase::Menu | Phase::Roster => self.enter_lobby(),
                p => Err(illegal(format!("cannot enter the tournament from {p:?}"))),
            },
            ClientInput::LeaveLobby => match self.phase {
                Phase::Lobby => {
                    self.lobby = None;
                    self.phase = Phase::Menu;
                    Ok(())
                }
                p => Err(illegal(format!("cannot leave the tournament from {p:?}"))),
            },
            ClientInput::RefreshChallenges => {
                let stage = self.lobby_stage()?;
                match stage {
                    LobbyStage::Idle | LobbyStage::Listing => {
                        let lobby = self.lobby.as_mut().expect("lobby checked above");
                        lobby.stage = LobbyStage::Listing;
                        lobby.notice = "Scanning the lists for open challenges...".to_string();
                        Ok(())
                    }
                    _ => Err(illegal(format!(
                        "the heralds are busy ({stage:?}); the lists wait"
                    ))),
                }
            }
            ClientInput::PostChallenge => {
                let stage = self.lobby_stage()?;
                if stage != LobbyStage::Idle {
                    return Err(illegal(format!("cannot issue a challenge while {stage:?}")));
                }
                if self
                    .store
                    .list_champions()
                    .map(|c| c.is_empty())
                    .unwrap_or(true)
                {
                    return Err(illegal("cannot enter the lists with an empty company"));
                }
                let lobby = self.lobby.as_mut().expect("lobby checked above");
                lobby.stage = LobbyStage::Posting;
                lobby.notice = "Issuing a challenge to all comers...".to_string();
                Ok(())
            }
            ClientInput::AcceptChallenge { event_id } => {
                let stage = self.lobby_stage()?;
                if stage != LobbyStage::Idle {
                    return Err(illegal(format!(
                        "cannot answer a challenge while {stage:?}"
                    )));
                }
                if self
                    .store
                    .list_champions()
                    .map(|c| c.is_empty())
                    .unwrap_or(true)
                {
                    return Err(illegal("cannot enter the lists with an empty company"));
                }
                let lobby = self.lobby.as_ref().expect("lobby checked above");
                let Some(card) = lobby.challenges.iter().find(|c| c.event_id == event_id) else {
                    return Err(illegal("that challenge is no longer listed"));
                };
                let label = card.host_label.clone();
                let lobby = self.lobby.as_mut().expect("lobby checked above");
                lobby.stage = LobbyStage::Accepting;
                lobby.notice = format!("Answering the challenge of {label}...");
                Ok(())
            }
            ClientInput::LobbyNet(msg) => self.handle_lobby_net(msg),
        }
    }

    /// The lobby stage, or an error when the phase has no lobby to show
    /// (stale clicks and late bridge reports land here and are dropped by
    /// the shell with a warning — never a panic).
    fn lobby_stage(&self) -> Result<LobbyStage, AppError> {
        if self.phase != Phase::Lobby {
            return Err(illegal(format!(
                "no tournament is open from {:?}",
                self.phase
            )));
        }
        self.lobby
            .as_ref()
            .map(|l| l.stage)
            .ok_or_else(|| illegal("tournament state lost — re-enter the lobby"))
    }

    /// Menu/Roster -> Lobby: mint or load the nostr seal (persisted in the
    /// store, so the tournament identity survives reloads) and open the
    /// lobby view in the Connecting stage; the bridge takes it from there.
    fn enter_lobby(&mut self) -> Result<(), AppError> {
        if self.battle.is_some() || self.campaign.is_some() {
            return Err(illegal("a battle or campaign is already active"));
        }
        if self.nostr_keys.is_none() {
            self.nostr_keys = Some(self.load_or_mint_seal()?);
        }
        self.lobby = Some(LobbyView::connecting());
        self.phase = Phase::Lobby;
        Ok(())
    }

    /// Load the persisted nostr seal, minting (and persisting) one on first
    /// entry. A corrupted stored secret is replaced by a fresh seal.
    fn load_or_mint_seal(&mut self) -> Result<Keys, AppError> {
        let stored = self.store.setting(NET_SECRET_KEY)?;
        if let Some(secret_hex) = stored.as_deref() {
            if let Ok(keys) = Keys::parse(secret_hex) {
                return Ok(keys);
            }
        }
        let keys = Keys::generate();
        self.store.set_setting(
            NET_SECRET_KEY,
            &hex::encode(keys.secret_key().to_secret_bytes()),
        )?;
        Ok(keys)
    }

    /// Bridge reports. Every variant is phase-guarded: a report arriving
    /// after the player withdrew, or after its window passed, is an illegal
    /// transition (the shell logs and drops it) rather than a panic.
    fn handle_lobby_net(&mut self, msg: LobbyNet) -> Result<(), AppError> {
        match msg {
            LobbyNet::Ready { relays, key_label } => {
                self.lobby_stage()?;
                let count = relays.len();
                let lobby = self.lobby.as_mut().expect("lobby checked above");
                lobby.stage = LobbyStage::Idle;
                lobby.relays = relays;
                lobby.key_label = key_label;
                lobby.notice =
                    format!("The tournament grounds are open - {count} herald(s) carry the lists.");
                Ok(())
            }
            LobbyNet::Notice { stage, text } => {
                self.lobby_stage()?;
                let lobby = self.lobby.as_mut().expect("lobby checked above");
                lobby.stage = stage;
                lobby.notice = text;
                Ok(())
            }
            LobbyNet::ChallengeList { challenges } => {
                self.lobby_stage()?;
                let count = challenges.len();
                let lobby = self.lobby.as_mut().expect("lobby checked above");
                lobby.challenges = challenges;
                if lobby.stage == LobbyStage::Listing {
                    lobby.stage = LobbyStage::Idle;
                }
                lobby.notice = if count == 0 {
                    "The lists are empty - no champion stands challenged. Issue yours.".to_string()
                } else {
                    format!("{count} open challenge(s) await an answer.")
                };
                Ok(())
            }
            LobbyNet::DuelReady => {
                self.lobby_stage()?;
                self.lobby = None;
                self.fresh_events.borrow_mut().clear();
                self.push_log(vec![
                    "The covenant is sealed - the duel is joined.".to_string()
                ]);
                self.phase = Phase::BattleSetup;
                Ok(())
            }
            LobbyNet::DuelFinished { won } => {
                if self.phase != Phase::BattleSetup {
                    return Err(illegal(format!(
                        "no net duel to settle from {:?}",
                        self.phase
                    )));
                }
                let rating = record_ladder_result(&mut self.store, won, None)?;
                self.push_log(vec![format!(
                    "{} The honor ladder records a standing of {rating} (kept on good faith).",
                    if won {
                        "Victory - Deo Gratias!"
                    } else {
                        "Defeat - pray and regroup."
                    }
                )]);
                self.phase = Phase::BattleOver;
                Ok(())
            }
            LobbyNet::DuelAborted { text } => {
                if self.phase != Phase::BattleSetup {
                    return Err(illegal(format!(
                        "no net duel to abandon from {:?}",
                        self.phase
                    )));
                }
                self.push_log(vec![text]);
                self.phase = Phase::BattleOver;
                Ok(())
            }
            LobbyNet::Failed { text } => {
                self.lobby_stage()?;
                let lobby = self.lobby.as_mut().expect("lobby checked above");
                lobby.stage = LobbyStage::Idle;
                lobby.notice = text;
                Ok(())
            }
        }
    }

    /// StartPractice: clone the (first 6) roster champions, field-reset to
    /// the requested level, and face the deterministic theme team.
    fn start_practice(&mut self, level: u8) -> Result<(), AppError> {
        if !matches!(self.phase, Phase::Menu | Phase::Roster)
            || self.battle.is_some()
            || self.campaign.is_some()
        {
            return Err(illegal("a battle or campaign is already active"));
        }
        if level == 0 || level > MAX_LEVEL {
            return Err(illegal(format!(
                "level must be 1..={MAX_LEVEL}, got {level}"
            )));
        }
        let mut roster = self.store.list_champions()?;
        if roster.is_empty() {
            return Err(illegal("cannot start practice with an empty roster"));
        }
        // Stable party order: roster ids sort deterministically.
        roster.sort_by(|a, b| a.0.cmp(&b.0));
        let mut party_a = Vec::new();
        for (_, c) in roster.into_iter().take(PARTY_CAP) {
            party_a.push(field_reset(c, level)?);
        }
        // Never mirror the party: the fixed theme team loses every species
        // the player already fields (*that* Bastien sparring Bastien read
        // as a bug even under the display-side "Foe" tag), backfilled from
        // the champion pool in stable id order.
        let party_species: Vec<u16> = party_a.iter().map(|c| c.species_id).collect();
        let mut party_b = Vec::new();
        for id in practice_foe_team(&party_species) {
            party_b.push(practice_champion(id, level)?);
        }
        let foes = party_b
            .iter()
            .map(species_name)
            .collect::<Vec<_>>()
            .join(", ");
        let battle = Battle::new(party_a, party_b, PRACTICE_SEED)?;
        self.battle = Some(battle);
        self.phase = Phase::InBattle;
        self.fresh_events.borrow_mut().clear();
        self.push_log(vec![format!(
            "Practice duel at level {level} begins - facing {foes}."
        )]);
        Ok(())
    }

    /// StartCampaign: clone the (first 6) roster champions, field-reset at
    /// their stored levels (capped at MAX_LEVEL), and open the gauntlet at
    /// node 0. The roster store itself is not touched by campaign battles.
    fn start_campaign(&mut self) -> Result<(), AppError> {
        if !matches!(self.phase, Phase::Menu | Phase::Roster)
            || self.battle.is_some()
            || self.campaign.is_some()
        {
            return Err(illegal("a battle or campaign is already active"));
        }
        let mut roster = self.store.list_champions()?;
        if roster.is_empty() {
            return Err(illegal("cannot start a campaign with an empty roster"));
        }
        // Stable party order: roster ids sort deterministically.
        roster.sort_by(|a, b| a.0.cmp(&b.0));
        let mut party = Vec::new();
        for (_, c) in roster.into_iter().take(PARTY_CAP) {
            let level = c.level.clamp(1, MAX_LEVEL);
            party.push(field_reset(c, level)?);
        }
        self.campaign = Some(CampaignState {
            node: 0,
            party,
            defeated: 0,
            shards_earned: 0,
        });
        self.push_log(vec![
            "The crusade begins: seven sins bar the road.".to_string()
        ]);
        self.start_campaign_node(0)
    }

    /// Field the next gauntlet node: the (living, rested) campaign party vs
    /// two minions with the boss of this node's sin waiting behind them.
    fn start_campaign_node(&mut self, node: u8) -> Result<(), AppError> {
        let Some(c) = self.campaign.as_ref() else {
            return Err(illegal("no active campaign"));
        };
        if c.party.is_empty() {
            return Err(illegal("the campaign party has nobody left standing"));
        }
        let party_a = c.party.clone();
        let boss_id = CAMPAIGN_NODE_SPECIES[usize::from(node)];
        let boss_lv = boss_level(node);
        let minion_lv = minion_level(node);
        // Boss last: the spawn screen the party before the sin itself steps
        // out for the finale.
        let party_b = vec![
            practice_champion(boss_id, minion_lv)?,
            practice_champion(boss_id, minion_lv)?,
            practice_champion(boss_id, boss_lv)?,
        ];
        let boss_name = data::species_by_id(boss_id)
            .map(|sp| sp.name)
            .expect("static species table covers all campaign nodes");
        let mut seed = CAMPAIGN_SEED;
        seed[0] = node;
        let battle = Battle::new(party_a, party_b, seed)?;
        self.battle = Some(battle);
        self.phase = Phase::InBattle;
        self.fresh_events.borrow_mut().clear();
        self.push_log(vec![format!(
            "Node {}/7 - {boss_name} (level {boss_lv}) waits behind its spawn (level {minion_lv}).",
            node + 1
        )]);
        Ok(())
    }

    /// CampaignRest -> next node battle.
    fn continue_campaign(&mut self) -> Result<(), AppError> {
        let Some(c) = self.campaign.as_ref() else {
            return Err(illegal("no active campaign"));
        };
        if usize::from(c.node) >= CAMPAIGN_NODE_SPECIES.len() {
            return Err(illegal("the gauntlet is already cleared"));
        }
        let node = c.node;
        self.start_campaign_node(node)
    }

    /// Walk away mid-run: keep earned shards (already in the store), drop
    /// the campaign clones, back to Menu.
    fn abandon_campaign(&mut self) -> Result<(), AppError> {
        if self.campaign.is_none() {
            return Err(illegal("no active campaign to abandon"));
        }
        self.battle = None;
        self.campaign = None;
        self.fresh_events.borrow_mut().clear();
        self.phase = Phase::Menu;
        self.push_log(vec![
            "The campaign is abandoned; earned contrition shards remain.".to_string(),
        ]);
        Ok(())
    }

    /// One full turn: my chosen action vs the greedy foe, then any forced
    /// switch the foe owes is resolved immediately (greedy first-alive).
    fn play_turn(&mut self, mine: Action) -> Result<(), AppError> {
        let Some(b) = self.battle.as_mut() else {
            return Err(illegal("no active battle"));
        };
        if !b.legal_actions(MY_SIDE).contains(&mine) {
            return Err(illegal(format!("{mine:?} is not a legal action right now")));
        }
        // Track each side's active party slot across the event batch so log
        // lines attribute acts to the right champion.
        let mut actives = [
            b.sides[usize::from(MY_SIDE)].active_idx,
            b.sides[usize::from(FOE_SIDE)].active_idx,
        ];
        let foe = greedy_action(b, FOE_SIDE);
        let mut events = b.apply_turn(mine, foe)?;
        events.extend(resolve_foe_forced(b)?);
        let lines = render_events(b, &events, &mut actives, MY_SIDE);
        let result = b.winner;
        let my_forced = {
            let acts = b.legal_actions(MY_SIDE);
            result.is_none()
                && !acts.is_empty()
                && acts.iter().all(|a| matches!(a, Action::Switch { .. }))
        };
        self.push_log(lines);
        self.push_fresh_events(events);
        if let Some(w) = result {
            self.on_battle_end(w)?;
        } else if my_forced {
            self.phase = Phase::ForcedSwitchSelect;
        } else {
            self.phase = Phase::InBattle;
        }
        Ok(())
    }

    /// Route a finished battle: practice duels go straight to BattleOver;
    /// campaign nodes pay out shards, rest the survivors, and either open a
    /// CampaignRest (next node pending) or end the run (clear / wipe).
    fn on_battle_end(&mut self, winner: u8) -> Result<(), AppError> {
        let Some((node, defeated, shards)) = self
            .campaign
            .as_ref()
            .map(|c| (c.node, c.defeated, c.shards_earned))
        else {
            self.phase = Phase::BattleOver;
            return Ok(());
        };
        let boss = data::species_by_id(CAMPAIGN_NODE_SPECIES[usize::from(node.min(6))])
            .map(|sp| sp.name)
            .expect("static species table covers all campaign nodes");
        if winner != MY_SIDE {
            self.push_log(vec![format!(
                "The company breaks before {boss}. The campaign is over - {defeated} node(s) cleared, {shards} contrition shard(s) kept."
            )]);
            self.phase = Phase::BattleOver;
            return Ok(());
        }
        // Node cleared: the bounty is the node's 1-based index, paid into
        // the shared inventory (the one store write a campaign performs).
        let bounty = i64::from(node) + 1;
        let carried = self.store.add_item(SHARD_ITEM, bounty)?;
        // ...and at that same sanctioned write point the Manuscript of
        // Sins records the fallen sin for good (the second and last store
        // write a campaign performs): reveal = defeat-mask bit per node.
        let mask = self.sins_defeated_mask() | (1u8 << node.min(6));
        self.store.set_setting(SINS_DEFEATED_KEY, &mask.to_string())?;
        let mut lines = vec![format!(
            "{boss} yields! The node grants {bounty} contrition shard(s) (carried: {carried})."
        )];
        // Survivors rest: resolve persists across the gauntlet, the living
        // recover half their maximum, ailments pass with sleep. Zeal is
        // restored because battle entry requires full-zeal rites.
        let b = self
            .battle
            .as_ref()
            .expect("on_battle_end called with a live battle");
        let mut survivors = Vec::new();
        for ch in &b.sides[usize::from(MY_SIDE)].party {
            if ch.current_resolve == 0 {
                continue;
            }
            let mut ch = ch.clone();
            let sp = data::species_by_id(ch.species_id)
                .ok_or_else(|| illegal(format!("unknown species {}", ch.species_id)))?;
            let max = data::compute_stats(&ch, sp).max_resolve;
            let before = ch.current_resolve;
            ch.current_resolve = before.saturating_add(max / 2).min(max);
            if ch.current_resolve > before {
                lines.push(format!(
                    "{} rests (resolve {before} -> {}/{}).",
                    sp.name, ch.current_resolve, max
                ));
            }
            for slot in ch.rites.iter_mut().flatten() {
                if let Some(r) = data::rite_by_id(slot.rite_id) {
                    slot.zeal_left = r.zeal;
                }
            }
            ch.ailment = None;
            survivors.push(ch);
        }
        let done = {
            let c = self.campaign.as_mut().expect("campaign checked above");
            c.party = survivors;
            c.node += 1;
            c.defeated += 1;
            c.shards_earned += bounty as u64;
            usize::from(c.node) >= CAMPAIGN_NODE_SPECIES.len()
        };
        if done {
            lines.push("Pride itself is broken - the gauntlet is cleared!".to_string());
            self.push_log(lines);
            self.phase = Phase::BattleOver;
        } else {
            self.battle = None;
            self.push_log(lines);
            self.phase = Phase::CampaignRest;
        }
        Ok(())
    }

    /// ForcedSwitchSelect: the player owes a switch; resolve it and hand the
    /// initiative back to normal play.
    fn forced_switch(&mut self, party_idx: u8) -> Result<(), AppError> {
        let Some(b) = self.battle.as_mut() else {
            return Err(illegal("no active battle"));
        };
        let mine = Action::Switch { party_idx };
        if !b.legal_actions(MY_SIDE).contains(&mine) {
            return Err(illegal(format!(
                "switch target {party_idx} is not available"
            )));
        }
        let mut actives = [
            b.sides[usize::from(MY_SIDE)].active_idx,
            b.sides[usize::from(FOE_SIDE)].active_idx,
        ];
        let from = b.events().len();
        b.apply_forced_switch(MY_SIDE, party_idx)?;
        let events = b.events()[from..].to_vec();
        let lines = render_events(b, &events, &mut actives, MY_SIDE);
        self.push_log(lines);
        self.push_fresh_events(events);
        self.phase = Phase::InBattle;
        Ok(())
    }

    /// Queue raw events for the next snapshot's take-drain (UI animation
    /// queue only). Growth is capped: beyond FRESH_EVENTS_CAP the oldest
    /// undrained events are dropped so only the newest 512 survive.
    fn push_fresh_events(&mut self, events: Vec<BattleEvent>) {
        let mut queue = self.fresh_events.borrow_mut();
        queue.extend(events);
        let excess = queue.len().saturating_sub(FRESH_EVENTS_CAP);
        if excess > 0 {
            queue.drain(..excess);
        }
    }

    fn push_log(&mut self, lines: Vec<String>) {
        self.log.extend(lines);
        if self.log.len() > LOG_CAP {
            let excess = self.log.len() - LOG_CAP;
            self.log.drain(..excess);
        }
    }
}

fn illegal(msg: impl Into<String>) -> AppError {
    AppError::IllegalTransition(msg.into())
}

/// Parse a relay-list override: split on commas, semicolons and whitespace,
/// strip URL-hostile punctuation from pasted entries, keep only
/// `ws://`/`wss://` URLs, dedupe preserving order.
fn parse_relay_list(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in raw.split(|c: char| c == ',' || c == ';' || c.is_whitespace()) {
        let url = part.trim_matches(|c: char| {
            c.is_whitespace() || matches!(c, ',' | ';' | '"' | '\'' | '<' | '>' | '|')
        });
        if (url.starts_with("wss://") || url.starts_with("ws://"))
            && !out.iter().any(|seen| seen == url)
        {
            out.push(url.to_string());
        }
    }
    out
}

/// Resolve the foe's pending forced switch, if any, with the greedy
/// first-alive choice (first switch offered by `legal_actions`). Returns the
/// events emitted by those switches.
fn resolve_foe_forced(b: &mut Battle) -> Result<Vec<BattleEvent>, AppError> {
    let mut out = Vec::new();
    while b.winner.is_none() {
        let acts = b.legal_actions(FOE_SIDE);
        if acts.is_empty() || !acts.iter().all(|a| matches!(a, Action::Switch { .. })) {
            break;
        }
        let Action::Switch { party_idx } = acts[0] else {
            break;
        };
        let from = b.events().len();
        b.apply_forced_switch(FOE_SIDE, party_idx)?;
        out.extend_from_slice(&b.events()[from..]);
    }
    Ok(out)
}

/// Field-reset a champion for a duel at `level`: keep identity (species,
/// calling, grace, devotion, rite selection) but recompute resolve and zeal
/// for the given level and clear ailments.
fn field_reset(mut c: ChampionInstance, level: u8) -> Result<ChampionInstance, AppError> {
    let sp = data::species_by_id(c.species_id)
        .ok_or_else(|| illegal(format!("unknown species {}", c.species_id)))?;
    c.level = level;
    c.ailment = None;
    for slot in c.rites.iter_mut().flatten() {
        if let Some(r) = data::rite_by_id(slot.rite_id) {
            slot.zeal_left = r.zeal;
        }
    }
    c.current_resolve = data::compute_stats(&c, sp).max_resolve;
    Ok(c)
}

/// The practice foe trio with mirror-matches removed at the root: the
/// fixed [`PRACTICE_FOE_SPECIES`] theme drops every id the player's party
/// already fields, then backfills from the champion pool in stable id
/// order until the trio is full. Deterministic — the selection derives
/// from party contents alone, never from rng; theme survivors keep their
/// relative order. The pool is the twelve battle champions and a party is
/// capped at six, so at least six backfill candidates ALWAYS exist: a
/// shortfall is impossible, and the expect below asserts exactly that
/// invariant rather than handling it.
fn practice_foe_team(party_species: &[u16]) -> [u16; 3] {
    let mut team: Vec<u16> = PRACTICE_FOE_SPECIES
        .iter()
        .copied()
        .filter(|id| !party_species.contains(id))
        .collect();
    for id in 1..=12u16 {
        if team.len() == PRACTICE_FOE_SPECIES.len() {
            break;
        }
        if !party_species.contains(&id) && !team.contains(&id) {
            team.push(id);
        }
    }
    team.try_into()
        .expect("the 12-champion pool minus any 6-party always fills the practice trio")
}

/// Build a fresh practice-team champion: up to 4 rites taken from the most
/// recent learnset entries available at `level` (same recipe as crusade-sim).
fn practice_champion(species_id: u16, level: u8) -> Result<ChampionInstance, AppError> {
    let sp = data::species_by_id(species_id)
        .ok_or_else(|| illegal(format!("unknown species {species_id}")))?;
    let mut picked: Vec<u16> = Vec::new();
    for rite_id in sp
        .learnset
        .iter()
        .filter(|e| e.level <= level)
        .map(|e| e.rite_id)
        .rev()
    {
        if picked.len() == 4 {
            break;
        }
        if !picked.contains(&rite_id) {
            picked.push(rite_id);
        }
    }
    picked.reverse();
    let mut rites: [Option<RiteSlot>; 4] = [None; 4];
    for (i, id) in picked.iter().enumerate() {
        let r = data::rite_by_id(*id).ok_or_else(|| illegal(format!("unknown rite {id}")))?;
        rites[i] = Some(RiteSlot {
            rite_id: *id,
            zeal_left: r.zeal,
        });
    }
    field_reset(
        ChampionInstance {
            species_id,
            level,
            xp: 0,
            grace: GraceStats {
                resolve: 0,
                valour: 0,
                piety: 0,
                guard: 0,
                aegis: 0,
                alacrity: 0,
            },
            devotion: DevotionStats {
                resolve: 0,
                valour: 0,
                piety: 0,
                guard: 0,
                aegis: 0,
                alacrity: 0,
            },
            calling_id: 0,
            rites,
            current_resolve: 0,
            ailment: None,
            converted: false,
            converted_from: None,
        },
        level,
    )
}

// ---------- View helpers (pure rendering over battle/store state) ----------

/// The Hagiography's witness roll as renderable cards, straight off the
/// static core table. Pure lore: a witness is read, never targeted (ids
/// stay in crusade-core), so this never touches the store or a battle.
fn witness_cards() -> Vec<WitnessCard> {
    data::all_witnesses()
        .iter()
        .map(|w| WitnessCard {
            name: w.name.to_string(),
            epithet: w.epithet.to_string(),
            vignette: w.vignette.to_string(),
        })
        .collect()
}

/// The Manuscript of Sins as renderable cards: one row per gauntlet boss,
/// in gauntlet order, drawn off the same static table+[`SINS_DEFEATED_KEY`]
/// pairing the campaign mechanics use. A boss's entry is revealed once bit
/// `node` of the defeat mask is set; sealed rows keep the name (shown
/// dimmed) but carry EMPTY sin/lore, so the reward text never leaves the
/// client before it is earned. Like [`witness_cards`], this reads the
/// static lore verbatim and never targets anything.
fn sin_cards(defeated_mask: u8) -> Vec<SinCard> {
    CAMPAIGN_NODE_SPECIES
        .iter()
        .enumerate()
        .map(|(node, &id)| {
            let sp = data::species_by_id(id)
                .expect("static species table covers all campaign nodes");
            let Typing::Sin(sin) = sp.typing else {
                panic!("gauntlet node species {} is a sin-spawn", sp.name)
            };
            let revealed = defeated_mask & (1u8 << node) != 0;
            SinCard {
                species_id: id,
                name: sp.name.to_string(),
                sin: if revealed {
                    sin_name(sin).to_string()
                } else {
                    String::new()
                },
                lore: if revealed {
                    sp.lore.to_string()
                } else {
                    String::new()
                },
                defeated: revealed,
            }
        })
        .collect()
}

pub(crate) fn species_name(c: &ChampionInstance) -> String {
    data::species_by_id(c.species_id).map_or_else(
        || format!("Champion #{}", c.species_id),
        |sp| sp.name.to_string(),
    )
}

/// The gallery-card stat line shared by Devotion and Grace renders: six
/// per-instance values in the class's canonical order (resolve, valour,
/// piety, guard, aegis, alacrity), compactly abbreviated (Ae/Al
/// distinguish aegis/alacrity). Pure ASCII, fixed labels.
fn training_display(values: [u8; 6]) -> String {
    format!(
        "R{} V{} P{} G{} Ae{} Al{}",
        values[0], values[1], values[2], values[3], values[4], values[5]
    )
}

/// Render one side's active champion. `local` is the side the UI controls:
/// it gets `ours: true`. Practice/campaign pass MY_SIDE; the net driver
/// passes its own side (a joiner sees the duel from side 1).
pub(crate) fn champion_view(b: &Battle, side: u8, local: u8) -> BattleChampionView {
    let s = &b.sides[usize::from(side)];
    let c = s.active();
    let sp = data::species_by_id(c.species_id);
    BattleChampionView {
        ours: side == local,
        species_name: sp.map_or_else(|| format!("Champion #{}", c.species_id), |s| s.name.into()),
        level: c.level,
        current_resolve: c.current_resolve,
        max_resolve: sp.map_or(c.current_resolve, |sp| {
            data::compute_stats(c, sp).max_resolve
        }),
        ailment: c.ailment,
        types_display: sp.map_or_else(Vec::new, |sp| typing_display(sp.typing)),
        stages: stages_display(b, side),
        alive_reserves: s
            .party
            .iter()
            .enumerate()
            .filter(|(i, ch)| *i != s.active_idx && ch.current_resolve > 0)
            .count() as u8,
    }
}

pub(crate) fn rite_cards(c: &ChampionInstance) -> Vec<RiteCard> {
    c.rites
        .iter()
        .enumerate()
        .filter_map(|(slot, rs)| {
            let rs = rs.as_ref()?;
            let r = data::rite_by_id(rs.rite_id)?;
            Some(RiteCard {
                slot: slot as u8,
                rite_id: rs.rite_id,
                name: r.name.to_string(),
                type_display: type_display(r.rite_type),
                category_display: category_display(r.category),
                power: r.power,
                accuracy: r.accuracy,
                zeal_left: rs.zeal_left,
                zeal_max: r.zeal,
                usable: rs.zeal_left > 0,
            })
        })
        .collect()
}

/// Stage rows (stat name, stage) for the side's current active state; zero
/// stages are omitted. Mirrors the engine's stat_idx ordering (not public).
fn stages_display(b: &Battle, side: u8) -> Vec<(String, i8)> {
    let st = &b.sides[usize::from(side)].active_state;
    [
        BattleStat::Valour,
        BattleStat::Piety,
        BattleStat::Guard,
        BattleStat::Aegis,
        BattleStat::Alacrity,
    ]
    .into_iter()
    .enumerate()
    .filter(|(i, _)| st.stages[*i] != 0)
    .map(|(i, stat)| (stat_name(stat).to_string(), st.stages[i]))
    .collect()
}

fn virtue_name(v: Virtue) -> &'static str {
    match v {
        Virtue::Humility => "Humility",
        Virtue::Charity => "Charity",
        Virtue::Chastity => "Chastity",
        Virtue::Diligence => "Diligence",
        Virtue::Patience => "Patience",
        Virtue::Temperance => "Temperance",
        Virtue::Kindness => "Kindness",
    }
}

fn sin_name(s: Sin) -> &'static str {
    match s {
        Sin::Pride => "Pride",
        Sin::Envy => "Envy",
        Sin::Lust => "Lust",
        Sin::Sloth => "Sloth",
        Sin::Wrath => "Wrath",
        Sin::Gluttony => "Gluttony",
        Sin::Greed => "Greed",
    }
}

fn typing_display(t: Typing) -> Vec<String> {
    match t {
        Typing::Virtues([first, second]) => match (first, second) {
            (Some(a), Some(b)) => vec![format!("{}+{}", virtue_name(a), virtue_name(b))],
            (Some(a), None) | (None, Some(a)) => vec![virtue_name(a).to_string()],
            (None, None) => vec![],
        },
        Typing::Sin(s) => vec![format!("Sin of {}", sin_name(s))],
        Typing::Valor => vec!["Valor".to_string()],
    }
}

fn type_display(t: RiteType) -> String {
    match t {
        RiteType::Typeless => "Typeless".to_string(),
        RiteType::Virtue(v) => virtue_name(v).to_string(),
        RiteType::Sin(s) => format!("Sin of {}", sin_name(s)),
    }
}

fn category_display(c: RiteCategory) -> String {
    match c {
        RiteCategory::Martial => "Martial",
        RiteCategory::Spiritual => "Spiritual",
        RiteCategory::Status => "Status",
    }
    .to_string()
}

fn stat_name(s: BattleStat) -> &'static str {
    match s {
        BattleStat::Valour => "Valour",
        BattleStat::Piety => "Piety",
        BattleStat::Guard => "Guard",
        BattleStat::Aegis => "Aegis",
        BattleStat::Alacrity => "Alacrity",
    }
}

fn ailment_faded(a: Ailment) -> &'static str {
    match a {
        Ailment::Brand => "brand faded",
        Ailment::Shackled => "shackles fell away",
        Ailment::Venom => "venom was purged",
        Ailment::Slumber => "awoke",
        Ailment::Frost => "thawed",
    }
}

fn ailment_struck(a: Ailment) -> &'static str {
    match a {
        Ailment::Brand => "was branded",
        Ailment::Shackled => "was shackled",
        Ailment::Venom => "was envenomed",
        Ailment::Slumber => "fell into a slumber",
        Ailment::Frost => "was frost-bound",
    }
}

fn volatile_struck(v: Volatile) -> &'static str {
    match v {
        Volatile::Confusion => "grew confused",
        Volatile::Flinch => "flinched",
        Volatile::SeedLeech => "was seeded",
    }
}

// ---------- Event -> log line rendering ----------

pub(crate) fn render_events(
    b: &Battle,
    events: &[BattleEvent],
    actives: &mut [usize; 2],
    local: u8,
) -> Vec<String> {
    events
        .iter()
        .map(|e| render_event(b, e, actives, local))
        .collect()
}

fn party_name(b: &Battle, side: u8, party_idx: usize) -> String {
    b.sides[usize::from(side)]
        .party
        .get(party_idx)
        .map(species_name)
        .unwrap_or_else(|| "?".to_string())
}

fn active_name(b: &Battle, side: u8, actives: &[usize; 2]) -> String {
    party_name(b, side, actives[usize::from(side)])
}

/// Render one event as a short human line. `actives` tracks each side's
/// active party index across the batch; `Switched` updates it so following
/// lines attribute names correctly. `local` is the UI-controlled side, used
/// for the "you" phrasing on forced switches. Unknown future variants
/// degrade to their Debug form.
#[allow(unreachable_patterns)] // forward-compat: future BattleEvent variants hit the catch-all
fn render_event(b: &Battle, e: &BattleEvent, actives: &mut [usize; 2], local: u8) -> String {
    match e {
        BattleEvent::TurnStart { turn } => format!("- Turn {turn} -"),
        BattleEvent::RiteUsed { side, rite_id } => {
            let who = active_name(b, *side, actives);
            let rite = data::rite_by_id(*rite_id)
                .map_or_else(|| format!("rite #{rite_id}"), |r| r.name.to_string());
            format!("{who} used {rite}.")
        }
        BattleEvent::Missed { side } => {
            format!("{}'s rite missed.", active_name(b, *side, actives))
        }
        BattleEvent::Immune { side } => {
            let other = 1usize.saturating_sub(*side as usize) as u8;
            format!("{} is unaffected.", active_name(b, other, actives))
        }
        BattleEvent::Damage {
            side,
            amount,
            crit,
            effectiveness_pct,
        } => {
            let who = active_name(b, *side, actives);
            let mut tag = String::new();
            if *crit {
                tag.push_str("critical");
            }
            if *effectiveness_pct > 100 {
                if !tag.is_empty() {
                    tag.push_str(", ");
                }
                tag.push_str("super effective");
            } else if *effectiveness_pct < 100 {
                if !tag.is_empty() {
                    tag.push_str(", ");
                }
                tag.push_str("not very effective");
            }
            if tag.is_empty() {
                format!("{who} took {amount} damage.")
            } else {
                format!("{who} took {amount} damage ({tag}).")
            }
        }
        BattleEvent::Healed { side, amount } => {
            format!(
                "{} recovered {amount} resolve.",
                active_name(b, *side, actives)
            )
        }
        BattleEvent::AilmentApplied { side, ailment } => {
            format!(
                "{} {}.",
                active_name(b, *side, actives),
                ailment_struck(*ailment)
            )
        }
        BattleEvent::AilmentEnded { side, ailment } => {
            format!(
                "{} {}.",
                active_name(b, *side, actives),
                ailment_faded(*ailment)
            )
        }
        BattleEvent::VolatileApplied { side, volatile_ } => {
            format!(
                "{} {}.",
                active_name(b, *side, actives),
                volatile_struck(*volatile_)
            )
        }
        BattleEvent::StageChanged {
            side,
            stat,
            new_stage,
        } => {
            let who = active_name(b, *side, actives);
            let dir = if *new_stage >= 0 { "rose" } else { "fell" };
            format!("{}'s {} {dir} to {new_stage:+}.", who, stat_name(*stat))
        }
        BattleEvent::AilmentChip { side, amount } => {
            format!(
                "{} took {amount} damage from its affliction.",
                active_name(b, *side, actives)
            )
        }
        BattleEvent::Switched { side, to_party_idx } => {
            actives[usize::from(*side)] = usize::from(*to_party_idx);
            format!(
                "{} took the field.",
                party_name(b, *side, usize::from(*to_party_idx))
            )
        }
        BattleEvent::ChampionYielded { side, party_idx } => {
            format!(
                "{}'s resolve broke.",
                party_name(b, *side, usize::from(*party_idx))
            )
        }
        BattleEvent::ForcedSwitchNeeded { side } => {
            if *side == local {
                "Choose your next champion.".to_string()
            } else {
                "The foe must send out another champion.".to_string()
            }
        }
        BattleEvent::BattleEnded { winner } => {
            if *winner == MY_SIDE {
                "Battle over - you prevail.".to_string()
            } else {
                "Battle over - the foe prevails.".to_string()
            }
        }
        unknown => format!("{unknown:?}"),
    }
}

#[cfg(test)]
mod practice_tests {
    //! The practice foe trio's mirror-avoidance: pure table-driven
    //! selection from the party's species ids (no rng anywhere).

    use super::*;

    #[test]
    fn party_holding_the_whole_theme_yields_a_disjoint_trio() {
        // A company fielding exactly Bastien, Aldwyn, Raimbaud (the theme
        // ids) must never spar itself.
        let team = practice_foe_team(&[3, 8, 10]);
        assert_eq!(team.len(), 3);
        for id in team {
            assert!((1..=12).contains(&id), "foe {id} is a champion id");
            assert!(![3, 8, 10].contains(&id), "foe {id} never mirrors");
        }
        // All three distinct, and the stable backfill order is 1, 2, 4.
        assert_eq!(team, [1, 2, 4]);
    }

    #[test]
    fn empty_party_keeps_the_pure_theme() {
        assert_eq!(practice_foe_team(&[]), PRACTICE_FOE_SPECIES);
    }

    #[test]
    fn partial_overlap_replaces_only_the_colliders() {
        // Party fields Bastien only: Bastien leaves, Aldwyn and Raimbaud
        // keep their theme order, and the smallest free pool id appends.
        assert_eq!(practice_foe_team(&[3]), [8, 10, 1]);
        // Duplicated party species change nothing; nor do surplus members.
        assert_eq!(practice_foe_team(&[10, 10, 3, 5]), [8, 1, 2]);
    }
}

#[cfg(test)]
mod lobby_tests {
    //! Lobby/net phase-transition tests: the synchronous half of the
    //! Tournament of Virtue. Everything here is pure `handle()`/`snapshot()`
    //! driving — the relay-facing half lives in `tests/net_client_tests.rs`.

    use super::*;

    /// Single-champion roster fixture (learnset-legal — same shape as the
    /// lobby.rs test fixtures).
    fn fixture_store() -> RosterStore {
        let mut store = RosterStore::new();
        store
            .add_champion(&practice_champion(1, 20).expect("fixture champion"))
            .expect("fixture store accepts the champion");
        store
    }

    fn card(id: u8) -> crate::view::ChallengeCard {
        crate::view::ChallengeCard {
            event_id: hex::encode([id; 32]),
            host_label: format!("npub1{id:04x}…"),
            party_size: 2,
        }
    }

    fn ready(game: &mut ClientGame) {
        game.handle(ClientInput::LobbyNet(LobbyNet::Ready {
            relays: vec!["wss://relay.test".into()],
            key_label: "npub1us…".into(),
        }))
        .expect("lobby accepts Ready");
    }

    #[test]
    fn enter_mints_and_persists_seal_leave_clears() {
        let mut game = ClientGame::new(fixture_store());
        game.handle(ClientInput::EnterLobby).expect("enter lobby");
        assert_eq!(game.phase(), Phase::Lobby);
        let snap = game.snapshot();
        let lobby = snap.lobby.as_ref().expect("lobby view present");
        assert_eq!(lobby.stage, LobbyStage::Connecting);
        assert!(lobby.challenges.is_empty());

        let keys = game.net_identity().expect("seal minted");
        let stored = game
            .store()
            .setting(NET_SECRET_KEY)
            .expect("setting readable")
            .expect("seal persisted");
        assert_eq!(
            Keys::parse(&stored)
                .expect("stored seal parses")
                .public_key(),
            keys.public_key(),
            "the persisted seal round-trips"
        );

        game.handle(ClientInput::LeaveLobby).expect("leave lobby");
        assert_eq!(game.phase(), Phase::Menu);
        assert!(game.snapshot().lobby.is_none());

        // Re-entry loads the SAME seal (identity survives, no re-mint).
        game.handle(ClientInput::EnterLobby).expect("re-enter");
        assert_eq!(
            game.net_identity().expect("seal reloaded").public_key(),
            keys.public_key()
        );
    }

    #[test]
    fn relay_override_falls_back_and_filters() {
        let mut game = ClientGame::new(fixture_store());
        assert_eq!(
            game.lobby_relays(),
            DEFAULT_LOBBY_RELAYS
                .iter()
                .map(|s| (*s).to_string())
                .collect::<Vec<_>>()
        );
        game.store()
            .set_setting(
                NET_RELAYS_KEY,
                "wss://a.example,  wss://a.example; http://nope\nwss://b.example",
            )
            .expect("write override");
        // The wss entries survive (dedup'd; the `;` tails strip away);
        // non-websocket URLs are dropped.
        let relays = game.lobby_relays();
        assert!(relays.contains(&"wss://a.example".to_string()));
        assert!(relays.contains(&"wss://b.example".to_string()));
        assert_eq!(relays.len(), 2, "dedupe + scheme filter apply: {relays:?}");
    }

    #[test]
    fn lobby_input_guards_hold() {
        // Empty roster: PostChallenge/AcceptChallenge are rejected.
        let mut game = ClientGame::new(RosterStore::new());
        game.handle(ClientInput::EnterLobby).expect("enter");
        assert!(game.handle(ClientInput::PostChallenge).is_err());
        assert!(game
            .handle(ClientInput::AcceptChallenge {
                event_id: hex::encode([9u8; 32]),
            })
            .is_err());

        // With a roster, but wrong stage (Connecting) and wrong phase.
        let mut game = ClientGame::new(fixture_store());
        assert!(game.handle(ClientInput::RefreshChallenges).is_err());
        game.handle(ClientInput::EnterLobby).expect("enter");
        assert!(game.handle(ClientInput::PostChallenge).is_err());
        assert!(game.handle(ClientInput::AcknowledgeResult).is_err());
        ready(&mut game);
        // Listed check: unknown event ids cannot be answered.
        assert!(game
            .handle(ClientInput::AcceptChallenge {
                event_id: hex::encode([9u8; 32]),
            })
            .is_err());
    }

    #[test]
    fn ladder_rating_flows_from_the_store_unrated_until_written() {
        let mut game = ClientGame::new(fixture_store());
        assert_eq!(
            game.ladder_rating(),
            None,
            "absent key reads unranked (menu/lobby render 'Ladder: unranked')"
        );

        game.store()
            .set_setting(LADDER_RATING_KEY, "1212")
            .expect("rating writable");
        assert_eq!(game.ladder_rating(), Some(1212));

        game.store()
            .set_setting(LADDER_RATING_KEY, "not-a-rating")
            .expect("rating writable");
        assert_eq!(
            game.ladder_rating(),
            None,
            "unparseable rating degrades to unranked, never to a crash"
        );
    }

    #[test]
    fn post_flow_reaches_duel_and_rates_the_ladder() {
        let mut game = ClientGame::new(fixture_store());
        game.handle(ClientInput::EnterLobby).expect("enter");
        ready(&mut game);

        game.handle(ClientInput::PostChallenge)
            .expect("post from idle");
        assert_eq!(game.snapshot().lobby.unwrap().stage, LobbyStage::Posting);

        game.handle(ClientInput::LobbyNet(LobbyNet::Notice {
            stage: LobbyStage::AwaitingAccept,
            text: "Challenge issued — awaiting a worthy answer.".into(),
        }))
        .expect("notice");
        assert_eq!(
            game.snapshot().lobby.unwrap().stage,
            LobbyStage::AwaitingAccept
        );

        game.handle(ClientInput::LobbyNet(LobbyNet::DuelReady))
            .expect("duel ready");
        assert_eq!(game.phase(), Phase::BattleSetup);
        assert!(game.snapshot().lobby.is_none(), "lobby view is closed");

        game.handle(ClientInput::LobbyNet(LobbyNet::DuelFinished { won: true }))
            .expect("settle");
        assert_eq!(game.phase(), Phase::BattleOver);
        assert_eq!(
            game.store().setting("ladder.rating").unwrap().as_deref(),
            Some("1016"),
            "win off the default rating is +16"
        );

        game.handle(ClientInput::AcknowledgeResult)
            .expect("acknowledge");
        assert_eq!(game.phase(), Phase::Menu);
    }

    #[test]
    fn accept_flow_and_abort_paths_hold() {
        let mut game = ClientGame::new(fixture_store());
        game.handle(ClientInput::EnterLobby).expect("enter");
        ready(&mut game);

        game.handle(ClientInput::LobbyNet(LobbyNet::ChallengeList {
            challenges: vec![card(1), card(2)],
        }))
        .expect("listing");
        assert_eq!(game.snapshot().lobby.unwrap().challenges.len(), 2);

        game.handle(ClientInput::AcceptChallenge {
            event_id: hex::encode([1u8; 32]),
        })
        .expect("accept listed challenge");
        assert_eq!(game.snapshot().lobby.unwrap().stage, LobbyStage::Accepting);

        game.handle(ClientInput::LobbyNet(LobbyNet::DuelReady))
            .expect("duel ready");
        assert_eq!(game.phase(), Phase::BattleSetup);

        // The foe vanishes mid-duel: the ladder must not move.
        game.handle(ClientInput::LobbyNet(LobbyNet::DuelAborted {
            text: "The foe has fled the field.".into(),
        }))
        .expect("abort");
        assert_eq!(game.phase(), Phase::BattleOver);
        assert!(game
            .store()
            .setting("ladder.rating")
            .expect("readable")
            .is_none());
        game.handle(ClientInput::AcknowledgeResult)
            .expect("acknowledge");
        assert_eq!(game.phase(), Phase::Menu);
    }

    #[test]
    fn lobby_failure_recovers_to_idle_with_notice() {
        let mut game = ClientGame::new(fixture_store());
        game.handle(ClientInput::EnterLobby).expect("enter");
        ready(&mut game);
        game.handle(ClientInput::PostChallenge).expect("post");
        game.handle(ClientInput::LobbyNet(LobbyNet::Failed {
            text: "None answered the challenge before the candles burned low.".into(),
        }))
        .expect("failure is recoverable");
        let lobby = game.snapshot().lobby.expect("lobby still open");
        assert_eq!(lobby.stage, LobbyStage::Idle);
        assert!(lobby.notice.contains("None answered"));

        // Bridge reports in wrong phases stay errors (the shell drops them).
        assert!(game
            .handle(ClientInput::LobbyNet(LobbyNet::DuelFinished { won: true }))
            .is_err());
    }
}

#[cfg(test)]
mod roster_gallery_tests {
    //! The company gallery page's fuller card fields: per-instance Devotion
    //! and Grace blocks must arrive pre-rendered on every roster card
    //! (serde tail discipline for the fields themselves is pinned over in
    //! view.rs; here we pin the population recipe).

    use super::*;

    /// Same fixture shape as the shell's starter band: one level-25 Audric
    /// with zeroed Devotion/Grace (an untrained fresh champion).
    fn fixture_store() -> RosterStore {
        let mut store = RosterStore::new();
        store
            .add_champion(&practice_champion(1, 25).expect("fixture champion"))
            .expect("fixture store accepts the champion");
        store
    }

    #[test]
    fn roster_cards_carry_devotion_and_grace_displays() {
        let game = ClientGame::new(fixture_store());
        let cards = game.roster_cards();
        assert_eq!(cards.len(), 1);
        let card = &cards[0];
        assert_eq!(
            card.devotion_display, "R0 V0 P0 G0 Ae0 Al0",
            "an untrained champion's devotion block renders all-zero"
        );
        assert_eq!(
            card.grace_display, "R0 V0 P0 G0 Ae0 Al0",
            "the fixture champion carries no rolled grace"
        );
        // The pre-existing rows are untouched by the gallery fields.
        assert_eq!(card.species_name, "Squire Audric");
        assert_eq!(card.level, 25);
        assert!(!card.rites.is_empty());
        for line in [&card.devotion_display, &card.grace_display] {
            assert!(line.is_ascii(), "gallery lines stay ASCII: {line}");
        }
    }

    #[test]
    fn training_display_uses_class_order_and_caps_labels() {
        // Class order: resolve, valour, piety, guard, aegis, alacrity; the
        // Ae/Al spellings keep aegis and alacrity distinct at card width.
        let line = training_display([1, 2, 3, 4, 5, 6]);
        assert_eq!(line, "R1 V2 P3 G4 Ae5 Al6");
    }
}

#[cfg(test)]
mod hagiography_tests {
    //! The witness roll: a read-only memorial screen off the menu (pure
    //! lore — no may-enter requirements, no store reads, no battle state).

    use super::*;

    #[test]
    fn hagiography_carries_the_roll_in_phase_and_returns_to_menu() {
        let mut game = ClientGame::new(RosterStore::new());
        // The roll never rides along in any other phase's snapshot.
        assert!(game.snapshot().witnesses.is_empty());
        assert!(game.handle(ClientInput::CloseToMenu).is_err());

        game.handle(ClientInput::ViewHagiography)
            .expect("the menu opens the hagiography");
        assert_eq!(game.phase(), Phase::Hagiography);
        let snap = game.snapshot();
        assert_eq!(snap.witnesses.len(), 1, "the roll holds its one witness");
        let kirk = &snap.witnesses[0];
        assert_eq!(kirk.name, "Kirk");
        assert_eq!(kirk.epithet, "the Witness of the Open Square");
        for image in ["kirk", "bench", "widow", "forgav"] {
            assert!(
                kirk.vignette.contains(image),
                "vignette arrived incomplete (lost '{image}')"
            );
        }
        // Re-entry is a stale click, not a panic.
        assert!(game.handle(ClientInput::ViewHagiography).is_err());

        game.handle(ClientInput::CloseToMenu)
            .expect("the back-path returns to the menu");
        assert_eq!(game.phase(), Phase::Menu);
        assert!(game.snapshot().witnesses.is_empty());
    }

    /// The Manuscript of Sins: 7 rows in gauntlet order, phase-gated like
    /// the witness roll, each entry sealed (name only) until its node bit
    /// is set in the persisted defeat mask — and the mask rides the roster
    /// document, so a revealed entry survives a reload. ASCII only.
    #[test]
    fn manuscript_of_sins_gates_by_phase_and_defeat_mask() {
        let mut game = ClientGame::new(RosterStore::new());
        // Menu (and every other phase): no sins ride the snapshot.
        assert!(game.snapshot().sins.is_empty());

        game.handle(ClientInput::ViewHagiography)
            .expect("the menu opens the hagiography");
        let snap = game.snapshot();
        assert_eq!(snap.sins.len(), 7, "one row per gauntlet sin-boss");
        // Gauntlet order is the campaign's: Lust, Gluttony, Greed, Sloth,
        // Envy, Wrath, Pride last (client.rs::CAMPAIGN_NODE_SPECIES).
        let order: Vec<u16> = snap.sins.iter().map(|s| s.species_id).collect();
        assert_eq!(order, vec![103, 106, 107, 104, 102, 105, 101]);
        for card in &snap.sins {
            assert!(!card.defeated, "a fresh manuscript has faced nothing");
            assert!(!card.name.is_empty(), "even a sealed row is named");
            assert!(card.name.is_ascii(), "{}", card.name);
            assert!(
                card.sin.is_empty() && card.lore.is_empty(),
                "a sealed row carries no sin/lore text: {}",
                card.name
            );
        }

        // Node 1 defeated (Glutton Maw): exactly that row flips.
        game.store()
            .set_setting(SINS_DEFEATED_KEY, "2") // bit 1
            .expect("defeat mask writable");
        let snap = game.snapshot();
        let glutton = &snap.sins[1];
        assert!(glutton.defeated, "node 1's row is revealed");
        assert_eq!(glutton.species_id, 106);
        assert_eq!(glutton.name, "Glutton Maw");
        assert_eq!(glutton.sin, "Gluttony");
        assert_eq!(
            glutton.lore,
            data::species_by_id(106).expect("boss in static table").lore,
            "revealed lore is the species table's, verbatim"
        );
        for (node, card) in snap.sins.iter().enumerate() {
            if node != 1 {
                assert!(
                    !card.defeated && card.sin.is_empty() && card.lore.is_empty(),
                    "node {node} stays sealed"
                );
            }
        }

        // The mask persists through the roster document: a reloaded client
        // opens the same manuscript.
        let bytes = game.store().save_bytes().expect("document saves");
        let store = RosterStore::load_bytes(&bytes).expect("document loads");
        let mut reloaded = ClientGame::new(store);
        reloaded
            .handle(ClientInput::ViewHagiography)
            .expect("the reloaded menu opens the hagiography");
        assert_eq!(
            reloaded.snapshot().sins,
            snap.sins,
            "the manuscript survives reload unchanged"
        );
    }
}

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
use crusade_core::data::overworld::{self, PropRef};
use crusade_core::engine::trail::{self as trail_engine, SuccessionOutcome, TrailState};
use crusade_core::engine::{Action, Battle, BattleEvent};
use crusade_core::rng::BattleRng;
use crusade_core::types::{
    Ailment, BattleStat, ChampionInstance, DevotionStats, GraceStats, RiteCategory, RiteSlot,
    RiteType, Sin, Typing, Virtue, Volatile, MAX_LEVEL,
};
use crusade_data::RosterStore;
use nostr::Keys;

use crate::conversion as conv;
use crate::persist::{self, TrailSave};
use crate::policy::greedy_action;
use crate::pvp::{record_ladder_result, LADDER_RATING_KEY};
use crate::view::{
    BattleChampionView, CampaignView, ChampionCard, ClientInput, ConversionView, FoeCard,
    LobbyNet, LobbyStage, LobbyView, Phase, RiteCard, SinCard, TestimonyCard, TrailView,
    ViewSnapshot, WitnessCard,
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
/// V12-3 taste (v1.2, docs/HISTORICAL-FOES.md §9): one Road-East adversary
/// rotates in — the Askari Alamdar (116), the rank-and-file line infantryman
/// the company learns to read. No story, no unlocks.
const PRACTICE_FOE_SPECIES: [u16; 3] = [3, 8, 116];
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

/// A yielded convertible adversary awaiting the player's Call decision
/// (CONVERSION.md, M7b). Caller piety/level and the chance are captured at
/// battle end: the caller is the standing MY_SIDE champion (v1
/// simplification — v1 does not attribute the final blow to a striker, so
/// `caller_used_virtue_kill` is always false and the caller is the field's
/// survivor; fervor items are not yet wired, so fervor_bonus is 0).
#[derive(Debug, Clone)]
struct PendingCall {
    species_id: u16,
    foe_level: u8,
    /// The contested-roll chance (3..=85), captured at battle end off
    /// `data::conversion_chance` so the choice screen shows exactly the
    /// odds the sealed draw resolves against.
    chance_pct: u8,
}
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

// ---------- The Pilgrim Road (M8b; docs/OVERWORLD.md sections 2-6) ----------
//
// The trail is a PARALLEL lane beside the seven-sin gauntlet `campaign`:
// same battle driver, same conversion flow, different journey engine. All
// trail randomness derives from the document's pilgrimage seed (minted
// once into `persist::TRAIL_SEED_KEY`, so reloads replay identically) via
// `BattleRng::for_turn(seed, day)` — one rng stream per travel day, the
// engine's own golden-sim discipline.

/// Morale the road grants a won battle (docs/OVERWORLD.md section 3:
/// morale rises on victories); the clamp is the engine's.
const TRAIL_VICTORY_MORALE: i8 = 5;
/// v1 WILD ENCOUNTER SIMPLIFICATION (docs/OVERWORLD.md section 2's
/// per-step roll): the client rolls ONE wild check per travel day on
/// entering an `encounter_zone` screen, not per tile step — the world
/// layer's tile stepping lands with the bevy shell's world pass.
const TRAIL_WILD_ROLL_PCT: u8 = 8;

/// A named man of the van (A3, docs/TRAIL-BATTLE-DEPTH.md section 3): his
/// roster identity (the truth of whether he lives) and his battle body,
/// resolve persisting between road battles.
#[derive(Debug, Clone)]
struct VanguardMan {
    name: String,
    instance: ChampionInstance,
}

/// At most this many named men stand beside the captain on the road.
const VANGUARD_MAX: usize = 2;
/// Vanguardsmen are minted at the captain's level less this (banded up
/// when a risen captain raises the file — a fresh mint joins at the new
/// band; there is no van XP lane in phase 1).
const VANGUARD_LEVEL_OFF: u8 = 2;

/// A live trail campaign: the engine's journey state plus the app-lane
/// battle facts the engine deliberately does not own (M8A review: the
/// roster here is ONE file — the champion's file).
#[derive(Debug, Clone)]
struct TrailCampaign {
    state: TrailState,
    /// The battle-lane party: ONE champion instance bearing the mantle
    /// (v1's single file), resolve persisting between road battles.
    party: Vec<ChampionInstance>,
    /// The battle-body cache of the standing vanguard, keyed by name
    /// (docs/TRAIL-BATTLE-DEPTH.md: the roster remains the truth of who
    /// lives; synced against it at every road-battle entry).
    vanguard: Vec<VanguardMan>,
    /// The current/fallen champion's level (succession banding input).
    champion_level: u8,
    /// The pilgrimage ended (every soul lost or the mantle unclaimed).
    over: bool,
}

impl TrailCampaign {
    /// The man bearing the mantle, if any lives.
    fn champion(&self) -> Option<&trail_engine::ManEntry> {
        self.state.living().find(|m| m.champion)
    }

    /// Sync the van against the roster (A3): dead or promoted men are
    /// struck off (their bodies retire with the old mantle), vacant
    /// places are filled from the living non-champion file, minted at the
    /// captain's level less [`VANGUARD_LEVEL_OFF`] with the Order's own
    /// breed. Mercy is arithmetic: the slots are
    /// `min(VANGUARD_MAX, living non-champion men)` — a depleted file
    /// simply fights short, a lone captain fights alone.
    fn sync_vanguard(&mut self) -> Result<(), AppError> {
        self.vanguard.retain(|v| {
            self.state
                .men
                .iter()
                .any(|m| m.alive && !m.champion && m.name == v.name)
        });
        let species = self
            .party
            .first()
            .map(|c| c.species_id)
            .ok_or_else(|| illegal("the mantle is vacant"))?;
        let level = self
            .champion_level
            .saturating_sub(VANGUARD_LEVEL_OFF)
            .clamp(5, MAX_LEVEL);
        while self.vanguard.len() < VANGUARD_MAX {
            let taken: Vec<&str> = self.vanguard.iter().map(|v| v.name.as_str()).collect();
            let next = self
                .state
                .living()
                .find(|m| !m.champion && !taken.contains(&m.name.as_str()));
            let Some(m) = next else { break };
            self.vanguard.push(VanguardMan {
                name: m.name.clone(),
                instance: practice_champion(species, level)?,
            });
        }
        Ok(())
    }
}

/// Wild-encounter and ambush mixes: per-battle_ref (engine `TrailVerb::
/// Battle` payloads) and per-encounter_zone tables, 2-3 archetype ids from
/// the ADVERSARIES lane, matched loosely to region geography
/// (docs/HISTORICAL-FOES.md section 2). Refs 2xx are ambushes off the
/// trail-event table; zone ids key the per-day wild roll.
fn trail_foe_mix(battle_ref: u16, zone: Option<u8>) -> &'static [u16] {
    match battle_ref {
        // 201 "brigands upon the road" (any region).
        201 => &[112, 113],
        // 203 "Seljuk riders shadowing the column" (Anatolia).
        203 => &[108, 109, 111],
        _ => match zone {
            // 1: the Marches (home wilds; deserters and broken men).
            Some(1) => &[112, 113],
            // 3: Anatolia (the Rum Seljuk band: bowriders, lancers, levy).
            Some(3) => &[108, 109, 111],
            // 4: the Coasts (mixed Zengid garrisons).
            Some(4) => &[111, 114, 116],
            // 5: the Roads of Egypt (Shirkuh's column screens).
            Some(5) => &[113, 110, 114],
            // 6: the Horns of Hattin (the gauntlet's pickets).
            Some(6) => &[114, 116, 117],
            // 7: the Holy Land (Ayyubid field army).
            Some(7) => &[108, 109, 115],
            _ => &[112],
        },
    }
}

/// Gate escort mix by gate `battle_ref` (1xx): the commander's two
/// archetype escorts — the battle of record's banner-guard. Ids per
/// docs/GATE-CONTENT.md section 0 (the map's escort column, verbatim).
fn gate_escort_mix(battle_ref: u16) -> &'static [u16] {
    match battle_ref {
        // 101 Dorylaeum (Kilij Arslan): the Seljuk van.
        101 => &[108, 109],
        // 102 Harim (Nur ad-Din): ghazw riders around the askar's banners.
        102 => &[114, 116],
        // 103 Egypt (Shirkuh): the great-shield foot and the column's core.
        103 => &[110, 116],
        // 104 Hattin, the left horn (Gokbori): tribal Kurdish horse.
        104 => &[116, 114],
        // 105 Hattin, the right horn (Taki al-Din Umar): the tulb officer
        // beneath the wing commander.
        105 => &[116, 117],
        // 108 the King's road (Saladin): the Ayyubid guard.
        108 => &[116, 117],
        _ => &[112],
    }
}

/// The chronicle lines of a gate's battle of record: the pre-battle
/// warning, the victory routing, and the defeat where-phrase (the Book
/// of the Fallen `where_fell` family — the cause line plus where the
/// company broke). Verbatim from docs/GATE-CONTENT.md sections 1-5.
struct GateText {
    pre: &'static str,
    victory: &'static str,
    defeat: &'static str,
}

/// Gate text by `battle_ref` (1xx). Gates 102-108 carry the authored
/// chronicle lines; gate 101 (Kilij Arslan) shipped before the content
/// doc and keeps its shipped lines, so it reads as `None` and the
/// generic road text stands.
fn gate_text(battle_ref: u16) -> Option<&'static GateText> {
    Some(match battle_ref {
        // 102 Harim — Nur ad-Din (screen 13).
        102 => &GateText {
            pre: "The ridge holds against you: spears in good order, the atabeg's banner above them. Nur ad-Din Mahmud ibn Zengi has ridden out to Harim's ground, and the road will not open while he stands it.",
            victory: "The askar breaks in good order and withdraws toward Damascus; we count our dead and give thanks. The chroniclers of both hosts call his justice without reproach; the road is open, and a worthy man rides from it unharmed.",
            defeat: "fell in battle upon the pilgrim road, at the Harim marches before the atabeg Nur ad-Din",
        },
        // 103 Egypt roads — Asad al-Din Shirkuh (screen 16).
        103 => &GateText {
            pre: "A column upon the march, dust to the horizon - it has outwalked every rumor of itself. Asad al-Din Shirkuh, thrice the master of Egypt's roads, turns his host across yours.",
            victory: "His tired men scatter into the sands and he leads them without looking back; the road is ours, though the march has cost us dear. Within the year the chronicle records his death in Egypt - in the very seat he won - and the vizierate passing to his nephew Yusuf, called Salah al-Din.",
            defeat: "fell in battle upon the pilgrim road, upon the roads of Egypt before Asad al-Din Shirkuh",
        },
        // 104 Hattin, the left horn — Gokbori (screen 19).
        104 => &GateText {
            pre: "Under the waterless hills the King's road narrows, and horsemen dark as the ridge-line wait where it turns. Muzaffar al-Din Gokbori holds the left horn. There is no way around.",
            victory: "The horn is turned; Gokbori draws his riders off the hill in good order, his banner still uncut, and the company gasps on toward the springs. History remembers him at Hattin a victor; here the road remembers an honorable man outfought, and let go.",
            defeat: "fell in battle upon the pilgrim road, beneath the Horns of Hattin before Muzaffar al-Din Gokbori",
        },
        // 105 Hattin, the right horn — Taqi al-Din Umar (screen 20).
        105 => &GateText {
            pre: "The road turns once more, and the horn turns with it: a wall of lances under a master's eye. Taqi al-Din Umar holds the right hand of Hattin's field. None pass the Horns who will not fight both.",
            victory: "The second horn gives way at the ninth hour; the nephew withdraws to his uncle's command, fighting as he goes. The company passes between the Horns with empty water-skins and whole souls, and does not boast.",
            defeat: "fell in battle upon the pilgrim road, between the Horns of Hattin before Taqi al-Din Umar",
        },
        // 108 the King's road — Saladin (screen 25, the finale).
        108 => &GateText {
            pre: "At the last ridge the whole host stands, and above it a man the Latin chronicles themselves honor: Salah al-Din Yusuf ibn Ayyub, uniter of Egypt and Syria, between you and the City. He keeps his safe-conducts and he keeps his word; he will not give you the road. Kneel; pray; then rise and take the field.",
            victory: "The host of Egypt and Syria yields the field, and its lord withdraws with his banners unbroken and his honor entire, as the chronicles of our own side confess he ever kept it. The road lies open to the City. Let no man boast over him in the telling: he was the worthiest foe this war produced, and the chronicler who is honest sets his virtue beside our own men's, that the rest of the story may be believed.",
            defeat: "fell in battle upon the pilgrim road, upon the King's road before Salah al-Din Yusuf ibn Ayyub",
        },
        _ => return None,
    })
}

/// The defeat where-phrase for a gate screen: the map's own words for
/// where the company broke, keyed through the screen's gate. Only the
/// battle-dead of a gate screen read it; every other page keeps the
/// shipped screen-and-cause row.
fn gate_defeat_phrase(screen: u16) -> Option<&'static str> {
    let gate = overworld::screen_by_id(screen)?.scripted_gate?;
    gate_text(gate.battle_ref).map(|t| t.defeat)
}

/// Mint-or-load the document's pilgrimage seed: deterministic bytes mixed
/// (FNV-1a -> splitmix64) from the document's persisted salts — the codex
/// masks, the calls tally, and the roster ids — then persisted under
/// `persist::TRAIL_SEED_KEY` at first embark so a mid-journey reload
/// replays the same per-day streams (conversion.rs's store-derived-salt
/// discipline).
fn pilgrim_seed(store: &RosterStore) -> Result<[u8; 32], AppError> {
    if let Some(hex_seed) = store.setting(persist::TRAIL_SEED_KEY)? {
        let bytes = hex::decode(&hex_seed).map_err(|e| illegal(format!("trail seed: {e}")))?;
        if let Ok(arr) = <[u8; 32]>::try_from(bytes.as_slice()) {
            return Ok(arr);
        }
    }
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    fn mix(h: &mut u64, s: &str) {
        for b in s.as_bytes() {
            *h ^= u64::from(*b);
            *h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    for key in [
        SINS_DEFEATED_KEY,
        conv::FOES_DEFEATED_KEY,
        conv::CONVERTS_BAPTIZED_KEY,
        conv::CALLS_ISSUED_KEY,
    ] {
        mix(&mut h, &store.setting(key)?.unwrap_or_default());
    }
    let mut roster = store.list_champions()?;
    roster.sort_by(|a, b| a.0.cmp(&b.0));
    for (id, c) in &roster {
        mix(&mut h, id);
        h ^= u64::from(c.species_id);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    // Stir in a per-document nonce so two identical fresh rosters do not
    // walk the same road: stored settings count at mint time plays that
    // role via the calls-tally read above; here splitmix into 32 bytes.
    let mut seed = [0u8; 32];
    let mut x = h.wrapping_add(0x9E37_79B9_7F4A_7C15);
    for chunk in seed.chunks_mut(8) {
        x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        x ^= x >> 31;
        chunk.copy_from_slice(&x.to_le_bytes());
    }
    Ok(seed)
}

/// The rng stream for one travel day of a pilgrimage.
fn trail_day_rng(seed: [u8; 32], day: u16) -> BattleRng {
    BattleRng::for_turn(seed, u32::from(day) + 1)
}

/// The seed for a road battle: the pilgrimage seed day-keyed and
/// domain-separated from the day streams, so battle rolls never alias a
/// travel day's rolls.
fn trail_battle_seed(seed: [u8; 32], day: u16) -> [u8; 32] {
    let mut s = seed;
    s[0] ^= 0xB8;
    s[2..4].copy_from_slice(&day.to_le_bytes());
    s
}

#[derive(Debug)]
pub struct ClientGame {
    store: RosterStore,
    battle: Option<Battle>,
    phase: Phase,
    /// Active gauntlet run, if any.
    campaign: Option<CampaignState>,
    /// Active Pilgrim Road campaign, if any (M8b; restored from the
    /// document's `trail.save` at construction). Parallel to the sin
    /// gauntlet: the lanes never share a live battle.
    trail: Option<TrailCampaign>,
    /// True while `battle` belongs to the trail lane (gate, ambush, wild,
    /// or choice encounter) — routes `on_battle_end` and the conversion
    /// queue back to the road instead of the gauntlet/practice plumbing.
    trail_battle: bool,
    /// The `battle_ref` of the live road battle while it is a gate's
    /// battle of record (None for wilds, ambushes, and elective
    /// engagements) — routes the gate's own chronicle lines at battle
    /// end. Not persisted: it lives at most one battle, the same
    /// lifetime discipline as `battle` itself.
    trail_gate: Option<u16>,
    /// A camp's mend awaiting its night-ambush battle (RECOVERY.md: a
    /// won night fight earns the balms at dawn; a lost one is an ordinary
    /// road defeat and the mend dies with it). Resolve-after value; the
    /// ailment lift rides along. Same one-battle lifetime as
    /// `trail_gate`, never persisted.
    camp_mend: Option<u16>,
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
    /// Yielded convertible adversaries awaiting a Call decision, head
    /// first; non-empty exactly while [`Phase::ConversionChoice`] is up.
    /// Not persisted — the queue lives at most one frame-pair past its
    /// battle (same lifetime discipline as `battle`).
    pending_calls: Vec<PendingCall>,
    /// Raw events since the last snapshot. `RefCell` so `snapshot(&self)`
    /// can drain them (take-drain semantics for animation queues).
    fresh_events: RefCell<Vec<BattleEvent>>,
}

impl ClientGame {
    pub fn new(store: RosterStore) -> Self {
        // A persisted mid-journey pilgrimage rejoins the road exactly
        // where the document left it (M8b save/load).
        let trail = persist::load_trail(&store).ok().flatten().map(|save| {
            TrailCampaign {
                state: save.to_state(),
                party: save.party.clone(),
                vanguard: save
                    .vanguard
                    .iter()
                    .map(|v| VanguardMan {
                        name: v.name.clone(),
                        instance: v.instance.clone(),
                    })
                    .collect(),
                champion_level: save.champion_level,
                over: save.over,
            }
        });
        Self {
            store,
            battle: None,
            phase: if trail.is_some() { Phase::Trail } else { Phase::Menu },
            campaign: None,
            trail,
            trail_battle: false,
            trail_gate: None,
            camp_mend: None,
            lobby: None,
            nostr_keys: None,
            log: Vec::new(),
            pending_calls: Vec::new(),
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
            // Two more codex books on the same phase gate: the Roll of
            // Worthy Adversaries (foes_defeated mask) and the Testimonies
            // of the Baptized (converts_baptized mask), both u32-keyed.
            foes: if self.phase == Phase::Hagiography {
                foe_cards(self.foes_defeated_mask())
            } else {
                Vec::new()
            },
            testimonies: if self.phase == Phase::Hagiography {
                testimony_cards(self.converts_baptized_mask())
            } else {
                Vec::new()
            },
            // The live Call rides only while its choice is pending.
            conversion: if self.phase == Phase::ConversionChoice {
                self.pending_calls.first().map(|pc| ConversionView {
                    species_id: pc.species_id,
                    species_name: data::species_by_id(pc.species_id)
                        .map_or_else(|| format!("Adversary #{}", pc.species_id), |s| {
                            s.name.to_string()
                        }),
                    foe_level: pc.foe_level,
                    chance_pct: pc.chance_pct,
                    pending: self.pending_calls.len() as u8,
                })
            } else {
                None
            },
            // The Pilgrim Road rides along whenever a trail campaign is on
            // the document (one small row — the road screen + the Book's
            // summary), restored trails included.
            trail: self.trail.as_ref().map(trail_view),
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
                    // A baptized convert reads Humility in the gallery
                    // (CONVERSION 4's typing shift, v1 collapses "the
                    // virtue that felled him" to Humility). Display-level
                    // only: typing lives on the static species table, so
                    // battle mechanics stay Valor until crusade-core grows
                    // an instance typing field.
                    types_display: if c.converted {
                        vec!["Humility".to_string()]
                    } else {
                        sp.map_or_else(Vec::new, |s| typing_display(s.typing))
                    },
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

    /// The persisted adversary defeat mask (`campaign.foes_defeated`, u32 —
    /// 14 foe ids exceed a u8; HF 6.5). Bit packing: [`conv::foe_bit`].
    /// Absent/unparseable degrades to 0 — a sealed Roll — never an error,
    /// mirroring [`Self::sins_defeated_mask`].
    fn foes_defeated_mask(&self) -> u32 {
        self.store
            .setting(conv::FOES_DEFEATED_KEY)
            .ok()
            .flatten()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0)
    }

    /// The persisted baptism mask (`campaign.converts_baptized`, u32; bits
    /// 0..=9 only). Same degrade-to-sealed discipline.
    fn converts_baptized_mask(&self) -> u32 {
        self.store
            .setting(conv::CONVERTS_BAPTIZED_KEY)
            .ok()
            .flatten()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0)
    }

    /// A convert's baptismal name as persisted under
    /// `roster.baptismal.<id>`; absent = unbaptized/unrenamed.
    pub fn baptismal_name_of(&self, roster_id: &str) -> Option<String> {
        self.store
            .setting(&format!("{}{roster_id}", conv::BAPTISMAL_KEY_PREFIX))
            .ok()
            .flatten()
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
                    self.pending_calls.clear();
                    self.fresh_events.borrow_mut().clear();
                    self.phase = Phase::Menu;
                    Ok(())
                }
                p => Err(illegal(format!("no result to acknowledge from {p:?}"))),
            },
            ClientInput::CallToRepentance => match self.phase {
                Phase::ConversionChoice => self.resolve_call(true),
                p => Err(illegal(format!("no yielded foe to call from {p:?}"))),
            },
            ClientInput::ReleaseSoul => match self.phase {
                Phase::ConversionChoice => self.resolve_call(false),
                p => Err(illegal(format!("no yielded foe to release from {p:?}"))),
            },
            ClientInput::StartTrail {
                charter_quality,
                provisioning_spend,
            } => self.start_trail(charter_quality, provisioning_spend),
            ClientInput::Travel { to_screen } => match self.phase {
                Phase::Trail => self.travel(to_screen),
                p => Err(illegal(format!("cannot march from {p:?}"))),
            },
            ClientInput::EngageChoice { encounter_ref } => match self.phase {
                Phase::Trail => self.engage_choice_encounter(encounter_ref),
                p => Err(illegal(format!("no encounter to engage from {p:?}"))),
            },
            ClientInput::MakeCamp => match self.phase {
                Phase::Trail => self.trail_make_camp(),
                p => Err(illegal(format!("no camp to pitch from {p:?}"))),
            },
            ClientInput::AbandonTrail => match self.phase {
                Phase::Trail => {
                    persist::clear_trail(&mut self.store)?;
                    self.trail = None;
                    self.push_log(vec![
                        "The company turns back; the pilgrim road is left unwalked.".to_string()
                    ]);
                    self.phase = Phase::Menu;
                    Ok(())
                }
                p => Err(illegal(format!("no pilgrimage to abandon from {p:?}"))),
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

    // ---------- The Pilgrim Road (M8b) ----------

    /// StartTrail: resolve the Crossing (charter quality + provisioning
    /// spend are the UI's only dials, OVERWORLD 6.4), land the company,
    /// name the mantle-bearer, and open the road at the far shore. The
    /// roster's lead champion template supplies the mantle's first battle
    /// instance; the store is never mutated by road battles.
    fn start_trail(&mut self, charter_quality: u8, provisioning_spend: u16) -> Result<(), AppError> {
        if !matches!(self.phase, Phase::Menu | Phase::Trail)
            || self.battle.is_some()
            || self.campaign.is_some()
        {
            return Err(illegal("a battle or campaign is already active"));
        }
        if let Some(t) = &self.trail {
            if !t.over {
                return Err(illegal("a pilgrimage is already underway"));
            }
        }
        let mut roster = self.store.list_champions()?;
        if roster.is_empty() {
            return Err(illegal("cannot take the pilgrim road with an empty roster"));
        }
        roster.sort_by(|a, b| a.0.cmp(&b.0));
        let template = roster[0].1.clone();
        let champion_level = template.level.clamp(1, MAX_LEVEL);

        let seed = pilgrim_seed(&self.store)?;
        let mut crossing_rng = trail_day_rng(seed, 0);
        let out = trail_engine::the_crossing(charter_quality, provisioning_spend, &mut crossing_rng);
        let mut state = out.state;
        // The mantle lands on the hardiest of the landing party (the
        // engine leaves the champion flag to the caller, M8A issue 6).
        if let Some(m) = state.living_mut().next() {
            m.champion = true;
        }
        let party = vec![field_reset(template, champion_level)?];
        let t = TrailCampaign {
            champion_level,
            party,
            vanguard: Vec::new(),
            state,
            over: false,
        };
        let mut lines = vec![format!(
            "The Crossing is behind you. {} soul(s) went down into the sea; the company lands with {} supplies and a {} heart. Day {} dawns on the far shore.",
            out.men_lost_at_sea, out.supplies_carried,
            if out.morale_scar < 0 { "chastened" } else { "whole" },
            t.state.day,
        )];
        if let Some(c) = t.champion() {
            lines.push(format!("{} takes up the mantle of command.", c.name));
        }
        self.push_log(lines);
        self.trail = Some(t);
        self.store
            .set_setting(persist::TRAIL_SEED_KEY, &hex::encode(seed))?;
        self.save_trail()?;
        self.phase = Phase::Trail;
        Ok(())
    }

    /// Persist the live trail row into the document (one settings write;
    /// every road-mutating transition ends here, same sanctioned-write
    /// discipline as the shard bounties).
    fn save_trail(&mut self) -> Result<(), AppError> {
        let Some(t) = &self.trail else {
            return Ok(());
        };
        let save = TrailSave::capture(
            &t.state,
            &t.party,
            t.vanguard
                .iter()
                .map(|v| persist::VanguardSave {
                    name: v.name.clone(),
                    instance: v.instance.clone(),
                })
                .collect(),
            t.champion_level,
            t.over,
        )
        .map_err(illegal)?;
        persist::save_trail(&mut self.store, &save)?;
        Ok(())
    }

    /// The document's pilgrimage seed (minted at embark, kept forever).
    fn pilgrim_seed(&self) -> Result<[u8; 32], AppError> {
        pilgrim_seed(&self.store)
    }

    /// Travel: march one day to an adjacent screen. Upkeep, the day's
    /// trail event, gate funnels, and the per-day wild roll all resolve
    /// here; battles and yielded-hearer Calls route into their phases.
    fn travel(&mut self, to_screen: u16) -> Result<(), AppError> {
        if self.trail.as_ref().is_some_and(|t| t.over) {
            return Err(illegal("the pilgrimage is over"));
        }
        let seed = self.pilgrim_seed()?;
        let day = self
            .trail
            .as_ref()
            .ok_or_else(|| illegal("no pilgrimage is underway"))?
            .state
            .day;
        let mut rng = trail_day_rng(seed, day);

        /// What the day leaves behind; routed after the state's borrow ends.
        enum FollowUp {
            Convert(u16),
            Gate(u16, u16, u8),     // battle_ref, commander id, boss level offset
            Ambush(u16, Option<u8>), // battle_ref, encounter zone
            Wild(u8),                // encounter zone
            Quiet,
        }
        let (lines, follow) = {
            let t = self.trail.as_mut().expect("trail checked above");
            let out = trail_engine::travel_day(&mut t.state, to_screen, &mut rng)
                .map_err(|e| illegal(e.to_string()))?;
            let mut lines = vec![format!(
                "Day {}: the company marches to screen {} (supplies {}, morale {}).",
                out.day, out.screen, out.supplies, out.morale
            )];
            for name in &out.men_lost {
                lines.push(format!("{name} is lost; the Book of the Fallen gains a page."));
            }
            let dest =
                overworld::screen_by_id(to_screen).expect("travel_day validated the screen");
            // Order of the road: the funnel gate is never missed — a gate
            // preempts a day's willing hearer (owner ruling 2026-10-17);
            // the Call still queues and is honored when the gate is
            // settled (victory joins the road; the queue then surfaces).
            let follow = if let Some(gate) = dest.scripted_gate {
                if let Some(c) = out.convert_ref {
                    self.queue_trail_call(c)?;
                }
                FollowUp::Gate(
                    gate.battle_ref,
                    gate.commander_id.expect("gate commanders are named adversaries"),
                    gate.boss_level_offset,
                )
            } else if let Some(c) = out.convert_ref {
                FollowUp::Convert(c)
            } else if let Some(b) = out.battle_ref {
                FollowUp::Ambush(b, dest.encounter_zone)
            } else if let Some(zone) = dest.encounter_zone {
                // v1 wild roll: ONE roll per travel day on an
                // encounter-zone screen (not per tile step; see
                // TRAIL_WILD_ROLL_PCT).
                if rng.pct() < TRAIL_WILD_ROLL_PCT {
                    FollowUp::Wild(zone)
                } else {
                    FollowUp::Quiet
                }
            } else {
                FollowUp::Quiet
            };
            (lines, follow)
        };
        self.push_log(lines);
        match follow {
            FollowUp::Quiet => self.save_trail(),
            FollowUp::Convert(c) => {
                // A willing hearer on the road: the Call decision is
                // queued exactly like a post-battle yield (M7b flow).
                self.queue_trail_call(c)?;
                self.save_trail()?;
                self.phase = Phase::ConversionChoice;
                Ok(())
            }
            FollowUp::Gate(battle_ref, commander, boss_offset) => {
                // Scripted funnel (OVERWORLD section 2): the battle of
                // record — one boss commander with two archetype escorts;
                // no skipping. The commander stands `boss_offset` above
                // the mantle (the map's data, not a hardcoded +2);
                // escorts keep the shipped `lv - 2` clamp.
                let lv = self.trail.as_ref().expect("trail").champion_level;
                let boss_lv = lv.saturating_add(boss_offset).min(MAX_LEVEL);
                let escort_lv = lv.saturating_sub(2).clamp(5, MAX_LEVEL);
                let mut foes = Vec::with_capacity(3);
                for id in gate_escort_mix(battle_ref).iter().take(2) {
                    foes.push(practice_champion(*id, escort_lv)?);
                }
                foes.push(practice_champion(commander, boss_lv)?);
                let name = data::species_by_id(commander)
                    .map(|s| s.name.to_string())
                    .expect("gate commanders are named adversaries");
                // Authored gates open with their own chronicle warning
                // (docs/GATE-CONTENT.md); the shipped Kilij gate keeps
                // its line.
                let intro = gate_text(battle_ref).map_or_else(
                    || format!("{name} bars the road. There is no way around."),
                    |t| t.pre.to_string(),
                );
                self.trail_gate = Some(battle_ref);
                self.save_trail()?;
                self.enter_trail_battle(foes, intro)
            }
            FollowUp::Ambush(battle_ref, zone) => {
                let mix = trail_foe_mix(battle_ref, zone);
                let foes = self.build_trail_foes(mix)?;
                self.save_trail()?;
                self.enter_trail_battle(foes, "Ambush upon the road!".to_string())
            }
            FollowUp::Wild(zone) => {
                let mix = trail_foe_mix(0, Some(zone));
                let foes = self.build_trail_foes(mix)?;
                self.save_trail()?;
                self.enter_trail_battle(foes, "Foes rise from the wilds!".to_string())
            }
        }
    }

    /// Build a wild/ambush foe party from a mix table at mantle level.
    fn build_trail_foes(&self, mix: &[u16]) -> Result<Vec<ChampionInstance>, AppError> {
        let level = self
            .trail
            .as_ref()
            .map(|t| t.champion_level.clamp(5, MAX_LEVEL))
            .unwrap_or(5);
        mix.iter().map(|&id| practice_champion(id, level)).collect()
    }

    /// Queue a road-side Call to Repentance (trail ConvertOpportunity or a
    /// won road battle): the choice is a plain `pending_calls` entry; the
    /// M7b `resolve_call` flow owns the roll.
    fn queue_trail_call(&mut self, adversary_id: u16) -> Result<(), AppError> {
        let t = self.trail.as_ref().expect("trail checked above");
        let caller = t.party.first().ok_or_else(|| illegal("the mantle is vacant"))?;
        let caller_sp = data::species_by_id(caller.species_id)
            .ok_or_else(|| illegal(format!("unknown species {}", caller.species_id)))?;
        let piety = data::compute_stats(caller, caller_sp).piety;
        let sp = data::species_by_id(adversary_id)
            .ok_or_else(|| illegal(format!("unknown adversary {adversary_id}")))?;
        let champion_level = t.champion_level;
        let chance = data::conversion_chance(
            piety,
            champion_level,
            false, // final-blow virtue attribution: not tracked in v1
            0,     // fervor items: not wired in v1
            sp,
            champion_level,
        )
        .expect("convertible ids carry obstinacy (3..=85)");
        let foe_name = sp.name.to_string();
        self.push_log(vec![format!(
            "A {foe_name} heard the Word gladly by the way; the company may call him to repentance."
        )]);
        self.pending_calls.push(PendingCall {
            species_id: adversary_id,
            foe_level: champion_level,
            chance_pct: chance,
        });
        Ok(())
    }

    /// Enter a road battle (gate funnel, ambush, wild roll, or elective
    /// choice encounter): the mantle's file vs `foes`, battle rolls
    /// day-keyed off the pilgrimage seed. Under A3 (docs/TRAIL-BATTLE-
    /// DEPTH.md section 3) ROAD battles field the mantle plus the standing
    /// vanguard synced against the roster; the GATES of record (trail_gate
    /// set) remain the captain's own fight, single file, exactly as
    /// shipped.
    fn enter_trail_battle(&mut self, foes: Vec<ChampionInstance>, intro: String) -> Result<(), AppError> {
        let seed = self.pilgrim_seed()?;
        let gate = self.trail_gate.is_some();
        let t = self.trail.as_mut().ok_or_else(|| illegal("no pilgrimage is underway"))?;
        let bseed = trail_battle_seed(seed, t.state.day);
        let mut party = t.party.clone();
        if !gate {
            t.sync_vanguard()?;
            for v in &t.vanguard {
                party.push(v.instance.clone());
            }
        }
        // The trail battle intro names who stands (A3 view line). The
        // engine renders the battle screen's party row itself; this is
        // the only new ledger text the muster needs.
        let muster = if gate {
            None
        } else if t.vanguard.is_empty() {
            Some("The captain marches on with no man beside him.".to_string())
        } else {
            let van: Vec<&str> = t.vanguard.iter().map(|v| v.name.as_str()).collect();
            let captain = t.champion().map(|m| m.name.clone()).unwrap_or_default();
            Some(match van.as_slice() {
                [one] => format!(
                    "{captain} bears the mantle; with him stands {one}."
                ),
                [one, two] => format!(
                    "{captain} bears the mantle; with him stand {one} and {two}."
                ),
                _ => format!("{captain} bears the mantle; the van stands with him."),
            })
        };
        let battle = Battle::new(party, foes, bseed)?;
        self.battle = Some(battle);
        self.trail_battle = true;
        self.phase = Phase::InBattle;
        self.fresh_events.borrow_mut().clear();
        let mut lines = vec![intro];
        if let Some(line) = muster {
            lines.push(line);
        }
        self.push_log(lines);
        Ok(())
    }

    /// Make camp at a Camp prop screen (docs/RECOVERY.md section 5,
    /// Candidate A): a day passes at double upkeep — the men eat double
    /// rations and the wounded are shriven; nature's table is never
    /// rolled for a halted company; morale is steadied. The mantle is
    /// mended toward the 3/4 field-dressing ceiling (mend applied now on
    /// a quiet night, or at dawn after a KEPT night watch: the 10% camp
    /// ambush is fought from the unmended stance, and a lost camp battle
    /// is an ordinary road defeat). One sanctioned write at every exit,
    /// same discipline as `travel`.
    fn trail_make_camp(&mut self) -> Result<(), AppError> {
        let seed = self.pilgrim_seed()?;
        let t = self.trail.as_ref().ok_or_else(|| illegal("no pilgrimage is underway"))?;
        if t.over {
            return Err(illegal("the pilgrimage is over"));
        }
        let screen = overworld::screen_by_id(t.state.screen)
            .ok_or_else(|| illegal(format!("unknown screen {}", t.state.screen)))?;
        let has_camp = screen.props.iter().any(|p| matches!(p, PropRef::Camp));
        if !has_camp {
            return Err(illegal("no camp ground on this screen"));
        }
        // The day stream keyed on the PRE-increment day, mirroring
        // travel's discipline (a camp and a march on "day N" never alias).
        let day = t.state.day;
        let zone = screen.encounter_zone;
        let mantle = t.party.first().ok_or_else(|| illegal("the mantle is vacant"))?;
        let sp = data::species_by_id(mantle.species_id)
            .ok_or_else(|| illegal(format!("unknown species {}", mantle.species_id)))?;
        let max_resolve = data::compute_stats(mantle, sp).max_resolve;
        let resolve_before = mantle.current_resolve;
        let champion_name = t.champion().map(|c| c.name.clone()).unwrap_or_default();

        // The stores must bear the halt's double feed before the verb
        // stands at all.
        let cost = trail_engine::daily_supply_cost(t.state.living_count()) * 2;
        if t.state.supplies < cost {
            self.push_log(vec![
                "The stores are too lean for a halt; the company must march or starve."
                    .to_string(),
            ]);
            return Ok(());
        }

        let mut rng = trail_day_rng(seed, day);
        let t = self.trail.as_mut().expect("trail checked above");
        let out = trail_engine::make_camp(&mut t.state, &mut rng, max_resolve, resolve_before);

        if out.ambush {
            // The watch cries: fought from the unmended stance; the mend
            // rides the battle and is paid only at a kept dawn.
            let mix = match zone {
                Some(z) => trail_foe_mix(0, Some(z)),
                None => trail_foe_mix(201, None),
            };
            self.camp_mend = Some(out.resolve_after);
            self.save_trail()?;
            let foes = self.build_trail_foes(mix)?;
            self.enter_trail_battle(
                foes,
                "A cry from the watch — the camp is fallen upon in the dark!".to_string(),
            )?;
            return Ok(());
        }

        // A quiet night: the balms go on.
        let line = if out.resolve_after == resolve_before {
            format!(
                "The company makes camp. The watches are posted; the office is sung. Day {} passes quietly.",
                out.day
            )
        } else {
            let sp_cap = max_resolve * trail_engine::CAMP_RESOLVE_CAP_NUM
                / trail_engine::CAMP_RESOLVE_CAP_DEN;
            if out.resolve_after == sp_cap && out.resolve_after < max_resolve {
                format!(
                    "The company makes camp. The balms have done what field-dressing can; the rest must knit of itself. ({}/{} — no more may be mended on the road.)",
                    out.resolve_after, max_resolve
                )
            } else {
                format!(
                    "The company makes camp. The wounds are washed and bound; the office is sung. {} rises steadier than he lay down (resolve {} -> {}/{}).",
                    champion_name, resolve_before, out.resolve_after, max_resolve
                )
            }
        };
        {
            let t = self.trail.as_mut().expect("trail checked above");
            if let Some(m) = t.party.first_mut() {
                m.current_resolve = out.resolve_after;
                m.ailment = None;
            }
        }
        self.push_log(vec![line]);
        self.save_trail()?;
        Ok(())
    }

    /// EngageChoice: an elective prop encounter on the current screen
    /// (walk up and engage; OVERWORLD section 2's choice species).
    fn engage_choice_encounter(&mut self, encounter_ref: u16) -> Result<(), AppError> {
        let t = self
            .trail
            .as_ref()
            .ok_or_else(|| illegal("no pilgrimage is underway"))?;
        if t.over {
            return Err(illegal("the pilgrimage is over"));
        }
        let screen = overworld::screen_by_id(t.state.screen)
            .ok_or_else(|| illegal(format!("unknown screen {}", t.state.screen)))?;
        let on_screen = screen
            .props
            .iter()
            .any(|p| matches!(p, PropRef::ChoiceEncounter(r) if *r == encounter_ref));
        if !on_screen {
            return Err(illegal(format!(
                "no choice encounter {encounter_ref} on this screen"
            )));
        }
        let mix = trail_foe_mix(0, screen.encounter_zone);
        let foes = self.build_trail_foes(mix)?;
        self.enter_trail_battle(foes, format!("The company engages the camp (encounter {encounter_ref})."))
    }

    /// Route a finished ROAD battle (the trail lane's end-of-battle card).
    /// Victory: the yielded foes are scanned for the Roll and any Calls
    /// (the M7b flow rides unchanged), the men take heart, the survivors
    /// march on UNRESTED (the road does not pause). Defeat: the mantle
    /// falls — the champion's own page is written to the Book, then
    /// `champion_falls` resolves THE MANTLE over the file: a rising in
    /// righteousness promotes a named man into a new mantle instance
    /// (level band former-5..+2, day-seeded), a reordering bleeds the file
    /// and fields a lesser captain at former-5. When no soul remains, the
    /// pilgrimage is over.
    fn on_trail_battle_end(&mut self, winner: u8) -> Result<(), AppError> {
        self.trail_battle = false;
        let gate_ref = self.trail_gate.take();
        let camp_mend = self.camp_mend.take();
        let won = winner == MY_SIDE;
        if won {
            self.scan_defeated_foes()?;
        }

        // Read the WHOLE post-battle line: the felled stand present at 0
        // resolve so the van's dead and a fallen mantle can be paged
        // (A3). nb: the battle party is [mantle, van-in-entry-order];
        // forced switches never reorder the party array.
        let post: Vec<ChampionInstance> = {
            let b = self.battle.as_ref().expect("a battle just ended");
            b.sides[usize::from(MY_SIDE)]
                .party
                .iter()
                .cloned()
                .map(|mut c| {
                    // Survivors march on UNRESTED (resolve persists per
                    // the journey's attrition design), but their zeal is
                    // refilled: battle entry validates full-zeal rites
                    // (a field-fix, no rest).
                    if c.current_resolve > 0 {
                        for slot in c.rites.iter_mut().flatten() {
                            if let Some(r) = data::rite_by_id(slot.rite_id) {
                                slot.zeal_left = r.zeal;
                            }
                        }
                    }
                    c
                })
                .collect()
        };
        let mantle_stands = post.first().is_some_and(|c| c.current_resolve > 0);
        let mut lines: Vec<String> = Vec::new();

        {
            let t = self.trail.as_mut().expect("trail battle implies trail");
            // Settle the van first (docs/TRAIL-BATTLE-DEPTH.md section 3:
            // a vanguardsman at 0 resolve after the battle — victory or
            // defeat — died where the line stood: his page is written,
            // the roster marked).
            let mut van_lines: Vec<String> = Vec::new();
            for (i, v) in t.vanguard.iter_mut().enumerate() {
                let Some(body) = post.get(i + 1) else {
                    continue;
                };
                if body.current_resolve > 0 {
                    v.instance = body.clone();
                } else {
                    if let Some(m) = t.state.men.iter_mut().find(|m| m.alive && m.name == v.name) {
                        m.alive = false;
                    }
                    let name = v.name.clone();
                    t.state.fallen.push(trail_engine::FallenEntry {
                        name: name.clone(),
                        day: t.state.day,
                        cause: persist::FallenCause::SlainInTheVan.as_str(),
                        where_fell: t.state.screen,
                    });
                    van_lines.push(if won {
                        format!("The van held, and {name} did not rise with it.")
                    } else {
                        format!(
                            "{name} is slain beside the captain; the Book of the Fallen gains a page."
                        )
                    });
                }
            }
            // Dead men's bodies retire with them (the next sync's retain
            // strikes them off; do it now so the saved van holds only the
            // standing).
            t.vanguard.retain(|v| {
                t.state
                    .men
                    .iter()
                    .any(|m| m.alive && !m.champion && m.name == v.name)
            });
            lines.extend(van_lines);

            if mantle_stands {
                // The mantle carries his wounds forward, zeal field-fixed.
                if let (Some(slot), Some(body)) = (t.party.first_mut(), post.first()) {
                    *slot = body.clone();
                }
            }
            if won {
                t.state.morale =
                    (t.state.morale + TRAIL_VICTORY_MORALE).min(trail_engine::MORALE_MAX);
                // A gate's battle of record closes with its own routing
                // line (docs/GATE-CONTENT.md); the wild road keeps the
                // shipped one.
                lines.push(gate_ref.and_then(gate_text).map_or_else(
                    || "The road is won; the men take heart and march on.".to_string(),
                    |t| t.victory.to_string(),
                ));
                // A camp's mend rides on a KEPT night watch by a
                // STANDING mantle (RECOVERY.md: balms go on after the
                // fight, at victory routing; a captain felled in the
                // dark goes to the succession path and no dawn line).
                if let (true, Some(resolve_after)) = (mantle_stands, camp_mend) {
                    if let Some(m) = t.party.first_mut() {
                        m.current_resolve = resolve_after;
                        m.ailment = None;
                    }
                    lines.push(
                        "The night is kept. By dawn the wounded are tended.".to_string()
                    );
                }
            }
        }
        self.push_log(lines);

        if won && mantle_stands {
            self.battle = None;
            self.phase = if self.pending_calls.is_empty() {
                Phase::Trail
            } else {
                Phase::ConversionChoice
            };
            self.save_trail()?;
            return Ok(());
        }
        // The mantle fell where the line stood — won field or lost, the
        // captain's page is written and THE MANTLE answers.

        // Defeat: the mantle falls where the company broke.
        let seed = self.pilgrim_seed()?;
        let former_level = self
            .trail
            .as_ref()
            .map(|t| t.champion_level)
            .unwrap_or(1);
        let former_species = self
            .battle
            .as_ref()
            .map(|b| b.sides[usize::from(MY_SIDE)].party[0].species_id)
            .unwrap_or(1);
        self.battle = None;

        /// What the succession left to assemble after the borrow ends.
        enum Mantle {
            Promoted { level: u8 },
            /// The run is over.
            Over,
            /// A lesser captain of the remnant at this level.
            ReorderedContinue { level: u8 },
        }
        let (lines, mantle) = {
            let t = self.trail.as_mut().expect("trail battle implies trail");
            let mut lines: Vec<String> = Vec::new();
            // The champion's own Book page — the engine's mantle note
            // deliberately does not write it (M8A review issue 6).
            if let Some(ch) = t.state.men.iter_mut().find(|m| m.alive && m.champion) {
                ch.alive = false;
                ch.champion = false;
                let name = ch.name.clone();
                t.state.fallen.push(trail_engine::FallenEntry {
                    name: name.clone(),
                    day: t.state.day,
                    cause: persist::FallenCause::SlainInBattle.as_str(),
                    where_fell: t.state.screen,
                });
                lines.push(format!(
                    "{name} is slain where the company broke. The Book of the Fallen gains a page."
                ));
            }
            let mut rng = trail_day_rng(seed, t.state.day);
            let mantle = match trail_engine::champion_falls(&mut t.state, &mut rng) {
                SuccessionOutcome::RisesInRighteousness(name) => {
                    let level = former_level
                        .saturating_sub(5)
                        .saturating_add(rng.range_u8(0, 7))
                        .clamp(1, MAX_LEVEL);
                    t.champion_level = level;
                    lines.push(format!(
                        "{name} rises in righteousness and takes up the mantle (level {level}). The company marches on."
                    ));
                    Mantle::Promoted { level }
                }
                SuccessionOutcome::Reordered { lost } => {
                    lines.push(format!(
                        "No man is found worthy; the file reorders and {lost} are lost in the rout."
                    ));
                    if t.state.living_count() == 0 {
                        t.over = true;
                        lines.push(
                            "The last of the company is gone. The pilgrimage ends here; the Book of the Fallen closes."
                                .to_string(),
                        );
                        t.party.clear();
                        Mantle::Over
                    } else {
                        // A lesser captain of the remnant takes the road
                        // (named: the engine's reorder leaves no
                        // mantle-bearer, so the first living man is
                        // marked to keep the Book honest).
                        let level = former_level.saturating_sub(5).max(1);
                        t.champion_level = level;
                        if let Some(m) = t.state.living_mut().next() {
                            m.champion = true;
                        }
                        Mantle::ReorderedContinue { level }
                    }
                }
            };
            (lines, mantle)
        };
        self.push_log(lines);
        match mantle {
            Mantle::Over => {
                self.phase = Phase::BattleOver;
            }
            Mantle::Promoted { level } | Mantle::ReorderedContinue { level } => {
                let t = self.trail.as_mut().expect("trail battle implies trail");
                t.party = vec![practice_champion(former_species, level)?];
                // A risen (or reordered) captain's vanguard body retires
                // with the old mantle; the road-claimed are struck off
                // too. The next road entry re-mints from the living file.
                t.vanguard.retain(|v| {
                    t.state
                        .men
                        .iter()
                        .any(|m| m.alive && !m.champion && m.name == v.name)
                });
                self.phase = Phase::Trail;
            }
        }
        self.save_trail()?;
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
    /// Either way, a WON battle first records yielded historical
    /// adversaries in the codex mask and queues any Calls to Repentance
    /// (M7b) — a non-empty queue takes the phase to ConversionChoice
    /// before BattleOver.
    fn on_battle_end(&mut self, winner: u8) -> Result<(), AppError> {
        if self.trail_battle {
            return self.on_trail_battle_end(winner);
        }
        if winner == MY_SIDE {
            self.scan_defeated_foes()?;
        }
        let Some((node, defeated, shards)) = self
            .campaign
            .as_ref()
            .map(|c| (c.node, c.defeated, c.shards_earned))
        else {
            self.phase = if self.pending_calls.is_empty() {
                Phase::BattleOver
            } else {
                Phase::ConversionChoice
            };
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
            self.phase = if self.pending_calls.is_empty() {
                Phase::BattleOver
            } else {
                Phase::ConversionChoice
            };
        } else {
            self.battle = None;
            self.push_log(lines);
            self.phase = if self.pending_calls.is_empty() {
                Phase::CampaignRest
            } else {
                Phase::ConversionChoice
            };
        }
        Ok(())
    }

    /// After a won battle: walk the yielded foe party, set the
    /// `campaign.foes_defeated` codex bits (HF 6.5), and queue one Call
    /// decision per defeated CONVERTIBLE adversary (108..=117; commanders
    /// are recorded but never called — CONVERSION 2). The caller's part of
    /// the contested roll is captured here from the standing MY_SIDE
    /// champion: v1 does not attribute the final blow, so the virtue-kill
    /// bonus is forfeited (false) and fervor items are unwired (0).
    fn scan_defeated_foes(&mut self) -> Result<(), AppError> {
        let Some(b) = self.battle.as_ref() else {
            return Ok(());
        };
        let caller = b.sides[usize::from(MY_SIDE)].active();
        let caller_sp = data::species_by_id(caller.species_id)
            .ok_or_else(|| illegal(format!("unknown species {}", caller.species_id)))?;
        let caller_stats = data::compute_stats(caller, caller_sp);
        let caller_piety = caller_stats.piety;
        let caller_level = caller.level;
        // Copy the foe facts out before any store write (the battle borrow
        // ends here).
        let foes: Vec<(u16, u8)> = b.sides[usize::from(FOE_SIDE)]
            .party
            .iter()
            .map(|c| (c.species_id, c.level))
            .collect();
        let mut mask = self.foes_defeated_mask();
        for (species_id, level) in foes {
            let Some(bit) = conv::foe_bit(species_id) else {
                continue;
            };
            mask |= 1u32 << bit;
            if !conv::CONVERTIBLE_IDS.contains(&species_id) {
                continue;
            }
            let sp = data::species_by_id(species_id).expect("static table covers adversaries");
            let chance = data::conversion_chance(
                caller_piety,
                caller_level,
                false, // final-blow virtue attribution: not tracked in v1
                0,     // fervor items: not wired in v1
                sp,
                level,
            )
            .expect("convertible ids carry obstinacy (3..=85)");
            self.pending_calls.push(PendingCall {
                species_id,
                foe_level: level,
                chance_pct: chance,
            });
        }
        if mask != self.foes_defeated_mask() {
            self.store
                .set_setting(conv::FOES_DEFEATED_KEY, &mask.to_string())?;
        }
        Ok(())
    }

    /// ConversionChoice: settle the head pending Call. `call == true`
    /// issues the Call (one sealed contested roll; success baptizes,
    /// failure is the Mercy release); `call == false` is the direct
    /// release. Either way the head leaves the queue and the last
    /// resolution returns the phase to BattleOver.
    fn resolve_call(&mut self, call: bool) -> Result<(), AppError> {
        let Some(pc) = self.pending_calls.first().cloned() else {
            return Err(illegal("no yielded foe awaits a choice"));
        };
        let sp = data::species_by_id(pc.species_id).expect("pending ids live in the static table");
        if !call {
            // Release without the Call: Mercy outright (CONVERSION's
            // client-flow note — no roll spent).
            let carried = self
                .store
                .add_item(conv::MERCY_SHARD_ITEM, conv::MERCY_BOUNTY)?;
            self.push_log(vec![format!(
                "The company released the {} without the Call; his soul was commended unto God, and he departed the field. The Order gained {} mercy shard(s) (carried: {carried}).",
                sp.name,
                conv::MERCY_BOUNTY
            )]);
        } else {
            // The Call: count it (hagiography statistic + the draw salt's
            // moving part), then the sealed deterministic draw.
            let issued = self
                .store
                .setting(conv::CALLS_ISSUED_KEY)
                .ok()
                .flatten()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0)
                .saturating_add(1);
            self.store
                .set_setting(conv::CALLS_ISSUED_KEY, &issued.to_string())?;
            let salt = (u64::from(self.foes_defeated_mask()) << 32) | u64::from(issued);
            let roll = conv::conversion_roll(salt);
            if conv::decide_call(pc.chance_pct, roll) {
                self.baptize(&pc, sp.name)?;
            } else {
                let carried = self
                    .store
                    .add_item(conv::MERCY_SHARD_ITEM, conv::MERCY_BOUNTY)?;
                self.push_log(vec![format!(
                    "The {} heard the Call and would not turn. The company commended his soul unto God, and he departed the field. The Order gained {} mercy shard(s) (carried: {carried}).",
                    sp.name,
                    conv::MERCY_BOUNTY
                )]);
            }
        }
        self.pending_calls.remove(0);
        if self.pending_calls.is_empty() {
            // Road-queued Calls (trail battles, willing hearers) hand the
            // phase back to the pilgrimage; every other lane returns to
            // the battle-over screen as before.
            let back_to_road = self.campaign.is_none()
                && self.trail.as_ref().is_some_and(|t| !t.over);
            self.phase = if back_to_road {
                Phase::Trail
            } else {
                Phase::BattleOver
            };
        }
        Ok(())
    }

    /// Baptism (CONVERSION 4): the foe repents and joins the roster as a
    /// NEW instance of his own species (his kit and arms are his trade),
    /// flagged `converted` with his origin recorded. His typing displays
    /// as Humility in the gallery (v1 simplification of "the virtue that
    /// felled him" — the charity that spared him); mechanical typing stays
    /// the species' Valor until crusade-core grows instance typing. His
    /// baptismal name is seeded from the archetype table into
    /// `roster.baptismal.<id>` — the future rename path rides that key.
    fn baptize(&mut self, pc: &PendingCall, species_name: &str) -> Result<(), AppError> {
        let level = pc.foe_level.clamp(1, MAX_LEVEL);
        let mut convert = practice_champion(pc.species_id, level)?;
        convert.converted = true;
        convert.converted_from = Some(pc.species_id);
        let id = self.store.add_champion(&convert)?;
        let name =
            conv::baptismal_name(pc.species_id).expect("convertible ids carry baptismal names");
        self.store
            .set_setting(&format!("{}{id}", conv::BAPTISMAL_KEY_PREFIX), name)?;
        let carried = self
            .store
            .add_item(conv::FIRSTFRUITS_SHARD_ITEM, conv::FIRSTFRUITS_BOUNTY)?;
        let mask = self.converts_baptized_mask() | (1u32 << (pc.species_id - 108));
        self.store
            .set_setting(conv::CONVERTS_BAPTIZED_KEY, &mask.to_string())?;
        self.push_log(vec![format!(
            "\"I will follow the Christ,\" the {species_name} said, and was baptized {name}. He joined the company at level {level}. The Order gained {} firstfruits shard(s) (carried: {carried}).",
            conv::FIRSTFRUITS_BOUNTY
        )]);
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
            typing_override: None,
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

/// The Roll of Worthy Adversaries as renderable cards (HF 6.5): one row
/// per field archetype then commander, in id order, gated on the persisted
/// `campaign.foes_defeated` mask. Same recipe as [`sin_cards`]: the row
/// always names the man, but a sealed row carries EMPTY lore so the reward
/// text never leaves the client before it is earned.
fn foe_cards(defeated_mask: u32) -> Vec<FoeCard> {
    conv::CONVERTIBLE_IDS
        .chain(conv::COMMANDER_IDS)
        .map(|id| {
            let sp = data::species_by_id(id).expect("static species table covers all adversaries");
            let revealed = defeated_mask & (1u32 << conv::foe_bit(id).expect("adversaries earn a bit")) != 0;
            FoeCard {
                species_id: id,
                name: sp.name.to_string(),
                lore: if revealed { sp.lore.to_string() } else { String::new() },
                defeated: revealed,
            }
        })
        .collect()
}

/// The Testimonies of the Baptized as renderable cards (CONVERSION 4): one
/// row per convertible archetype, gated on the persisted
/// `campaign.converts_baptized` mask; sealed rows carry EMPTY testimony.
fn testimony_cards(converts_mask: u32) -> Vec<TestimonyCard> {
    conv::CONVERTIBLE_IDS
        .map(|id| {
            let sp = data::species_by_id(id).expect("static species table covers all adversaries");
            let baptized = converts_mask & (1u32 << (id - 108)) != 0;
            TestimonyCard {
                species_id: id,
                name: sp.name.to_string(),
                testimony: if baptized {
                    conv::testimony(id)
                        .expect("convertible ids carry testimonies")
                        .to_string()
                } else {
                    String::new()
                },
                baptized,
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

/// The Pilgrim Road's renderable row (serde tail field `ViewSnapshot::
/// trail`). Pre-rendered summaries only; names, never numbers, for the
/// fallen.
fn trail_view(t: &TrailCampaign) -> TrailView {
    let screen = overworld::screen_by_id(t.state.screen);
    let region_name = screen
        .and_then(|s| overworld::region_by_id(s.region))
        .map_or("Unknown Lands", |r| r.name)
        .to_string();
    let mut exits: Vec<u16> = screen
        .map(|s| s.edges.iter().flatten().copied().collect())
        .unwrap_or_default();
    exits.sort_unstable();
    let choice_encounters = screen
        .map(|s| {
            s.props
                .iter()
                .filter_map(|p| match p {
                    PropRef::ChoiceEncounter(r) => Some(*r),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    let last_fallen = t.state.fallen.last().map_or_else(String::new, |f| {
        // The battle-dead of a gate screen read the map's own
        // where-phrase (docs/GATE-CONTENT.md defeat lines); every other
        // page keeps the shipped screen-and-cause row.
        if f.cause == persist::FallenCause::SlainInBattle.as_str() {
            if let Some(phrase) = gate_defeat_phrase(f.where_fell) {
                return format!("{}, day {} — {}", f.name, f.day, phrase);
            }
        }
        format!("{}, day {}, at screen {} — {}", f.name, f.day, f.where_fell, f.cause)
    });
    let can_camp = screen
        .map(|s| s.props.iter().any(|p| matches!(p, PropRef::Camp)))
        .unwrap_or(false)
        && !t.over;
    let mantle_resolve = t
        .party
        .first()
        .and_then(|m| data::species_by_id(m.species_id).map(|sp| (m, sp)))
        .map_or((0, 0), |(m, sp)| {
            (m.current_resolve, data::compute_stats(m, sp).max_resolve)
        });
    // The roster is the truth of who lives; the view shows only men the
    // van would actually field (road events can fell a man with no battle
    // to reconcile his cache entry — the next sync strikes him off).
    let vanguard = t
        .vanguard
        .iter()
        .filter(|v| {
            t.state
                .men
                .iter()
                .any(|m| m.alive && !m.champion && m.name == v.name)
        })
        .map(|v| v.name.clone())
        .collect();
    TrailView {
        screen: t.state.screen,
        region_name,
        day: t.state.day,
        supplies: t.state.supplies,
        morale: t.state.morale,
        living: t.state.living_count(),
        champion_name: t.champion().map_or_else(String::new, |m| m.name.clone()),
        fallen_count: t.state.fallen.len() as u16,
        last_fallen,
        exits,
        choice_encounters,
        over: t.over,
        can_camp,
        mantle_resolve,
        vanguard,
    }
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
        // A company fielding exactly Bastien, Aldwyn, Raimbaud (the theme
        // ids) must never spar itself. Note 116 is an adversary id and
        // cannot appear on a roster, so the mirror case is only 3+8.
        let team = practice_foe_team(&[3, 8, 116]);
        assert_eq!(team.len(), 3);
        for id in team {
            assert!(
                (1..=12).contains(&id) || id == 116,
                "foe {id} is a champion id or the rotated adversary"
            );
            assert!(![3, 8, 116].contains(&id), "foe {id} never mirrors");
        }
        // All three distinct, and the stable backfill order is 1, 2, 4.
        assert_eq!(team, [1, 2, 4]);
    }

    #[test]
    fn empty_party_keeps_the_pure_theme() {
        assert_eq!(practice_foe_team(&[]), PRACTICE_FOE_SPECIES);
    }

    #[test]
    fn the_rotated_adversary_is_a_road_east_archetype() {
        // V12-3 bounds pin: exactly one member of the theme is an adversary,
        // it sits in the 108..=117 archetype band (never a named commander),
        // and it resolves in the species table.
        let adv: Vec<u16> = PRACTICE_FOE_SPECIES
            .iter()
            .copied()
            .filter(|id| !(1..=12).contains(id))
            .collect();
        assert_eq!(adv.len(), 1, "exactly one rotated adversary");
        let id = adv[0];
        assert!((108..=117).contains(&id), "foe {id} out of archetype band");
        assert!(data::species_by_id(id).is_some(), "foe {id} unresolved");
    }

    #[test]
    fn partial_overlap_replaces_only_the_colliders() {
        // Party fields Bastien only: Bastien leaves, Aldwyn and Raimbaud
        // keep their theme order, and the smallest free pool id appends.
        assert_eq!(practice_foe_team(&[3]), [8, 116, 1]);
        // Duplicated party species change nothing; nor do surplus members.
        assert_eq!(practice_foe_team(&[10, 10, 3, 5]), [8, 116, 1]);
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
mod conversion_tests {
    //! The M7b Call to Repentance flow end-to-end at the client level:
    //! a won battle over a convertible adversary (ids 108..=117) opens the
    //! ConversionChoice phase; the Call's sealed draw decides baptism
    //! (Firstfruits) or refusal (Mercy); the Release button takes Mercy
    //! directly. Plus the two codex bitmasks and the Hagiography books.

    use super::*;

    /// One-champion roster fixture (same shape as the other test mods).
    fn fixture_store() -> RosterStore {
        let mut store = RosterStore::new();
        store
            .add_champion(&practice_champion(1, 20).expect("fixture champion"))
            .expect("fixture store accepts the champion");
        store
    }

    /// Fabricate a WON battle against the given adversary species: the
    /// client gets a foe party already yielded, and the end-of-battle
    /// router runs. This stands in for whatever battle mode fields
    /// historical adversaries (none ships yet — M7b lands the flow first,
    /// headless-tested).
    fn beat_adversaries(game: &mut ClientGame, foe_ids: &[u16], foe_level: u8) {
        let me = practice_champion(1, 20).expect("fixture champion");
        let foes: Vec<ChampionInstance> = foe_ids
            .iter()
            .map(|&s| practice_champion(s, foe_level).expect("fixture foe"))
            .collect();
        let mut b = Battle::new(vec![me], foes, PRACTICE_SEED).expect("fixture battle");
        for f in &mut b.sides[usize::from(FOE_SIDE)].party {
            f.current_resolve = 0;
        }
        b.winner = Some(MY_SIDE);
        game.battle = Some(b);
        game.phase = Phase::InBattle;
        game.on_battle_end(MY_SIDE).expect("won battle routes");
    }

    /// The salt a Call decision will use: per resolve_call,
    /// `(foes_defeated_mask << 32) | calls_issued(after increment)`.
    fn decision_salt(foes_mask: u32, calls_issued: u32) -> u64 {
        (u64::from(foes_mask) << 32) | u64::from(calls_issued)
    }

    /// The (species, chance, roll) each convertible archetype would meet on
    /// a fresh document (foes mask = only that archetype's bit yet, first
    /// Call issued). The caller is the fixture's level-20 champion.
    fn call_table() -> Vec<(u16, u8, u8)> {
        let me = practice_champion(1, 20).expect("caller");
        let sp = data::species_by_id(1).expect("caller species");
        let piety = data::compute_stats(&me, sp).piety;
        conv::CONVERTIBLE_IDS
            .map(|id| {
                let foe = data::species_by_id(id).expect("adversary");
                let chance = data::conversion_chance(piety, 20, false, 0, foe, 20)
                    .expect("convertible");
                let salt = decision_salt(1u32 << (id - 108), 1);
                (id, chance, conv::conversion_roll(salt))
            })
            .collect()
    }

    #[test]
    fn won_battle_over_convertible_foe_opens_the_choice_and_seals_the_foe_bit() {
        let mut game = ClientGame::new(fixture_store());
        beat_adversaries(&mut game, &[108], 20);
        assert_eq!(game.phase(), Phase::ConversionChoice);
        let snap = game.snapshot();
        let call = snap.conversion.as_ref().expect("a live call rides the snapshot");
        assert_eq!(call.species_id, 108);
        assert_eq!(call.species_name, "Turcoman Bowrider");
        assert_eq!(call.foe_level, 20);
        assert!((3..=85).contains(&call.chance_pct));
        assert_eq!(call.pending, 1);
        // The foe's codex bit was set at battle end (before any choice).
        assert_eq!(
            game.store().setting(conv::FOES_DEFEATED_KEY).unwrap().as_deref(),
            Some("1") // bit 0 = archetype 108
        );
        // And a commander earns his Roll bit with no Call queued.
        let mut game = ClientGame::new(fixture_store());
        beat_adversaries(&mut game, &[123], 20);
        assert_eq!(game.phase(), Phase::BattleOver, "commanders are never called");
        assert_eq!(
            game.store().setting(conv::FOES_DEFEATED_KEY).unwrap().as_deref(),
            Some("32768") // bit 15 = commander 123
        );
    }

    /// Harness pick: the static tables must offer at least one archetype
    /// whose sealed draw succeeds and one whose draw fails, so both branch
    /// ends are exercised by the flow tests below. (The draws are fixed —
    /// this asserts the tables keep both reachable.)
    #[test]
    fn the_call_table_offers_both_outcomes() {
        let table = call_table();
        let mut any_pass = false;
        let mut any_fail = false;
        for &(_, chance, roll) in &table {
            if conv::decide_call(chance, roll) {
                any_pass = true;
            } else {
                any_fail = true;
            }
        }
        assert!(any_pass && any_fail, "table: {table:?}");
    }

    /// Baptism (success path, seeded construction): the convert joins the
    /// roster flagged `converted` with origin recorded; his baptismal name
    /// is persisted; the converts bit flips; Firstfruits land.
    #[test]
    fn successful_call_baptizes_into_the_roster() {
        let &(id, _, _) = call_table()
            .iter()
            .find(|&&(id, chance, roll)| conv::decide_call(chance, roll))
            .expect("table offers a passing draw (see the harness test)");
        let mut game = ClientGame::new(fixture_store());
        beat_adversaries(&mut game, &[id], 20);
        assert_eq!(game.phase(), Phase::ConversionChoice);

        game.handle(ClientInput::CallToRepentance)
            .expect("the call is legal from the choice phase");
        assert_eq!(game.phase(), Phase::BattleOver, "the drained queue returns the over screen");

        let converts = game
            .store()
            .setting(conv::CONVERTS_BAPTIZED_KEY)
            .unwrap()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);
        assert_eq!(converts, 1u32 << (id - 108), "the converts bit flips");
        assert_eq!(
            game.store().setting(conv::CALLS_ISSUED_KEY).unwrap().as_deref(),
            Some("1")
        );
        let mut inv: Vec<(String, u64)> = game.store().inventory().unwrap();
        inv.sort();
        assert_eq!(
            inv,
            vec![(conv::FIRSTFRUITS_SHARD_ITEM.to_string(), conv::FIRSTFRUITS_BOUNTY as u64)],
            "Firstfruits, not Mercy"
        );
        // The roster grew by the convert: species kept, flags set, level kept.
        let roster = game.store().list_champions().unwrap();
        let (rid, convert) = roster
            .iter()
            .find(|(_, c)| c.species_id == id)
            .expect("the convert joined the roster");
        assert!(convert.converted);
        assert_eq!(convert.converted_from, Some(id));
        assert_eq!(convert.level, 20);
        assert_eq!(
            game.baptismal_name_of(rid).as_deref(),
            conv::baptismal_name(id),
            "the baptismal name is seeded"
        );
        // Serde round-trip through the roster store: flags, origin, name
        // and masks all survive a document reload.
        let bytes = game.store().save_bytes().expect("document saves");
        let reloaded = RosterStore::load_bytes(&bytes).expect("document loads");
        let (_, back) = reloaded
            .list_champions()
            .unwrap()
            .into_iter()
            .find(|(_, c)| c.species_id == id)
            .expect("the convert survives reload");
        assert_eq!(&back, convert);
        assert_eq!(
            reloaded.setting(conv::FOES_DEFEATED_KEY).unwrap().as_deref(),
            game.store().setting(conv::FOES_DEFEATED_KEY).unwrap().as_deref()
        );
        assert_eq!(
            reloaded.setting(conv::CONVERTS_BAPTIZED_KEY).unwrap().as_deref(),
            game.store().setting(conv::CONVERTS_BAPTIZED_KEY).unwrap().as_deref()
        );
        assert_eq!(
            reloaded
                .setting(&format!("{}{rid}", conv::BAPTISMAL_KEY_PREFIX))
                .unwrap()
                .as_deref(),
            conv::baptismal_name(id)
        );
    }

    /// Refusal path (seeded construction): the Call fails on the draw —
    /// Mercy shards, no convert, no converts bit.
    #[test]
    fn refused_call_releases_the_soul_with_mercy() {
        let &(id, _, _) = call_table()
            .iter()
            .find(|&&(id, chance, roll)| !conv::decide_call(chance, roll))
            .expect("table offers a failing draw (see the harness test)");
        let mut game = ClientGame::new(fixture_store());
        beat_adversaries(&mut game, &[id], 20);
        game.handle(ClientInput::CallToRepentance)
            .expect("the call is legal");
        assert_eq!(game.phase(), Phase::BattleOver);
        let inv = game.store().inventory().unwrap();
        assert_eq!(
            inv,
            vec![(conv::MERCY_SHARD_ITEM.to_string(), conv::MERCY_BOUNTY as u64)],
            "failure pays Mercy"
        );
        assert!(game
            .store()
            .setting(conv::CONVERTS_BAPTIZED_KEY)
            .unwrap()
            .is_none(), "no baptism: no bit written");
        assert_eq!(
            game.store().list_champions().unwrap().len(),
            1,
            "the roster holds only the fixture champion"
        );
    }

    /// Release path: no Call, no roll — Mercy directly, and no
    /// calls-issued count moves.
    #[test]
    fn releasing_without_a_call_pays_mercy_without_a_roll() {
        let mut game = ClientGame::new(fixture_store());
        beat_adversaries(&mut game, &[110], 20);
        assert_eq!(game.phase(), Phase::ConversionChoice);
        game.handle(ClientInput::ReleaseSoul).expect("release is legal");
        assert_eq!(game.phase(), Phase::BattleOver);
        let inv = game.store().inventory().unwrap();
        assert_eq!(
            inv,
            vec![(conv::MERCY_SHARD_ITEM.to_string(), conv::MERCY_BOUNTY as u64)]
        );
        assert!(game
            .store()
            .setting(conv::CALLS_ISSUED_KEY)
            .unwrap()
            .is_none(), "a released soul is never counted as called");
        assert_eq!(game.store().list_champions().unwrap().len(), 1);
        // Both Call inputs are dead ends outside the choice phase.
        assert!(game.handle(ClientInput::CallToRepentance).is_err());
        assert!(game.handle(ClientInput::ReleaseSoul).is_err());
        game.handle(ClientInput::AcknowledgeResult)
            .expect("the over screen closes as ever");
        assert_eq!(game.phase(), Phase::Menu);
    }

    /// Multiple yielded converts queue in order and drain one decision at
    /// a time into BattleOver.
    #[test]
    fn several_yielded_foes_queue_in_order() {
        let mut game = ClientGame::new(fixture_store());
        beat_adversaries(&mut game, &[108, 113], 20);
        for (n, id) in [(2, 108), (1, 113)] {
            let call = game
                .snapshot()
                .conversion
                .expect("a live call rides the snapshot");
            assert_eq!(call.species_id, id);
            assert_eq!(call.pending, n);
            game.handle(ClientInput::ReleaseSoul).expect("release is legal");
        }
        assert_eq!(game.phase(), Phase::BattleOver);
        assert!(game.snapshot().conversion.is_none());
        let inv = game.store().inventory().unwrap();
        assert_eq!(
            inv,
            vec![(conv::MERCY_SHARD_ITEM.to_string(), 2 * conv::MERCY_BOUNTY as u64)]
        );
        // Both archetypes' Roll bits are set; no converts.
        assert_eq!(
            game.store().setting(conv::FOES_DEFEATED_KEY).unwrap().as_deref(),
            Some("33") // bits 0 and 5 (ids 108 and 113)
        );
    }

    /// The two books: Roll rows always name the man, sealed rows carry no
    /// lore; Testimony rows seal until the baptism bit; phase-gated like
    /// the other books. Mirrors the Manuscript-of-Sins assertions.
    #[test]
    fn roll_and_testimonies_gate_by_phase_and_masks() {
        let mut game = ClientGame::new(RosterStore::new());
        assert!(game.snapshot().foes.is_empty());
        assert!(game.snapshot().testimonies.is_empty());

        game.handle(ClientInput::ViewHagiography)
            .expect("the menu opens the hagiography");
        let snap = game.snapshot();
        assert_eq!(snap.foes.len(), 16, "ten archetypes + six commanders");
        let order: Vec<u16> = snap.foes.iter().map(|c| c.species_id).collect();
        assert_eq!(order, (108..=117).chain(120..=125).collect::<Vec<u16>>());
        for card in &snap.foes {
            assert!(!card.defeated && card.lore.is_empty());
            assert!(!card.name.is_empty() && card.name.is_ascii(), "{}", card.name);
        }
        assert_eq!(snap.testimonies.len(), 10);
        for card in &snap.testimonies {
            assert!(!card.baptized && card.testimony.is_empty());
        }

        // Defeat Saladin (bit 15) and Bedouin Outrider (bit 5): exactly
        // those rows reveal, verbatim lore off the static table.
        game.store()
            .set_setting(conv::FOES_DEFEATED_KEY, &((1u32 << 15) | (1u32 << 5)).to_string())
            .expect("defeat mask writable");
        // And a baptism of the Outrider: his testimony alone unseals.
        game.store()
            .set_setting(conv::CONVERTS_BAPTIZED_KEY, &(1u32 << 5).to_string())
            .expect("converts mask writable");
        let snap = game.snapshot();
        for card in &snap.foes {
            let want = matches!(card.species_id, 113 | 123);
            assert_eq!(card.defeated, want, "{}", card.name);
            if want {
                assert_eq!(
                    card.lore,
                    data::species_by_id(card.species_id).expect("adversary").lore,
                    "revealed lore is the species table's, verbatim"
                );
            } else {
                assert!(card.lore.is_empty(), "{} stays sealed", card.name);
            }
        }
        for card in &snap.testimonies {
            let want = card.species_id == 113;
            assert_eq!(card.baptized, want, "{}", card.name);
            assert_eq!(
                card.testimony,
                if want { conv::testimony(113).unwrap().to_string() } else { String::new() },
            );
        }

        // The books survive a document reload (masks ride the store).
        let bytes = game.store().save_bytes().expect("document saves");
        let mut reloaded = ClientGame::new(RosterStore::load_bytes(&bytes).expect("loads"));
        reloaded
            .handle(ClientInput::ViewHagiography)
            .expect("the reloaded menu opens the hagiography");
        let resnap = reloaded.snapshot();
        assert_eq!(resnap.foes, snap.foes);
        assert_eq!(resnap.testimonies, snap.testimonies);
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

#[cfg(test)]
mod trail_tests {
    //! M8b: the Pilgrim Road end-to-end at the client level — the Crossing,
    //! day travel with upkeep and seeded determinism, the Kilij Arslan
    //! scripted gate, post-battle conversion choices routed back to the
    //! road, the Book of the Fallen, THE MANTLE succession lanes, and
    //! mid-journey save/load. Battles are settled by the same fabrication
    //! the conversion tests use (foe resolve zeroed, winner set, the
    //! end-of-battle router run) — the battle engine is proven elsewhere;
    //! what these tests pin is the ROUTING.

    use super::*;
    use crusade_core::engine::trail as te;

    /// One-champion roster fixture (same shape as the other test mods),
    /// with the pilgrimage seed PRE-MINTED to a fixed value: the seed
    /// hash mixes the roster, and `add_champion` assigns a random uuid
    /// v4 id, so without a minted key every run would walk a different
    /// road (the march harness flaked on exactly that — a willing hearer
    /// would sometimes preempt a gate entry).
    fn fixture_store() -> RosterStore {
        let mut store = RosterStore::new();
        store
            .add_champion(&practice_champion(1, 20).expect("fixture champion"))
            .expect("fixture store accepts the champion");
        store
            .set_setting(persist::TRAIL_SEED_KEY, &hex::encode([0x5Eu8; 32]))
            .expect("fixture pre-mints the pilgrimage seed");
        store
    }

    /// March-friendly fixture store: lead champion high enough that gate
    /// escorts build (escort level floors at 5 regardless — clamped).
    fn embarked_game() -> ClientGame {
        let mut game = ClientGame::new(fixture_store());
        game.handle(ClientInput::StartTrail {
            charter_quality: 200,
            provisioning_spend: 400,
        })
        .expect("the crossing is legal from the menu");
        game
    }

    /// Fabricate a WON settlement of the live battle (conversion-test
    /// recipe), then drain any queued Calls via Release.
    fn auto_win(game: &mut ClientGame) {
        {
            let b = game.battle.as_mut().expect("a battle is live");
            for f in &mut b.sides[usize::from(FOE_SIDE)].party {
                f.current_resolve = 0;
            }
            b.winner = Some(MY_SIDE);
        }
        game.on_battle_end(MY_SIDE).expect("won battle routes");
        while game.phase() == Phase::ConversionChoice {
            game.handle(ClientInput::ReleaseSoul)
                .expect("release is legal");
        }
    }

    /// Fabricate a LOST settlement of the live battle.
    fn auto_lose(game: &mut ClientGame) {
        {
            let b = game.battle.as_mut().expect("a battle is live");
            for f in &mut b.sides[usize::from(MY_SIDE)].party {
                f.current_resolve = 0;
            }
            b.winner = Some(FOE_SIDE);
        }
        game.on_battle_end(FOE_SIDE).expect("lost battle routes");
    }

    /// March one screen east, settling anything the day raised.
    fn march_east(game: &mut ClientGame) {
        let next = game
            .snapshot()
            .trail
            .expect("trail view present")
            .exits
            .into_iter()
            .max()
            .expect("the spine has an east exit");
        game.handle(ClientInput::Travel { to_screen: next })
            .expect("travel is legal");
        while game.phase() == Phase::InBattle {
            auto_win(game);
        }
        // A willing hearer by the way lands in the same queue; the march
        // harness releases souls so the road resumes.
        while game.phase() == Phase::ConversionChoice {
            game.handle(ClientInput::ReleaseSoul)
                .expect("release is legal");
        }
    }

    #[test]
    fn crossing_lands_the_company_and_is_deterministic() {
        let mut g1 = embarked_game();
        let _ = &mut g1;
        let t = g1.snapshot().trail.expect("trail view rides the snapshot");
        assert_eq!(g1.phase(), Phase::Trail);
        assert_eq!((t.day, t.screen), (2, 5), "the crossing ends on the far shore");
        assert!(t.living >= 5, "the company always lands somebody");
        assert!(!t.champion_name.is_empty(), "the mantle is named");
        assert_eq!(t.region_name, "The Crossing");
        assert_eq!(t.exits, vec![4, 6]);
        assert!(t.choice_encounters.is_empty());
        assert!(!t.over);
        // The fallen at sea are already in the Book at day 0.
        assert_eq!(t.fallen_count, 20 - t.living, "every loss at sea has a page");
        // Reloaded onto the same document, the crossing is byte-identical
        // (the pilgrimage seed rides the store).
        let bytes = g1.store().save_bytes().expect("document saves");
        let g2r = ClientGame::new(RosterStore::load_bytes(&bytes).expect("document loads"));
        assert_eq!(g2r.snapshot().trail.expect("reloaded trail"), t);
    }

    #[test]
    fn travel_advances_days_charges_upkeep_and_stays_deterministic() {
        let mut g1 = embarked_game();
        let before = g1.snapshot().trail.expect("trail").supplies;
        for _ in 0..4 {
            march_east(&mut g1);
        }
        let t = g1.snapshot().trail.expect("trail");
        assert_eq!((t.day, t.screen), (6, 9), "four marches land at the gate");
        assert!(t.supplies < before + 15 * 4, "upkeep outruns even constant forage");
        // Determinism: the same document reloaded walks the same road.
        let bytes = g1.store().save_bytes().expect("document saves");
        let mut g2 = ClientGame::new(RosterStore::load_bytes(&bytes).expect("document loads"));
        let want = g1.snapshot().trail;
        march_east(&mut g1);
        march_east(&mut g2);
        assert_eq!(g1.snapshot().trail, g2.snapshot().trail, "same seeds, same road");
        let _ = want;
    }

    #[test]
    fn the_kilij_arslan_gate_fields_commander_and_escorts_and_routes_back() {
        let mut game = embarked_game();
        // March 5 -> 8 (three days); the fourth march enters screen 9 and
        // the scripted gate fires before any settlement of it.
        for _ in 0..3 {
            march_east(&mut game);
        }
        assert_eq!(game.snapshot().trail.expect("trail view").screen, 8);
        game.handle(ClientInput::Travel { to_screen: 9 })
            .expect("the gate march is legal");
        // Screen 9's scripted gate fired on entry: the company stands in
        // battle against Kilij Arslan (120) behind two escorts.
        let b = game.battle.as_ref().expect("the gate battle is live");
        let foes: Vec<u16> = b.sides[usize::from(FOE_SIDE)]
            .party
            .iter()
            .map(|c| c.species_id)
            .collect();
        assert_eq!(foes, vec![108, 109, 120], "escorts screen, commander last");
        assert_eq!(b.sides[usize::from(FOE_SIDE)].party[2].level, 22);
        assert_eq!(game.phase(), Phase::InBattle);

        auto_win(&mut game);
        // Road victories return to the pilgrimage (Calls drained by
        // auto_win's releases), never to the gauntlet's BattleOver.
        assert_eq!(game.phase(), Phase::Trail);
        assert_eq!(game.snapshot().trail.expect("trail view").screen, 9);
        // The Roll recorded all three (commander bit 12; escorts 0 and 1).
        let mask: u32 = game
            .store()
            .setting(conv::FOES_DEFEATED_KEY)
            .unwrap()
            .expect("the roll mask was written")
            .parse()
            .unwrap();
        assert_eq!(mask & (1 | 2 | (1 << 12)), 1 | 2 | (1 << 12));
    }

    /// Drive the company onto a gate screen until its battle of record
    /// stands live. Under the shipped Convert-first routing a willing
    /// hearer raised the same day PREEMPTS the funnel (the hearer is
    /// queued, the gate does not fire for that entry); when that
    /// happens, release him, step back west one screen, and come at the
    /// gate again on the next day — the gate fires on ENTRY. Approach
    /// traffic (ambushes, wilds) is settled with auto_win along the way.
    fn drive_into_gate(game: &mut ClientGame, gate_screen: u16) {
        assert_eq!(game.phase(), Phase::Trail, "the harness drives from the road");
        for _ in 0..12 {
            while game.phase() == Phase::ConversionChoice {
                game.handle(ClientInput::ReleaseSoul)
                    .expect("release is legal");
            }
            while game.phase() == Phase::InBattle {
                auto_win(game);
            }
            let screen = game.snapshot().trail.expect("trail view").screen;
            let target = if screen == gate_screen {
                // A hearer preempted the gate on entry; come again.
                screen - 1
            } else {
                gate_screen
            };
            game.handle(ClientInput::Travel { to_screen: target })
                .expect("the approach march is legal");
            if target == gate_screen && game.phase() == Phase::InBattle {
                return;
            }
        }
        panic!("the gate at screen {gate_screen} never fired");
    }

    /// One gate of the placement map (docs/GATE-CONTENT.md section 0),
    /// end to end: drive the company onto the gate screen from the
    /// screen before it, assert the battle of record — escorts first,
    /// commander last, the commander standing the map's offset above
    /// the mantle — then route the victory back to the road with the
    /// map's own chronicle lines.
    #[allow(clippy::too_many_arguments)]
    fn assert_gate_battle_of_record(
        game: &mut ClientGame,
        from_screen: u16,
        gate_screen: u16,
        battle_ref: u16,
        escorts: [u16; 2],
        commander: u16,
        offset: u8,
        pre_fragment: &str,
        victory_fragment: &str,
    ) {
        craft_trail_on(game, from_screen, &[(255, true), (10, false)], 50);
        drive_into_gate(game, gate_screen);
        assert_eq!(game.phase(), Phase::InBattle, "the gate fires on entry");
        assert_eq!(
            game.trail_gate, Some(battle_ref),
            "the battle of record is the map's"
        );
        assert!(
            game.snapshot()
                .log_tail
                .iter()
                .any(|l| l.contains(pre_fragment)),
            "the pre-battle warning is the map's own"
        );
        let b = game.battle.as_ref().expect("the gate battle is live");
        let foes: Vec<u16> = b.sides[usize::from(FOE_SIDE)]
            .party
            .iter()
            .map(|c| c.species_id)
            .collect();
        assert_eq!(
            foes,
            vec![escorts[0], escorts[1], commander],
            "escorts screen, commander last"
        );
        let lv = game.trail.as_ref().expect("trail").champion_level;
        let foep = &b.sides[usize::from(FOE_SIDE)].party;
        assert_eq!(
            foep[2].level,
            lv + offset,
            "the commander stands the map's offset above the mantle"
        );
        assert_eq!(foep[0].level, lv - 2, "escorts keep the shipped clamp");
        assert_eq!(foep[1].level, lv - 2, "escorts keep the shipped clamp");

        auto_win(game);
        assert_eq!(game.phase(), Phase::Trail, "the road resumes");
        assert_eq!(
            game.snapshot().trail.expect("trail view").screen,
            gate_screen
        );
        assert!(
            game.snapshot()
                .log_tail
                .iter()
                .any(|l| l.contains(victory_fragment)),
            "the victory line is the map's own"
        );
        assert_eq!(game.trail_gate, None, "the gate marker retires with the battle");
    }

    #[test]
    fn the_harim_gate_fields_nur_ad_din_and_escorts_and_routes_back() {
        let mut game = ClientGame::new(fixture_store());
        assert_gate_battle_of_record(
            &mut game,
            12,
            13,
            102,
            [114, 116],
            121,
            2,
            "spears in good order, the atabeg's banner above them",
            "The askar breaks in good order and withdraws toward Damascus",
        );
    }

    #[test]
    fn the_egypt_roads_gate_fields_shirkuh_and_escorts_and_routes_back() {
        let mut game = ClientGame::new(fixture_store());
        assert_gate_battle_of_record(
            &mut game,
            15,
            16,
            103,
            [110, 116],
            122,
            2,
            "thrice the master of Egypt's roads",
            "His tired men scatter into the sands",
        );
    }

    #[test]
    fn the_left_horn_gate_fields_gokbori_and_escorts_and_routes_back() {
        let mut game = ClientGame::new(fixture_store());
        assert_gate_battle_of_record(
            &mut game,
            18,
            19,
            104,
            [116, 114],
            124,
            2,
            "Muzaffar al-Din Gokbori holds the left horn",
            "The horn is turned; Gokbori draws his riders off the hill",
        );
    }

    /// The right horn escalates: +3 above the mantle, per the map's
    /// offset column (the shipped gates all stood at +2).
    #[test]
    fn the_right_horn_gate_fields_taqi_al_din_at_the_escalated_offset() {
        let mut game = ClientGame::new(fixture_store());
        assert_gate_battle_of_record(
            &mut game,
            19,
            20,
            105,
            [116, 117],
            125,
            3,
            "Taqi al-Din Umar holds the right hand of Hattin's field",
            "The second horn gives way at the ninth hour",
        );
    }

    /// The finale escalates the same way: Saladin at +3 on the King's
    /// road, gates only screen 25.
    #[test]
    fn the_kings_road_gate_fields_saladin_at_the_escalated_offset() {
        let mut game = ClientGame::new(fixture_store());
        assert_gate_battle_of_record(
            &mut game,
            24,
            25,
            108,
            [116, 117],
            123,
            3,
            "Salah al-Din Yusuf ibn Ayyub, uniter of Egypt and Syria",
            "The host of Egypt and Syria yields the field",
        );
    }

    /// Screens 23/24 carried M8a-spine stand-in gates; the content map
    /// clears them (GATE-CONTENT section 0, orchestrator note 2), so
    /// both are ordinary screens and Saladin gates only the finale.
    #[test]
    fn screens_23_and_24_are_ordinary_screens_the_finale_at_25() {
        let mut game = ClientGame::new(fixture_store());
        craft_trail_on(&mut game, 22, &[(255, true), (10, false)], 50);
        for ordinary in [23u16, 24] {
            game.handle(ClientInput::Travel { to_screen: ordinary })
                .expect("the march is legal");
            assert!(
                game.trail_gate.is_none(),
                "screen {ordinary} raises no scripted gate"
            );
            // The day may still raise its ordinary traffic — an ambush,
            // a willing hearer — which is not a battle of record; settle
            // it and march on.
            while game.phase() == Phase::InBattle {
                auto_win(&mut game);
            }
            while game.phase() == Phase::ConversionChoice {
                game.handle(ClientInput::ReleaseSoul)
                    .expect("release is legal");
            }
            assert_eq!(
                game.snapshot().trail.expect("trail view").screen,
                ordinary,
                "the company stands on the ordinary screen"
            );
        }
        drive_into_gate(&mut game, 25);
        assert_eq!(game.trail_gate, Some(108), "the finale gate fires at 25");
        assert_eq!(game.phase(), Phase::InBattle);
    }

    /// A gate defeat pages the Book of the Fallen with the map's own
    /// where-phrase (the `where_fell` family); the shipped Kilij gate,
    /// which predates the content doc, keeps the screen-and-cause row.
    #[test]
    fn a_gate_defeat_pages_the_book_with_the_maps_where_phrase() {
        let mut game = ClientGame::new(fixture_store());
        // A deep file of rising men: the road into the gate takes its
        // tithe, and the mantle must still find an heir at the gate.
        craft_trail_on(
            &mut game,
            12,
            &[(255, true), (250, false), (250, false), (250, false)],
            50,
        );
        drive_into_gate(&mut game, 13);
        auto_lose(&mut game);
        assert_eq!(game.phase(), Phase::Trail, "the mantle rises and the road holds");
        let view = game.snapshot().trail.expect("trail view");
        assert_eq!(view.screen, 13);
        assert!(
            view.last_fallen
                .contains("at the Harim marches before the atabeg Nur ad-Din"),
            "the Book keeps the map's where-phrase: {}",
            view.last_fallen
        );

        let mut kilij = ClientGame::new(fixture_store());
        craft_trail_on(
            &mut kilij,
            8,
            &[(255, true), (250, false), (250, false), (250, false)],
            50,
        );
        drive_into_gate(&mut kilij, 9);
        auto_lose(&mut kilij);
        let view = kilij.snapshot().trail.expect("trail view");
        assert!(
            view.last_fallen
                .contains("at screen 9 — fell in battle upon the pilgrim road"),
            "the shipped gate keeps its screen-and-cause row: {}",
            view.last_fallen
        );
    }

    #[test]
    fn road_calls_resolve_against_the_mantle_and_return_to_the_road() {
        let mut game = embarked_game();
        // A willing hearer by the way (trail ConvertOpportunity = the
        // convert_ref lane): queue through the same PendingCall flow.
        game.queue_trail_call(113).expect("road call queues");
        game.phase = Phase::ConversionChoice;
        let call = game
            .snapshot()
            .conversion
            .expect("a live call rides the snapshot");
        assert_eq!(call.species_id, 113);
        assert_eq!(call.foe_level, 20, "charged at mantle level");
        assert!((3..=85).contains(&call.chance_pct));
        game.handle(ClientInput::ReleaseSoul).expect("release is legal");
        assert_eq!(game.phase(), Phase::Trail, "the road resumes after the choice");
    }

    /// Craft a trail state around a game (succession fixtures), standing
    /// on screen 9.
    fn craft_trail(game: &mut ClientGame, devotions: &[(u8, bool)], morale: i8) {
        craft_trail_on(game, 9, devotions, morale);
    }

    /// The same fixture, positioned on any screen of the spine (gate
    /// tests drive onto their screen from the screen before it).
    fn craft_trail_on(game: &mut ClientGame, screen: u16, devotions: &[(u8, bool)], morale: i8) {
        let men: Vec<te::ManEntry> = devotions
            .iter()
            .enumerate()
            .map(|(i, &(d, champ))| te::ManEntry {
                name: format!("Test-Man {i}"),
                devotion: d,
                alive: true,
                champion: champ,
            })
            .collect();
        game.trail = Some(TrailCampaign {
            state: TrailState {
                day: 10,
                screen,
                supplies: 100,
                morale,
                men,
                fallen: vec![],
                region_flags: 0,
            },
            party: vec![practice_champion(1, 20).expect("mantle instance")],
            vanguard: Vec::new(),
            champion_level: 20,
            over: false,
        });
        game.phase = Phase::Trail;
    }

    /// Put the game into a live trail battle vs one weak foe.
    fn force_trail_battle(game: &mut ClientGame) {
        let foes = vec![practice_champion(108, 5).expect("foe")];
        let b = Battle::new(
            game.trail.as_ref().expect("trail").party.clone(),
            foes,
            [3u8; 32],
        )
        .expect("battle builds");
        game.battle = Some(b);
        game.trail_battle = true;
        game.phase = Phase::InBattle;
    }

    #[test]
    fn the_mantle_rises_in_righteousness_and_the_book_keeps_the_captain() {
        let mut game = ClientGame::new(fixture_store());
        craft_trail(&mut game, &[(255, true), (250, false), (90, false)], 0);
        let fallen_name = "Test-Man 0".to_string();
        force_trail_battle(&mut game);
        auto_lose(&mut game);

        assert_eq!(game.phase(), Phase::Trail, "the company marches on");
        let t = game.trail.as_ref().expect("trail");
        // THE MANTLE: the 250-devotion man rises; the fallen captain's
        // page carries place and cause.
        assert_eq!(t.champion().expect("a champion").name, "Test-Man 1");
        assert_eq!(t.state.fallen.len(), 1);
        let page = &t.state.fallen[0];
        assert_eq!(page.name, fallen_name);
        assert_eq!(page.cause, persist::FallenCause::SlainInBattle.as_str());
        assert_eq!(page.where_fell, 9, "the Book keeps where he fell");
        // The band: former 20 - 5..+2 -> 15..=22, seeded.
        assert!((15..=22).contains(&t.champion_level));
        assert_eq!(t.party[0].level, t.champion_level);
    }

    #[test]
    fn a_reordering_file_bleeds_but_marches_on_with_a_lesser_captain() {
        let mut game = ClientGame::new(fixture_store());
        // All low devotion; high morale -> the reorder roll spares them.
        craft_trail(
            &mut game,
            &[(255, true), (10, false), (10, false), (10, false)],
            100,
        );
        force_trail_battle(&mut game);
        auto_lose(&mut game);

        assert_eq!(game.phase(), Phase::Trail);
        let t = game.trail.as_ref().expect("trail");
        assert_eq!(t.state.living_count(), 3, "high morale spared the file");
        assert_eq!(t.champion_level, 15, "a lesser captain at former - 5");
        assert!(t.champion().is_some(), "some man is named to keep the Book honest");
    }

    #[test]
    fn when_no_soul_remains_the_pilgrimage_is_over() {
        let mut game = ClientGame::new(fixture_store());
        craft_trail(&mut game, &[(255, true), (10, false), (10, false)], -100);
        force_trail_battle(&mut game);
        auto_lose(&mut game);

        assert_eq!(game.phase(), Phase::BattleOver, "the run-over screen");
        let view = game.snapshot().trail.expect("the terminal trail view rides");
        assert!(view.over);
        assert_eq!(view.living, 0);
        assert_eq!(view.fallen_count, 3, "every man has his page");
        assert!(game
            .handle(ClientInput::Travel { to_screen: 10 })
            .is_err());
        game.handle(ClientInput::AcknowledgeResult)
            .expect("the run is acknowledged");
        assert_eq!(game.phase(), Phase::Menu);
    }

    #[test]
    fn save_and_load_mid_journey_restore_the_identical_road() {
        let mut game = embarked_game();
        for _ in 0..3 {
            march_east(&mut game);
        }
        assert_eq!(game.phase(), Phase::Trail);
        let want = persist::load_trail(&game.store)
            .expect("load")
            .expect("the lane is saved");
        let bytes = game.store().save_bytes().expect("document saves");
        let reloaded = ClientGame::new(RosterStore::load_bytes(&bytes).expect("document loads"));
        assert_eq!(reloaded.phase(), Phase::Trail, "the road resumes on load");
        let got = persist::load_trail(&reloaded.store)
            .expect("load")
            .expect("the lane is saved");
        assert_eq!(got, want, "the trail DTO is byte-faithful through the document");
        assert_eq!(reloaded.snapshot().trail, game.snapshot().trail);
        // And the per-day rng streams rebuild: the NEXT march is identical
        // on both sides of the reload.
        let mut a = game;
        let mut b = reloaded;
        for _ in 0..2 {
            march_east(&mut a);
            march_east(&mut b);
            assert_eq!(a.snapshot().trail, b.snapshot().trail);
        }
    }

    #[test]
    fn trail_input_guards_hold() {
        // No trail: Travel/Engage/Abandon are dead ends; StartTrail needs
        // a roster.
        let mut game = ClientGame::new(RosterStore::new());
        assert!(game.handle(ClientInput::Travel { to_screen: 6 }).is_err());
        assert!(game
            .handle(ClientInput::EngageChoice { encounter_ref: 301 })
            .is_err());
        assert!(game.handle(ClientInput::AbandonTrail).is_err());
        assert!(game
            .handle(ClientInput::StartTrail {
                charter_quality: 100,
                provisioning_spend: 100
            })
            .is_err());

        let mut game = embarked_game();
        // Non-adjacent marches are rejected without state movement.
        let day = game.snapshot().trail.expect("trail").day;
        assert!(game.handle(ClientInput::Travel { to_screen: 25 }).is_err());
        assert_eq!(game.snapshot().trail.expect("trail").day, day);
        // A second embark while underway is refused.
        assert!(game
            .handle(ClientInput::StartTrail {
                charter_quality: 100,
                provisioning_spend: 100
            })
            .is_err());
        // An encounter ref not on this screen is refused.
        assert!(game
            .handle(ClientInput::EngageChoice { encounter_ref: 301 })
            .is_err());
        // The Abandon path clears the lane (and the save) and returns.
        game.handle(ClientInput::AbandonTrail).expect("abandon");
        assert_eq!(game.phase(), Phase::Menu);
        assert!(game.snapshot().trail.is_none());
        assert_eq!(
            persist::load_trail(&game.store).expect("load"),
            None,
            "the lane is cleared from the document"
        );
    }

    #[test]
    fn the_choice_prop_opens_an_elective_battle() {
        let mut game = embarked_game();
        // Screen 7 carries ChoiceEncounter(301). March 5 -> 6 -> 7.
        for _ in 0..2 {
            march_east(&mut game);
        }
        let view = game.snapshot().trail.expect("trail");
        assert_eq!(view.screen, 7);
        assert_eq!(view.choice_encounters, vec![301]);
        game.handle(ClientInput::EngageChoice { encounter_ref: 301 })
            .expect("the elective battle engages");
        assert_eq!(game.phase(), Phase::InBattle);
        auto_win(&mut game);
        assert_eq!(game.phase(), Phase::Trail);
    }

    /// STORAGE-SOAK finding (bug fix, 2026-10-17): a road victory spent
    /// the champion's zeal, and the trail victory path cloned survivors
    /// unrested-in-zeal, so the next battle entry failed validation.
    #[test]
    fn road_victory_refills_zeal_for_the_next_battle_entry() {
        let mut game = embarked_game();
        // Reach the Kilij gate (screen 9) live, then fabricate a win in
        // which the mantle's zeal was genuinely spent.
        for _ in 0..3 {
            march_east(&mut game);
        }
        game.handle(ClientInput::Travel { to_screen: 9 })
            .expect("the gate march is legal");
        assert_eq!(game.phase(), Phase::InBattle);
        {
            let b = game.battle.as_mut().expect("gate battle live");
            let me = &mut b.sides[usize::from(MY_SIDE)].party[0];
            for slot in me.rites.iter_mut().flatten() {
                slot.zeal_left = 0;
            }
            for f in &mut b.sides[usize::from(FOE_SIDE)].party {
                f.current_resolve = 0;
            }
            b.winner = Some(MY_SIDE);
        }
        game.on_battle_end(MY_SIDE).expect("won battle routes");
        while game.phase() == Phase::ConversionChoice {
            game.handle(ClientInput::ReleaseSoul)
                .expect("release is legal");
        }
        // Survivors march on unrested in RESOLVE, but their rites are
        // field-reset to full zeal — the next Battle::new must be legal.
        let t = game.trail.as_ref().expect("trail underway");
        for ch in &t.party {
            for slot in ch.rites.iter().flatten() {
                let r = data::rite_by_id(slot.rite_id).expect("known rite");
                assert_eq!(
                    slot.zeal_left, r.zeal,
                    "zeal refilled for the next road battle"
                );
            }
        }
    }

    // ---------- A3: the van (docs/TRAIL-BATTLE-DEPTH.md section 3) ----------

    /// Open a road battle against one token foe via the real entry seam
    /// (gates unset `trail_gate`, so this is a road/wild entry — the van
    /// syncs). Returns nothing; the battle is live on the game.
    fn open_road_battle(game: &mut ClientGame) {
        let foes = vec![practice_champion(112, 5).expect("foe")];
        game.enter_trail_battle(foes, "Foes rise from the wilds!".to_string())
            .expect("road battle enters");
    }

    #[test]
    fn the_van_mints_at_captain_minus_two_and_fights_the_road() {
        let mut game = ClientGame::new(fixture_store());
        craft_trail_on(
            &mut game,
            8,
            &[(255, true), (200, false), (90, false), (90, false)],
            0,
        );
        let mantle_species = game.trail.as_ref().expect("trail").party[0].species_id;
        open_road_battle(&mut game);
        let b = game.battle.as_ref().expect("road battle live");
        let mine = &b.sides[usize::from(MY_SIDE)].party;
        assert_eq!(mine.len(), 3, "the mantle plus two vanguardsmen");
        assert_eq!(b.sides[usize::from(MY_SIDE)].active_idx, 0, "the mantle leads");
        for van in &mine[1..] {
            assert_eq!(van.level, 18, "minted at the captain's level - 2");
            assert_eq!(van.species_id, mantle_species, "the Order's own breed");
        }
        let t = game.trail.as_ref().expect("trail");
        assert_eq!(
            t.vanguard.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(),
            vec!["Test-Man 1", "Test-Man 2"],
            "the van is drawn from the living file, in roster order"
        );
        // The view names the standing van.
        let v = game.snapshot().trail.expect("trail view");
        assert_eq!(v.vanguard.len(), 2);
    }

    #[test]
    fn a_depleted_file_fights_short_and_a_lone_captain_alone() {
        // Two living non-champions -> two-man van (the slot cap).
        let mut game = ClientGame::new(fixture_store());
        craft_trail_on(&mut game, 8, &[(255, true), (200, false)], 0);
        open_road_battle(&mut game);
        let b = game.battle.as_ref().expect("road battle live");
        assert_eq!(b.sides[usize::from(MY_SIDE)].party.len(), 2, "one slot open");

        // No living non-champion -> the captain fights alone.
        let mut game = ClientGame::new(fixture_store());
        craft_trail_on(&mut game, 8, &[(255, true)], 0);
        open_road_battle(&mut game);
        let b = game.battle.as_ref().expect("road battle live");
        assert_eq!(b.sides[usize::from(MY_SIDE)].party.len(), 1, "the lone captain");
        assert!(
            game.snapshot()
                .log_tail
                .iter()
                .any(|l| l.contains("no man beside him")),
            "the lone captain's ledger line reads"
        );
    }

    #[test]
    fn the_gate_remains_the_captains_own_fight() {
        // Fabricate the gate seam directly: with trail_gate set, the van
        // does not stand even with a full file behind it.
        let mut game = ClientGame::new(fixture_store());
        craft_trail_on(
            &mut game,
            8,
            &[(255, true), (200, false), (90, false), (90, false)],
            0,
        );
        game.trail_gate = Some(101);
        open_road_battle(&mut game);
        let b = game.battle.as_ref().expect("gate battle live");
        assert_eq!(
            b.sides[usize::from(MY_SIDE)].party.len(),
            1,
            "gates are single file, exactly as shipped"
        );
        assert!(
            !game.snapshot()
                .log_tail
                .iter()
                .any(|l| l.contains("bears the mantle; with him stand")),
            "no muster line at a battle of record"
        );
    }

    /// Fabricate a lost road battle with named casualties: indices into
    /// the van (0-based) felled where they stood; the mantle felled per
    /// flag. Settles through the real router.
    fn settle_road_defeat(game: &mut ClientGame, van_felled: &[usize], mantle_felled: bool) {
        {
            let b = game.battle.as_mut().expect("a battle is live");
            for i in van_felled {
                b.sides[usize::from(MY_SIDE)].party[i + 1].current_resolve = 0;
            }
            if mantle_felled {
                b.sides[usize::from(MY_SIDE)].party[0].current_resolve = 0;
            }
            b.winner = Some(FOE_SIDE);
        }
        game.on_battle_end(FOE_SIDE).expect("lost battle routes");
    }

    #[test]
    fn a_felled_vanguardsman_gains_his_page_and_the_roster_marks_him() {
        let mut game = ClientGame::new(fixture_store());
        craft_trail_on(
            &mut game,
            8,
            &[(255, true), (200, false), (90, false), (90, false)],
            0,
        );
        open_road_battle(&mut game);
        // The shield-man (van slot 1) falls; the captain falls too; the
        // 200-devotion man of the van lives to rise.
        settle_road_defeat(&mut game, &[1], true);

        let t = game.trail.as_ref().expect("trail");
        assert!(
            !t.state
                .men
                .iter()
                .find(|m| m.name == "Test-Man 2")
                .expect("roster row")
                .alive,
            "his name is struck from the living"
        );
        let page = t
            .state
            .fallen
            .iter()
            .find(|f| f.name == "Test-Man 2")
            .expect("a page of the Book");
        assert_eq!(page.cause, persist::FallenCause::SlainInTheVan.as_str());
        assert_eq!(page.where_fell, 8);
        // THE MANTLE: the surviving van man rose in righteousness; his
        // vanguard body retires with the old mantle.
        assert_eq!(t.champion().expect("a champion").name, "Test-Man 1");
        assert!(
            t.vanguard.iter().all(|v| v.name != "Test-Man 1"),
            "the risen man's van body retires"
        );
        // The next road entry re-mints around him at the new band.
        open_road_battle(&mut game);
        let t = game.trail.as_ref().expect("trail");
        assert_eq!(t.vanguard.len(), 1, "Test-Man 3 fills the open place");
        assert_eq!(t.vanguard[0].name, "Test-Man 3");
        assert_eq!(
            t.vanguard[0].instance.level,
            t.champion_level.saturating_sub(2).clamp(5, MAX_LEVEL),
            "re-banded to the risen captain's -2"
        );
    }

    #[test]
    fn a_won_field_still_pages_the_felled_man_of_the_van() {
        let mut game = ClientGame::new(fixture_store());
        craft_trail_on(
            &mut game,
            8,
            &[(255, true), (200, false), (90, false), (90, false)],
            0,
        );
        open_road_battle(&mut game);
        {
            let b = game.battle.as_mut().expect("battle live");
            // The foe falls; the van loses its second man in the winning.
            for f in &mut b.sides[usize::from(FOE_SIDE)].party {
                f.current_resolve = 0;
            }
            b.sides[usize::from(MY_SIDE)].party[2].current_resolve = 0;
            b.winner = Some(MY_SIDE);
        }
        game.on_battle_end(MY_SIDE).expect("won battle routes");
        let t = game.trail.as_ref().expect("trail");
        assert!(t
            .state
            .fallen
            .iter()
            .any(|f| f.name == "Test-Man 2"
                && f.cause == persist::FallenCause::SlainInTheVan.as_str()),
            "victory does not spare the page");
        assert!(
            t.vanguard.iter().any(|v| v.name == "Test-Man 1")
                && !t.vanguard.iter().any(|v| v.name == "Test-Man 2"),
            "the standing survive, the slain leave the van"
        );
    }

    // ---------- Make Camp (docs/RECOVERY.md section 5) ----------

    #[test]
    fn make_camp_guards_hold() {
        // Off-prop screen: the verb does not stand.
        let mut game = ClientGame::new(fixture_store());
        craft_trail(&mut game, &[(255, true), (200, false)], 0);
        assert!(game.handle(ClientInput::MakeCamp).is_err());
        // From the menu there is no pilgrimage to halt.
        let mut game = ClientGame::new(fixture_store());
        assert!(game.handle(ClientInput::MakeCamp).is_err());

        // Lean stores refuse without spending the day.
        let mut game = ClientGame::new(fixture_store());
        craft_trail_on(&mut game, 3, &[(255, true), (200, false), (10, false)], 0);
        game.trail.as_mut().expect("trail").state.supplies = 0;
        let day0 = game.snapshot().trail.expect("trail").day;
        game.handle(ClientInput::MakeCamp)
            .expect("a lean stores refusal is a no-op, not an error");
        let t = game.trail.as_ref().expect("trail");
        assert_eq!(t.state.day, day0, "no day is spent on a refused halt");
        assert!(
            game.snapshot()
                .log_tail
                .iter()
                .any(|l| l.contains("too lean for a halt")),
            "the refusal keeps the register"
        );
        // An ended pilgrimage cannot camp.
        game.trail.as_mut().expect("trail").over = true;
        assert!(game.handle(ClientInput::MakeCamp).is_err());
    }

    #[test]
    fn a_camp_halt_pays_its_day_and_mends_only_toward_the_ceiling() {
        let mut game = ClientGame::new(fixture_store());
        craft_trail_on(&mut game, 3, &[(255, true), (200, false), (10, false)], 0);
        assert!(
            game.snapshot().trail.expect("trail").can_camp,
            "screen 3 carries the camp ground"
        );
        {
            let t = game.trail.as_mut().expect("trail");
            let m = &mut t.party[0];
            let sp = data::species_by_id(m.species_id).expect("species");
            let max = data::compute_stats(m, sp).max_resolve;
            m.current_resolve = max / 4; // a wounded captain
            m.ailment = Some(Ailment::Venom);
        }
        let (day0, sup0) = {
            let t = game.trail.as_ref().expect("trail");
            (t.state.day, t.state.supplies)
        };
        let want_resolve = {
            let t = game.trail.as_ref().expect("trail");
            let m = &t.party[0];
            let sp = data::species_by_id(m.species_id).expect("species");
            let max = data::compute_stats(m, sp).max_resolve;
            (m.current_resolve + max * te::CAMP_RESOLVE_PCT_NUM / te::CAMP_RESOLVE_PCT_DEN)
                .min(max * te::CAMP_RESOLVE_CAP_NUM / te::CAMP_RESOLVE_CAP_DEN)
        };
        game.handle(ClientInput::MakeCamp).expect("camp is legal here");
        let t = game.trail.as_ref().expect("trail");
        assert_eq!(t.state.day, day0 + 1, "a camp is a day");
        // 3 souls -> upkeep 1, doubled to 2; no trail-event roll.
        assert_eq!(t.state.supplies, sup0 - 2, "double rations, nothing foraged");
        assert_eq!(t.state.morale, te::CAMP_MORALE_GAIN);
        match game.phase() {
            Phase::Trail => {
                // The night was quiet: the mend is on the mantle.
                let m = &t.party[0];
                assert_eq!(m.current_resolve, want_resolve, "the balms bind");
                assert!(m.ailment.is_none(), "the ailment is lifted");
            }
            Phase::InBattle => {
                // The watch cried: fought from the UNMENDED stance.
                let m = &t.party[0];
                let sp = data::species_by_id(m.species_id).expect("species");
                let max = data::compute_stats(m, sp).max_resolve;
                assert_eq!(m.current_resolve, max / 4, "no balms before the fight");
                // The road battle fields the van, as any road fight.
                let b = game.battle.as_ref().expect("night battle live");
                assert_eq!(
                    b.sides[usize::from(MY_SIDE)].party.len(),
                    2,
                    "the van stands to the watch"
                );
                auto_win(&mut game);
                assert_eq!(game.phase(), Phase::Trail, "the night is kept");
                let t = game.trail.as_ref().expect("trail");
                let m = &t.party[0];
                assert_eq!(m.current_resolve, want_resolve, "the mend rides the dawn");
                assert!(m.ailment.is_none());
            }
            p => panic!("camp must end on the road or the night watch, got {p:?}"),
        }
    }

    #[test]
    fn a_pyrrhic_night_watch_writes_no_dawn_mend() {
        // The watch is kept but the captain fell holding it: the mend
        // and the dawn line belong to a STANDING mantle (review NIT).
        let mut game = ClientGame::new(fixture_store());
        craft_trail_on(&mut game, 3, &[(255, true), (200, false), (10, false)], 0);
        // A camp's mend awaiting its night-ambush battle.
        game.camp_mend = Some(1);
        force_trail_battle(&mut game);
        {
            // Pyrrhic: the foes are finished but the captain is felled.
            let b = game.battle.as_mut().expect("night battle live");
            for f in &mut b.sides[usize::from(FOE_SIDE)].party {
                f.current_resolve = 0;
            }
            b.sides[usize::from(MY_SIDE)].party[0].current_resolve = 0;
            b.winner = Some(MY_SIDE);
        }
        game.on_battle_end(MY_SIDE).expect("won battle routes");
        let lines = game.snapshot().log_tail;
        assert!(
            !lines.iter().any(|l| l.contains("By dawn the wounded are tended")),
            "no dawn mend over the captain's corpse"
        );
        assert!(
            !lines.iter().any(|l| l.contains("The night is kept")),
            "no kept-dawn line on a Pyrrhic watch"
        );
        // The ordinary succession path ran: the captain's page is in the
        // Book and the company answered the fall.
        let t = game.trail.as_ref().expect("trail");
        assert!(
            t.state.fallen.iter().any(|f| f.name == "Test-Man 0"),
            "the captain's page is written"
        );
    }
}

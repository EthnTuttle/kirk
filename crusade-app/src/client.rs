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

use crate::policy::greedy_action;
use crate::view::{BattleChampionView, ChampionCard, ClientInput, Phase, RiteCard, ViewSnapshot};
use crate::AppError;

/// Side 0 is always the local player in M5a practice battles.
const MY_SIDE: u8 = 0;
const FOE_SIDE: u8 = 1;
/// Ring buffer bound for the human-readable log.
const LOG_CAP: usize = 200;
/// How many log lines a snapshot exposes.
const LOG_TAIL: usize = 8;
/// Practice battles use a fixed seed; real duels arrive via commit-reveal.
const PRACTICE_SEED: [u8; 32] = [0x50; 32];
/// Deterministic practice theme team (ids into the species table).
const PRACTICE_FOE_SPECIES: [u16; 3] = [3, 8, 10];
/// A battle party fields at most 6 champions.
const PARTY_CAP: usize = 6;

#[derive(Debug)]
pub struct ClientGame {
    store: RosterStore,
    battle: Option<Battle>,
    phase: Phase,
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
            log: Vec::new(),
            fresh_events: RefCell::new(Vec::new()),
        }
    }

    pub fn store(&mut self) -> &mut RosterStore {
        &mut self.store
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
            fresh_events,
        };
        let Some(b) = self.battle.as_ref() else {
            return snap;
        };
        if matches!(
            self.phase,
            Phase::InBattle | Phase::ForcedSwitchSelect | Phase::BattleOver
        ) {
            snap.me = Some(champion_view(b, MY_SIDE));
            snap.foe = Some(champion_view(b, FOE_SIDE));
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
                }
            })
            .collect()
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
                Phase::Roster => {
                    self.phase = Phase::Menu;
                    Ok(())
                }
                p => Err(illegal(format!("cannot close to menu from {p:?}"))),
            },
            ClientInput::StartPractice { level } => self.start_practice(level),
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
                    self.fresh_events.borrow_mut().clear();
                    self.phase = Phase::Menu;
                    Ok(())
                }
                p => Err(illegal(format!("no result to acknowledge from {p:?}"))),
            },
        }
    }

    /// StartPractice: clone the (first 6) roster champions, field-reset to
    /// the requested level, and face the deterministic theme team.
    fn start_practice(&mut self, level: u8) -> Result<(), AppError> {
        if !matches!(self.phase, Phase::Menu | Phase::Roster) || self.battle.is_some() {
            return Err(illegal("a battle is already active"));
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
        let mut party_b = Vec::new();
        for &id in &PRACTICE_FOE_SPECIES {
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
            "Practice duel at level {level} begins — facing {foes}."
        )]);
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
        let lines = render_events(b, &events, &mut actives);
        let result = b.winner;
        let my_forced = {
            let acts = b.legal_actions(MY_SIDE);
            result.is_none()
                && !acts.is_empty()
                && acts.iter().all(|a| matches!(a, Action::Switch { .. }))
        };
        self.push_log(lines);
        self.fresh_events.borrow_mut().extend(events);
        self.phase = if result.is_some() {
            Phase::BattleOver
        } else if my_forced {
            Phase::ForcedSwitchSelect
        } else {
            Phase::InBattle
        };
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
        let lines = render_events(b, &events, &mut actives);
        self.push_log(lines);
        self.fresh_events.borrow_mut().extend(events);
        self.phase = Phase::InBattle;
        Ok(())
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
        },
        level,
    )
}

// ---------- View helpers (pure rendering over battle/store state) ----------

fn species_name(c: &ChampionInstance) -> String {
    data::species_by_id(c.species_id).map_or_else(
        || format!("Champion #{}", c.species_id),
        |sp| sp.name.to_string(),
    )
}

fn champion_view(b: &Battle, side: u8) -> BattleChampionView {
    let s = &b.sides[usize::from(side)];
    let c = s.active();
    let sp = data::species_by_id(c.species_id);
    BattleChampionView {
        ours: side == MY_SIDE,
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

fn rite_cards(c: &ChampionInstance) -> Vec<RiteCard> {
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

fn render_events(b: &Battle, events: &[BattleEvent], actives: &mut [usize; 2]) -> Vec<String> {
    events.iter().map(|e| render_event(b, e, actives)).collect()
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
/// lines attribute names correctly. Unknown future variants degrade to
/// their Debug form.
#[allow(unreachable_patterns)] // forward-compat: future BattleEvent variants hit the catch-all
fn render_event(b: &Battle, e: &BattleEvent, actives: &mut [usize; 2]) -> String {
    match e {
        BattleEvent::TurnStart { turn } => format!("— Turn {turn} —"),
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
            if *side == MY_SIDE {
                "Choose your next champion.".to_string()
            } else {
                "The foe must send out another champion.".to_string()
            }
        }
        BattleEvent::BattleEnded { winner } => {
            if *winner == MY_SIDE {
                "Battle over — you prevail.".to_string()
            } else {
                "Battle over — the foe prevails.".to_string()
            }
        }
        unknown => format!("{unknown:?}"),
    }
}

//! Turn-based battle engine (Worker B: implements bodies + tests).
//! Contract owned by orchestrator: do not change public signatures.
//!
//! Turn flow rules (classic, fixed order — determinism is mandatory):
//! 1. Yield resolves immediately. Switches happen before rites.
//! 2. Rite order: priority desc, then effective Alacrity desc, then RNG coin.
//! 3. Pre-action checks: Slumber/Frost/Shackled/Confusion gates, then zeal,
//!    then accuracy. Crit base rate 1/24 (+1 stage for HighCrit rites,
//!    stage>=3 => always). Damage = ((2L/5+2)*P*A/D)/50 + 2, then
//!    *STAB(150%) *effectiveness% *rand85..100% *crit150% *mods; all floored.
//!    Minimum 1 damage when effectiveness > 0. Immune (0%) => Missed-like event.
//! 4. Secondaries apply in rite's declared order, chance-gated, after damage.
//! 5. End of turn: Brand/Venom chip, SeedLeech drain, Slumber/Frost timers.
//! 6. Yielded (fainted) champions trigger ForcedSwitch; last one ends battle.
//! Effective Alacrity: Shackled halves; stages apply via data::stage_pct.

use serde::{Deserialize, Serialize};

use crate::rng::BattleRng;
use crate::types::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    /// Use the rite in slot 0..4.
    Rite { slot: u8 },
    /// Swap active champion to party index (must be alive, not active).
    Switch { party_idx: u8 },
    /// Concede the battle (honourable surrender).
    Yield,
}

/// Events form the signed, replayable transcript (crusade-net signs these).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BattleEvent {
    TurnStart { turn: u32 },
    RiteUsed { side: u8, rite_id: u16 },
    Missed { side: u8 },
    Immune { side: u8 },
    Damage { side: u8, amount: u16, crit: bool, effectiveness_pct: u8 },
    Healed { side: u8, amount: u16 },
    AilmentApplied { side: u8, ailment: Ailment },
    AilmentEnded { side: u8, ailment: Ailment },
    VolatileApplied { side: u8, volatile_: Volatile },
    StageChanged { side: u8, stat: BattleStat, new_stage: i8 },
    AilmentChip { side: u8, amount: u16 },
    Switched { side: u8, to_party_idx: u8 },
    ChampionYielded { side: u8, party_idx: u8 },
    ForcedSwitchNeeded { side: u8 },
    BattleEnded { winner: u8 },
}

/// Per-active-champion volatile battle state (stages, volatiles, timers).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveState {
    pub stages: [i8; 5], // indexed by BattleStat as usize
    pub volatiles: Vec<(Volatile, u8)>, // (kind, turns_remaining)
    pub slumber_turns: u8,
    pub seeded_by: Option<u8>,
}

/// One player's squad. Max 6. First non-yielded champ starts active.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Side {
    pub party: Vec<ChampionInstance>,
    pub active_idx: usize,
    pub active_state: ActiveState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Battle {
    pub sides: [Side; 2],
    pub turn: u32,
    pub winner: Option<u8>,
    seed: [u8; 32],
    log: Vec<BattleEvent>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BattleError {
    #[error("party size must be 1..=6, got {0}")]
    BadPartySize(usize),
    #[error("champion {0} failed legality: {1}")]
    IllegalChampion(u8, InstanceError),
    #[error("battle is over")]
    BattleOver,
    #[error("rite slot {0} is empty or out of range")]
    BadSlot(u8),
    #[error("rite has no zeal left")]
    NoZeal,
    #[error("switch target invalid")]
    BadSwitch,
    #[error("both actions belong to the same side order violation")]
    SideOrder,
}

impl Battle {
    /// Validate legality of both parties, set first champion active.
    pub fn new(party_a: Vec<ChampionInstance>, party_b: Vec<ChampionInstance>, seed: [u8; 32]) -> Result<Self, BattleError> {
        let _ = (party_a, party_b, seed);
        unimplemented!("worker B")
    }

    pub fn events(&self) -> &[BattleEvent] {
        &self.log
    }

    /// Apply both sides' actions for one turn; appends to the event log.
    /// Returns the events produced this turn (a suffix clone).
    pub fn apply_turn(&mut self, _a: Action, _b: Action) -> Result<Vec<BattleEvent>, BattleError> {
        unimplemented!("worker B")
    }

    /// After a ForcedSwitchNeeded, the side must submit Switch via this.
    pub fn apply_forced_switch(&mut self, _side: u8, _party_idx: u8) -> Result<(), BattleError> {
        unimplemented!("worker B")
    }

    /// Legal actions for a side right now (UI + tests + AI).
    pub fn legal_actions(&self, _side: u8) -> Vec<Action> {
        unimplemented!("worker B")
    }

    pub(crate) fn turn_rng(&self) -> BattleRng {
        BattleRng::for_turn(self.seed, self.turn)
    }
}

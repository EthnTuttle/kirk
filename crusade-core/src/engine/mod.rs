//! Turn-based battle engine. Determinism: all randomness via
//! [`BattleRng::for_turn`]; integer math only; floors at every division.
//!
//! Turn flow:
//! 1. BattleOver check. Unresolved forced switch -> BadSwitch.
//! 2. TurnStart. Yield resolves immediately (side 0 processed first if both).
//! 3. Switches (both sides) before rites.
//! 4. Rite order: priority desc, effective Alacrity desc, rng coin on ties.
//! 5. Per rite: gates (Slumber/Frost/Shackled/Confusion/Flinch) -> attempt
//!    (RiteUsed + zeal) -> accuracy -> hits (MultiHit 2..=5) -> damage
//!    (base -> STAB -> effectiveness -> roll 85..=100 -> crit; min 1 unless
//!    immune) -> secondaries (chance-gated, declared order).
//! 6. End of turn (side 0 then 1): Brand max/16, Venom max/8, SeedLeech
//!    drain max/16 to opponent; Flinch volatile removed (confusion/leech
//!    timers are handled at gate/seeded drains).
//! 7. Faints: ChampionYielded -> ForcedSwitchNeeded (pending flag) or
//!    BattleEnded. turn += 1.
//!
//! Judgment calls (reviewer note):
//! - Blocked attempts (para/sleep/frost/flinch/confusion self-hit) do NOT
//!   spend zeal and do NOT emit RiteUsed.
//! - If the target's active champion is already yielded, the rite misses.
//! - Confusion self-hit: power 40, Typeless, Martial vs own Guard, staged,
//!   no STAB, no variance, no crit.
//! - Sleep counter (1..=3 turns) rolled at application time.
//! - Sleep wears off when a champion leaves the field (switch or forced
//!   switch): the outgoing champion's Slumber ailment is cleared.
//! - Ailment chip min 1.

use serde::{Deserialize, Serialize};

use crate::data;
use crate::rng::BattleRng;
use crate::types::*;

mod attack;
mod resolve;
pub mod trail;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    /// Use the rite in slot 0..4.
    Rite { slot: u8 },
    /// Swap active champion to party index (alive, not active).
    Switch { party_idx: u8 },
    /// Concede the battle.
    Yield,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BattleEvent {
    TurnStart {
        turn: u32,
    },
    RiteUsed {
        side: u8,
        rite_id: u16,
    },
    Missed {
        side: u8,
    },
    Immune {
        side: u8,
    },
    Damage {
        side: u8,
        amount: u16,
        crit: bool,
        effectiveness_pct: u8,
    },
    Healed {
        side: u8,
        amount: u16,
    },
    AilmentApplied {
        side: u8,
        ailment: Ailment,
    },
    AilmentEnded {
        side: u8,
        ailment: Ailment,
    },
    VolatileApplied {
        side: u8,
        volatile_: Volatile,
    },
    StageChanged {
        side: u8,
        stat: BattleStat,
        new_stage: i8,
    },
    AilmentChip {
        side: u8,
        amount: u16,
    },
    Switched {
        side: u8,
        to_party_idx: u8,
    },
    ChampionYielded {
        side: u8,
        party_idx: u8,
    },
    ForcedSwitchNeeded {
        side: u8,
    },
    BattleEnded {
        winner: u8,
    },
}

/// Per-active-champion volatile battle state.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveState {
    /// Indexed by stat_idx(). Range -6..=6.
    pub stages: [i8; 5],
    /// (kind, turns_remaining). Flinch uses u8::MAX sentinel (removed at EOT).
    pub volatiles: Vec<(Volatile, u8)>,
    pub slumber_turns: u8,
}

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
    pending_switch: [bool; 2],
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
}

pub(crate) fn stat_idx(s: BattleStat) -> usize {
    match s {
        BattleStat::Valour => 0,
        BattleStat::Piety => 1,
        BattleStat::Guard => 2,
        BattleStat::Aegis => 3,
        BattleStat::Alacrity => 4,
    }
}

impl Side {
    pub fn active(&self) -> &ChampionInstance {
        &self.party[self.active_idx]
    }
    pub fn active_mut(&mut self) -> &mut ChampionInstance {
        &mut self.party[self.active_idx]
    }
    pub fn alive(&self, idx: usize) -> bool {
        self.party.get(idx).is_some_and(|c| c.current_resolve > 0)
    }
    pub fn any_alive(&self) -> bool {
        self.party.iter().any(|c| c.current_resolve > 0)
    }
    pub fn computed(&self) -> ComputedStats {
        let c = self.active();
        let sp = data::species_by_id(c.species_id).expect("validated at construction");
        data::compute_stats(c, sp)
    }
    fn has_volatile(&self, v: Volatile) -> bool {
        self.active_state.volatiles.iter().any(|(k, _)| *k == v)
    }
}

impl Battle {
    pub fn new(
        party_a: Vec<ChampionInstance>,
        party_b: Vec<ChampionInstance>,
        seed: [u8; 32],
    ) -> Result<Self, BattleError> {
        if party_a.is_empty() || party_a.len() > 6 {
            return Err(BattleError::BadPartySize(party_a.len()));
        }
        if party_b.is_empty() || party_b.len() > 6 {
            return Err(BattleError::BadPartySize(party_b.len()));
        }
        for (i, party) in [&party_a, &party_b].into_iter().enumerate() {
            for (j, c) in party.iter().enumerate() {
                let sp = data::species_by_id(c.species_id).ok_or(BattleError::IllegalChampion(
                    (i * 6 + j) as u8,
                    InstanceError::IllegalRite(0),
                ))?;
                validate_instance(c, sp)
                    .map_err(|e| BattleError::IllegalChampion((i * 6 + j) as u8, e))?;
                if c.current_resolve == 0 {
                    return Err(BattleError::IllegalChampion(
                        (i * 6 + j) as u8,
                        InstanceError::NotBattleReady,
                    ));
                }
            }
        }
        Ok(Self {
            sides: [
                Side {
                    party: party_a,
                    active_idx: 0,
                    active_state: ActiveState::default(),
                },
                Side {
                    party: party_b,
                    active_idx: 0,
                    active_state: ActiveState::default(),
                },
            ],
            turn: 1,
            winner: None,
            seed,
            log: vec![],
            pending_switch: [false, false],
        })
    }

    pub fn events(&self) -> &[BattleEvent] {
        &self.log
    }

    pub(crate) fn turn_rng(&self) -> BattleRng {
        BattleRng::for_turn(self.seed, self.turn)
    }
}

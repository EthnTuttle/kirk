//! ClientGame: the state machine a UI drives. Worker H implements.
//!
//! Owns a RosterStore + an optional active battle (practice mode for M5a;
//! net duels plug in via crusade-net later without view changes).
//! `handle(input)` mutates state; `snapshot()` renders.
//!
//! Practice battle flow: StartPractice -> ForcedSwitchSelect/InBattle...
//! The AI opponent uses policy::greedy_policy. Events append to log_tail
//! (human-readable) and fresh_events (raw, cleared on next input).

use crusade_core::engine::Battle;
use crusade_data::RosterStore;

use crate::view::{ClientInput, ViewSnapshot};
use crate::AppError;

#[derive(Debug)]
pub struct ClientGame {
    store: RosterStore,
    battle: Option<Battle>,
    // worker: phase, log buffer, my_side, etc.
}

impl ClientGame {
    pub fn new(store: RosterStore) -> Self {
        Self {
            store,
            battle: None,
        }
    }

    pub fn store(&mut self) -> &mut RosterStore {
        &mut self.store
    }

    pub fn snapshot(&self) -> ViewSnapshot {
        unimplemented!("worker H")
    }

    pub fn handle(&mut self, _input: ClientInput) -> Result<(), AppError> {
        unimplemented!("worker H")
    }
}

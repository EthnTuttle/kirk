//! Practice-opponent AI (worker H). Same greedy policy as crusade-sim:
//! highest power*effectiveness usable rite, ties -> lowest slot; switch only
//! when forced (first alive).

use crusade_core::engine::{Action, Battle};

/// Choose the AI's action for `side`. Callers ensure the battle isn't over
/// and no forced-switch is pending (handle those via legal_actions first).
pub fn greedy_action(_battle: &Battle, _side: u8) -> Action {
    unimplemented!("worker H")
}

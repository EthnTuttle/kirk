//! Practice-opponent AI (worker H). Same greedy policy as crusade-sim:
//! highest power*effectiveness usable rite, ties -> lowest slot; switch only
//! when forced (first alive).

use crusade_core::data;
use crusade_core::engine::{Action, Battle};
use crusade_core::types::RiteCategory;

/// Choose the AI's action for `side`. Callers ensure the battle isn't over
/// and no forced-switch is pending (handle those via legal_actions first).
///
/// Selection runs over `battle.legal_actions(side)`: every damaging usable
/// rite is valued at `power * effectiveness(rite type, opponent active
/// typing)` and the maximum is played (ties keep the lowest slot, since
/// `legal_actions` enumerates slots in ascending order and only a strict
/// improvement displaces the incumbent). With no damaging rite usable the
/// first usable rite is played; with no rite at all the first switch is
/// taken; `Yield` is returned only when nothing else is legal.
pub fn greedy_action(battle: &Battle, side: u8) -> Action {
    debug_assert!(side <= 1, "sides are 0-indexed");
    let s = usize::from(side.min(1));
    let me = battle.sides[s].active();
    let opp_typing =
        data::species_by_id(battle.sides[1 - s].active().species_id).map(|sp| sp.typing);

    let mut first_rite: Option<u8> = None;
    let mut first_switch: Option<u8> = None;
    let mut best: Option<(u8, u32)> = None;
    for action in battle.legal_actions(side) {
        match action {
            Action::Rite { slot } => {
                if first_rite.is_none() {
                    first_rite = Some(slot);
                }
                let Some(slot_ref) = me.rites.get(usize::from(slot)).and_then(Option::as_ref)
                else {
                    continue;
                };
                let Some(rite) = data::rite_by_id(slot_ref.rite_id) else {
                    continue;
                };
                if rite.category == RiteCategory::Status || rite.power == 0 {
                    continue;
                }
                let eff = opp_typing.map_or(100, |t| data::effectiveness(rite.rite_type, t));
                let score = u32::from(rite.power) * u32::from(eff);
                if best.is_none_or(|(_, incumbent)| score > incumbent) {
                    best = Some((slot, score));
                }
            }
            Action::Switch { party_idx } => {
                if first_switch.is_none() {
                    first_switch = Some(party_idx);
                }
            }
            Action::Yield => {}
        }
    }
    match (best, first_rite, first_switch) {
        (Some((slot, _)), _, _) | (None, Some(slot), _) => Action::Rite { slot },
        (None, None, Some(party_idx)) => Action::Switch { party_idx },
        (None, None, None) => Action::Yield,
    }
}

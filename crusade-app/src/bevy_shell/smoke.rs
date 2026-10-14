//! `--smoke`: a scripted practice battle for headless CI. Drives the exact
//! same [`ClientInput`]s a player would click (one per frame), for 120
//! frames, then asks the app to exit with [`AppExit::Success`].

use bevy::prelude::*;

use crusade_app::{ClientInput, Phase, ViewSnapshot};

use crate::state::{switch_target_idx, GameShell, InputQueue};

/// Total frames a smoke run lasts before exiting 0.
pub const SMOKE_FRAMES: u64 = 120;
/// Smoke duels always play the level-25 practice table.
pub const SMOKE_LEVEL: u8 = 25;

#[derive(Resource, Default)]
pub struct SmokeDriver {
    /// Frames elapsed so far.
    pub frame: u64,
    /// Whether the practice duel has been kicked off yet.
    pub started: bool,
    /// Battle turns played (rites picked or switches made).
    pub turns_played: u32,
    /// Battles concluded and acknowledged.
    pub battles_done: u32,
}

/// One scripted input per frame (queued inputs process the same frame, so a
/// fresh snapshot is visible next frame). Runs before `process_inputs`.
pub fn drive(
    shell: Res<GameShell>,
    mut queue: ResMut<InputQueue>,
    mut driver: ResMut<SmokeDriver>,
    mut exit: MessageWriter<AppExit>,
) {
    driver.frame += 1;
    if driver.frame >= SMOKE_FRAMES {
        let game = shell.game.lock().expect("game mutex poisoned");
        let snap = game.snapshot();
        // Evidence line for CI logs.
        println!(
            "crusade-desktop smoke: frames={} turns={} battles_done={} phase={:?} turn={}",
            driver.frame, driver.turns_played, driver.battles_done, snap.phase, snap.turn
        );
        exit.write(AppExit::Success);
        return;
    }
    // Never pile up the queue: one input per frame lets every intermediate
    // phase (forced switches, battle-over) render at least once.
    if !queue.0.is_empty() {
        return;
    }
    let snap = shell.game.lock().expect("game mutex poisoned").snapshot();
    let input = match snap.phase {
        Phase::Menu if !driver.started => {
            driver.started = true;
            Some(ClientInput::StartPractice { level: SMOKE_LEVEL })
        }
        Phase::InBattle => {
            driver.turns_played += 1;
            Some(scripted_battle_input(&snap))
        }
        Phase::ForcedSwitchSelect => snap.can_switch_to.first().map(|t| ClientInput::PickSwitch {
            party_idx: switch_target_idx(t),
        }),
        Phase::BattleOver => {
            driver.battles_done += 1;
            Some(ClientInput::AcknowledgeResult)
        }
        _ => None,
    };
    if let Some(input) = input {
        queue.0.push(input);
    }
}

/// "Auto-pick rite 0 each turn", with the proven test-fixture fallbacks so
/// zeal exhaustion or a dead active can never stall the scripted battle.
pub(crate) fn scripted_battle_input(snap: &ViewSnapshot) -> ClientInput {
    if snap.my_rites.iter().any(|r| r.slot == 0 && r.usable) {
        ClientInput::PickRite { slot: 0 }
    } else if let Some(r) = snap.my_rites.iter().find(|r| r.usable) {
        ClientInput::PickRite { slot: r.slot }
    } else if let Some(t) = snap.can_switch_to.first() {
        ClientInput::PickSwitch {
            party_idx: switch_target_idx(t),
        }
    } else {
        ClientInput::Concede
    }
}

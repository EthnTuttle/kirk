//! Button interactions -> [`ClientInput`] queue, and the fixed system that
//! feeds the queue to the game once per frame.

use bevy::prelude::*;

use crusade_app::{ClientGame, Phase};

use crate::state::{ActionButton, BaseBackground, GameShell, InputQueue, SnapshotVersion};
use crate::style;

/// All action buttons whose interaction state changed this frame.
type ActionButtons<'w, 's> = Query<
    'w,
    's,
    (
        &'static Interaction,
        &'static ActionButton,
        &'static mut BackgroundColor,
        &'static BaseBackground,
    ),
    (Changed<Interaction>, With<Button>),
>;

/// A pressed action button queues its input. Hover/press tints give cheap
/// feedback between rebuilds; a rebuild respawns the button in its base
/// colour anyway.
pub fn collect_clicks(mut queue: ResMut<InputQueue>, mut buttons: ActionButtons) {
    for (interaction, action, mut bg, base) in &mut buttons {
        match interaction {
            Interaction::Pressed => {
                queue.0.push(action.0.clone());
                bg.0 = style::BUTTON_PRESSED;
            }
            Interaction::Hovered => bg.0 = style::BUTTON_HOVER,
            Interaction::None => bg.0 = base.0,
        }
    }
}

/// Feed every queued input to the game, one frame at a time, in order.
/// Each accepted input bumps [`SnapshotVersion`] — the sole rebuild trigger.
/// Rejected inputs (e.g. a click that a rebuild made stale) are logged and
/// dropped; the client stays the source of truth.
pub fn process_inputs(
    shell: Res<GameShell>,
    mut queue: ResMut<InputQueue>,
    mut version: ResMut<SnapshotVersion>,
) {
    if queue.0.is_empty() {
        return;
    }
    let inputs = std::mem::take(&mut queue.0);
    let mut game = shell.game.lock().expect("game mutex poisoned");
    for input in inputs {
        match game.handle(input) {
            Ok(()) => {
                version.0 = version.0.wrapping_add(1);
                persist_store(&mut game);
            }
            Err(err) => warn!("client rejected input: {err}"),
        }
    }
}

/// Browser persistence hook (a no-op off-wasm): write the roster document
/// after every accepted input EXCEPT the mid-battle phases — battles never
/// mutate the roster store, so per-turn writes there would be pure churn.
fn persist_store(game: &mut ClientGame) {
    if matches!(game.phase(), Phase::InBattle | Phase::ForcedSwitchSelect) {
        return;
    }
    if let Ok(bytes) = game.store().save_bytes() {
        crusade_app::persist::web_save_document(&bytes);
    }
}

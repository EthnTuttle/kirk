//! Shared resources and markers for the Bevy shell, plus the tiny string
//! helpers both the UI and the smoke driver need for switch targets.

use std::sync::Mutex;

use bevy::prelude::*;

use crusade_app::{ClientGame, ClientInput};

/// The one game state machine the shell drives. `ClientGame` is `Send` but
/// not `Sync` (its `fresh_events` take-drain buffer is a `RefCell`), and
/// bevy resources must be `Send + Sync`, hence the `Mutex`: every system
/// locks it only for the duration of a `handle()`/`snapshot()` call.
#[derive(Resource)]
pub struct GameShell {
    pub game: Mutex<ClientGame>,
}

/// Button presses (and the smoke script) land here; `process_inputs` drains
/// the queue once per frame, in order, so the synchronous client never sees
/// re-entrant calls.
#[derive(Resource, Default)]
pub struct InputQueue(pub Vec<ClientInput>);

/// Bumped once per successfully handled input. Rebuilding the UI compares
/// this against [`RenderedVersion`]; idle frames therefore cost nothing.
/// (Practice/campaign foes act inside `handle()`, so the next snapshot after
/// any handled input is already the full next visual state.)
#[derive(Resource, Default)]
pub struct SnapshotVersion(pub u64);

/// Snapshot version currently on screen; `None` before the first build.
#[derive(Resource, Default)]
pub struct RenderedVersion(pub Option<u64>);

/// Marks the single root UI node, rebuilt wholesale on version change.
#[derive(Component)]
pub struct UiRoot;

/// A button whose press queues one [`ClientInput`].
#[derive(Component, Clone)]
pub struct ActionButton(pub ClientInput);

/// A button's resting background, so hover/press tints can revert.
#[derive(Component)]
pub struct BaseBackground(pub Color);

/// `can_switch_to` entries arrive as `"<party_idx>:<species name>"`.
pub(crate) fn switch_target_idx(target: &str) -> u8 {
    target
        .split(':')
        .next()
        .and_then(|head| head.parse().ok())
        .expect("can_switch_to entries look like '<idx>:<name>'")
}

/// Display half of a `can_switch_to` entry (falls back to the whole string).
pub(crate) fn switch_target_name(target: &str) -> &str {
    target.split(':').nth(1).unwrap_or(target)
}

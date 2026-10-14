//! Shared resources and markers for the Bevy shell, plus the tiny string
//! helpers both the UI and the smoke driver need for switch targets.

use std::sync::{Arc, Mutex};

use bevy::prelude::*;

use crusade_app::{ClientGame, ClientInput, LobbyNet, ViewSnapshot};
use crusade_core::types::ChampionInstance;
use tokio::sync::mpsc::UnboundedSender;

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

// -------------------------------------------------------------------------
// Net bridge (Tournament of Virtue, v1.1 card B2)
//
// The shell is a synchronous bevy loop; lobby + net-duel work is async.
// Exactly ONE background task exists per lobby session (lobby) which then
// continues as the duel task — this struct is the shell-side half of it.
// `main.rs` owns the spawn/teardown system; this resource holds the ends.
// -------------------------------------------------------------------------

/// Sync-shell -> async-task commands.
#[derive(Debug)]
pub enum NetCmd {
    /// Re-fetch the open-challenge listing.
    Refresh,
    /// Issue our challenge with this party, then await an answer and seal.
    Post { party: Vec<ChampionInstance> },
    /// Answer the listed challenge `event_id` with this party, then seal.
    Accept {
        event_id: String,
        party: Vec<ChampionInstance>,
    },
    /// A net-duel turn input (`PickRite` / `PickSwitch` / `Concede`).
    Turn(ClientInput),
    /// Terminal: withdraw (leaving the lobby / acknowledging a duel over).
    Abort,
}

/// How a net duel ended, for the renderer's terminal screens.
#[derive(Debug, Clone)]
pub enum NetDuelEnd {
    /// FinalDigest verified, attestation signed, ladder move applied
    /// client-side (via `LobbyNet::DuelFinished`).
    Finished { won: bool },
    /// Peer fled, link timed out mid-turn, or the task died.
    Aborted { notice: String },
}

/// Async-task -> shell mailbox. Written ONLY by the net task (short lived
/// `std::sync::Mutex` sections, never held across an `.await`); drained by
/// the bridge system each frame.
#[derive(Default)]
pub struct NetShared {
    /// Client-bound lobby/duel reports; the bridge queues them as
    /// `ClientInput::LobbyNet` so the whole transition table stays in the
    /// synchronous client.
    pub events: Vec<LobbyNet>,
    /// The latest net-duel snapshot — emitted by
    /// `crusade_app::pvp::NetDuelDriver::snapshot()`, rendered verbatim by
    /// the existing battle screens.
    pub duel: Option<ViewSnapshot>,
    /// Terminal duel state once set.
    pub out: Option<NetDuelEnd>,
    /// Mutation counter: the bridge bumps `SnapshotVersion` whenever the
    /// task has touched anything since the last drain, so net progress
    /// repaints without a client input.
    pub notify: u64,
}

impl NetShared {
    /// One mutation point for the task: apply `f`, then flag the bridge.
    pub fn update(&mut self, f: impl FnOnce(&mut Self)) {
        f(self);
        self.notify = self.notify.wrapping_add(1);
    }
}

/// Bridge resource: channel into the live task plus the shared mailbox and
/// the last seen mutation counter.
#[derive(Resource, Default)]
pub struct NetBridge {
    pub cmd_tx: Option<UnboundedSender<NetCmd>>,
    pub shared: Option<Arc<Mutex<NetShared>>>,
    last_notify: u64,
}

impl NetBridge {
    /// Attach a freshly spawned task's channel + mailbox.
    pub fn attach(&mut self, tx: UnboundedSender<NetCmd>, shared: Arc<Mutex<NetShared>>) {
        self.cmd_tx = Some(tx);
        self.shared = Some(shared);
        self.last_notify = 0;
    }

    /// Send a command; on failure the task is gone — drop the handles so
    /// the bridge respawns a session on the next lobby frame.
    pub fn send(&mut self, cmd: NetCmd) -> Result<(), ()> {
        let Some(tx) = self.cmd_tx.take() else {
            return Err(());
        };
        match tx.send(cmd) {
            Ok(()) => {
                self.cmd_tx = Some(tx);
                Ok(())
            }
            Err(_) => {
                self.shared = None;
                Err(())
            }
        }
    }

    /// Drain the mailbox into `events`, returning the newest duel snapshot
    /// and terminal state when anything changed since the last drain.
    pub fn poll(&mut self) -> Option<(Vec<LobbyNet>, Option<ViewSnapshot>, Option<NetDuelEnd>)> {
        let shared = self.shared.as_ref()?;
        let mut s = shared.lock().expect("net mailbox mutex poisoned");
        if s.notify == self.last_notify {
            return None;
        }
        self.last_notify = s.notify;
        Some((std::mem::take(&mut s.events), s.duel.clone(), s.out.clone()))
    }

    /// Teardown: ask the task to unwind (channel closes with the drop) and
    /// release the mailbox. In-flight work cancels at its next await.
    pub fn detach(&mut self) {
        if let Some(tx) = self.cmd_tx.take() {
            let _ = tx.send(NetCmd::Abort);
        }
        self.shared = None;
        self.last_notify = 0;
    }
}

/// Native: one multi-thread tokio runtime per app for the (single) net
/// task — nostr-sdk's relay internals drive native tokio scheduling and
/// need a runtime context, so the task is spawned here.
/// (wasm has no tokio runtime; the task runs via `spawn_local` instead,
/// under the same executor model nostr-sdk uses there.)
#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource)]
pub struct NetRuntime(pub tokio::runtime::Runtime);

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

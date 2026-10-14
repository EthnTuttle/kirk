//! Button interactions -> [`ClientInput`] queue, and the fixed system that
//! feeds the queue to the game once per frame.

use bevy::prelude::*;

use crusade_app::{ClientGame, ClientInput, LobbyNet, Phase};

use crate::state::{
    ActionButton, BaseBackground, GameShell, InputQueue, NetBridge, NetCmd, SnapshotVersion,
};
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

/// True while the net task's mailbox holds a live, unfinished duel: battle
/// inputs route there instead of the synchronous client (which is parked
/// in `BattleSetup` for the duel's duration and would reject them).
fn net_duel_live(bridge: &NetBridge) -> bool {
    let Some(shared) = bridge.shared.as_ref() else {
        return false;
    };
    let s = shared.lock().expect("net mailbox mutex poisoned");
    s.duel.is_some() && s.out.is_none()
}

/// Feed every queued input to the game (or, mid-net-duel, to the duel
/// task), one frame at a time, in order. Each accepted input bumps
/// [`SnapshotVersion`] — the sole rebuild trigger. Rejected inputs (e.g. a
/// click that a rebuild made stale) are logged and dropped; the client
/// stays the source of truth.
pub fn process_inputs(
    shell: Res<GameShell>,
    mut bridge: ResMut<NetBridge>,
    mut queue: ResMut<InputQueue>,
    mut version: ResMut<SnapshotVersion>,
) {
    if queue.0.is_empty() {
        return;
    }
    let inputs = std::mem::take(&mut queue.0);
    let mut game = shell.game.lock().expect("game mutex poisoned");
    for input in inputs {
        if net_duel_live(&bridge)
            && matches!(
                input,
                ClientInput::PickRite { .. }
                    | ClientInput::PickSwitch { .. }
                    | ClientInput::Concede
            )
        {
            if bridge.send(NetCmd::Turn(input)).is_err() {
                // The task died silently: surface it as an abort, in-theme,
                // rather than leaving the duel screen frozen forever.
                net_task_died_during_duel(&mut game, &mut version.0);
            }
            continue;
        }
        match game.handle(input.clone()) {
            Ok(()) => {
                version.0 = version.0.wrapping_add(1);
                persist_store(&mut game);
                forward_lobby_command(&mut game, &mut bridge, &mut version.0, &input);
            }
            Err(err) => warn!("client rejected input: {err}"),
        }
    }
}

/// After a successful lobby-command input, hand the matching work order to
/// the async task. The party payload is read from the client NOW (sync,
/// authoritative) so the task never needs the store.
fn forward_lobby_command(
    game: &mut ClientGame,
    bridge: &mut NetBridge,
    version: &mut u64,
    input: &ClientInput,
) {
    let cmd = match input {
        ClientInput::RefreshChallenges => Some(NetCmd::Refresh),
        ClientInput::PostChallenge => game
            .ladder_party()
            .map_err(|err| warn!("could not muster the company: {err}"))
            .ok()
            .map(|party| NetCmd::Post { party }),
        ClientInput::AcceptChallenge { event_id } => game
            .ladder_party()
            .map_err(|err| warn!("could not muster the company: {err}"))
            .ok()
            .map(|party| NetCmd::Accept {
                event_id: event_id.clone(),
                party,
            }),
        _ => None,
    };
    let Some(cmd) = cmd else { return };
    if bridge.send(cmd).is_err() {
        match game.handle(ClientInput::LobbyNet(LobbyNet::Failed {
            text: "The heralds fell silent — withdraw and seek the tournament again.".into(),
        })) {
            Ok(()) => *version = version.wrapping_add(1),
            Err(err) => warn!("failed-notice was itself rejected: {err}"),
        }
    }
}

/// The task is gone mid-duel: tell the client the duel erred terminally.
fn net_task_died_during_duel(game: &mut ClientGame, version: &mut u64) {
    match game.handle(ClientInput::LobbyNet(LobbyNet::DuelAborted {
        text: "The heralds fell silent mid-duel — the field is abandoned.".into(),
    })) {
        Ok(()) => *version = version.wrapping_add(1),
        Err(err) => warn!("abort report was rejected: {err}"),
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

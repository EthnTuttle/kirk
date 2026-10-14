//! crusade-desktop: the Bevy visual shell for Deus Vult (M5b).
//!
//! One binary, two plugin sets over the same shell systems:
//!
//! - **windowed** (default): `DefaultPlugins` with a "Deus Vult" window;
//!   click buttons to drive the client.
//! - **headless** (`--smoke`, or no `DISPLAY`/`WAYLAND_DISPLAY`, e.g. CI):
//!   bevy's winit and wgpu setup would panic or stall without a working
//!   display server, so the app swaps in `MinimalPlugins` with an
//!   unthrottled loop. UI nodes become plain data; the shell systems and
//!   game code stay identical. `--smoke` drives 120 scripted frames of a
//!   practice duel (rite 0 each turn) and exits 0, so it always takes the
//!   headless path, display or not.
//!
//! v1.1 adds the Tournament of Virtue: `screens.rs` renders a lobby fed by
//! [`state::NetBridge`]; the system [`net_bridge`] (run between
//! `process_inputs` and `rebuild_ui`) spawns exactly ONE background async
//! task per lobby session (`net_task`), hands `ClientInput`s in through a
//! channel, and queues the task's `LobbyNet` reports back into the input
//! queue. The synchronous `ClientGame` stays the only state machine.

mod battle;
mod input;
mod screens;
mod smoke;
mod state;
mod style;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use bevy::app::{RunMode, ScheduleRunnerPlugin};
use bevy::prelude::*;

use crusade_app::lobby::{Lobby, LobbyCommitment, LobbyError, LobbyOutcome, DEFAULT_TIMEOUT};
use crusade_app::pvp::NetDuelDriver;
use crusade_app::{ChallengeCard, ClientGame, ClientInput, LobbyNet, LobbyStage, Phase};
use crusade_core::data;
use crusade_core::types::{ChampionInstance, DevotionStats, GraceStats, RiteSlot};
use crusade_data::RosterStore;
use crusade_net::nostr_transport::NostrTransport;
use kirk::events::EventParser;
use nostr::{Keys, ToBech32};
use tokio::sync::mpsc::UnboundedReceiver;

use state::{
    GameShell, InputQueue, NetBridge, NetCmd, NetDuelEnd, NetShared, RenderedVersion,
    SnapshotVersion,
};

fn main() {
    let smoke_mode = smoke_mode_requested();
    // `--smoke` is the CI harness: MinimalPlugins unconditionally. (In this
    // very environment a display is "present" per the env vars, yet winit's
    // redraw/present loop stalled after creating the window — so DISPLAY
    // detection alone cannot guarantee a clean smoke exit.)
    let headless = smoke_mode || headless_environment();

    let mut app = App::new();
    if headless {
        // `ScheduleRunnerPlugin` defaults to RunMode::Once; loop flat out
        // until AppExit instead (a smoke run is only 120 frames).
        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin {
            run_mode: RunMode::Loop { wait: None },
        }));
    } else {
        app.add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Deus Vult".to_string(),
                ..default()
            }),
            ..default()
        }));
    }

    app.insert_resource(GameShell {
        game: Mutex::new(ClientGame::new(initial_store())),
    })
    .init_resource::<InputQueue>()
    .init_resource::<SnapshotVersion>()
    .init_resource::<RenderedVersion>()
    .init_resource::<NetBridge>()
    .add_systems(Startup, setup);
    #[cfg(not(target_arch = "wasm32"))]
    app.insert_resource(state::NetRuntime(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("net runtime builds on any desktop OS"),
    ));
    if smoke_mode {
        app.init_resource::<smoke::SmokeDriver>().add_systems(
            Update,
            (
                smoke::drive,
                input::collect_clicks,
                input::process_inputs,
                net_bridge,
                screens::rebuild_ui,
            )
                .chain(),
        );
    } else {
        app.add_systems(
            Update,
            (
                input::collect_clicks,
                input::process_inputs,
                net_bridge,
                screens::rebuild_ui,
            )
                .chain(),
        );
    }
    app.run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
}

/// True when `--smoke` was passed on the command line (native only — the
/// wasm event loop has no argv and always boots normally).
fn smoke_mode_requested() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::args().skip(1).any(|arg| arg == "--smoke")
    }
    #[cfg(target_arch = "wasm32")]
    {
        false
    }
}

/// No X11/Wayland display means winit/wgpu cannot start. (Always false on
/// wasm, where the canvas is the "display".)
fn headless_environment() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none()
    }
    #[cfg(target_arch = "wasm32")]
    {
        false
    }
}

// =========================================================================
// Net bridge: sync shell <-> the one background async task
// =========================================================================

/// System: spawn the lobby task when the client enters [`Phase::Lobby`],
/// drain its mailbox into the input queue, and tear everything down when
/// the client lands back at the menu. A dead channel (task panicked or the
/// lobby failed before connecting) self-heals next time the player acts:
/// `NetBridge::send` drops the stale handles and this system respawns on
/// the next lobby frame.
#[allow(unused_variables)]
fn net_bridge(
    shell: Res<GameShell>,
    mut bridge: ResMut<NetBridge>,
    mut queue: ResMut<InputQueue>,
    mut version: ResMut<SnapshotVersion>,
    #[cfg(not(target_arch = "wasm32"))] runtime: Res<state::NetRuntime>,
) {
    let (phase, spawn_args) = {
        let game = shell.game.lock().expect("game mutex poisoned");
        let spawn_args = if game.phase() == Phase::Lobby && bridge.cmd_tx.is_none() {
            game.net_identity().map(|keys| (keys, game.lobby_relays()))
        } else {
            None
        };
        (game.phase(), spawn_args)
    };

    if let Some((keys, relays)) = spawn_args {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let shared = Arc::new(Mutex::new(NetShared::default()));
        spawn_net_task(
            #[cfg(not(target_arch = "wasm32"))]
            &runtime.0,
            net_task(keys, relays, rx, shared.clone()),
        );
        bridge.attach(tx, shared);
    }

    if let Some((events, _, _)) = bridge.poll() {
        if !events.is_empty() {
            for event in events {
                queue.0.push(ClientInput::LobbyNet(event));
            }
        }
        // Lobby stage moves and duel frames both land via the mailbox; bump
        // even when no client input resulted (duel snapshot refresh).
        version.0 = version.0.wrapping_add(1);
    }

    // Fully back at the menu: the lobby session and any finished duel are
    // over. Dropping the channel closes the task's receiver; `driven()`
    // cancels any in-flight await on the next poll.
    if phase == Phase::Menu && bridge.cmd_tx.is_some() {
        bridge.detach();
    }
}

/// Spawn the session future on the one permitted runtime/executor.
#[cfg(not(target_arch = "wasm32"))]
fn spawn_net_task(
    runtime: &tokio::runtime::Runtime,
    fut: impl std::future::Future<Output = ()> + Send + 'static,
) {
    runtime.spawn(fut);
}

/// Browsers have no tokio runtime; nostr-sdk's own wasm wasm glue runs
/// under `wasm_bindgen_futures::spawn_local`, so the lobby task does too.
#[cfg(target_arch = "wasm32")]
fn spawn_net_task(fut: impl std::future::Future<Output = ()> + 'static) {
    wasm_bindgen_futures::spawn_local(fut);
}

// -------------------------------------------------------------------------
// The async task itself
// -------------------------------------------------------------------------

/// Push a report into the mailbox (client-bound) and flag the bridge.
fn report(shared: &Arc<Mutex<NetShared>>, event: LobbyNet) {
    shared
        .lock()
        .expect("net mailbox mutex poisoned")
        .update(|s| s.events.push(event));
}

/// Run `op` while staying responsive to [`NetCmd::Abort`]/channel close.
/// Non-abort commands arriving mid-op buffer for later processing; only an
/// abort (or the shell dropping the channel) cancels the op, in which case
/// `None` is returned and the caller winds the session down.
async fn driven<F: std::future::Future>(
    rx: &mut UnboundedReceiver<NetCmd>,
    buffered: &mut VecDeque<NetCmd>,
    op: F,
) -> Option<F::Output> {
    let mut op = std::pin::pin!(op);
    loop {
        tokio::select! {
            out = &mut op => return Some(out),
            cmd = rx.recv() => match cmd {
                Some(NetCmd::Abort) | None => return None,
                Some(other) => buffered.push_back(other),
            },
        }
    }
}

/// Next command, buffered ones first (FIFO), or `None` on channel close.
async fn next_cmd(
    rx: &mut UnboundedReceiver<NetCmd>,
    buffered: &mut VecDeque<NetCmd>,
) -> Option<NetCmd> {
    if let Some(cmd) = buffered.pop_front() {
        Some(cmd)
    } else {
        rx.recv().await
    }
}

/// One lobby session = one task: connect, serve lobby commands, run at most
/// one sealed duel, then disconnect and exit. Anything the player-facing
/// state machine must learn goes through `shared` as [`LobbyNet`] reports.
async fn net_task(
    keys: Keys,
    relays: Vec<String>,
    mut rx: UnboundedReceiver<NetCmd>,
    shared: Arc<Mutex<NetShared>>,
) {
    let key_label = short_label(&keys.public_key());
    let lobby = match Lobby::connect(keys.clone(), &relays).await {
        Ok(lobby) => lobby,
        Err(err) => {
            warn!("lobby connect failed: {err}");
            report(
                &shared,
                LobbyNet::Failed {
                    text: "The heralds could not be reached — the tournament grounds stay closed."
                        .into(),
                },
            );
            return;
        }
    };
    report(&shared, LobbyNet::Ready { relays, key_label });

    let mut buffered: VecDeque<NetCmd> = VecDeque::new();
    while let Some(cmd) = next_cmd(&mut rx, &mut buffered).await {
        match cmd {
            NetCmd::Abort => break,
            NetCmd::Turn(_) => {
                // Battle clicks before a duel exists can't happen through the
                // UI (routing requires a live duel); drop strays, never die.
                warn!("net task: turn input with no live duel — dropped");
            }
            NetCmd::Refresh => {
                let Some(res) =
                    driven(&mut rx, &mut buffered, lobby.fetch_open_challenges(32)).await
                else {
                    break;
                };
                match res {
                    Ok(open) => {
                        let challenges = open.iter().filter_map(challenge_card).collect::<Vec<_>>();
                        report(&shared, LobbyNet::ChallengeList { challenges });
                    }
                    Err(err) => {
                        warn!("challenge listing failed: {err}");
                        report(
                            &shared,
                            LobbyNet::Failed {
                                text: "The heralds faltered; the lists could not be read.".into(),
                            },
                        );
                    }
                }
            }
            NetCmd::Post { party } => {
                if post_challenge(&lobby, &keys, party, &mut rx, &mut buffered, &shared)
                    .await
                    .is_none()
                {
                    break;
                }
            }
            NetCmd::Accept { event_id, party } => {
                if accept_challenge(
                    &lobby,
                    &keys,
                    &event_id,
                    party,
                    &mut rx,
                    &mut buffered,
                    &shared,
                )
                .await
                .is_none()
                {
                    break;
                }
            }
        }
    }
    lobby.disconnect().await;
}

/// Host flow: publish our challenge, await an answer, seal the covenant,
/// run the duel. Returns `Some(())` while the session may continue; `None`
/// means the task was aborted mid-flight and must unwind.
async fn post_challenge(
    lobby: &Lobby,
    keys: &Keys,
    party: Vec<ChampionInstance>,
    rx: &mut UnboundedReceiver<NetCmd>,
    buffered: &mut VecDeque<NetCmd>,
    shared: &Arc<Mutex<NetShared>>,
) -> Option<()> {
    let commitment = match LobbyCommitment::new(party) {
        Ok(c) => c,
        Err(err) => {
            warn!("lobby commitment failed: {err}");
            report(
                shared,
                LobbyNet::Failed {
                    text: "The company could not be bound to an oath.".into(),
                },
            );
            return Some(());
        }
    };
    let challenge = driven(rx, buffered, lobby.publish_challenge(&commitment)).await;
    let challenge = match challenge {
        Some(Ok(challenge)) => challenge,
        Some(Err(err)) => {
            warn!("publish failed: {err}");
            report(
                shared,
                LobbyNet::Failed {
                    text: "The heralds would not carry the challenge.".into(),
                },
            );
            return Some(());
        }
        None => return None,
    };
    report(
        shared,
        LobbyNet::Notice {
            stage: LobbyStage::AwaitingAccept,
            text: "Challenge issued — awaiting a worthy answer.".into(),
        },
    );
    let accept = driven(
        rx,
        buffered,
        lobby.await_accept(&challenge, DEFAULT_TIMEOUT),
    )
    .await;
    let accept = match accept {
        Some(Ok(accept)) => accept,
        Some(Err(err)) => {
            report(shared, lobby_failed("the answer", &err));
            return Some(());
        }
        None => return None,
    };
    report(
        shared,
        LobbyNet::Notice {
            stage: LobbyStage::AwaitingAccept,
            text: "An answer arrived — sealing the covenant.".into(),
        },
    );
    let outcome = driven(
        rx,
        buffered,
        lobby.complete(&challenge, &accept, &commitment, DEFAULT_TIMEOUT),
    )
    .await;
    match outcome {
        Some(Ok(outcome)) => run_duel(lobby, keys, outcome, rx, buffered, shared).await,
        Some(Err(err)) => {
            report(shared, lobby_failed("the sealing", &err));
            Some(())
        }
        None => None,
    }
}

/// Join flow: re-fetch the listing, find the chosen challenge, answer it,
/// seal, run the duel. Same return contract as [`post_challenge`].
async fn accept_challenge(
    lobby: &Lobby,
    keys: &Keys,
    event_id: &str,
    party: Vec<ChampionInstance>,
    rx: &mut UnboundedReceiver<NetCmd>,
    buffered: &mut VecDeque<NetCmd>,
    shared: &Arc<Mutex<NetShared>>,
) -> Option<()> {
    let commitment = match LobbyCommitment::new(party) {
        Ok(c) => c,
        Err(err) => {
            warn!("lobby commitment failed: {err}");
            report(
                shared,
                LobbyNet::Failed {
                    text: "The company could not be bound to an oath.".into(),
                },
            );
            return Some(());
        }
    };
    // Accept needs the full challenge event; only its id crossed the UI.
    let open = driven(rx, buffered, lobby.fetch_open_challenges(500)).await;
    let challenge = match open {
        Some(Ok(open)) => match open.into_iter().find(|c| c.event.id.to_hex() == event_id) {
            Some(c) => c,
            None => {
                report(
                    shared,
                    LobbyNet::Failed {
                        text: "That challenge was struck from the lists before we answered.".into(),
                    },
                );
                return Some(());
            }
        },
        Some(Err(err)) => {
            report(shared, lobby_failed("the listing", &err));
            return Some(());
        }
        None => return None,
    };
    let accept = driven(rx, buffered, lobby.accept(&challenge, &commitment)).await;
    let accept = match accept {
        Some(Ok(accept)) => accept,
        Some(Err(err)) => {
            report(shared, lobby_failed("the answer", &err));
            return Some(());
        }
        None => return None,
    };
    report(
        shared,
        LobbyNet::Notice {
            stage: LobbyStage::Accepting,
            text: "Answer carried — sealing the covenant.".into(),
        },
    );
    let outcome = driven(
        rx,
        buffered,
        lobby.complete(&challenge.event, &accept, &commitment, DEFAULT_TIMEOUT),
    )
    .await;
    match outcome {
        Some(Ok(outcome)) => run_duel(lobby, keys, outcome, rx, buffered, shared).await,
        Some(Err(err)) => {
            report(shared, lobby_failed("the sealing", &err));
            Some(())
        }
        None => None,
    }
}

/// The sealed duel: build the NostrTransport off the lobby's relay client
/// (it stays connected until this function returns), construct the
/// NetDuelDriver for our role, then drive turns until the duel settles or
/// dies; `finalize` runs exactly as the driver defines (verified digest,
/// signed attestation kept with the driver, ladder math) against a scratch
/// store — the ONLY durable ladder write is applied by the client itself
/// when it handles [`LobbyNet::DuelFinished`], so the store never crosses
/// the async boundary.
async fn run_duel(
    lobby: &Lobby,
    keys: &Keys,
    outcome: LobbyOutcome,
    rx: &mut UnboundedReceiver<NetCmd>,
    buffered: &mut VecDeque<NetCmd>,
    shared: &Arc<Mutex<NetShared>>,
) -> Option<()> {
    let transport = match NostrTransport::new(
        lobby.client().clone(),
        keys.clone(),
        outcome.peer,
        outcome.battle_id,
    )
    .await
    {
        Ok(t) => t,
        Err(err) => {
            warn!("transport build failed: {err}");
            report(
                shared,
                LobbyNet::Failed {
                    text: "The duel field could not be laid out.".into(),
                },
            );
            return Some(());
        }
    };
    // Role dispatch: engine side 0 is the challenge author (host). Both
    // replicas build the identical Battle from the same seed + rosters.
    let built = if outcome.we_host {
        NetDuelDriver::host(
            outcome.our_roster,
            outcome.their_roster,
            transport,
            outcome.seed,
            outcome.battle_id,
            Some(keys),
        )
    } else {
        NetDuelDriver::join(
            outcome.our_roster,
            outcome.their_roster,
            transport,
            outcome.seed,
            outcome.battle_id,
            Some(keys),
        )
    };
    let mut driver = match built {
        Ok(d) => d,
        Err(err) => {
            warn!("driver build failed: {err}");
            report(
                shared,
                LobbyNet::Failed {
                    text: "The companies could not take the field.".into(),
                },
            );
            return Some(());
        }
    };
    // First frame of the duel + the phase flip, in one mailbox write so the
    // client never sees DuelReady with no snapshot behind it.
    shared
        .lock()
        .expect("net mailbox mutex poisoned")
        .update(|s| {
            s.duel = Some(driver.snapshot());
            s.events.push(LobbyNet::DuelReady);
        });

    loop {
        let Some(cmd) = next_cmd(rx, buffered).await else {
            return None; // channel closed: unwind via the caller
        };
        let input = match cmd {
            NetCmd::Abort => return None,
            NetCmd::Turn(input) => input,
            // Lobby commands mid-duel are stale leftovers; drop them.
            other => {
                warn!("net task: ignoring {other:?} mid-duel");
                continue;
            }
        };
        // One op shape for both APIs: a forced-switch pick goes through
        // `my_switch`, everything else is a full turn. A non-switch input
        // while a switch is owed is dropped, never fatal.
        let op = async {
            if driver.pending_forced_switch() {
                match input {
                    ClientInput::PickSwitch { party_idx } => {
                        Some(driver.my_switch(party_idx).await)
                    }
                    _ => {
                        warn!("net duel: non-switch input while a switch is owed — dropped");
                        None
                    }
                }
            } else {
                Some(driver.my_turn(input).await)
            }
        };
        let Some(res) = driven(rx, buffered, op).await else {
            return None;
        };
        let Some(res) = res else { continue };
        if let Err(err) = res {
            warn!("net duel turn failed: {err}");
            abort_duel(shared, &err);
            return Some(());
        }
        if driver.battle_result().is_some() {
            // The duel is decided: FinalDigest exchange + attestation +
            // scratch-store ladder math, then report the settlement.
            let mut scratch = RosterStore::new();
            let Some(res) = driven(rx, buffered, driver.finalize(&mut scratch)).await else {
                return None;
            };
            match res {
                Ok(()) => {
                    let won = driver.we_won().unwrap_or(false);
                    if driver.attestation_event().is_none() {
                        warn!("net duel: finalized without an attestation event");
                    }
                    shared
                        .lock()
                        .expect("net mailbox mutex poisoned")
                        .update(|s| {
                            s.duel = Some(driver.snapshot());
                            s.out = Some(NetDuelEnd::Finished { won });
                            s.events.push(LobbyNet::DuelFinished { won });
                        });
                }
                Err(err) => {
                    warn!("finalize failed: {err}");
                    abort_duel(shared, &err);
                }
            }
            return Some(());
        }
        // Ordinary turn resolved: publish the fresh frame.
        shared
            .lock()
            .expect("net mailbox mutex poisoned")
            .update(|s| s.duel = Some(driver.snapshot()));
    }
}

/// A duel that died out of band: in-theme notice + terminal state; the
/// ladder never records unsettled duels.
fn abort_duel(shared: &Arc<Mutex<NetShared>>, err: &crusade_app::AppError) {
    let text = match err {
        crusade_app::AppError::Net(crusade_net::NetError::Closed) => {
            "The foe has fled the field — the duel is abandoned.".to_string()
        }
        crusade_app::AppError::Net(crusade_net::NetError::TranscriptDiverged(_)) => {
            "The two chronicles disagree — the duel is void.".to_string()
        }
        other => format!("The duel is abandoned: {other}"),
    };
    shared
        .lock()
        .expect("net mailbox mutex poisoned")
        .update(|s| {
            s.out = Some(NetDuelEnd::Aborted {
                notice: text.clone(),
            });
            s.events.push(LobbyNet::DuelAborted { text });
        });
}

/// In-theme report for a lobby-stage failure; details stay in the log.
fn lobby_failed(stage: &'static str, err: &LobbyError) -> LobbyNet {
    warn!("lobby stage {stage} failed: {err}");
    let text = match err {
        LobbyError::Timeout(_) => match stage {
            "the answer" => "None answered the challenge before the candles burned low.",
            _ => "The heralds grew weary waiting — try again.",
        },
        LobbyError::Client(_) => "The heralds faltered; the grounds could not be reached.",
        LobbyError::Protocol(_) | LobbyError::Bridge(_) => {
            "The covenant could not be sealed — the answer was not honest."
        }
    };
    LobbyNet::Failed {
        text: text.to_string(),
    }
}

/// Render an [`crusade_app::lobby::OpenChallenge`] as a lobby row.
/// Malformed challenges (cannot re-parse) are dropped defense-in-depth —
/// the listing already suite-checked them.
fn challenge_card(open: &crusade_app::lobby::OpenChallenge) -> Option<ChallengeCard> {
    let content = EventParser::parse_challenge(&open.event).ok()?;
    Some(ChallengeCard {
        event_id: open.event.id.to_hex(),
        host_label: short_label(&open.host),
        party_size: content.commitment_hashes.len().saturating_sub(1) as u8,
    })
}

/// `npub1abcd…` display label (hex prefix as fallback).
fn short_label(key: &nostr::PublicKey) -> String {
    let full = key.to_bech32().unwrap_or_else(|_| key.to_hex());
    let end = full.len().min(16);
    format!("{}…", &full[..end])
}

/// Startup roster: a previously saved browser document wins; the starter
/// band is seeded only when there is no usable save (first launch, corrupt
/// blob, or a saved-but-empty roster). Off-wasm `web_load_document` is a
/// `None` stub, so native keeps the always-seeded behavior.
fn initial_store() -> RosterStore {
    if let Some(bytes) = crusade_app::persist::web_load_document() {
        match RosterStore::load_bytes(&bytes) {
            Ok(store)
                if store
                    .list_champions()
                    .map(|champs| !champs.is_empty())
                    .unwrap_or(false) =>
            {
                return store;
            }
            Ok(_) => warn!("saved roster holds zero champions; reseeding starter band"),
            Err(err) => warn!("ignoring unreadable saved roster ({err}); seeding starter band"),
        }
    }
    seeded_demo_store()
}

/// Seed the demo roster exactly once at startup: the starter band of six
/// champions (species ids 1..=6) at level 25. A fresh `RosterStore` starts
/// empty, and the deeper engine flows (practice/campaign) require a
/// non-empty roster.
fn seeded_demo_store() -> RosterStore {
    let mut store = RosterStore::new();
    for species_id in 1..=6u16 {
        let champ = starter_champion(species_id)
            .unwrap_or_else(|| panic!("species {species_id} missing from the static table"));
        store
            .add_champion(&champ)
            .expect("fresh store accepts the starter band");
    }
    store
}

/// Build one member of the starter band: level 25, rites drawn from the
/// newest four learnset entries legal at that level — the same legality
/// recipe as crusade-app's `mkchamp` test fixtures and the practice-foe
/// builder, so `Battle::new` accepts them unconditionally.
fn starter_champion(species_id: u16) -> Option<ChampionInstance> {
    const STARTER_LEVEL: u8 = 25;
    let species = data::species_by_id(species_id)?;
    let mut picked: Vec<u16> = Vec::new();
    for entry in species
        .learnset
        .iter()
        .filter(|e| e.level <= STARTER_LEVEL)
        .rev()
    {
        if picked.len() == 4 {
            break;
        }
        if !picked.contains(&entry.rite_id) {
            picked.push(entry.rite_id);
        }
    }
    picked.reverse();
    let mut rites: [Option<RiteSlot>; 4] = [None; 4];
    for (i, id) in picked.iter().enumerate() {
        let rite = data::rite_by_id(*id)?;
        rites[i] = Some(RiteSlot {
            rite_id: *id,
            zeal_left: rite.zeal,
        });
    }
    let mut champ = ChampionInstance {
        species_id,
        level: STARTER_LEVEL,
        xp: 0,
        grace: GraceStats {
            resolve: 0,
            valour: 0,
            piety: 0,
            guard: 0,
            aegis: 0,
            alacrity: 0,
        },
        devotion: DevotionStats {
            resolve: 0,
            valour: 0,
            piety: 0,
            guard: 0,
            aegis: 0,
            alacrity: 0,
        },
        calling_id: 0,
        rites,
        current_resolve: 0,
        ailment: None,
    };
    champ.current_resolve = data::compute_stats(&champ, species).max_resolve;
    Some(champ)
}

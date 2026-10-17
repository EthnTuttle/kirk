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
mod tooltip;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy::app::{RunMode, ScheduleRunnerPlugin};
use bevy::prelude::*;

use crusade_app::lobby::{Lobby, LobbyCommitment, LobbyError, LobbyOutcome, DEFAULT_TIMEOUT};
use crusade_app::pvp::NetDuelDriver;
use crusade_app::{ChallengeCard, ClientGame, ClientInput, LobbyNet, LobbyStage, Phase};
use crusade_core::data;
use crusade_core::types::{ChampionInstance, DevotionStats, GraceStats, RiteSlot};
use crusade_data::RosterStore;
use crusade_net::nostr_transport::NostrTransport;
use crusade_net::zaps::{self, PayParams, ZapError};
use kirk::events::EventParser;
use nostr::{Event, Filter, Keys, Kind, PublicKey, ToBech32};
use nostr_sdk::EventSource;
use tokio::sync::mpsc::UnboundedReceiver;

use state::{
    AlmsInvoice, AlmsPurse, AlmsState, AttestationPost, GameShell, InputQueue, NetBridge, NetCmd,
    NetDuelEnd, NetShared, RenderedVersion, SnapshotVersion,
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
        // Nearest-neighbour default sampler: pixel-art sprites stay crisp
        // (docs/ART-PIPELINE.md section 2, seam 4). On native, bevy's asset
        // root resolves via BEVY_ASSET_ROOT / CARGO_MANIFEST_DIR (cargo run
        // sets the latter to crusade-app/), so ../assets just works; on wasm
        // the root is the page URL and trunk's copy-dir ships the files.
        app.add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Deus Vult".to_string(),
                        ..default()
                    }),
                    ..default()
                })
                .set(ImagePlugin::default_nearest()),
        );
    }

    app.insert_resource(GameShell {
        game: Mutex::new(ClientGame::new(initial_store())),
    })
    .init_resource::<InputQueue>()
    .init_resource::<SnapshotVersion>()
    .init_resource::<RenderedVersion>()
    .init_resource::<NetBridge>()
    // The menu support panel's shell-local state (A3 expansion): resource
    // only, never part of the client; support_bridge is its invoice-fetch
    // mailbox drain (the NetBridge pattern at menu size).
    .init_resource::<screens::SupportPanel>()
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
                screens::support_bridge,
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
                screens::support_bridge,
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
                    text: "The heralds could not be reached - the tournament grounds stay closed."
                        .into(),
                },
            );
            return;
        }
    };
    report(
        &shared,
        LobbyNet::Ready {
            relays: relays.clone(),
            key_label,
        },
    );

    let mut buffered: VecDeque<NetCmd> = VecDeque::new();
    while let Some(cmd) = next_cmd(&mut rx, &mut buffered).await {
        match cmd {
            NetCmd::Abort => break,
            NetCmd::Turn(_) => {
                // Battle clicks before a duel exists can't happen through the
                // UI (routing requires a live duel); drop strays, never die.
                warn!("net task: turn input with no live duel — dropped");
            }
            NetCmd::AlmsInvoice { .. } => {
                // Same stray policy: the alms chooser only renders under a
                // settled duel; anything arriving here is stale.
                warn!("net task: alms invoice with no settled duel — dropped");
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
                if post_challenge(
                    &lobby,
                    &keys,
                    party,
                    &relays,
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
            NetCmd::Accept { event_id, party } => {
                if accept_challenge(
                    &lobby,
                    &keys,
                    &event_id,
                    party,
                    &relays,
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
    relays: &[String],
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
            text: "Challenge issued - awaiting a worthy answer.".into(),
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
            text: "An answer arrived - sealing the covenant.".into(),
        },
    );
    let outcome = driven(
        rx,
        buffered,
        lobby.complete(&challenge, &accept, &commitment, DEFAULT_TIMEOUT),
    )
    .await;
    match outcome {
        Some(Ok(outcome)) => run_duel(lobby, keys, outcome, relays, rx, buffered, shared).await,
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
    relays: &[String],
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
            text: "Answer carried - sealing the covenant.".into(),
        },
    );
    let outcome = driven(
        rx,
        buffered,
        lobby.complete(&challenge.event, &accept, &commitment, DEFAULT_TIMEOUT),
    )
    .await;
    match outcome {
        Some(Ok(outcome)) => run_duel(lobby, keys, outcome, relays, rx, buffered, shared).await,
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
    relays: &[String],
    rx: &mut UnboundedReceiver<NetCmd>,
    buffered: &mut VecDeque<NetCmd>,
    shared: &Arc<Mutex<NetShared>>,
) -> Option<()> {
    // The peer's seal, kept for after the duel: when we lose, the victor's
    // pubkey is what the alms zap request must `p`-tag (B3b).
    let peer = outcome.peer;
    let transport = match NostrTransport::new(
        lobby.client().clone(),
        keys.clone(),
        peer,
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
                    // The kind-9265 attestation is already fully signed; the
                    // alms zap code below only ever READS it (firewall: soft
                    // money may never touch a hard outcome — ZAP §2).
                    let attestation = driver.attestation_event();
                    shared
                        .lock()
                        .expect("net mailbox mutex poisoned")
                        .update(|s| {
                            s.duel = Some(driver.snapshot());
                            s.out = Some(NetDuelEnd::Finished { won });
                            s.events.push(LobbyNet::DuelFinished { won });
                            // B3b: seed the receipt/alms state in the same
                            // frame, so the battle-over screen never waits on
                            // the network — the task just fills this mailbox.
                            s.alms = Some(AlmsState::settled(attestation.is_some(), won));
                        });
                    // (a) publish the attestation to the duel's relay set and
                    // (b) resolve the victor's alms purse. Cosmetic; aborting
                    // mid-way unwinds exactly like any driven() op.
                    let settle = settle_alms(lobby, won, peer, attestation, relays, &shared);
                    let Some(alms) = driven(rx, buffered, settle).await else {
                        return None;
                    };
                    // Serve alms-invoice choices until the player walks back
                    // to the menu (Abort / channel close), then unwind.
                    post_duel_alms_loop(keys, alms, rx, buffered, shared).await;
                    return None;
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

// -------------------------------------------------------------------------
// Battle-over alms (v1.1 card B3b; docs/ZAP-MONETIZATION.md surface #2)
//
// INTEGRITY FIREWALL (ZAP-MONETIZATION §2): everything below runs only
// AFTER the duel settled and the kind-9265 attestation was fully signed at
// `finalize`. It reads the settlement; it can never write to a battle, a
// seed, matchmaking, or the ladder. A zap buys gratitude and visibility —
// soft money, hard outcomes; alms buy nothing.
// -------------------------------------------------------------------------

/// How long one kind-0 profile fetch may take before the purse resolution
/// falls back to the house address. Cosmetic: the duel is already settled.
const ALMS_PROFILE_FETCH_TIMEOUT: Duration = Duration::from_secs(4);

/// Everything the post-duel alms loop needs to draw bolt11s from the
/// victor's purse. Built once at settle time by [`settle_alms`].
struct AlmsSession {
    /// The fully-signed kind-9265 attestation — the `e` tag of every
    /// kind-9734 this session builds.
    attestation: Event,
    /// The victor's pubkey (the peer once we've lost) — the `p` tag.
    victor: PublicKey,
    /// The duel's relay set — the 9734 `relays` tag (receipt destination).
    relays: Vec<String>,
    /// The victor purse's LNURL pay params, probed on wasm; `None` on the
    /// desktop shell (no HTTPS stack there — invoices are a PWA feature,
    /// the address-and-`lightning:` fallback stays mirrored from the menu).
    pay_params: Option<PayParams>,
}

/// Mutate the mailbox's alms state when one exists (a write landing after
/// teardown silently no-ops), flagging the bridge for a repaint.
fn set_alms(shared: &Arc<Mutex<NetShared>>, f: impl FnOnce(&mut AlmsState)) {
    shared
        .lock()
        .expect("net mailbox mutex poisoned")
        .update(|s| {
            if let Some(alms) = &mut s.alms {
                f(alms);
            }
        });
}

/// The spec's fallback (ZAP-MONETIZATION §1/risk 3): an absent,
/// unreachable, or failing victor purse sends the alms to the Crusade's
/// works instead. Any in-flight invoice state clears with it.
fn fall_back_to_house_purse(shared: &Arc<Mutex<NetShared>>) {
    set_alms(shared, |a| {
        a.purse = AlmsPurse::House;
        a.invoice = AlmsInvoice::Idle;
    });
}

/// (a) Publish the driver's signed kind-9265 attestation to the relays the
/// duel lived on, stamping the mailbox receipt so re-rendering the
/// battle-over screen can never re-publish. Once-only by construction: the
/// settle arm runs exactly once per duel.
async fn publish_attestation(
    client: &nostr_sdk::Client,
    attestation: &Event,
    shared: &Arc<Mutex<NetShared>>,
) {
    let post = match client.send_event(attestation.clone()).await {
        Ok(out) if !out.success.is_empty() => AttestationPost::Published,
        Ok(_) => {
            warn!("no relay accepted the duel attestation");
            AttestationPost::Failed
        }
        Err(err) => {
            warn!("attestation publish failed: {err}");
            AttestationPost::Failed
        }
    };
    set_alms(shared, |a| a.attestation = Some(post));
}

/// Fetch the victor's newest kind-0 profile from the duel's relays and read
/// its lightning route (`lud16` preferred; see [`victor_lightning_address`]).
/// Purely cosmetic: any failure resolves to the house purse by the caller.
async fn fetch_victor_address(client: &nostr_sdk::Client, victor: PublicKey) -> Option<String> {
    let filter = Filter::new().author(victor).kind(Kind::Metadata);
    let events = client
        .get_events_of(
            vec![filter],
            EventSource::relays(Some(ALMS_PROFILE_FETCH_TIMEOUT)),
        )
        .await
        .map_err(|err| warn!("victor profile fetch failed: {err}"))
        .ok()?;
    events
        .into_iter()
        .filter(|e| e.verify().is_ok())
        .max_by_key(|e| e.created_at)
        .and_then(|e| crusade_app::pvp::victor_lightning_address(&e.content))
}

/// The cosmetic post-settlement work, in one abort-able step: publish OUR
/// attestation to the duel's relay set, then (only when we lost) resolve
/// the victor's alms purse. Returns the live alms session only while a
/// zap-receiving victor purse stands; the house purse renders from the
/// mailbox alone and needs no session.
async fn settle_alms(
    lobby: &Lobby,
    we_won: bool,
    peer: PublicKey,
    attestation: Option<Event>,
    relays: &[String],
    shared: &Arc<Mutex<NetShared>>,
) -> Option<AlmsSession> {
    if let Some(att) = &attestation {
        publish_attestation(lobby.client(), att, shared).await;
    }
    // A winner is offered no alms box: alms are given, never taken. A
    // keyless duel signed nothing a zap request could tag — same result.
    if we_won {
        return None;
    }
    let attestation = attestation?;
    let victor = peer; // we lost: the peer is the victor
    let Some(address) = fetch_victor_address(lobby.client(), victor).await else {
        warn!("the victor publishes no alms purse — house fallback");
        fall_back_to_house_purse(shared);
        return None;
    };
    // Shape-check before rendering (pure parse, no network).
    if zaps::lud16_to_url(&address).is_err() {
        warn!("the victor's purse address is malformed — house fallback");
        fall_back_to_house_purse(shared);
        return None;
    }
    build_alms_session(attestation, victor, relays.to_vec(), address, shared).await
}

/// Assemble the alms session for a purse whose address shape validated.
/// The wasm PWA probes the endpoint up front: it MUST accept NIP-57 zaps
/// (`allowsNostr`) or the 9734 tagging this flow exists for can never
/// happen, so an endpoint without it falls back to the house immediately.
#[cfg(target_arch = "wasm32")]
async fn build_alms_session(
    attestation: Event,
    victor: PublicKey,
    relays: Vec<String>,
    address: String,
    shared: &Arc<Mutex<NetShared>>,
) -> Option<AlmsSession> {
    match zaps::fetch_pay_params(&screens::shell_http(), &address).await {
        Ok(params) if params.allows_nostr => {
            set_alms(shared, |a| a.purse = AlmsPurse::Victor(address));
            Some(AlmsSession {
                attestation,
                victor,
                relays,
                pay_params: Some(params),
            })
        }
        Ok(_) => {
            warn!("the victor's purse takes no zaps — house fallback");
            fall_back_to_house_purse(shared);
            None
        }
        Err(err) => {
            warn!("victor purse probe failed ({err}) — house fallback");
            fall_back_to_house_purse(shared);
            None
        }
    }
}

/// Desktop shell: no HTTPS stack ships in this dependency tree by design,
/// so the purse stays display-only (address + `lightning:` affordance,
/// mirroring the menu); the first chooser click fails into the house
/// fallback, in-theme.
#[cfg(not(target_arch = "wasm32"))]
async fn build_alms_session(
    attestation: Event,
    victor: PublicKey,
    relays: Vec<String>,
    address: String,
    shared: &Arc<Mutex<NetShared>>,
) -> Option<AlmsSession> {
    set_alms(shared, |a| a.purse = AlmsPurse::Victor(address));
    Some(AlmsSession {
        attestation,
        victor,
        relays,
        pay_params: None,
    })
}

/// After settlement the only live work left is the alms flow: invoice
/// requests from the chooser (B3b) until the player walks back to the menu
/// (Abort / channel close), which unwinds the lobby like any duel abort.
async fn post_duel_alms_loop(
    keys: &Keys,
    mut session: Option<AlmsSession>,
    rx: &mut UnboundedReceiver<NetCmd>,
    buffered: &mut VecDeque<NetCmd>,
    shared: &Arc<Mutex<NetShared>>,
) {
    loop {
        let Some(cmd) = next_cmd(rx, buffered).await else {
            return;
        };
        match cmd {
            NetCmd::Abort => return,
            NetCmd::AlmsInvoice { amount_msat } => {
                let Some(sess) = &session else {
                    warn!("net task: alms invoice with no victor purse — dropped");
                    continue;
                };
                set_alms(shared, |a| {
                    a.invoice = AlmsInvoice::Fetching { amount_msat }
                });
                let Some(result) =
                    driven(rx, buffered, alms_invoice(sess, keys, amount_msat)).await
                else {
                    return;
                };
                match result {
                    Ok(bolt11) => set_alms(shared, move |a| {
                        a.invoice = AlmsInvoice::Ready {
                            amount_msat,
                            bolt11,
                        };
                    }),
                    Err(err) => {
                        // "invoice fetch fails" => house fallback (spec);
                        // the chooser disappears with the victor purse.
                        warn!("alms invoice failed ({err}) — house fallback");
                        fall_back_to_house_purse(shared);
                        session = None;
                    }
                }
            }
            other => warn!("net task: ignoring {other:?} after the duel settled"),
        }
    }
}

/// Draw one invoice from the victor's purse. The kind-9734 is built by the
/// single reviewed constructor [`crusade_app::pvp::victor_alms_request`]
/// (the attestation event id and the victor's pubkey visibly threaded in
/// there), signed here with the player's own seal, and attached to the
/// LNURL callback via B3a's [`zaps::request_invoice`].
async fn alms_invoice(
    session: &AlmsSession,
    keys: &Keys,
    amount_msat: u64,
) -> Result<String, ZapError> {
    let zap = crusade_app::pvp::victor_alms_request(
        &session.attestation,
        session.victor,
        amount_msat,
        &session.relays,
    );
    let signed = zap.sign(keys)?;
    let Some(params) = &session.pay_params else {
        return Err(ZapError::Http(
            "the desktop shell carries no lightning heralds — give by hand".into(),
        ));
    };
    let http = screens::shell_http();
    zaps::request_invoice(&http, params, amount_msat, &signed).await
}

/// A duel that died out of band: in-theme notice + terminal state; the
/// ladder never records unsettled duels.
fn abort_duel(shared: &Arc<Mutex<NetShared>>, err: &crusade_app::AppError) {
    let text = match err {
        crusade_app::AppError::Net(crusade_net::NetError::Closed) => {
            "The foe has fled the field - the duel is abandoned.".to_string()
        }
        crusade_app::AppError::Net(crusade_net::NetError::TranscriptDiverged(_)) => {
            "The two chronicles disagree - the duel is void.".to_string()
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
            _ => "The heralds grew weary waiting - try again.",
        },
        LobbyError::Client(_) => "The heralds faltered; the grounds could not be reached.",
        LobbyError::Protocol(_) | LobbyError::Bridge(_) => {
            "The covenant could not be sealed - the answer was not honest."
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
    format!("{}...", &full[..end])
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
        converted: false,
        converted_from: None,
    };
    champ.current_resolve = data::compute_stats(&champ, species).max_resolve;
    Some(champ)
}

//! ui-shots: headless UI screenshot harness (development tooling).
//!
//! Unlike `--smoke` (MinimalPlugins, no rendering), this binary boots the
//! REAL Bevy UI shell and renders it to pixels with no window and no
//! display server: the winit plugin is disabled, a single `Camera2d`
//! renders into a fixed-size offscreen [`Image`] target, and bevy's own
//! [`Screenshot`]/`save_to_disk` machinery writes PNGs. Any GPU works;
//! on display-less CI the lavapipe software Vulkan ICD does the drawing.
//!
//!     cargo run -p crusade-app --features bevy-ui --bin ui-shots -- <outdir> [--screens a,b,...] [--size WxH]
//!
//! All game-state changes ride the same `InputQueue -> process_inputs ->
//! rebuild_ui` path as the windowed shell and the smoke driver; screen
//! layouts are never reimplemented here. Screens: `menu`, `roster`,
//! `battle`, `battle-over`, `battle-over-net` (post-duel alms CTA via an
//! offline-seeded mailbox), `campaign`, `lobby` (offline fixture cards).
//! Exits 0 only when every requested capture landed on disk.

mod battle;
mod input;
mod screens;
// Scripted turn policy is shared verbatim with the desktop smoke driver.
#[allow(dead_code)]
mod smoke;
// Shared shell resources; the net-task-facing halves are driven by the
// real task in `crusade-desktop`, only the render-side halves run here.
#[allow(dead_code)]
mod state;
mod style;

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use bevy::app::{RunMode, ScheduleRunnerPlugin};
use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::window::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured};
use bevy::ui::IsDefaultUiCamera;
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;

use crusade_app::{ChallengeCard, ClientGame, ClientInput, LobbyNet, Phase, ViewSnapshot};
use crusade_core::data;
use crusade_core::types::{ChampionInstance, DevotionStats, GraceStats, RiteSlot};
use crusade_data::RosterStore;
use tokio::sync::mpsc::UnboundedReceiver;

use state::{
    switch_target_idx, AlmsPurse, AlmsState, AttestationPost, GameShell, InputQueue, NetBridge,
    NetCmd, NetDuelEnd, NetShared, RenderedVersion, SnapshotVersion,
};

/// Default capture size (CSS px = px; scale factor is 1.0 headless).
const DEFAULT_SIZE: (u32, u32) = (1280, 720);
/// Every screen name `--screens` may select (also the full pass order).
const ALL_SCREENS: [&str; 7] = [
    "menu",
    "roster",
    "battle",
    "battle-over",
    "battle-over-net",
    "campaign",
    "lobby",
];
/// Whole-run frame cap: a hung pipeline must not block CI forever.
const MAX_TOTAL_FRAMES: u64 = 120_000;
/// Per-step caps: phase waits, battle drives, and capture readbacks.
const WAIT_PHASE_CAP: u32 = 3_000;
const DRIVE_CAP: u32 = 8_000;
const CAPTURE_CAP: u32 = 900;
/// Frames before the first capture: let the embedded font and the first
/// UI rebuild round-trip through the render world.
const BOOT_SETTLE: u32 = 45;
/// Fixture alms address for the offline post-duel CTA (mirrors the menu's
/// display-only address style; no network call is ever made).
const ALMS_FIXTURE_ADDRESS: &str = "crusader-friend@minibits.cash";

// =========================================================================
// CLI
// =========================================================================

struct Cli {
    outdir: PathBuf,
    screens: HashSet<&'static str>,
    width: u32,
    height: u32,
}

fn usage() -> String {
    format!(
        "usage: ui-shots <outdir> [--screens {}] [--size WxH]",
        ALL_SCREENS.join(",")
    )
}

fn parse_args() -> Result<Cli, String> {
    let mut args = std::env::args().skip(1);
    let mut outdir = None;
    let mut screens = None;
    let mut size = DEFAULT_SIZE;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--screens" => {
                let list = args.next().ok_or_else(|| usage())?;
                let mut picked = HashSet::new();
                for name in list.split(',') {
                    match ALL_SCREENS.iter().copied().find(|s| *s == name) {
                        Some(known) => {
                            picked.insert(known);
                        }
                        None => {
                            return Err(format!(
                                "unknown screen {name:?}; {}\n{}",
                                ALL_SCREENS.join(","),
                                usage()
                            ));
                        }
                    }
                }
                screens = Some(picked);
            }
            "--size" => {
                let wh = args.next().ok_or_else(|| usage())?;
                let (w, h) = wh.split_once('x').ok_or_else(|| usage())?;
                let (w, h) = (
                    w.parse::<u32>().map_err(|_| usage())?,
                    h.parse::<u32>().map_err(|_| usage())?,
                );
                if !(64..=4096).contains(&w) || !(64..=4096).contains(&h) {
                    return Err(format!("size {w}x{h} out of range 64..=4096\n{}", usage()));
                }
                size = (w, h);
            }
            other if outdir.is_none() && !other.starts_with("--") => {
                outdir = Some(PathBuf::from(other));
            }
            _ => return Err(usage()),
        }
    }
    let outdir = outdir.ok_or_else(|| usage())?;
    Ok(Cli {
        outdir,
        screens: screens.unwrap_or_else(|| ALL_SCREENS.iter().copied().collect()),
        width: size.0,
        height: size.1,
    })
}

// =========================================================================
// Harness resources
// =========================================================================

#[derive(Resource)]
struct ShotConfig {
    outdir: PathBuf,
    width: u32,
    height: u32,
}

/// The offscreen render target the one camera draws every frame into.
#[derive(Resource)]
struct ShotTarget(Handle<Image>);

/// Names of screenshots whose PNG bytes have come back from the renderer.
#[derive(Resource, Default)]
struct SaveLog(HashSet<String>);

/// One scripted leg of the run. Non-capture steps always execute;
/// `--screens` only filters which `Capture` steps fire.
#[derive(Clone, Debug)]
enum Step {
    /// Queue these inputs this frame (one frame, in order).
    Push(Vec<ClientInput>),
    /// Wait frames for rebuild/layout/readback settle.
    Settle(u32),
    /// Block until the client is in this phase (frame-capped).
    WaitPhase(Phase, u32),
    /// Feed the scripted battle policy exactly N turns (one per frame).
    DriveTurns(u32),
    /// Drive the scripted battle until `BattleOver` (practice) or
    /// `CampaignRest | BattleOver` (campaign). Stashes the terminal
    /// snapshot for the offline alms fixture.
    DriveBattle { cap: u32, stash: bool },
    /// Offline stand-in for the net task's post-settle mailbox write.
    SeedNetOver,
    /// Drop the seeded mailbox so the menu path renders again.
    ClearNet,
    /// Route out of the campaign leg: rest -> abandon, over -> amen.
    LeaveCampaign,
    /// Render-target capture to `<outdir>/<name>.png`, confirmed on disk.
    Capture(&'static str),
}

#[derive(Resource)]
struct ShotDriver {
    steps: Vec<Step>,
    idx: usize,
    /// Frames spent inside the current step (uncapped steps ignore it).
    step_frames: u32,
    settle_left: u32,
    turns_left: u32,
    /// Whether the `Capture` screenshot entity is already in flight.
    capture_pending: bool,
    /// Terminal battle snapshot for [`Step::SeedNetOver`].
    duel_stash: Option<ViewSnapshot>,
    /// Keeps the seeded mailbox's command channel open while attached.
    hold_rx: Option<UnboundedReceiver<NetCmd>>,
    total_frames: u64,
}

impl ShotDriver {
    fn new(cli: &Cli) -> Self {
        let wanted = |name: &'static str| cli.screens.contains(name);
        let capture = |name: &'static str| {
            if wanted(name) {
                vec![Step::Capture(name)]
            } else {
                Vec::new()
            }
        };
        let mut steps = vec![Step::Settle(BOOT_SETTLE)];
        steps.extend(capture("menu"));
        steps.extend([
            Step::Push(vec![ClientInput::OpenRoster]),
            Step::WaitPhase(Phase::Roster, WAIT_PHASE_CAP),
            Step::Settle(10),
        ]);
        steps.extend(capture("roster"));
        steps.extend([
            Step::Push(vec![ClientInput::CloseToMenu]),
            Step::WaitPhase(Phase::Menu, WAIT_PHASE_CAP),
            Step::Push(vec![ClientInput::StartPractice { level: 25 }]),
            Step::WaitPhase(Phase::InBattle, WAIT_PHASE_CAP),
            Step::Settle(8),
            // Two turns exactly: the deterministic third scripted turn drops
            // our active to 0 resolve and flips the screen into the forced-
            // switch modal; holding at two keeps this the plain field view.
            Step::DriveTurns(2),
            Step::Settle(6),
        ]);
        steps.extend(capture("battle"));
        steps.extend([
            Step::DriveBattle {
                cap: DRIVE_CAP,
                stash: true,
            },
            Step::Settle(10),
        ]);
        steps.extend(capture("battle-over"));
        steps.extend([
            Step::Push(vec![ClientInput::AcknowledgeResult]),
            Step::WaitPhase(Phase::Menu, WAIT_PHASE_CAP),
            Step::SeedNetOver,
            Step::Settle(10),
        ]);
        steps.extend(capture("battle-over-net"));
        steps.extend([
            Step::ClearNet,
            Step::Settle(4),
            Step::Push(vec![ClientInput::StartCampaign]),
            Step::WaitPhase(Phase::InBattle, WAIT_PHASE_CAP),
            Step::DriveBattle {
                cap: DRIVE_CAP,
                stash: false,
            },
            Step::Settle(10),
        ]);
        steps.extend(capture("campaign"));
        steps.extend([
            Step::LeaveCampaign,
            Step::WaitPhase(Phase::Menu, WAIT_PHASE_CAP),
            Step::Push(vec![ClientInput::EnterLobby]),
            Step::WaitPhase(Phase::Lobby, WAIT_PHASE_CAP),
            Step::Push(vec![lobby_ready_fixture()]),
            Step::Settle(6),
            Step::Push(vec![lobby_list_fixture()]),
            Step::Settle(8),
        ]);
        steps.extend(capture("lobby"));
        steps.push(Step::Push(vec![ClientInput::LeaveLobby]));
        Self {
            steps,
            idx: 0,
            step_frames: 0,
            settle_left: 0,
            turns_left: 0,
            capture_pending: false,
            duel_stash: None,
            hold_rx: None,
            total_frames: 0,
        }
    }
}

// =========================================================================
// App wiring
// =========================================================================

fn main() -> ExitCode {
    let cli = match parse_args() {
        Ok(cli) => cli,
        Err(msg) => {
            eprintln!("ui-shots: {msg}");
            return ExitCode::from(2);
        }
    };
    if let Err(err) = std::fs::create_dir_all(&cli.outdir) {
        eprintln!("ui-shots: cannot create {}: {err}", cli.outdir.display());
        return ExitCode::from(2);
    }
    println!(
        "ui-shots: outdir={} size={}x{} screens={}",
        cli.outdir.display(),
        cli.width,
        cli.height,
        ALL_SCREENS
            .iter()
            .filter(|s| cli.screens.contains(**s))
            .copied()
            .collect::<Vec<_>>()
            .join(",")
    );

    let mut app = App::new();
    // The harness recipe: full engine plugins minus winit (no display
    // server here), no primary window, an unthrottled runner. Rendering —
    // and therefore bevy's screenshot readback — stays fully real.
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..default()
            }),
    );
    app.add_plugins(ScheduleRunnerPlugin {
        run_mode: RunMode::Loop { wait: None },
    });

    app.insert_resource(ShotConfig {
        outdir: cli.outdir.clone(),
        width: cli.width,
        height: cli.height,
    })
    .insert_resource(GameShell {
        game: Mutex::new(ClientGame::new(fixture_store())),
    })
    .init_resource::<InputQueue>()
    .init_resource::<SnapshotVersion>()
    .init_resource::<RenderedVersion>()
    .init_resource::<NetBridge>()
    .init_resource::<SaveLog>()
    .insert_resource(ShotDriver::new(&cli))
    .add_systems(Startup, setup_target)
    .add_systems(
        Update,
        (
            driver,
            input::collect_clicks,
            input::process_inputs,
            screens::rebuild_ui,
        )
            .chain(),
    );

    let exit = app.run();
    drop(app); // let the pipelined renderer join its thread
    if exit.is_success() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Fixed-size offscreen render target + the one camera pointed at it.
fn setup_target(mut commands: Commands, cfg: Res<ShotConfig>, mut images: ResMut<Assets<Image>>) {
    let image = Image::new_target_texture(cfg.width, cfg.height, TextureFormat::Rgba8UnormSrgb, None);
    debug_assert_eq!(image.asset_usage, RenderAssetUsages::default());
    let target = images.add(image);
    // 0.18: the render target is its own component on the camera entity.
    // `IsDefaultUiCamera` is REQUIRED headless: the UI's default-camera
    // probe otherwise only considers primary-window cameras and every UI
    // tree would compute its layout against a 0x0 placeholder target.
    commands.spawn((
        Camera2d,
        RenderTarget::Image(target.clone().into()),
        IsDefaultUiCamera,
    ));
    commands.insert_resource(ShotTarget(target));
}

// =========================================================================
// The driver: one state machine step per frame-set
// =========================================================================

#[allow(clippy::too_many_arguments)]
fn driver(
    mut commands: Commands,
    cfg: Res<ShotConfig>,
    target: Option<Res<ShotTarget>>,
    shell: Res<GameShell>,
    mut queue: ResMut<InputQueue>,
    mut version: ResMut<SnapshotVersion>,
    mut bridge: ResMut<NetBridge>,
    mut drv: ResMut<ShotDriver>,
    saved: Res<SaveLog>,
    mut exit: MessageWriter<AppExit>,
) {
    if drv.finished() {
        return;
    }
    drv.total_frames += 1;
    if drv.total_frames > MAX_TOTAL_FRAMES {
        fail(&mut exit, "whole-run frame cap exceeded".into());
        return;
    }
    let Some(target) = target else { return };

    // Instant steps loop until one blocks; blocking steps own the frame.
    loop {
        let step = drv.steps[drv.idx].clone();
        match step {
            Step::Push(inputs) => {
                for input in inputs {
                    queue.0.push(input);
                }
                drv.advance();
            }
            Step::Settle(frames) => {
                if drv.settle_left == 0 {
                    drv.settle_left = frames;
                }
                drv.settle_left -= 1;
                if drv.settle_left == 0 {
                    drv.advance();
                } else {
                    return;
                }
            }
            Step::WaitPhase(phase, cap) => {
                let current = shell.game.lock().expect("game mutex poisoned").phase();
                if current == phase {
                    println!("ui-shots: step {} reached {phase:?}", drv.idx);
                    drv.advance();
                } else if drv.step_frames >= cap {
                    fail(
                        &mut exit,
                        format!("timed out waiting for {phase:?} (stuck at {current:?})"),
                    );
                    return;
                } else {
                    drv.step_frames += 1;
                    return;
                }
            }
            Step::DriveTurns(n) => {
                if drv.turns_left == 0 {
                    drv.turns_left = n;
                }
                let snap = shell.game.lock().expect("game mutex poisoned").snapshot();
                queue.0.push(scripted_input(&snap));
                drv.turns_left -= 1;
                if drv.turns_left == 0 {
                    drv.advance();
                } else {
                    return;
                }
            }
            Step::DriveBattle { cap, stash } => {
                let snap = shell.game.lock().expect("game mutex poisoned").snapshot();
                let done = match snap.phase {
                    Phase::BattleOver => true,
                    // The campaign leg also stops at the rest screen (win).
                    Phase::CampaignRest if !stash => true,
                    _ => false,
                };
                if done {
                    if stash {
                        drv.duel_stash = Some(snap.clone());
                    }
                    println!("ui-shots: step {} battle settled at {:?}", drv.idx, snap.phase);
                    drv.advance();
                } else if drv.step_frames >= cap {
                    fail(
                        &mut exit,
                        format!("battle drive exceeded {cap} frames (phase {:?})", snap.phase),
                    );
                    return;
                } else {
                    drv.step_frames += 1;
                    if queue.0.is_empty() {
                        queue.0.push(scripted_input(&snap));
                    }
                    return;
                }
            }
            Step::SeedNetOver => match seed_net_over(&mut drv, &mut bridge, &mut version) {
                Ok(()) => drv.advance(),
                Err(msg) => {
                    fail(&mut exit, msg);
                    return;
                }
            },
            Step::ClearNet => {
                bridge.detach();
                version.0 = version.0.wrapping_add(1);
                drv.advance();
            }
            Step::LeaveCampaign => {
                let phase = shell.game.lock().expect("game mutex poisoned").phase();
                let input = match phase {
                    Phase::CampaignRest => Some(ClientInput::AbandonCampaign),
                    Phase::BattleOver => Some(ClientInput::AcknowledgeResult),
                    Phase::Menu => None,
                    other => {
                        fail(&mut exit, format!("cannot leave a campaign from {other:?}"));
                        return;
                    }
                };
                if let Some(input) = input {
                    queue.0.push(input);
                }
                drv.advance();
            }
            Step::Capture(name) => {
                if !drv.capture_pending {
                    spawn_shot(&mut commands, &target.0, cfg.outdir.join(format!("{name}.png")), name);
                    drv.capture_pending = true;
                    println!("ui-shots: [{}] capturing {name}.png", drv.idx);
                    drv.step_frames = 0;
                    return;
                }
                if saved.0.contains(name) {
                    let path = cfg.outdir.join(format!("{name}.png"));
                    match std::fs::metadata(&path) {
                        Ok(meta) if meta.len() > 0 => {
                            println!("ui-shots: {name}.png saved ({} bytes)", meta.len());
                            drv.advance();
                        }
                        _ => {
                            fail(&mut exit, format!("{name}.png missing or empty after save"));
                            return;
                        }
                    }
                } else if drv.step_frames >= CAPTURE_CAP {
                    fail(&mut exit, format!("{name}.png capture timed out"));
                    return;
                } else {
                    drv.step_frames += 1;
                    return;
                }
            }
        }
        if drv.finished() {
            println!(
                "ui-shots: done — {} frame(s), output in {}",
                drv.total_frames,
                cfg.outdir.display()
            );
            exit.write(AppExit::Success);
            return;
        }
    }
}

impl ShotDriver {
    fn finished(&self) -> bool {
        self.idx >= self.steps.len()
    }

    /// Next step, clearing the per-step counters.
    fn advance(&mut self) {
        self.idx += 1;
        self.step_frames = 0;
        self.settle_left = 0;
        self.capture_pending = false;
    }
}

/// Fail the run in the bevy idiom: a non-success [`AppExit`] becomes the
/// process's non-zero exit code in `main`.
fn fail(exit: &mut MessageWriter<AppExit>, msg: String) {
    eprintln!("ui-shots: FAIL — {msg}");
    exit.write(AppExit::error());
}

/// The smoke driver's exact scripted policy ("rite 0 with fixtures"), so
/// the harness battles replay deterministically.
fn scripted_input(snap: &ViewSnapshot) -> ClientInput {
    match snap.phase {
        Phase::ForcedSwitchSelect => snap
            .can_switch_to
            .first()
            .map(|t| ClientInput::PickSwitch {
                party_idx: switch_target_idx(t),
            })
            .unwrap_or(ClientInput::Concede),
        _ => smoke::scripted_battle_input(snap),
    }
}

/// One screenshot request: bevy's save_to_disk does PNG encode; the second
/// observer flags the driver that the readback round-tripped.
fn spawn_shot(commands: &mut Commands, target: &Handle<Image>, path: PathBuf, name: &'static str) {
    let tag = name.to_string();
    commands
        .spawn(Screenshot::image(target.clone()))
        .observe(save_to_disk(path))
        .observe(move |_ev: On<ScreenshotCaptured>, mut saved: ResMut<SaveLog>| {
            saved.0.insert(tag.clone());
        });
}

// =========================================================================
// Offline fixtures
// =========================================================================

/// Publish the settled net-duel mailbox exactly as the net task would
/// after `finalize` (seeded-duel snapshot, terminal state, alms state),
/// minus the relay client that cannot exist offline. The render path from
/// here — `rebuild_ui`'s net-duel branch into `net_over_screen` — is the
/// shell's own, untouched.
fn seed_net_over(
    drv: &mut ShotDriver,
    bridge: &mut NetBridge,
    version: &mut SnapshotVersion,
) -> Result<(), String> {
    let Some(snap) = drv.duel_stash.clone() else {
        return Err("no settled battle snapshot to seed the alms fixture".into());
    };
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    drv.hold_rx = Some(rx);
    let shared = Arc::new(Mutex::new(NetShared::default()));
    shared
        .lock()
        .expect("net mailbox mutex poisoned")
        .update(|s| {
            s.duel = Some(snap);
            s.out = Some(NetDuelEnd::Finished { won: false });
            // Post-settle states as the desktop task leaves them: receipt
            // posted, victor purse resolved display-only (no HTTPS stack).
            let mut alms = AlmsState::settled(true, false);
            alms.attestation = Some(AttestationPost::Published);
            alms.purse = AlmsPurse::Victor(ALMS_FIXTURE_ADDRESS.into());
            s.alms = Some(alms);
        });
    bridge.attach(tx, shared);
    version.0 = version.0.wrapping_add(1);
    println!("ui-shots: seeded offline net-duel-over mailbox (alms CTA fixture)");
    Ok(())
}

/// `LobbyNet::Ready` fixture — what the real bridge reports after connect.
fn lobby_ready_fixture() -> ClientInput {
    ClientInput::LobbyNet(LobbyNet::Ready {
        relays: vec!["wss://relay.damus.io".into(), "wss://nos.lol".into()],
        key_label: "npub1crusade...".into(),
    })
}

/// `LobbyNet::ChallengeList` fixture — three open challenges on the boards.
fn lobby_list_fixture() -> ClientInput {
    let card = |tag: u8, label: &str, party_size: u8| ChallengeCard {
        event_id: hex::encode([tag; 32]),
        host_label: label.into(),
        party_size,
    };
    ClientInput::LobbyNet(LobbyNet::ChallengeList {
        challenges: vec![
            card(0xA1, "npub1valiant...", 2),
            card(0xB2, "npub1pilgrim...", 3),
            card(0xC3, "npub1hermit...", 1),
        ],
    })
}

/// Starter band, mirroring `main.rs::seeded_demo_store`'s level-25
/// learnset recipe — but inserted through `set_champion` with FIXED ids.
/// `add_champion` assigns uuid v4 keys, and the practice/campaign flows
/// sort roster ids before fielding a party, so random ids would reshuffle
/// the party (and therefore every battle frame) between runs.
fn fixture_store() -> RosterStore {
    const STARTER_LEVEL: u8 = 25;
    let mut store = RosterStore::new();
    for species_id in 1..=6u16 {
        let species =
            data::species_by_id(species_id).expect("species ids 1..=6 exist in the static table");
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
            let rite = data::rite_by_id(*id).expect("learnset rite exists");
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
        store
            .set_champion(&format!("00000000-0000-4000-8000-{species_id:012}"), &champ)
            .expect("fresh store accepts the starter band");
    }
    store
}

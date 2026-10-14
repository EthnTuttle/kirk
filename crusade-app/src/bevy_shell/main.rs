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

mod battle;
mod input;
mod screens;
mod smoke;
mod state;
mod style;

use std::sync::Mutex;

use bevy::app::{RunMode, ScheduleRunnerPlugin};
use bevy::prelude::*;

use crusade_app::ClientGame;
use crusade_core::data;
use crusade_core::types::{ChampionInstance, DevotionStats, GraceStats, RiteSlot};
use crusade_data::RosterStore;

use state::{GameShell, InputQueue, RenderedVersion, SnapshotVersion};

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
    .add_systems(Startup, setup);
    if smoke_mode {
        app.init_resource::<smoke::SmokeDriver>().add_systems(
            Update,
            (
                smoke::drive,
                input::collect_clicks,
                input::process_inputs,
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

//! M8c part 1: the Pilgrim Road overworld shell (docs/OVERWORLD.md §1-2).
//!
//! Zelda discipline: fixed tile screens rendered one ScreenDef at a time,
//! tile-stepped movement with a short tween slide, Zelda wipe transitions
//! at screen edges, NO scrolling (the camera never pans; the player token
//! is the only sprite that moves). Pokemon's scrolling overworld is
//! expressly out of register.
//!
//! v1 data-flow: the client (`client.rs`, owned elsewhere) has no trail
//! state in its ViewSnapshot yet, so everything here is shell-local and
//! reads crusade-core's `data::overworld` statics directly:
//!
//! // M8c-integration: read-only statics from crusade-core (ScreenDef,
//! // edges, props, gates). All journey state (supplies/morale/day/pos)
//! // lives in the shell-local stub [`TrailState`] below until the client
//! // grows the trail vector (docs/OVERWORLD.md §5, engine side).
//!
//! v1 art discipline: tiles/props/avatar are flat palette rects drawn from
//! the LOCKED 16-color master palette (handlers: `tools/artbake/quant.py`
//! PALETTE, hexes mirrored in `pal` below; docs/ART-PIPELINE.md style
//! bible). A "tile" is a flat fill with a 2px darker border — hand-drawn
//! tiles from the art pass replace these wholesale later (one renderer
//! seam: [`terrain_fill`]/[`terrain_border`]).

use bevy::prelude::*;

// M8c-integration: the ONLY client-facing surface the walker reads/writes
// is the ViewSnapshot (`TrailView`) and `ClientInput::{Travel,
// EngageChoice}` queued through the ordinary InputQueue — exactly like a
// battle button. The shell-local demo mode (no engine trail) still exists
// for manual QA.
use crusade_app::{ClientInput, Phase, TrailView};

// M8c-integration: read-only statics from crusade-core (see module header).
use crusade_core::data::overworld::{region_by_id, screen_by_id, PropRef, ScreenDef};

// ------------------------------------------------------------------
// Metrics (v1; the art pass owns the final internal resolution)
// ------------------------------------------------------------------

/// Tiles across/down one fixed screen.
pub const COLS: usize = 20;
pub const ROWS: usize = 11;
/// CSS px per tile (nearest-neighbour UI rects; no smoothing anywhere).
pub const TILE_PX: f32 = 32.0;
/// Movement tween length in rendered frames (deterministic, frame-counted
/// like `AnimTick`, never wall time: ui-shots captures stay stable).
const STEP_FRAMES: u8 = 8;
/// Zelda wipe length in rendered frames, each direction (fade out, swap
/// screen at the fully-black midpoint, fade back in).
const WIPE_FRAMES: u8 = 10;

// ------------------------------------------------------------------
// The LOCKED 16-color palette
// ------------------------------------------------------------------
//
// Mirrored 1:1 from `tools/artbake/quant.py`'s `PALETTE` (the repo's
// single source of truth; index 0 there is transparency). Do NOT invent
// new hexes here — terrain/props/avatar recolor between these slots only.

#[allow(dead_code)]
mod pal {
    use bevy::prelude::*;
    pub const INK: Color = Color::srgb(0x0B as f32 / 255.0, 0x0E as f32 / 255.0, 0x1B as f32 / 255.0);
    pub const NAVY2: Color = Color::srgb(0x23 as f32 / 255.0, 0x29 as f32 / 255.0, 0x46 as f32 / 255.0);
    pub const GOLD: Color = Color::srgb(0xC9 as f32 / 255.0, 0xA2 as f32 / 255.0, 0x27 as f32 / 255.0);
    pub const PALEGOLD: Color = Color::srgb(0xF4 as f32 / 255.0, 0xD3 as f32 / 255.0, 0x5E as f32 / 255.0);
    pub const BONE: Color = Color::srgb(0xD8 as f32 / 255.0, 0xD3 as f32 / 255.0, 0xC0 as f32 / 255.0);
    pub const STONE: Color = Color::srgb(0x8A as f32 / 255.0, 0x85 as f32 / 255.0, 0x77 as f32 / 255.0);
    pub const RED: Color = Color::srgb(0xA6 as f32 / 255.0, 0x26 as f32 / 255.0, 0x39 as f32 / 255.0);
    pub const RED2: Color = Color::srgb(0xD1 as f32 / 255.0, 0x49 as f32 / 255.0, 0x5B as f32 / 255.0);
    pub const BROWN: Color = Color::srgb(0x8B as f32 / 255.0, 0x5E as f32 / 255.0, 0x34 as f32 / 255.0);
    pub const DARKBROWN: Color = Color::srgb(0x5D as f32 / 255.0, 0x40 as f32 / 255.0, 0x20 as f32 / 255.0);
    pub const STEEL: Color = Color::srgb(0x9A as f32 / 255.0, 0xA0 as f32 / 255.0, 0xA8 as f32 / 255.0);
    pub const DARKSTEEL: Color = Color::srgb(0x5C as f32 / 255.0, 0x64 as f32 / 255.0, 0x70 as f32 / 255.0);
    pub const WHITE: Color = Color::srgb(0xF5 as f32 / 255.0, 0xF1 as f32 / 255.0, 0xE6 as f32 / 255.0);
    pub const GREEN: Color = Color::srgb(0x4A as f32 / 255.0, 0x7C as f32 / 255.0, 0x59 as f32 / 255.0);
    pub const PURPLE: Color = Color::srgb(0x50 as f32 / 255.0, 0x3A as f32 / 255.0, 0x6B as f32 / 255.0);
}

/// A palette color darkened for the 2px tile border (flat fill + darker
/// border IS the v1 tile; hand tiles replace later).
fn darker(c: Color) -> Color {
    let c = c.to_srgba();
    Color::srgb(c.red * 0.72, c.green * 0.72, c.blue * 0.72)
}

// ------------------------------------------------------------------
// Terrain (v1 stand-in for `tiles_ref` layouts)
// ------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Terrain {
    Grass,
    Sand,
    Road,
    Rock,
    Water,
    Wall,
}

fn terrain_fill(t: Terrain) -> Color {
    match t {
        Terrain::Grass => pal::GREEN,
        Terrain::Sand => pal::BONE,
        Terrain::Road => pal::DARKBROWN,
        Terrain::Rock => pal::DARKSTEEL,
        Terrain::Water => pal::NAVY2,
        Terrain::Wall => pal::STONE,
    }
}

fn terrain_border(t: Terrain) -> Color {
    darker(terrain_fill(t))
}

/// v1 blocking rule (docs/OVERWORLD.md leaves walk masks to the art-pass
/// data; until `tiles_ref` carries real layouts, water and wall block).
const fn walkable(t: Terrain) -> bool {
    !matches!(t, Terrain::Water | Terrain::Wall)
}

/// Small deterministic xorshift hash of (screen, x, y); the only source of
/// scatter in the v1 layouts.
fn noise(screen_id: u16, x: usize, y: usize) -> u32 {
    let mut h = (screen_id as u32)
        .wrapping_mul(0x9E37_79B1)
        .wrapping_add((x as u32).wrapping_mul(0x85EB_CA6B))
        .wrapping_add((y as u32).wrapping_mul(0xC2B2_AE35));
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}

/// Deterministic v1 terrain for a screen (ART-PASS SEAM: when `tiles_ref`
/// gains real hand layouts this function is deleted in favor of data).
///
/// Contract the procedurals keep: a road row at mid-screen carries the
/// E/W march, edge exits are always open where `edges` is `Some`, and the
/// split between fields/water/rock is per-region flavored but never seals
/// the road.
fn terrain_at(screen: &ScreenDef, x: usize, y: usize) -> Terrain {
    let mid = ROWS / 2;
    // The march road, always open across the screen.
    if y == mid {
        return Terrain::Road;
    }
    // Edge exits: open lanes where a neighbor exists, a wall ring else.
    if y == 0 {
        return if screen.edges[0].is_some() && x.abs_diff(COLS / 2) <= 1 {
            Terrain::Road
        } else {
            Terrain::Wall
        };
    }
    if y == ROWS - 1 {
        return if screen.edges[2].is_some() && x.abs_diff(COLS / 2) <= 1 {
            Terrain::Road
        } else {
            Terrain::Wall
        };
    }
    if x == 0 {
        return if screen.edges[3].is_some() && y.abs_diff(mid) <= 1 {
            Terrain::Road
        } else {
            Terrain::Wall
        };
    }
    if x == COLS - 1 {
        return if screen.edges[1].is_some() && y.abs_diff(mid) <= 1 {
            Terrain::Road
        } else {
            Terrain::Wall
        };
    }
    // Interior scatter, flavored per region.
    let n = noise(screen.id, x, y);
    let roll = n % 100;
    match screen.region {
        // The Crossing: water-heavy gauntlet screens.
        2 => match roll {
            0..=11 => Terrain::Water,
            12..=15 => Terrain::Rock,
            _ => Terrain::Sand,
        },
        // The Roads of Egypt: sand flats, rare rock.
        5 => match roll {
            0..=5 => Terrain::Rock,
            _ => Terrain::Sand,
        },
        // The Horns of Hattin: rocky, some water (the springs).
        6 => match roll {
            0..=9 => Terrain::Rock,
            10..=13 => Terrain::Water,
            _ => Terrain::Grass,
        },
        // Marches/Anatolia/Coasts/Holy Land: fields with light scatter.
        _ => match roll {
            0..=4 => Terrain::Water,
            5..=8 => Terrain::Rock,
            _ => Terrain::Grass,
        },
    }
}

/// Is tile (x, y) blocked on `screen`? (v1 = terrain + prop/gate tiles.)
fn blocked_at(screen: &ScreenDef, x: usize, y: usize) -> bool {
    !walkable(terrain_at(screen, x, y)) || props_at(screen).contains(&(x, y))
}

// ------------------------------------------------------------------
// Props and gates
// ------------------------------------------------------------------

/// Deterministic display position of the screen's props/gate, one tile
/// each (ART-PASS SEAM: prop tiles arrive with real layouts). Props sit
/// above the road line marching westward by index; a scripted gate bars
/// the east exit road.
fn props_at(screen: &ScreenDef) -> Vec<(usize, usize)> {
    let mid = ROWS / 2;
    let mut out = Vec::new();
    for (i, _prop) in screen.props.iter().enumerate() {
        let x = COLS / 2 + 3 * i + 1;
        let y = if i % 2 == 0 { mid - 2 } else { mid + 2 };
        out.push((x.min(COLS - 2), y));
    }
    if screen.scripted_gate.is_some() {
        out.push((COLS - 2, mid));
    }
    out
}

/// The prop/gate occupying a tile, if any.
fn prop_at<'s>(screen: &'s ScreenDef, x: usize, y: usize) -> Option<PropRef> {
    for (i, p) in props_at(screen).iter().enumerate() {
        if *p == (x, y) {
            if i < screen.props.len() {
                return Some(screen.props[i]);
            }
            // The scripted gate shares the props_at table (last entry).
            return None;
        }
    }
    None
}

fn is_gate_at(screen: &ScreenDef, x: usize, y: usize) -> bool {
    screen.scripted_gate.is_some() && props_at(screen).last() == Some(&(x, y))
}

/// Prompt line for an adjacent prop/gate (chronicle register).
fn prompt_for(screen: &ScreenDef, x: usize, y: usize) -> Option<String> {
    if is_gate_at(screen, x, y) {
        return Some("The gate is held. A battle of record awaits (engine seam: M8).".into());
    }
    match prop_at(screen, x, y)? {
        PropRef::ChoiceEncounter(_) => {
            Some("A warband camp smokes beyond the rocks. [E] approach, or pass by.".into())
        }
        PropRef::Shrine => Some("A wayside shrine. [E] kneel with the company.".into()),
        PropRef::Camp => Some(
            "Your own camp could stand here. [E] make camp - a day, double rations, and the watches posted."
                .into(),
        ),
        PropRef::Caravan => Some("A wrecked caravan. [E] search what the road left.".into()),
    }
}

// ------------------------------------------------------------------
// Shell-local trail state (stub until the client's trail vector lands)
// ------------------------------------------------------------------

/// The demo run's journey state. M8c-integration stub ONLY: the real
/// vector (supplies, morale, day, roster, Book of the Fallen) is the
/// engine's, additive-serde, owned by crusade-app's client (see
/// docs/OVERWORLD.md §5); this struct exists so the renderer and manual
/// QA can walk screens today.
#[derive(Debug, Clone)]
pub struct TrailState {
    pub screen: u16,
    pub px: usize,
    pub py: usize,
    pub supplies: u16,
    pub morale: i16,
    pub day: u16,
}

impl TrailState {
    /// Demo spawn: screen 1 (the Marches chapter house), road row.
    pub fn demo() -> Self {
        Self {
            screen: 1,
            px: COLS / 2,
            py: ROWS / 2,
            supplies: 100,
            morale: 0,
            day: 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Facing {
    N,
    E,
    #[default]
    S,
    W,
}

impl Facing {
    fn idx(self) -> usize {
        match self {
            Facing::N => 0,
            Facing::E => 1,
            Facing::S => 2,
            Facing::W => 3,
        }
    }

    fn delta(self) -> (isize, isize) {
        match self {
            Facing::N => (0, -1),
            Facing::E => (1, 0),
            Facing::S => (0, 1),
            Facing::W => (-1, 0),
        }
    }
}

/// A tile-step in flight: pixel tween from (`from_px`) toward (`to`).
#[derive(Debug, Clone)]
struct Move {
    from_px: (f32, f32),
    to_tile: (usize, usize),
    frame: u8,
}

/// The Zelda wipe: fade the field to black, swap the screen at the black
/// frame, fade back. `Some` only during a transition.
#[derive(Debug, Clone)]
struct Wipe {
    target: u16,
    entered_from: Facing,
    frame: u8,
}

/// Shell-local overworld mode state. `active` flips from the menu's
/// demo button, the journey screen's walk button, or the ui-shots driver;
/// everything else is managed by this module's systems.
///
/// Two modes:
/// - **demo** (`engine == false`): the stub [`TrailState`] is the world;
///   screen 1 spawn, honest stub verbs at props. Hot for manual QA.
/// - **engine** (`engine == true`): entered from the Trail phase's
///   journey screen; screen position, day, supplies, morale and all verbs
///   mirror the client's [`TrailView`], and edge crossings/engagements go
///   out as ordinary ClientInputs. Tile-position-within-screen is the one
///   thing the snapshot does not carry, so it stays shell-local (the
///   engine's day is the march unit; the walk is the presentation).
#[derive(Resource, Default)]
pub struct Overworld {
    pub active: bool,
    pub trail: Option<TrailState>,
    /// Engine-mirror mode (Phase::Trail): true when walking a real
    /// pilgrimage; `mirror` then mirrors the snapshot's `TrailView`.
    engine: bool,
    mirror: Option<TrailView>,
    /// Snapshot version the mirror last synced from.
    seen_version: u64,
    facing: Facing,
    mv: Option<Move>,
    wipe: Option<Wipe>,
    /// Chronicle line (prompts / notices) currently under the field.
    notice: Option<String>,
}

impl Overworld {
    /// Whether any animation blocks input this frame.
    fn busy(&self) -> bool {
        self.mv.is_some() || self.wipe.is_some()
    }

    /// Enter the demo walk (screen 1, stub state).
    pub fn start_demo(&mut self) {
        if !self.active && self.trail.is_none() {
            self.trail = Some(TrailState::demo());
        }
        self.engine = false;
        self.mirror = None;
        self.active = true;
    }

    /// Enter the engine-mirrored walk from a live [`TrailView`].
    pub fn start_walk(&mut self, view: &TrailView) {
        self.trail = Some(TrailState {
            screen: view.screen,
            px: COLS / 2,
            py: ROWS / 2,
            supplies: view.supplies,
            morale: view.morale as i16,
            day: view.day,
        });
        self.engine = true;
        self.mirror = Some(view.clone());
        self.seen_version = 0;
        self.active = true;
    }
}

// ------------------------------------------------------------------
// Markers
// ------------------------------------------------------------------

/// The overworld root node (floating above the ordinary shell UI).
#[derive(Component)]
struct OwRoot;
/// The fixed tile grid; children rebuilt on every screen load.
#[derive(Component)]
struct OwGrid;
/// The player token.
#[derive(Component)]
struct OwAvatar;
/// The header line (day/supplies/morale), refreshed from the mirror in
/// engine mode.
#[derive(Component)]
struct OwHeader;
/// The wipe overlay (black slab, hidden when idle).
#[derive(Component)]
struct OwWipeVeil;
/// The prompt/notice line.
#[derive(Component)]
struct OwPrompt;

// ------------------------------------------------------------------
// System registration
// ------------------------------------------------------------------

/// Register the overworld with a shell binary (feature `overworld` only).
/// The systems are cheap while the mode is inactive (one empty-query
/// check), but the whole module is compiled out without the feature.
/// Ordering: after the shell's input pump, so an engine-mode edge cross
/// sees the client's answer (or rejection) by the wipe midpoint.
pub fn register(app: &mut App) {
    app.init_resource::<Overworld>().add_systems(
        Update,
        (overworld_sync, overworld_bootstrap, overworld_input, overworld_tick)
            .chain()
            .after(crate::input::process_inputs),
    );
}

/// Engine-mode mirror upkeep. When a walker rides a real pilgrimage, the
/// client's snapshot is the truth: a phase flip out of `Trail` (gate
/// battle, wild roll, menu) ends the walk view — the player re-enters
/// from the journey screen — while a same-phase refresh (day/supplies/
/// morale moved) updates the header and mirror. Version-gated so idle
/// frames cost a single u64 compare.
fn overworld_sync(
    mut ow: ResMut<Overworld>,
    version: Res<crate::state::SnapshotVersion>,
    shell: Res<crate::state::GameShell>,
    mut headers: Query<&mut Text, (With<OwHeader>, Without<OwPrompt>)>,
) {
    if !ow.active || !ow.engine {
        return;
    }
    if version.0 == ow.seen_version {
        return;
    }
    ow.seen_version = version.0;
    let snap = shell.game.lock().expect("game mutex poisoned").snapshot();
    match snap.trail {
        Some(view) if snap.phase == Phase::Trail && !view.over => {
            // A confirmed screen change mid-wipe completes the crossing.
            if let Some(trail) = ow.trail.as_mut() {
                if trail.screen != view.screen {
                    trail.screen = view.screen;
                }
                // Display fields shadow the mirror (header only).
                trail.day = view.day;
                trail.supplies = view.supplies;
                trail.morale = view.morale as i16;
            }
            ow.mirror = Some(view.clone());
            for mut text in &mut headers {
                **text = header_line(&view);
            }
        }
        _ => {
            // The engine stepped in (battle, journey over, or out): hand
            // the stage back to the ordinary screens.
            ow.active = false;
        }
    }
}

// ------------------------------------------------------------------
// Construction
// ------------------------------------------------------------------

/// Spawn the full overworld UI tree (header, field grid, prompt line).
/// `veiled` spawns with the wipe veil showing (the transition midpoint).
/// `header` is pre-rendered (demo or engine-mirror form) by the caller.
fn spawn_root(
    cmd: &mut Commands,
    trail: &TrailState,
    facing: Facing,
    veiled: bool,
    header: String,
) -> Entity {
    let screen = screen_by_id(trail.screen).expect("demo trail screens exist");
    let header = header_node(cmd, header);
    let grid_e = cmd
        .spawn((
            OwGrid,
            Node {
                width: Val::Px(COLS as f32 * TILE_PX),
                height: Val::Px(ROWS as f32 * TILE_PX),
                ..default()
            },
            BorderColor::all(darker(pal::INK)),
            BackgroundColor(pal::INK),
        ))
        .id();
    layout_screen(cmd, grid_e, screen);
    let avatar = avatar_node(cmd, grid_e, trail.px as f32 * TILE_PX, trail.py as f32 * TILE_PX, facing);
    let _ = avatar;
    // The wipe veil: last child of the grid, drawn above everything in it.
    let veil = cmd
        .spawn((
            OwWipeVeil,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Px(COLS as f32 * TILE_PX),
                height: Val::Px(ROWS as f32 * TILE_PX),
                ..default()
            },
            ZIndex(50),
            BackgroundColor(pal::INK),
            if veiled {
                Visibility::Visible
            } else {
                Visibility::Hidden
            },
        ))
        .id();
    cmd.entity(grid_e).add_children(&[veil]);
    let prompt = cmd
        .spawn((
            OwPrompt,
            Text::new(String::new()),
            crate::style::small_font(),
            TextColor(pal::PALEGOLD),
        ))
        .id();
    let help = cmd
        .spawn((
            Text::new("arrows / WASD to walk - E to engage - Esc to leave the road".to_string()),
            crate::style::small_font(),
            TextColor(pal::STONE),
        ))
        .id();
    cmd.spawn((
        OwRoot,
        GlobalZIndex(40),
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: Val::Px(10.0),
            ..default()
        },
        BackgroundColor(pal::INK),
    ))
    .add_children(&[header, grid_e, prompt, help])
    .id()
}

/// The header line's text (engine mirror form).
fn header_line(view: &TrailView) -> String {
    format!(
        "The Pilgrim Road - {} (screen {})  |  day {}  supplies {}  morale {}",
        view.region_name, view.screen, view.day, view.supplies, view.morale
    )
}

/// The header line's text (demo stub form).
fn demo_header_line(trail: &TrailState) -> String {
    let region = screen_by_id(trail.screen)
        .and_then(|s| region_by_id(s.region))
        .map_or("Unknown lands", |r| r.name);
    format!(
        "The Pilgrim Road - {} (screen {})  |  day {}  supplies {}  morale {}  [demo]",
        region, trail.screen, trail.day, trail.supplies, trail.morale
    )
}

/// The header line: region name, the day's march count, and the journey
/// stock (supplies/morale) — the UI text discipline of the existing
/// screens, same bone/gold types.
fn header_node(cmd: &mut Commands, text: String) -> Entity {
    cmd.spawn((
        OwHeader,
        Text::new(text),
        crate::style::body_font(),
        TextColor(pal::BONE),
    ))
    .id()
}

/// Rebuild a grid's tile/prop children for `screen` (called on load and
/// at the wipe midpoint). Does NOT touch avatar/veil children, so the
/// caller despawns the whole grid instead on full loads.
fn layout_screen(cmd: &mut Commands, grid: Entity, screen: &ScreenDef) {
    let mut tiles: Vec<Entity> = Vec::with_capacity(COLS * ROWS);
    for y in 0..ROWS {
        for x in 0..COLS {
            let t = terrain_at(screen, x, y);
            tiles.push(
                cmd.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(x as f32 * TILE_PX),
                        top: Val::Px(y as f32 * TILE_PX),
                        width: Val::Px(TILE_PX),
                        height: Val::Px(TILE_PX),
                        border: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(terrain_fill(t)),
                    BorderColor::all(terrain_border(t)),
                ))
                .id(),
            );
        }
    }
    // Props and gates ride their tiles as palette markers.
    for (i, &(px, py)) in props_at(screen).iter().enumerate() {
        let marker = if i < screen.props.len() {
            prop_marker(cmd, screen.props[i])
        } else {
            gate_marker(cmd)
        };
        cmd.entity(marker).insert(Node {
            position_type: PositionType::Absolute,
            left: Val::Px(px as f32 * TILE_PX),
            top: Val::Px(py as f32 * TILE_PX),
            width: Val::Px(TILE_PX),
            height: Val::Px(TILE_PX),
            ..default()
        });
        cmd.entity(marker).insert(ZIndex(5));
        tiles.push(marker);
    }
    cmd.entity(grid).add_children(&tiles);
}

/// One small palette rect inside a marker/avatar.
fn rect(cmd: &mut Commands, x: f32, y: f32, w: f32, h: f32, color: Color) -> Entity {
    cmd.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(x),
            top: Val::Px(y),
            width: Val::Px(w),
            height: Val::Px(h),
            ..default()
        },
        BackgroundColor(color),
    ))
    .id()
}

/// ChoiceEncounter: a banner (pole + pennant).
fn banner_marker(cmd: &mut Commands) -> Entity {
    let mut kids = vec![
        rect(cmd, 13.0, 4.0, 3.0, 24.0, pal::DARKBROWN),
        rect(cmd, 16.0, 4.0, 12.0, 8.0, pal::RED),
        rect(cmd, 16.0, 12.0, 8.0, 4.0, pal::RED),
    ];
    kids.push(rect(cmd, 11.0, 26.0, 7.0, 3.0, pal::DARKBROWN));
    cmd.spawn(Node::default()).add_children(&kids).id()
}

/// Shrine: a gold cross on white.
fn shrine_marker(cmd: &mut Commands) -> Entity {
    let kids = vec![
        rect(cmd, 14.0, 4.0, 4.0, 18.0, pal::GOLD),
        rect(cmd, 9.0, 9.0, 14.0, 4.0, pal::GOLD),
        rect(cmd, 10.0, 22.0, 12.0, 6.0, pal::WHITE),
    ];
    cmd.spawn(Node::default()).add_children(&kids).id()
}

/// Camp: a fire (red flame over brown logs).
fn camp_marker(cmd: &mut Commands) -> Entity {
    let kids = vec![
        rect(cmd, 12.0, 10.0, 8.0, 12.0, pal::RED),
        rect(cmd, 14.0, 6.0, 4.0, 6.0, pal::RED2),
        rect(cmd, 8.0, 22.0, 16.0, 4.0, pal::DARKBROWN),
        rect(cmd, 10.0, 26.0, 12.0, 3.0, pal::DARKBROWN),
    ];
    cmd.spawn(Node::default()).add_children(&kids).id()
}

/// Caravan: a small cart.
fn caravan_marker(cmd: &mut Commands) -> Entity {
    let kids = vec![
        rect(cmd, 6.0, 10.0, 18.0, 10.0, pal::BROWN),
        rect(cmd, 8.0, 5.0, 14.0, 6.0, pal::BONE),
        rect(cmd, 8.0, 20.0, 5.0, 5.0, pal::DARKBROWN),
        rect(cmd, 18.0, 20.0, 5.0, 5.0, pal::DARKBROWN),
    ];
    cmd.spawn(Node::default()).add_children(&kids).id()
}

fn prop_marker(cmd: &mut Commands, prop: PropRef) -> Entity {
    match prop {
        PropRef::ChoiceEncounter(_) => banner_marker(cmd),
        PropRef::Shrine => shrine_marker(cmd),
        PropRef::Camp => camp_marker(cmd),
        PropRef::Caravan => caravan_marker(cmd),
    }
}

/// Scripted gate: dark steel bars with a skull (white slab + dark eyes).
fn gate_marker(cmd: &mut Commands) -> Entity {
    let kids = vec![
        rect(cmd, 6.0, 4.0, 4.0, 24.0, pal::DARKSTEEL),
        rect(cmd, 14.0, 4.0, 4.0, 24.0, pal::STEEL),
        rect(cmd, 22.0, 4.0, 4.0, 24.0, pal::DARKSTEEL),
        rect(cmd, 11.0, 9.0, 10.0, 9.0, pal::WHITE),
        rect(cmd, 13.0, 12.0, 2.0, 2.0, pal::INK),
        rect(cmd, 17.0, 12.0, 2.0, 2.0, pal::INK),
        rect(cmd, 13.0, 18.0, 6.0, 3.0, pal::WHITE),
    ];
    cmd.spawn(Node::default()).add_children(&kids).id()
}

/// The player token: a small robed figure (bone robe, gold hem-cross,
/// white crown), 4-direction facing cue (the hood shadow moves).
/// v1: 1-frame step (the feet strip rides every re-pose on move start).
/// Hand sprites from the art pass replace this, same gesture contract.
fn avatar_node(
    cmd: &mut Commands,
    grid: Entity,
    x_px: f32,
    y_px: f32,
    facing: Facing,
) -> Entity {
    let kids = avatar_children(cmd, facing);
    cmd.spawn((
        OwAvatar,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(x_px),
            top: Val::Px(y_px),
            width: Val::Px(TILE_PX),
            height: Val::Px(TILE_PX),
            ..default()
        },
        ZIndex(10),
        ChildOf(grid),
    ))
    .add_children(&kids)
    .id()
}

// ------------------------------------------------------------------
// Systems
// ------------------------------------------------------------------

/// Spawn/despawn the overworld tree as `active` flips.
fn overworld_bootstrap(
    mut cmd: Commands,
    mut ow: ResMut<Overworld>,
    roots: Query<Entity, With<OwRoot>>,
) {
    let exists = roots.iter().next().is_some();
    if ow.active && ow.engine && ow.mirror.is_none() {
        // Engine mode without a live TrailView has nothing to mirror.
        ow.active = false;
    }
    match (ow.active, exists) {
        (true, false) => {
            let trail = ow.trail.clone().unwrap_or_else(TrailState::demo);
            let facing = ow.facing;
            let header = match &ow.mirror {
                Some(view) => header_line(view),
                None => demo_header_line(&trail),
            };
            spawn_root(&mut cmd, &trail, facing, false, header);
            ow.notice = Some(if ow.engine {
                format!("Day {}. The company takes the road.", trail.day)
            } else {
                format!(
                    "Day {}. The company takes the road. (v1 overworld demo)",
                    trail.day
                )
            });
        }
        (false, true) => {
            for root in &roots {
                cmd.entity(root).despawn();
            }
            ow.trail = None;
            ow.mv = None;
            ow.wipe = None;
            ow.notice = None;
        }
        _ => {}
    }
}

/// Clicking the menu's "The Pilgrim Road" button (or the ui-shots driver
/// step) enters the demo; Esc backs out. Keyboard is Optional because
/// headless binaries (MinimalPlugins smoke) install no input plugin.
fn overworld_input(
    mut cmd: Commands,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut ow: ResMut<Overworld>,
    mut queue: ResMut<crate::state::InputQueue>,
    avatars: Query<Entity, With<OwAvatar>>,
    grids: Query<Entity, With<OwGrid>>,
) {
    let Some(keys) = keys else { return };
    if !ow.active {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        ow.active = false;
        return;
    }
    if ow.busy() {
        return;
    }
    if keys.just_pressed(KeyCode::KeyE) || keys.just_pressed(KeyCode::Space) {
        engage(&mut ow, &mut queue);
        return;
    }
    let dir = if keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyW) {
        Some(Facing::N)
    } else if keys.just_pressed(KeyCode::ArrowRight) || keys.just_pressed(KeyCode::KeyD) {
        Some(Facing::E)
    } else if keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::KeyS) {
        Some(Facing::S)
    } else if keys.just_pressed(KeyCode::ArrowLeft) || keys.just_pressed(KeyCode::KeyA) {
        Some(Facing::W)
    } else {
        None
    };
    let Some(dir) = dir else { return };
    let Some(trail) = ow.trail.clone() else { return };
    let screen = *screen_by_id(trail.screen).expect("trail screen exists");
    if ow.facing != dir {
        ow.facing = dir;
        // Turning also re-poses the token (the hood shadow moves).
        respawn_avatar(&mut cmd, &avatars, &grids, dir, trail.px, trail.py);
    }
    let (dx, dy) = dir.delta();
    let (tx, ty) = (trail.px as isize + dx, trail.py as isize + dy);
    if tx < 0 || ty < 0 || tx as usize >= COLS || ty as usize >= ROWS {
        // Screen edge: Zelda wipe into the neighbor, or a soft bump on None.
        match screen.edges[dir.idx()] {
            Some(target) => {
                if ow.engine {
                    // Engine mode: the march is the client's verb. Queue
                    // the ordinary ClientInput; the wipe midpoint only
                    // loads the neighbor if the mirror confirms it.
                    // M8c-integration: same queue every battle button uses.
                    let allowed = ow
                        .mirror
                        .as_ref()
                        .is_some_and(|m| m.exits.contains(&target));
                    if !allowed {
                        ow.notice = Some("The road ahead is not walked this way.".into());
                        return;
                    }
                    queue.0.push(ClientInput::Travel { to_screen: target });
                }
                ow.wipe = Some(Wipe {
                    target,
                    entered_from: dir,
                    frame: 0,
                });
                ow.notice = None;
            }
            None => {
                ow.notice = Some("The way is barred; the company halts.".into());
            }
        }
        return;
    }
    let to = (tx as usize, ty as usize);
    if blocked_at(&screen, to.0, to.1) {
        // Soft bump: no move; a prop tile announces itself instead.
        match prompt_for(&screen, to.0, to.1) {
            Some(line) => ow.notice = Some(line),
            None => {
                ow.notice = Some("The ground forbids the step.".into());
            }
        }
        return;
    }
    let from_px = (trail.px as f32 * TILE_PX, trail.py as f32 * TILE_PX);
    ow.mv = Some(Move {
        from_px,
        to_tile: to,
        frame: 0,
    });
}

/// Re-pose the avatar for a new facing: the old token despawns (children
/// go with it) and a fresh one spawns under the same grid. Cheap — it
/// fires once per direction keypress, at move start.
fn respawn_avatar(
    cmd: &mut Commands,
    avatars: &Query<Entity, With<OwAvatar>>,
    grids: &Query<Entity, With<OwGrid>>,
    facing: Facing,
    x: usize,
    y: usize,
) {
    for avatar in avatars.iter() {
        cmd.entity(avatar).despawn();
    }
    for grid in grids.iter() {
        avatar_node(cmd, grid, x as f32 * TILE_PX, y as f32 * TILE_PX, facing);
    }
}

/// Avatar gesture rects as an owned list (shared by spawn and re-pose).
fn avatar_children(cmd: &mut Commands, facing: Facing) -> Vec<Entity> {
    let robe = rect(cmd, 10.0, 12.0, 12.0, 16.0, pal::BONE);
    let hem = rect(cmd, 12.0, 20.0, 8.0, 3.0, pal::GOLD);
    let cross = rect(cmd, 15.0, 14.0, 2.0, 8.0, pal::GOLD);
    let cross_bar = rect(cmd, 13.0, 16.0, 6.0, 2.0, pal::GOLD);
    let head = rect(cmd, 12.0, 6.0, 8.0, 7.0, pal::WHITE);
    let hood = match facing {
        Facing::S => rect(cmd, 12.0, 10.0, 8.0, 3.0, pal::STONE),
        Facing::N => rect(cmd, 12.0, 6.0, 8.0, 3.0, pal::STONE),
        Facing::W => rect(cmd, 12.0, 7.0, 3.0, 6.0, pal::STONE),
        Facing::E => rect(cmd, 17.0, 7.0, 3.0, 6.0, pal::STONE),
    };
    let feet = rect(cmd, 11.0, 28.0, 10.0, 3.0, pal::DARKBROWN);
    vec![robe, hem, cross, cross_bar, head, hood, feet]
}

/// Elective engagement with an adjacent prop. Demo mode runs the stub's
/// honest verbs (shrine/camp/caravan move the stub's supplies/morale/day
/// so QA feels the economy); engine mode sends ordinary ClientInputs -
/// only ChoiceEncounter is an engine verb in v1 (M8b).
fn engage(ow: &mut Overworld, queue: &mut crate::state::InputQueue) {
    let Some(trail) = ow.trail.as_mut() else { return };
    let screen = *screen_by_id(trail.screen).expect("trail screen exists");
    for facing in [Facing::N, Facing::E, Facing::S, Facing::W] {
        let (dx, dy) = facing.delta();
        let (tx, ty) = (trail.px as isize + dx, trail.py as isize + dy);
        if tx < 0 || ty < 0 || tx as usize >= COLS || ty as usize >= ROWS {
            continue;
        }
        let (x, y) = (tx as usize, ty as usize);
        if is_gate_at(&screen, x, y) {
            ow.notice = Some(if ow.engine {
                "The gate answers through the battle that comes (engine seam)."
                    .into()
            } else {
                "The gate is barred. Engines will answer here (M8 start_battle seam)."
                    .into()
            });
            return;
        }
        if let Some(prop) = prop_at(&screen, x, y) {
            if ow.engine {
                match prop {
                    PropRef::ChoiceEncounter(id) => {
                        let known = ow
                            .mirror
                            .as_ref()
                            .is_some_and(|m| m.choice_encounters.contains(&id));
                        if known {
                            queue.0.push(ClientInput::EngageChoice { encounter_ref: id });
                            ow.notice = None;
                        } else {
                            ow.notice = Some("That encounter has moved on.".into());
                        }
                    }
                    PropRef::Camp => {
                        let known = ow.mirror.as_ref().is_some_and(|m| m.can_camp);
                        if known {
                            queue.0.push(ClientInput::MakeCamp);
                            ow.notice = None;
                        } else {
                            ow.notice =
                                Some("There is no pitching a camp on this ground.".into());
                        }
                    }
                    _ => {
                        ow.notice = Some("The road keeps its own counsel here (v1: scenery).".into())
                    }
                }
                return;
            }
            ow.notice = Some(match prop {
                PropRef::ChoiceEncounter(_) => {
                    "You hail the camp. Parley and battle decks ride with the engine seam (M8)."
                        .into()
                }
                PropRef::Shrine => {
                    // The stub trail state's morale moves here so the demo
                    // has one honest verb; the engine owns the real rule.
                    trail.morale = (trail.morale + 4).min(100);
                    "The company kneels at the wayside shrine. Morale rises.".into()
                }
                PropRef::Camp => {
                    trail.day += 1;
                    trail.morale = (trail.morale + 2).min(100);
                    "The company makes camp; the watches are posted. A day passes.".into()
                }
                PropRef::Caravan => {
                    trail.supplies += 10;
                    "You glean what the wreck still holds. Supplies +10.".into()
                }
            });
            return;
        }
    }
    ow.notice = None;
}

/// Per-frame: advance the move tween, run the wipe, refresh the prompt
/// line. Frame-counted everywhere, never wall time.
fn overworld_tick(
    mut cmd: Commands,
    mut ow: ResMut<Overworld>,
    mut avatars: Query<&mut Node, With<OwAvatar>>,
    mut veils: Query<&mut Visibility, With<OwWipeVeil>>,
    roots: Query<Entity, With<OwRoot>>,
    mut prompts: Query<&mut Text, With<OwPrompt>>,
) {
    if !ow.active {
        return;
    }
    // Field-level borrow splitting needs a plain &mut, not the ResMut
    // deref: pin it once here.
    let ow = &mut *ow;
    let Some(trail) = ow.trail.as_mut() else { return };

    // The wipe: black out the field, swap screens at the midpoint frame
    // (the whole tree respawns for the neighbor screen, veil already
    // showing), then lift the veil. Frame-counted, never wall time.
    if let Some(wipe) = ow.wipe.as_mut() {
        wipe.frame += 1;
        let f = wipe.frame;
        if f <= WIPE_FRAMES / 2 {
            for mut vis in &mut veils {
                *vis = Visibility::Visible;
            }
        }
        if f == WIPE_FRAMES / 2 {
            let target = wipe.target;
            let entered_from = wipe.entered_from;
            // Engine mode honors the client: the neighbor loads only if
            // the mirror confirms the march (otherwise the day's fortunes
            // kept the company — a gate battle or a night on the road).
            if ow.engine
                && !ow
                    .mirror
                    .as_ref()
                    .is_some_and(|m| m.screen == target)
            {
                ow.wipe = None;
                ow.notice = Some("The road will not have the company today.".into());
                for mut vis in &mut veils {
                    *vis = Visibility::Hidden;
                }
                publish_prompt(&mut prompts, ow);
                return;
            }
            if let Some(screen) = screen_by_id(target).copied() {
                trail.screen = target;
                // Enter along the matching edge, on the road-adjacent lane.
                let (px, py) = entry_tile(&screen, entered_from);
                trail.px = px;
                trail.py = py;
                if !ow.engine {
                    // Demo toll only: in engine mode the client has
                    // already ledgered the day's supplies (M8b).
                    trail.supplies = trail.supplies.saturating_sub(1);
                }
                for root in &roots {
                    cmd.entity(root).despawn();
                }
                let header = match &ow.mirror {
                    Some(view) => header_line(view),
                    None => demo_header_line(trail),
                };
                spawn_root(&mut cmd, trail, ow.facing, true, header);
            } else {
                ow.wipe = None;
                return;
            }
        }
        if f >= WIPE_FRAMES {
            ow.wipe = None;
            for mut vis in &mut veils {
                *vis = Visibility::Hidden;
            }
            // Arrival notice: scripted gates announce themselves on entry.
            if let Some(screen) = screen_by_id(trail.screen) {
                if screen.scripted_gate.is_some() {
                    ow.notice = Some(
                        "A host bars the road: a battle of record stands here (engine seam: M8)."
                            .into(),
                    );
                }
            }
        }
        publish_prompt(&mut prompts, ow);
        return;
    }

    // The move tween.
    if let Some(mv) = ow.mv.as_mut() {
        mv.frame += 1;
        let t = mv.frame as f32 / STEP_FRAMES as f32;
        let (tx, ty) = (
            mv.to_tile.0 as f32 * TILE_PX,
            mv.to_tile.1 as f32 * TILE_PX,
        );
        let (lx, ly) = (
            mv.from_px.0 + (tx - mv.from_px.0) * t,
            mv.from_px.1 + (ty - mv.from_px.1) * t,
        );
        for mut node in &mut avatars {
            node.left = Val::Px(lx);
            node.top = Val::Px(ly);
        }
        if mv.frame >= STEP_FRAMES {
            trail.px = mv.to_tile.0;
            trail.py = mv.to_tile.1;
            for mut node in &mut avatars {
                node.left = Val::Px(tx);
                node.top = Val::Px(ty);
            }
            ow.mv = None;
        }
        publish_prompt(&mut prompts, ow);
        return;
    }

    // Idle: keep the prompt line honest (adjacent props announce).
    if ow.notice.is_none() {
        if let Some(screen) = screen_by_id(trail.screen).copied() {
            for facing in [Facing::N, Facing::E, Facing::S, Facing::W] {
                let (dx, dy) = facing.delta();
                let (tx, ty) = (trail.px as isize + dx, trail.py as isize + dy);
                if tx < 0 || ty < 0 || tx as usize >= COLS || ty as usize >= ROWS {
                    continue;
                }
                if let Some(line) = prompt_for(&screen, tx as usize, ty as usize) {
                    ow.notice = Some(line);
                    break;
                }
            }
        }
    }
    publish_prompt(&mut prompts, ow);
}

/// Where the player stands after crossing `entered_from` into `screen`:
/// the opposite edge's open lane tile.
fn entry_tile(screen: &ScreenDef, entered_from: Facing) -> (usize, usize) {
    let mid = ROWS / 2;
    let cand = match entered_from {
        Facing::E => (1, mid),
        Facing::W => (COLS - 2, mid),
        Facing::S => (COLS / 2, 1),
        Facing::N => (COLS / 2, ROWS - 2),
    };
    if !blocked_at(screen, cand.0, cand.1) {
        return cand;
    }
    // The lane's mid tile should always be open by construction; walk a
    // small neighborhood as defense against future layout drift.
    for dy in 0..ROWS {
        for dx in 0..COLS {
            if !blocked_at(screen, dx, dy) {
                return (dx, dy);
            }
        }
    }
    (cand.0, cand.1)
}

/// Respawn the whole overworld tree, is only ever done through
/// [`spawn_root`]; no separate re-rooting helper exists (the wipe
/// midpoint despawns [`OwRoot`] entities and re-spawns them veiled).
/// Mirror the current notice into the prompt line (one writer).
fn publish_prompt(prompts: &mut Query<&mut Text, With<OwPrompt>>, ow: &Overworld) {
    for mut text in prompts {
        **text = ow.notice.clone().unwrap_or_default();
    }
}

// ------------------------------------------------------------------
// Menu seam (feature-gated button on the menu screen)
// ------------------------------------------------------------------

/// The menu entry point: "The Pilgrim Road" demo button. Shell-local like
/// the support panel: an observer flips [`Overworld::active`], no
/// ClientInput is introduced, and the shipped game (feature off) never
/// compiles this or the button. When a real pilgrimage is on the document
/// the Journey screen's [`walk_button`] is the entry instead.
pub fn pilgrim_road_button(cmd: &mut Commands) -> Entity {
    let text = cmd
        .spawn((
            Text::new("The Pilgrim Road (overworld demo)".to_string()),
            crate::style::body_font(),
            TextColor(crate::style::BONE),
        ))
        .id();
    cmd.spawn((
        Button,
        crate::style::button_node(),
        BackgroundColor(crate::style::BUTTON),
        BorderColor::all(crate::style::GOLD_DIM),
    ))
    .add_children(&[text])
    .observe(|_click: On<Pointer<Click>>, mut ow: ResMut<Overworld>| {
        ow.start_demo();
    })
    .id()
}

/// The journey screen's walk button (engine-mirror mode): the walker
/// becomes a presentation of the live [`TrailView`]; edge crossings and
/// engagement go out as ordinary ClientInputs. The view is cloned into
/// the observer closure — same discipline as [`pilgrim_road_button`].
pub fn walk_button(cmd: &mut Commands, view: &TrailView) -> Entity {
    let text = cmd
        .spawn((
            Text::new("Take the road on foot".to_string()),
            crate::style::body_font(),
            TextColor(crate::style::BONE),
        ))
        .id();
    let view = view.clone();
    cmd.spawn((
        Button,
        crate::style::button_node(),
        BackgroundColor(crate::style::BUTTON),
        BorderColor::all(crate::style::GOLD_DIM),
    ))
    .add_children(&[text])
    .observe(
        move |_click: On<Pointer<Click>>, mut ow: ResMut<Overworld>| {
            ow.start_walk(&view);
        },
    )
    .id()
}

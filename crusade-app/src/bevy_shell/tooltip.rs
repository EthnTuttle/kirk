//! Hover tooltips: one floating panel, spawned and reaped purely by
//! pointer events. This is shell-local infrastructure ONLY — the widget's
//! [`TooltipText`] holds a caller-picked string (a fixed literal, or one
//! composed at attach time from the same snapshot/view rows the widget was
//! built from), so the system never reads game state, never starts a
//! timer, and never touches IO: it is wall-clock-free, wasm-safe, and
//! deterministic in headless runs.
//!
//! The house rules (each pinned by a test below):
//!
//! - `On<Pointer<Over>>` on a `TooltipText` entity spawns ONE panel under
//!   [`UiRoot`]: dark navy fill, 1 px gold border, bone text wrapping at
//!   480 px, anchored top-right of the pointer — its bottom-left corner
//!   sits 14 px right / 10 px above the cursor so the box grows upward,
//!   clear of the hovered widget. Where no window exists to measure
//!   (smoke runs, ui-shots) it deterministically drops below-right.
//! - A later `Over` SWAPS the panel (despawn-then-spawn): at most one
//!   tooltip is ever alive.
//! - `On<Pointer<Out>>` reaps it. Because bevy dispatches every `Out`
//!   before any `Over` in a frame (`pointer_events` walks the previous
//!   hover map first), a widget switch — or a padding/label hop inside
//!   one widget — always leaves the fresh tip standing.
//! - Rebuilds reap it too for free: `rebuild_ui` despawns `UiRoot` with
//!   all its children, and the panel is a `UiRoot` child.
//!
//! Call sites (the content wave) attach help via [`with_tooltip`]; the
//! observers are entity-local, the same pattern `screens.rs` uses for its
//! `On<Pointer<Click>>` handlers — nothing is registered app-wide.

use std::borrow::Cow;

use bevy::prelude::*;

use crate::state::UiRoot;
use crate::style;

/// A tip never renders wider than this; longer text wraps inside the box.
pub(crate) const TOOLTIP_MAX_WIDTH_PX: f32 = 480.0;
/// Horizontal gap between the pointer and the tooltip's left edge.
pub(crate) const TOOLTIP_GAP_X_PX: f32 = 14.0;
/// Vertical gap between the pointer and the tooltip's anchored edge.
pub(crate) const TOOLTIP_GAP_Y_PX: f32 = 10.0;
/// Breathing room so the text does not kiss the gold border.
pub(crate) const TOOLTIP_PADDING_PX: f32 = 8.0;

/// Hover-help body for one widget. `Cow` covers both caller shapes the
/// content wave needs: a fixed compile-time literal for static guidance,
/// or a String composed at attach time from the same snapshot/view row the
/// widget renders (per-rite stats, per-species matchups) — attached once at
/// spawn, never recomputed, so a rebuilt string never drifts from the row
/// it explains. This module never computes tip contents and never touches
/// `GameShell`.
#[derive(Component)]
pub struct TooltipText(pub Cow<'static, str>);

/// The one live tooltip panel. Pure despawn-then-spawn bookkeeping keeps
/// at most one of these in the world at any moment.
#[derive(Component)]
pub(crate) struct TooltipPopup;

/// Attach hover help to an already-spawned widget: `tip` shows while the
/// pointer is over the widget (or any child of it), and vanishes when the
/// pointer leaves or the screen rebuilds.
pub fn with_tooltip(cmd: &mut EntityCommands, tip: impl Into<Cow<'static, str>>) {
    cmd.insert(TooltipText(tip.into()))
        .observe(on_tooltip_over)
        .observe(on_tooltip_out);
}

/// Hover-in: swap in this widget's tip. Pointer events auto-propagate up
/// the `ChildOf` chain (a text label inside a button reaches the button's
/// observer with `entity` already set to the button), so one tip attached
/// to a container covers its contents; thanks to Out-before-Over dispatch
/// the swap runs strictly AFTER any outgoing tip was reaped, so exactly
/// one panel survives every hover change.
fn on_tooltip_over(
    over: On<Pointer<Over>>,
    tips: Query<&TooltipText>,
    roots: Query<Entity, With<UiRoot>>,
    popups: Query<Entity, With<TooltipPopup>>,
    windows: Query<&Window>,
    mut commands: Commands,
) {
    let Ok(tip) = tips.get(over.entity) else {
        return;
    };
    let Ok(root) = roots.single() else {
        // Hover cannot precede the first rebuild (UiRoot exists from frame
        // 1), but never panic on a stripped-down test world.
        return;
    };
    for popup in &popups {
        commands.entity(popup).despawn();
    }
    let cursor = over.pointer_location.position;
    let window_height = windows.iter().next().map(Window::height);
    spawn_tooltip(&mut commands, root, &tip.0, cursor, window_height);
}

/// Hover-out: reap the panel, whatever widget it belongs to. No
/// propagation filter on purpose: an `Out` that merely crossed a text
/// child bubbles up here too, and despawn-all is still correct — if the
/// pointer remains over a sibling piece of the SAME widget, that piece
/// just entered the hover map and its bubbled `Over` respawns the tip in
/// the same dispatch; if the pointer truly left, nothing respawns.
fn on_tooltip_out(
    _out: On<Pointer<Out>>,
    popups: Query<Entity, With<TooltipPopup>>,
    mut commands: Commands,
) {
    for popup in &popups {
        commands.entity(popup).despawn();
    }
}

/// Spawn the floating panel and its text line under `root`, returning the
/// panel entity. `Pickable::IGNORE` keeps the panel from ever shadowing
/// (or becoming) a hover target beneath the pointer. Spawned last under
/// `UiRoot`, it is also the topmost of its siblings.
fn spawn_tooltip(
    commands: &mut Commands,
    root: Entity,
    tip: &str,
    cursor: Vec2,
    window_height: Option<f32>,
) -> Entity {
    let text = commands
        .spawn((Text::new(tip), style::small_font(), TextColor(style::BONE)))
        .id();
    commands
        .spawn((
            TooltipPopup,
            tooltip_box_node(cursor, window_height),
            BackgroundColor(style::PANEL),
            BorderColor::all(style::GOLD),
            Pickable::IGNORE,
            ChildOf(root),
        ))
        .add_children(&[text])
        .id()
}

/// The tooltip box's layout, as a pure function of pointer position and
/// (when the app has a window) its height: absolute-positioned, dark navy
/// panel, gold-edged, wrapping at [`TOOLTIP_MAX_WIDTH_PX`]. With a measured
/// window the box is bottom-anchored 10 px above the cursor — true
/// "top-right of cursor", growing upward as text wraps; headless worlds
/// have no height to anchor against, so the fallback pins the box below
/// the cursor instead. Anchors clamp at 0 so degenerate pointer positions
/// can never produce negative offsets.
pub(crate) fn tooltip_box_node(cursor: Vec2, window_height: Option<f32>) -> Node {
    let mut node = Node {
        position_type: PositionType::Absolute,
        max_width: Val::Px(TOOLTIP_MAX_WIDTH_PX),
        padding: UiRect::all(Val::Px(TOOLTIP_PADDING_PX)),
        border: UiRect::all(Val::Px(1.0)),
        ..default()
    };
    node.left = Val::Px((cursor.x + TOOLTIP_GAP_X_PX).max(0.0));
    match window_height {
        Some(height) => node.bottom = Val::Px((height - cursor.y + TOOLTIP_GAP_Y_PX).max(0.0)),
        None => node.top = Val::Px((cursor.y + TOOLTIP_GAP_Y_PX).max(0.0)),
    }
    node
}

#[cfg(test)]
mod tests {
    //! Headless-testing choice, on record: under `MinimalPlugins` (or a
    //! bare `World`, as here) bevy_ui's picking backend cannot GENERATE
    //! `Pointer` events — no window, no input devices, no per-frame hover
    //! dispatch runs without winit/wgpu. But observers are pure ECS:
    //! `World::trigger(Pointer { .. })` fires the entity-local `On<..>`
    //! handlers synchronously, exactly as the backend's own
    //! `commands.trigger(event)` would. So the spawn/swap/despawn logic is
    //! exercised END-TO-END against real bevy observer machinery (no
    //! winit needed), and the placement math is pinned separately as a
    //! pure function. Nothing is mocked.
    use super::*;
    use bevy::camera::NormalizedRenderTarget;
    use bevy::picking::backend::HitData;
    use bevy::picking::pointer::{Location, PointerId};
    use bevy::window::WindowResolution;

    const TIP_A: &str = "Grace steels the arm; resolve keeps the feet.";
    const TIP_B: &str = "A rite spends zeal.";

    /// The exact event shape `pointer_events` raises on hover-in. The
    /// `None` render target stops the bubble at the top of an entity's own
    /// ancestry so the fixture needs no window entity.
    fn over_at(entity: Entity, position: Vec2) -> Pointer<Over> {
        Pointer {
            entity,
            pointer_id: PointerId::Mouse,
            pointer_location: Location {
                target: NormalizedRenderTarget::None {
                    width: 1,
                    height: 1,
                },
                position,
            },
            event: Over {
                hit: HitData::new(Entity::PLACEHOLDER, 0.0, None, None),
            },
        }
    }

    /// The matching hover-out shape.
    fn out_at(entity: Entity, position: Vec2) -> Pointer<Out> {
        Pointer {
            entity,
            pointer_id: PointerId::Mouse,
            pointer_location: Location {
                target: NormalizedRenderTarget::None {
                    width: 1,
                    height: 1,
                },
                position,
            },
            event: Out {
                hit: HitData::new(Entity::PLACEHOLDER, 0.0, None, None),
            },
        }
    }

    fn popup_entities(world: &mut World) -> Vec<Entity> {
        world
            .query_filtered::<Entity, With<TooltipPopup>>()
            .iter(world)
            .collect()
    }

    /// Spawn two independent tip-bearing widgets the way the content wave
    /// will: `with_tooltip` on each. Returns (a, b).
    fn fixture_widgets(world: &mut World) -> (Entity, Entity) {
        let (a, b) = {
            let mut commands = world.commands();
            let a = {
                let mut ea = commands.spawn_empty();
                with_tooltip(&mut ea, TIP_A);
                ea.id()
            };
            let b = {
                let mut eb = commands.spawn_empty();
                with_tooltip(&mut eb, TIP_B);
                eb.id()
            };
            (a, b)
        };
        world.flush();
        (a, b)
    }

    /// End-to-end over the real `On<Pointer<..>>` plumbing: the builder
    /// attaches, Over spawns under UiRoot (exactly one, ever), a second
    /// Over swaps, bubbled events from a text child behave identically, a
    /// live window flips the anchor to above-the-cursor, and Out reaps.
    #[test]
    fn hover_roundtrip_swaps_and_reaps_headless() {
        let mut world = World::new();
        let root = world.spawn((UiRoot, Node::default())).id();
        let (a, b) = fixture_widgets(&mut world);
        assert_eq!(
            world.get::<TooltipText>(a).map(|t| t.0.as_ref()),
            Some(TIP_A)
        );
        assert!(popup_entities(&mut world).is_empty(), "no tip before hover");

        world.trigger(over_at(a, Vec2::new(120.0, 300.0)));
        world.flush();
        let popups = popup_entities(&mut world);
        assert_eq!(popups.len(), 1, "exactly one tip alive");
        let popup = popups[0];
        // Under UiRoot (so every rebuild reaps it), ignoring the pointer.
        assert_eq!(world.get::<ChildOf>(popup).map(ChildOf::parent), Some(root));
        assert_eq!(world.get::<Pickable>(popup), Some(&Pickable::IGNORE));
        // The visual contract: dark navy box, 1 px gold border, bone text.
        assert_eq!(
            world.get::<BackgroundColor>(popup),
            Some(&BackgroundColor(style::PANEL))
        );
        assert_eq!(
            world.get::<BorderColor>(popup),
            Some(&BorderColor::all(style::GOLD))
        );
        let node = world
            .get::<Node>(popup)
            .expect("popup is a UI node")
            .clone();
        // No window yet: the headless fallback pins below-right.
        assert_eq!(node.left, Val::Px(134.0));
        assert_eq!(node.top, Val::Px(310.0));
        assert_eq!(node.bottom, Val::Auto);
        let text = world
            .get::<Children>(popup)
            .and_then(|kids| kids.first().copied())
            .expect("popup wraps one text line");
        assert_eq!(world.get::<Text>(text).map(|t| t.0.as_str()), Some(TIP_A));
        assert_eq!(world.get::<TextColor>(text), Some(&TextColor(style::BONE)));
        assert_eq!(world.get::<TextFont>(text).map(|f| f.font_size), Some(14.0));

        // A second widget's hover swaps — never stacks.
        world.trigger(over_at(b, Vec2::new(200.0, 100.0)));
        world.flush();
        let popups = popup_entities(&mut world);
        assert_eq!(popups.len(), 1, "despawn-then-spawn: still exactly one");
        let text = world
            .get::<Children>(popups[0])
            .and_then(|kids| kids.first().copied())
            .expect("popup wraps one text line");
        assert_eq!(world.get::<Text>(text).map(|t| t.0.as_str()), Some(TIP_B));
        assert_eq!(
            world.get::<Node>(popups[0]).map(|n| n.left),
            Some(Val::Px(214.0))
        );

        // Bubbled Over from a bare text child reaches the parent's observer.
        let label = world.spawn((Text::new("label"), ChildOf(b))).id();
        world.flush();
        world.trigger(over_at(label, Vec2::new(201.0, 101.0)));
        world.flush();
        assert_eq!(
            popup_entities(&mut world).len(),
            1,
            "bubbled hover swaps in place"
        );

        // A window lets the observer anchor above the cursor (true
        // top-right): bottom = height - y + gap.
        world.spawn(Window {
            resolution: WindowResolution::new(1280, 720),
            ..default()
        });
        world.flush();
        world.trigger(over_at(a, Vec2::new(120.0, 300.0)));
        world.flush();
        let popups = popup_entities(&mut world);
        assert_eq!(popups.len(), 1);
        let node = world
            .get::<Node>(popups[0])
            .expect("popup is a UI node")
            .clone();
        assert_eq!(node.bottom, Val::Px(430.0));
        assert_eq!(node.top, Val::Auto);

        // Hover-out reaps the last panel; the world goes quiet.
        world.trigger(out_at(a, Vec2::new(120.0, 300.0)));
        world.flush();
        assert!(popup_entities(&mut world).is_empty(), "Out reaps the tip");
    }

    /// The box itself, as a pure function of pointer position: gold-edged
    /// navy panel, absolute, wrapped at 480 px — and both anchor modes,
    /// including that degenerate cursor positions can never produce a
    /// negative anchor.
    #[test]
    fn tooltip_box_layout_pins_the_contract() {
        assert_eq!(
            TOOLTIP_MAX_WIDTH_PX, 480.0,
            "the wrap width is the contract"
        );

        // Measured window: bottom-left corner sits above-right of the
        // pointer — the box grows upward, clear of what it explains.
        let node = tooltip_box_node(Vec2::new(100.0, 200.0), Some(720.0));
        assert_eq!(node.position_type, PositionType::Absolute);
        assert_eq!(node.left, Val::Px(114.0));
        assert_eq!(node.bottom, Val::Px(530.0));
        assert_eq!(node.top, Val::Auto);
        assert_eq!(node.right, Val::Auto);
        assert_eq!(node.max_width, Val::Px(480.0));
        assert_eq!(node.border, UiRect::all(Val::Px(1.0)));
        assert_eq!(node.padding, UiRect::all(Val::Px(TOOLTIP_PADDING_PX)));

        // Headless fallback: below-right, bottom unanchored.
        let node = tooltip_box_node(Vec2::new(100.0, 200.0), None);
        assert_eq!(node.left, Val::Px(114.0));
        assert_eq!(node.top, Val::Px(210.0));
        assert_eq!(node.bottom, Val::Auto);

        // Degenerate positions clamp at 0 instead of going off-screen.
        let node = tooltip_box_node(Vec2::new(-50.0, 999.0), Some(720.0));
        assert_eq!(node.left, Val::Px(0.0));
        assert_eq!(node.bottom, Val::Px(0.0));
    }
}

//! Palette and node recipes. Dark and reverent: deep navy ground, gold
//! accents, bone-coloured text, resolve (HP) in green over ember red.

use bevy::prelude::*;

pub const NAVY: Color = Color::srgb(0.045, 0.055, 0.105);
pub const PANEL: Color = Color::srgb(0.095, 0.110, 0.180);
pub const PANEL_EDGE: Color = Color::srgb(0.240, 0.260, 0.360);
pub const GOLD: Color = Color::srgb(0.800, 0.620, 0.220);
pub const GOLD_DIM: Color = Color::srgb(0.500, 0.410, 0.190);
pub const BONE: Color = Color::srgb(0.850, 0.820, 0.720);
pub const DIM: Color = Color::srgb(0.480, 0.480, 0.540);
pub const HP_GREEN: Color = Color::srgb(0.270, 0.620, 0.300);
pub const HP_RED: Color = Color::srgb(0.550, 0.160, 0.140);
pub const BUTTON: Color = Color::srgb(0.140, 0.160, 0.250);
pub const BUTTON_HOVER: Color = Color::srgb(0.220, 0.240, 0.360);
pub const BUTTON_PRESSED: Color = GOLD_DIM;
pub const BUTTON_DISABLED: Color = Color::srgb(0.085, 0.095, 0.140);
pub const OVERLAY_DIM: Color = Color::srgba(0.010, 0.010, 0.025, 0.78);

pub fn title_font() -> TextFont {
    TextFont {
        font_size: 56.0,
        ..default()
    }
}

pub fn head_font() -> TextFont {
    TextFont {
        font_size: 28.0,
        ..default()
    }
}

pub fn body_font() -> TextFont {
    TextFont {
        font_size: 18.0,
        ..default()
    }
}

pub fn small_font() -> TextFont {
    TextFont {
        font_size: 14.0,
        ..default()
    }
}

/// Full-screen root column. `centered` screens (menu, rest, battle over)
/// centre their column; battle/roster screens flow from the top.
pub fn screen_node(centered: bool) -> Node {
    Node {
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        flex_direction: FlexDirection::Column,
        justify_content: if centered {
            JustifyContent::Center
        } else {
            JustifyContent::Start
        },
        align_items: if centered {
            AlignItems::Center
        } else {
            AlignItems::Start
        },
        padding: UiRect::all(Val::Px(24.0)),
        row_gap: Val::Px(14.0),
        ..default()
    }
}

/// Bordered dark panel; children stack vertically with a small gap.
pub fn panel_node() -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        padding: UiRect::all(Val::Px(14.0)),
        border: UiRect::all(Val::Px(1.0)),
        row_gap: Val::Px(6.0),
        ..default()
    }
}

/// Primary clickable row.
pub fn button_node() -> Node {
    Node {
        padding: UiRect::axes(Val::Px(18.0), Val::Px(10.0)),
        border: UiRect::all(Val::Px(1.0)),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    }
}

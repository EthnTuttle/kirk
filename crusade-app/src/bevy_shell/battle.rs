//! The in-battle screen: me/foe panels (species, level, typing, resolve bar,
//! ailment, stage badges, reserves), the chronicle, and the action rows —
//! rites from `my_rites`, switches from `can_switch_to`, Concede. The
//! forced-switch phase draws the same screen under a modal overlay.

use bevy::prelude::*;

use crusade_app::{BattleChampionView, ClientInput, RiteCard, ViewSnapshot};
use crusade_core::types::Ailment;

use crate::screens::{action_button, log_panel, panel, text_node};
use crate::state::{switch_target_idx, switch_target_name};
use crate::style;

/// Full in-battle layout for [`Phase::InBattle`].
pub(crate) fn battle_screen(cmd: &mut Commands, snap: &ViewSnapshot) -> Vec<Entity> {
    let mut kids = vec![
        battle_row(cmd, snap),
        log_panel(cmd, "Chronicle", &snap.log_tail),
    ];
    if !snap.my_rites.is_empty() {
        kids.push(rite_row(cmd, &snap.my_rites));
    }
    kids.push(command_row(cmd, snap));
    kids
}

/// [`Phase::ForcedSwitchSelect`]: the normal field, greyed under a modal.
pub(crate) fn forced_switch_screen(cmd: &mut Commands, snap: &ViewSnapshot) -> Vec<Entity> {
    let mut kids = battle_screen(cmd, snap);
    kids.push(forced_switch_overlay(cmd, snap));
    kids
}

/// Me (left) vs foe (right), stretched across the window.
fn battle_row(cmd: &mut Commands, snap: &ViewSnapshot) -> Entity {
    let me = snap
        .me
        .as_ref()
        .map(|v| champion_panel(cmd, v))
        .unwrap_or_else(|| text_node(cmd, "...".into(), style::body_font(), style::DIM));
    let foe = snap
        .foe
        .as_ref()
        .map(|v| champion_panel(cmd, v))
        .unwrap_or_else(|| text_node(cmd, "...".into(), style::body_font(), style::DIM));
    cmd.spawn(Node {
        width: Val::Percent(100.0),
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Start,
        ..default()
    })
    .add_children(&[me, foe])
    .id()
}

/// One side's champion card: gold edge for ours, ember for the foe.
fn champion_panel(cmd: &mut Commands, view: &BattleChampionView) -> Entity {
    let border = if view.ours {
        style::GOLD_DIM
    } else {
        style::HP_RED
    };
    let mut kids = vec![
        text_node(
            cmd,
            format!("{} - Lv {}", view.species_name, view.level),
            style::head_font(),
            style::BONE,
        ),
        resolve_row(cmd, view.current_resolve, view.max_resolve),
    ];
    if !view.types_display.is_empty() {
        kids.push(text_node(
            cmd,
            view.types_display.join(" + "),
            style::small_font(),
            style::GOLD,
        ));
    }
    if let Some(ailment) = view.ailment {
        kids.push(text_node(
            cmd,
            ailment_label(ailment).to_string(),
            style::small_font(),
            style::HP_RED,
        ));
    }
    if !view.stages.is_empty() {
        kids.push(stage_badge_row(cmd, &view.stages));
    }
    kids.push(text_node(
        cmd,
        format!("reserves standing: {}", view.alive_reserves),
        style::small_font(),
        style::DIM,
    ));
    let mut card = cmd.spawn((
        style::panel_node(),
        BackgroundColor(style::PANEL),
        BorderColor::all(border),
    ));
    card.add_children(&kids).id()
}

/// Green-over-red resolve bar, sized current/max, with the numbers beside.
fn resolve_row(cmd: &mut Commands, current: u16, max: u16) -> Entity {
    let pct = if max == 0 {
        0.0
    } else {
        (100.0 * f32::from(current) / f32::from(max)).clamp(0.0, 100.0)
    };
    let fill = cmd
        .spawn((
            Node {
                width: Val::Percent(pct),
                height: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(style::HP_GREEN),
        ))
        .id();
    let bar = cmd
        .spawn((
            Node {
                width: Val::Px(220.0),
                height: Val::Px(14.0),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(style::HP_RED),
            BorderColor::all(style::PANEL_EDGE),
        ))
        .add_children(&[fill])
        .id();
    let numbers = text_node(
        cmd,
        format!("resolve {current}/{max}"),
        style::small_font(),
        style::BONE,
    );
    cmd.spawn(Node {
        align_items: AlignItems::Center,
        column_gap: Val::Px(10.0),
        ..default()
    })
    .add_children(&[bar, numbers])
    .id()
}

/// Small gold-edged badges like `Valour +2` for every non-zero stage.
fn stage_badge_row(cmd: &mut Commands, stages: &[(String, i8)]) -> Entity {
    let badges: Vec<Entity> = stages
        .iter()
        .map(|(stat, stage)| {
            let label = text_node(
                cmd,
                format!("{stat} {stage:+}"),
                style::small_font(),
                style::GOLD,
            );
            cmd.spawn((
                Node {
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BorderColor::all(style::GOLD_DIM),
            ))
            .add_children(&[label])
            .id()
        })
        .collect();
    cmd.spawn(Node {
        column_gap: Val::Px(6.0),
        flex_wrap: FlexWrap::Wrap,
        ..default()
    })
    .add_children(&badges)
    .id()
}

/// Up to four rite buttons; exhausted rites render as cold, dead slabs.
fn rite_row(cmd: &mut Commands, rites: &[RiteCard]) -> Entity {
    let buttons: Vec<Entity> = rites
        .iter()
        .map(|rite| {
            let label = rite_label(rite);
            if rite.usable {
                action_button(cmd, label, ClientInput::PickRite { slot: rite.slot })
            } else {
                crate::screens::disabled_button(cmd, label)
            }
        })
        .collect();
    cmd.spawn(Node {
        column_gap: Val::Px(10.0),
        row_gap: Val::Px(8.0),
        flex_wrap: FlexWrap::Wrap,
        ..default()
    })
    .add_children(&buttons)
    .id()
}

fn rite_label(rite: &RiteCard) -> String {
    if rite.power > 0 {
        format!(
            "{}\n{} {} | Pow {} | Acc {} | Zeal {}/{}",
            rite.name,
            rite.type_display,
            rite.category_display,
            rite.power,
            rite.accuracy,
            rite.zeal_left,
            rite.zeal_max
        )
    } else {
        format!(
            "{}\n{} {} | Acc {} | Zeal {}/{}",
            rite.name,
            rite.type_display,
            rite.category_display,
            rite.accuracy,
            rite.zeal_left,
            rite.zeal_max
        )
    }
}

/// Voluntary switches plus the walk-away concede.
fn command_row(cmd: &mut Commands, snap: &ViewSnapshot) -> Entity {
    let mut kids: Vec<Entity> = snap
        .can_switch_to
        .iter()
        .map(|target| {
            action_button(
                cmd,
                format!("Send {}", switch_target_name(target)),
                ClientInput::PickSwitch {
                    party_idx: switch_target_idx(target),
                },
            )
        })
        .collect();
    kids.push(action_button(cmd, "Concede".into(), ClientInput::Concede));
    cmd.spawn(Node {
        column_gap: Val::Px(10.0),
        row_gap: Val::Px(8.0),
        flex_wrap: FlexWrap::Wrap,
        ..default()
    })
    .add_children(&kids)
    .id()
}

/// Modal overlay: the engine demands a switch before anything else.
fn forced_switch_overlay(cmd: &mut Commands, snap: &ViewSnapshot) -> Entity {
    let mut kids = vec![text_node(
        cmd,
        "Choose your next champion.".into(),
        style::head_font(),
        style::GOLD,
    )];
    for target in &snap.can_switch_to {
        kids.push(action_button(
            cmd,
            switch_target_name(target).to_string(),
            ClientInput::PickSwitch {
                party_idx: switch_target_idx(target),
            },
        ));
    }
    let card = panel(cmd, &kids);
    cmd.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(style::OVERLAY_DIM),
    ))
    .add_children(&[card])
    .id()
}

fn ailment_label(ailment: Ailment) -> &'static str {
    match ailment {
        Ailment::Brand => "branded (burning)",
        Ailment::Shackled => "shackled (slowed)",
        Ailment::Venom => "envenomed",
        Ailment::Slumber => "slumbering",
        Ailment::Frost => "frost-bound",
    }
}

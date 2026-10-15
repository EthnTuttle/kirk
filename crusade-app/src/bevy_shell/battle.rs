//! The in-battle screen: me/foe panels (species, level, typing, resolve bar,
//! ailment, stage badges, reserves), the chronicle, and the action rows —
//! rites from `my_rites`, switches from `can_switch_to`, Concede. The
//! forced-switch phase draws the same screen under a modal overlay.

use bevy::prelude::*;

use crusade_app::{BattleChampionView, ClientInput, RiteCard, ViewSnapshot};
use crusade_core::data;
use crusade_core::types::Ailment;

use crate::screens::{action_button, log_panel, panel, text_node};
use crate::state::{switch_target_idx, switch_target_name};
use crate::style;

/// Full in-battle layout for [`Phase::InBattle`].
pub(crate) fn battle_screen(
    cmd: &mut Commands,
    snap: &ViewSnapshot,
    assets: Option<&AssetServer>,
) -> Vec<Entity> {
    let mut kids = vec![
        battle_row(cmd, snap, assets),
        log_panel(cmd, "Chronicle", &snap.log_tail),
    ];
    if !snap.my_rites.is_empty() {
        kids.push(rite_row(cmd, &snap.my_rites));
    }
    kids.push(command_row(cmd, snap));
    kids
}

/// [`Phase::ForcedSwitchSelect`]: the normal field, greyed under a modal.
pub(crate) fn forced_switch_screen(
    cmd: &mut Commands,
    snap: &ViewSnapshot,
    assets: Option<&AssetServer>,
) -> Vec<Entity> {
    let mut kids = battle_screen(cmd, snap, assets);
    kids.push(forced_switch_overlay(cmd, snap));
    kids
}

/// Me (left) vs foe (right), stretched across the window.
fn battle_row(cmd: &mut Commands, snap: &ViewSnapshot, assets: Option<&AssetServer>) -> Entity {
    let me = snap
        .me
        .as_ref()
        .map(|v| champion_panel(cmd, v, assets))
        .unwrap_or_else(|| text_node(cmd, "...".into(), style::body_font(), style::DIM));
    let foe = snap
        .foe
        .as_ref()
        .map(|v| champion_panel(cmd, v, assets))
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

/// One side's champion card: portrait sprite (when the art lane has the
/// species), then gold edge for ours, ember for the foe.
fn champion_panel(
    cmd: &mut Commands,
    view: &BattleChampionView,
    assets: Option<&AssetServer>,
) -> Entity {
    let border = if view.ours {
        style::GOLD_DIM
    } else {
        style::HP_RED
    };
    let mut kids = Vec::new();
    if let Some(portrait) = champion_sprite(cmd, &view.species_name, assets) {
        kids.push(portrait);
    }
    kids.push(text_node(
        cmd,
        format!("{} - Lv {}", view.species_name, view.level),
        style::head_font(),
        style::BONE,
    ));
    kids.push(resolve_row(cmd, view.current_resolve, view.max_resolve));
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

/// The 128x128 battle sprite above the name row — the section 2 seam-1
/// `ImageNode` keyed by species id (`sprites/champions/{id:03}-{slug}.png`,
/// staged by `tools/artbake/stage_sprites.py`). Spawned only when the app
/// HAS an asset server (MinimalPlugins smoke runs do not) AND the species
/// is one of the 19 art-lane species; the name/resolve/type text rows below
/// are unconditional, so a sprite whose fetch 404s (wasm during a partial
/// deploy) simply draws nothing over an intact text panel.
fn champion_sprite(
    cmd: &mut Commands,
    species_name: &str,
    assets: Option<&AssetServer>,
) -> Option<Entity> {
    let sprites = champion_sprite_path(species_name)?;
    let assets = assets?;
    Some(
        cmd.spawn((
            ImageNode::new(assets.load(sprites)),
            Node {
                width: Val::Px(128.0),
                height: Val::Px(128.0),
                ..default()
            },
        ))
        .id(),
    )
}

/// Asset path for a battle view's species name. `BattleChampionView`
/// carries no species id, but its `species_name` is copied verbatim from
/// the static SPECIES table (crusade-app/src/client.rs::champion_view), so
/// a reverse name scan recovers exactly the id the client rendered. The
/// client's defensive "Champion #N" fallback name matches nothing and
/// yields `None` on purpose: unknown species keep the text-only render.
fn champion_sprite_path(species_name: &str) -> Option<String> {
    let sp = data::species::SPECIES
        .iter()
        .find(|sp| sp.name == species_name)?;
    Some(format!(
        "sprites/champions/{:03}-{}.png",
        sp.id,
        slugify(sp.name)
    ))
}

/// Asset-slug form of a species name: lowercase ASCII, with every run of
/// non-alphanumerics collapsed to one dash. "Vigil-Knight Wymar" and
/// "Squire Audric" both land on their staged file slugs.
fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut pending_dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(ch.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every static-table species name resolves to the exact staged sprite
    /// filename (the ui-shots pass then proves the files decode on screen).
    #[test]
    fn every_species_resolves_to_its_staged_sprite_path() {
        let expected: &[(u16, &str)] = &[
            (1, "squire-audric"),
            (2, "cleric-mabel"),
            (3, "serjeant-bastien"),
            (4, "hospitaller-ysolt"),
            (5, "wayfarer-aelis"),
            (6, "canoness-hrothwyn"),
            (7, "vexillor-anchetil"),
            (8, "brother-aldwyn"),
            (9, "cantor-sabina"),
            (10, "chevalier-raimbaud"),
            (11, "vigil-knight-wymar"),
            (12, "friar-osbert"),
            (101, "prince-of-pride"),
            (102, "wretch-of-envy"),
            (103, "fiend-of-lust"),
            (104, "sleeper-of-sloth"),
            (105, "brute-of-wrath"),
            (106, "glutton-maw"),
            (107, "grasper-of-greed"),
        ];
        assert_eq!(data::species::SPECIES.len(), expected.len());
        for (sp, (id, slug)) in data::species::SPECIES.iter().zip(expected) {
            assert_eq!(sp.id, *id);
            assert_eq!(
                champion_sprite_path(sp.name).as_deref(),
                Some(format!("sprites/champions/{id:03}-{slug}.png").as_str()),
                "{}",
                sp.name
            );
        }
    }

    /// The client's defensive fallback name (unknown species in a corrupt
    /// store/save) must NOT guess a sprite: text-only render is the contract.
    #[test]
    fn unknown_species_name_gets_no_sprite() {
        assert_eq!(champion_sprite_path("Champion #55"), None);
        assert_eq!(champion_sprite_path("Vigo the Carpathian"), None);
    }

    #[test]
    fn slugify_matches_staged_slugs() {
        assert_eq!(slugify("Squire Audric"), "squire-audric");
        assert_eq!(slugify("Vigil-Knight Wymar"), "vigil-knight-wymar");
        assert_eq!(slugify("Glutton Maw"), "glutton-maw");
    }
}

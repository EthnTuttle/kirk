//! The rebuild dispatch system, shared node builders, and the non-battle
//! screens (menu, roster, campaign rest, battle over).

use bevy::prelude::*;

use crusade_app::{ChampionCard, ClientInput, Phase, ViewSnapshot};

use crate::battle;
use crate::state::{
    ActionButton, BaseBackground, GameShell, RenderedVersion, SnapshotVersion, UiRoot,
};
use crate::style;

/// Repaint the world when (and only when) a handled input bumped the
/// snapshot version: the UI is a pure function of `ClientGame` state, so
/// whole-tree despawn-and-rebuild is both correct and fast enough at 60 fps.
pub fn rebuild_ui(
    mut commands: Commands,
    shell: Res<GameShell>,
    version: Res<SnapshotVersion>,
    mut rendered: ResMut<RenderedVersion>,
    roots: Query<Entity, With<UiRoot>>,
) {
    if rendered.0 == Some(version.0) {
        return;
    }
    rendered.0 = Some(version.0);
    for root in &roots {
        commands.entity(root).despawn(); // children go with it
    }
    let (snap, roster) = {
        let game = shell.game.lock().expect("game mutex poisoned");
        (game.snapshot(), game.roster_cards())
    };
    let (kids, centered) = match snap.phase {
        Phase::Menu => (menu_screen(&mut commands), true),
        Phase::Roster => (roster_screen(&mut commands, &roster), false),
        Phase::BattleSetup => (
            vec![text_node(
                &mut commands,
                "Mustering the host...".into(),
                style::head_font(),
                style::BONE,
            )],
            true,
        ),
        Phase::InBattle => (battle::battle_screen(&mut commands, &snap), false),
        Phase::ForcedSwitchSelect => (battle::forced_switch_screen(&mut commands, &snap), false),
        Phase::CampaignRest => (rest_screen(&mut commands, &snap), true),
        Phase::BattleOver => (over_screen(&mut commands, &snap), true),
    };
    commands
        .spawn((
            UiRoot,
            style::screen_node(centered),
            BackgroundColor(style::NAVY),
        ))
        .add_children(&kids);
}

// ---------- Shared builders ----------

/// Plain text line.
pub(crate) fn text_node(
    cmd: &mut Commands,
    content: String,
    font: TextFont,
    color: Color,
) -> Entity {
    cmd.spawn((Text::new(content), font, TextColor(color))).id()
}

/// Clickable button bound to one [`ClientInput`].
pub(crate) fn action_button(cmd: &mut Commands, label: String, input: ClientInput) -> Entity {
    let text = text_node(cmd, label, style::body_font(), style::BONE);
    cmd.spawn((
        Button,
        style::button_node(),
        BackgroundColor(style::BUTTON),
        BorderColor::all(style::GOLD_DIM),
        BaseBackground(style::BUTTON),
        ActionButton(input),
    ))
    .add_children(&[text])
    .id()
}

/// Inert slab for actions the client would reject (e.g. a zeal-empty rite).
pub(crate) fn disabled_button(cmd: &mut Commands, label: String) -> Entity {
    let text = text_node(cmd, label, style::body_font(), style::DIM);
    cmd.spawn((
        style::button_node(),
        BackgroundColor(style::BUTTON_DISABLED),
        BorderColor::all(style::PANEL_EDGE),
    ))
    .add_children(&[text])
    .id()
}

/// Bordered dark panel around `kids` (vertical stack).
pub(crate) fn panel(cmd: &mut Commands, kids: &[Entity]) -> Entity {
    cmd.spawn((
        style::panel_node(),
        BackgroundColor(style::PANEL),
        BorderColor::all(style::PANEL_EDGE),
    ))
    .add_children(kids)
    .id()
}

/// Titled log box ("Chronicle"); one dim line per `log_tail` entry.
pub(crate) fn log_panel(cmd: &mut Commands, title: &str, lines: &[String]) -> Entity {
    let mut kids = vec![text_node(
        cmd,
        title.to_string(),
        style::small_font(),
        style::GOLD_DIM,
    )];
    for line in lines {
        kids.push(text_node(
            cmd,
            line.clone(),
            style::small_font(),
            style::BONE,
        ));
    }
    panel(cmd, &kids)
}

// ---------- Menu ----------

fn menu_screen(cmd: &mut Commands) -> Vec<Entity> {
    vec![
        text_node(cmd, "DEUS VULT".into(), style::title_font(), style::GOLD),
        text_node(
            cmd,
            "— Champions of the Cross —".into(),
            style::head_font(),
            style::GOLD_DIM,
        ),
        action_button(cmd, "Roster".into(), ClientInput::OpenRoster),
        action_button(
            cmd,
            "Practice Duel (level 25)".into(),
            ClientInput::StartPractice { level: 25 },
        ),
        action_button(cmd, "Campaign".into(), ClientInput::StartCampaign),
        text_node(
            cmd,
            "Contrition and cold steel.".into(),
            style::small_font(),
            style::DIM,
        ),
    ]
}

// ---------- Roster ----------

fn roster_screen(cmd: &mut Commands, cards: &[ChampionCard]) -> Vec<Entity> {
    let title = text_node(cmd, "The Company".into(), style::head_font(), style::GOLD);
    let back = action_button(cmd, "Back".into(), ClientInput::CloseToMenu);
    let head = cmd
        .spawn(Node {
            width: Val::Percent(100.0),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            ..default()
        })
        .add_children(&[title, back])
        .id();
    let card_entities: Vec<Entity> = cards.iter().map(|c| champion_card(cmd, c)).collect();
    let grid = cmd
        .spawn(Node {
            column_gap: Val::Px(12.0),
            row_gap: Val::Px(12.0),
            flex_wrap: FlexWrap::Wrap,
            ..default()
        })
        .add_children(&card_entities)
        .id();
    vec![head, grid]
}

fn champion_card(cmd: &mut Commands, card: &ChampionCard) -> Entity {
    let mut kids = vec![
        text_node(
            cmd,
            format!("{} — Lv {}", card.species_name, card.level),
            style::body_font(),
            style::BONE,
        ),
        text_node(
            cmd,
            format!("resolve {}/{}", card.current_resolve, card.max_resolve),
            style::small_font(),
            style::GOLD,
        ),
    ];
    if let Some(ailment) = card.ailment {
        kids.push(text_node(
            cmd,
            format!("afflicted: {ailment:?}"),
            style::small_font(),
            style::HP_RED,
        ));
    }
    if !card.types_display.is_empty() {
        kids.push(text_node(
            cmd,
            card.types_display.join(" + "),
            style::small_font(),
            style::GOLD_DIM,
        ));
    }
    for rite in &card.rites {
        kids.push(text_node(
            cmd,
            format!(
                "- {} ({}, {} zeal)",
                rite.name, rite.type_display, rite.zeal_max
            ),
            style::small_font(),
            style::DIM,
        ));
    }
    let mut panel_entity = cmd.spawn((
        Node {
            max_width: Val::Px(280.0),
            ..style::panel_node()
        },
        BackgroundColor(style::PANEL),
        BorderColor::all(style::PANEL_EDGE),
    ));
    panel_entity.add_children(&kids).id()
}

// ---------- Campaign rest ----------

fn rest_screen(cmd: &mut Commands, snap: &ViewSnapshot) -> Vec<Entity> {
    let mut kids = vec![text_node(
        cmd,
        "The company rests.".into(),
        style::head_font(),
        style::GOLD,
    )];
    if let Some(c) = snap.campaign {
        kids.push(text_node(
            cmd,
            format!(
                "Bosses defeated: {}/7 — contrition shards earned: {}",
                c.defeated, c.shards_earned
            ),
            style::body_font(),
            style::BONE,
        ));
    }
    kids.push(log_panel(cmd, "Word from the camp", &snap.log_tail));
    let cont = action_button(
        cmd,
        "Press on to the next sin".into(),
        ClientInput::ContinueCampaign,
    );
    let abandon = action_button(
        cmd,
        "Abandon the crusade (keep shards)".into(),
        ClientInput::AbandonCampaign,
    );
    let buttons = cmd
        .spawn(Node {
            column_gap: Val::Px(12.0),
            ..default()
        })
        .add_children(&[cont, abandon])
        .id();
    kids.push(buttons);
    kids
}

// ---------- Battle over ----------

fn over_screen(cmd: &mut Commands, snap: &ViewSnapshot) -> Vec<Entity> {
    let (banner, color) = match snap.battle_result {
        Some(0) => ("Victory — Deo Gratias", style::GOLD),
        Some(_) => ("Defeat — Pray and Regroup", style::HP_RED),
        None => ("The field falls quiet.", style::BONE),
    };
    let mut kids = vec![text_node(cmd, banner.into(), style::title_font(), color)];
    if let Some(c) = snap.campaign {
        kids.push(text_node(
            cmd,
            format!(
                "Nodes cleared: {}/7 — contrition shards earned: {}",
                c.defeated, c.shards_earned
            ),
            style::body_font(),
            style::BONE,
        ));
    }
    kids.push(log_panel(cmd, "Last rites of the battle", &snap.log_tail));
    kids.push(action_button(
        cmd,
        "Amen".into(),
        ClientInput::AcknowledgeResult,
    ));
    kids
}

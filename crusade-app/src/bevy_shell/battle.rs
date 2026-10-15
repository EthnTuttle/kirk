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

/// One idle-loop frame plateau, in app frames: 90 frames ≈ 1.5 s at the
/// windowed shell's vsync (a full idle→idle2→idle cycle every ≈3 s — slow
/// and readable, not twitchy; "~1 fps or slower" per card D5). Under the
/// ui-shots unthrottled runner the same plateau logic runs with NO
/// wall-time input, so captures scripted at fixed frame counts always land
/// on the same frame of the cycle.
pub(crate) const IDLE_FRAME_TICKS: u64 = 90;

/// The cycle position a global tick shows: alternating 0/1 plateaus
/// (idle/idle2). Single-frame sets (the 4.3 pilot grid has no `idle2` cell)
/// hold frame 0 forever via [`ChampionAnim::frame_at`].
pub(crate) fn anim_phase(tick: u64) -> u64 {
    (tick / IDLE_FRAME_TICKS) % 2
}

/// A battle portrait with staged idle-loop frames (card D5), resolved by
/// [`champion_anim_paths`] from `sprites/anim/{id:03}-{slug}/`. Both
/// handles are loaded at spawn, so the first swap never draws a blank panel
/// while a fetch is in flight. Species without an anim set (bosses — 4.5
/// generated no sheets for them, by design — and any name outside the
/// static table) spawn no marker and keep the pre-D5 static sprite.
#[derive(Component)]
pub(crate) struct ChampionAnim {
    idle: Handle<Image>,
    idle2: Option<Handle<Image>>,
}

impl ChampionAnim {
    /// The handle the given cycle phase shows; phase 1 falls back to the
    /// base idle when the sheet had no second idle variant.
    fn frame_at(&self, phase: u64) -> Handle<Image> {
        match (phase, &self.idle2) {
            (1, Some(idle2)) => idle2.clone(),
            _ => self.idle.clone(),
        }
    }
}

/// Advance the slow idle loop exactly one app frame and swap any portrait
/// whose cycle phase flipped. Called from `input::process_inputs` — the one
/// per-frame system every shell binary (windowed, `--smoke`, ui-shots)
/// already registers — which is what lets this run with zero changes to
/// `main.rs`, `screens.rs`, or the harness. Mutation is guarded, so
/// plateaus cost no change-detection work.
pub(crate) fn advance_champion_anim(
    tick: &mut crate::state::AnimTick,
    anims: &mut Query<(&ChampionAnim, &mut ImageNode)>,
) {
    tick.0 = tick.0.wrapping_add(1);
    let phase = anim_phase(tick.0);
    for (anim, mut image) in anims.iter_mut() {
        let want = anim.frame_at(phase);
        if image.image != want {
            image.image = want;
        }
    }
}

/// Idle-loop frame paths for a species with a 4.5 anim set, mirroring the
/// ANIM table in `tools/artbake/stage_sprites.py`. Champions (ids 1..=12)
/// all staged `01_idle.png`; every batch-grid champion also staged
/// `04_idle2.png`, while Squire Audric's 4.3 pilot sheet is the
/// combat-family grid (idle/step/strike/hit/defend/pray) with no idle2, so
/// his loop holds the single idle frame. Bosses and unknown names get
/// `None` → the static [`champion_sprite_path`] render. Only idle/idle2 are
/// wired: pray/step/look/bow never auto-play in battle, and attack/hit
/// reactions stay out of scope until the combat-arc card.
fn champion_anim_paths(species_name: &str) -> Option<(String, Option<String>)> {
    let sp = data::species::SPECIES
        .iter()
        .find(|sp| sp.name == species_name)?;
    if !matches!(sp.id, 1..=12) {
        return None;
    }
    let dir = format!("sprites/anim/{:03}-{}", sp.id, slugify(sp.name));
    let idle = format!("{dir}/01_idle.png");
    let idle2 = (sp.id != 1).then(|| format!("{dir}/04_idle2.png"));
    Some((idle, idle2))
}

/// The 128x128 battle sprite above the name row — the section 2 seam-1
/// `ImageNode` keyed by species id (`sprites/champions/{id:03}-{slug}.png`,
/// staged by `tools/artbake/stage_sprites.py`). Spawned only when the app
/// HAS an asset server (MinimalPlugins smoke runs do not) AND the species
/// is one of the 19 art-lane species; the name/resolve/type text rows below
/// are unconditional, so a sprite whose fetch 404s (wasm during a partial
/// deploy) simply draws nothing over an intact text panel.
///
/// Card D5: champions with a staged anim set (`champion_anim_paths`) spawn
/// the animated idle portrait instead — same node size, handles preloaded —
/// while bosses and unknowns keep this static path unchanged.
fn champion_sprite(
    cmd: &mut Commands,
    species_name: &str,
    assets: Option<&AssetServer>,
) -> Option<Entity> {
    let assets = assets?;
    let node = Node {
        width: Val::Px(128.0),
        height: Val::Px(128.0),
        ..default()
    };
    if let Some((idle, idle2)) = champion_anim_paths(species_name) {
        let idle = assets.load(idle);
        let anim = ChampionAnim {
            idle: idle.clone(),
            idle2: idle2.map(|pose| assets.load(pose)),
        };
        return Some(cmd.spawn((ImageNode::new(idle), node, anim)).id());
    }
    let sprites = champion_sprite_path(species_name)?;
    Some(cmd.spawn((ImageNode::new(assets.load(sprites)), node)).id())
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

    /// The idle-loop resolver (card D5): every champion resolves to its
    /// staged `01_idle.png`; the 11 batch-grid champions get `04_idle2.png`
    /// too, while Squire Audric's 4.3 pilot grid has no idle2 cell. Bosses
    /// and unknown names get `None` — "no frames -> static" — and the static
    /// path still answers for every boss, which is exactly the fallback the
    /// spawner relies on.
    #[test]
    fn anim_paths_resolve_for_staged_sets_only() {
        assert_eq!(
            champion_anim_paths("Squire Audric"),
            Some(("sprites/anim/001-squire-audric/01_idle.png".to_string(), None))
        );
        assert_eq!(
            champion_anim_paths("Serjeant Bastien"),
            Some((
                "sprites/anim/003-serjeant-bastien/01_idle.png".to_string(),
                Some("sprites/anim/003-serjeant-bastien/04_idle2.png".to_string()),
            ))
        );
        for sp in data::species::SPECIES.iter().filter(|sp| matches!(sp.id, 1..=12)) {
            assert!(
                champion_anim_paths(sp.name).is_some(),
                "champion {} has a staged idle frame",
                sp.name
            );
        }
        for sp in data::species::SPECIES.iter().filter(|sp| sp.id >= 100) {
            assert_eq!(
                champion_anim_paths(sp.name),
                None,
                "boss {} keeps the static sprite",
                sp.name
            );
            assert!(
                champion_sprite_path(sp.name).is_some(),
                "static fallback still resolves for {}",
                sp.name
            );
        }
        assert_eq!(champion_anim_paths("Champion #55"), None);
        assert_eq!(champion_anim_paths("Vigo the Carpathian"), None);
    }

    /// The idle cycle is a pure function of the frame counter: 90-tick
    /// plateaus alternating frames 0/1, no wall time anywhere, so ui-shots
    /// captures land on a fixed cycle position run after run.
    #[test]
    fn idle_phase_alternates_on_frame_plateaus() {
        assert_eq!(anim_phase(0), 0);
        assert_eq!(anim_phase(IDLE_FRAME_TICKS - 1), 0);
        assert_eq!(anim_phase(IDLE_FRAME_TICKS), 1);
        assert_eq!(anim_phase(2 * IDLE_FRAME_TICKS - 1), 1);
        assert_eq!(anim_phase(2 * IDLE_FRAME_TICKS), 0);
        assert_eq!(anim_phase(7 * IDLE_FRAME_TICKS + 3), 1);
    }

    #[test]
    fn slugify_matches_staged_slugs() {
        assert_eq!(slugify("Squire Audric"), "squire-audric");
        assert_eq!(slugify("Vigil-Knight Wymar"), "vigil-knight-wymar");
        assert_eq!(slugify("Glutton Maw"), "glutton-maw");
    }
}

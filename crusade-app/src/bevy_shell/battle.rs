//! The in-battle screen: me/foe panels (species, level, typing, resolve bar,
//! ailment, stage badges, reserves), the chronicle, and the action rows —
//! rites from `my_rites`, switches from `can_switch_to`, Concede. The
//! forced-switch phase draws the same screen under a modal overlay.
//!
//! Portrait animation: the card D5 idle loop runs off the frame counter;
//! the card D6 combat arc (strike/hit poses, boss white-flash) is stamped
//! per rebuild from the snapshot's `fresh_events` and counts down in
//! frames — presentation only, never game state, fully deterministic.

use std::sync::Mutex;

use bevy::prelude::*;

use crusade_app::{BattleChampionView, ClientInput, RiteCard, ViewSnapshot};
use crusade_core::data;
use crusade_core::engine::BattleEvent;
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
    // Card D6 arc attribution. Engine events carry engine side ids; the
    // practice/campaign client always renders side 0 as "me"
    // (client.rs::champion_view(b, MY_SIDE, MY_SIDE)). The net-duel driver's
    // snapshot is the one exception: its `me` is the joiner's side 1 half
    // the time, and nothing in the snapshot band says which — so for the
    // driver-produced shape (its documented `roster_count: 0` tell;
    // practice/campaign battles can't exist with an empty store) arcs stay
    // idle-only rather than cross-wire a swing to the wrong panel. That is
    // the card-sanctioned honest approximation; the scripted harness paths
    // (practice, campaign) attribute exactly.
    let me_plan = arc_plan_for_panel(&snap.fresh_events, 0, snap.me.as_ref(), snap);
    let foe_plan = arc_plan_for_panel(&snap.fresh_events, 1, snap.foe.as_ref(), snap);
    let me = snap
        .me
        .as_ref()
        .map(|v| champion_panel(cmd, v, assets, me_plan))
        .unwrap_or_else(|| text_node(cmd, "...".into(), style::body_font(), style::DIM));
    let foe = snap
        .foe
        .as_ref()
        .map(|v| champion_panel(cmd, v, assets, foe_plan))
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

/// Compute one panel's card-D6 flourish: the plan for this side's engine
/// events, with strike beats only when the displayed species has staged arc
/// frames. Net-duel snapshots (see [`battle_row`]) suppress the arc.
fn arc_plan_for_panel(
    events: &[BattleEvent],
    side: u8,
    view: Option<&BattleChampionView>,
    snap: &ViewSnapshot,
) -> Vec<(ArcPose, u16)> {
    if snap.roster_count == 0 {
        return Vec::new();
    }
    let Some(view) = view else {
        return Vec::new();
    };
    arc_plan(
        events,
        side,
        champion_arc_paths(&view.species_name).is_some(),
    )
}

/// One side's champion card: portrait sprite (when the art lane has the
/// species), then gold edge for ours, ember for the foe.
fn champion_panel(
    cmd: &mut Commands,
    view: &BattleChampionView,
    assets: Option<&AssetServer>,
    arc: Vec<(ArcPose, u16)>,
) -> Entity {
    let border = if view.ours {
        style::GOLD_DIM
    } else {
        style::HP_RED
    };
    let mut kids = Vec::new();
    if let Some(portrait) = champion_sprite(cmd, &view.species_name, assets, arc) {
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

/// Card D6: how long one turn's combat-arc flourish holds before the panel
/// falls back to the idle loop (~0.75 s at vsync). The whole flourish —
/// every beat of the exchange in event order — is sliced out of this one
/// window, so slow multi-event turns never drag it out.
pub(crate) const ARC_HOLD_TICKS: u16 = 45;
/// Bosses have no strike/hit frames (4.5 skipped them, 4.3 hybrid decision
/// is champion-only per-frame); their hurt feedback is the classic JRPG
/// white flash — deliberately shorter than a champion's pose window.
pub(crate) const FLASH_TICKS: u16 = 15;

/// The hit-flash tint: a multiplicative boost that drives the palette's
/// light ramp to white while the ink shadows stay legible (`ImageNode`'s
/// default `Color::WHITE` = no tint).
fn flash_tint() -> Color {
    Color::srgb(4.0, 4.0, 4.0)
}

/// One beat of a combat-arc flourish (card D6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArcPose {
    /// The panel's champion mid-attack (`07_strike.png`).
    Strike,
    /// The panel's combatant taking a hit (`08_hit.png` for champions,
    /// tint-flash alone for frame-less bosses).
    Hit,
}

/// Plan one panel's combat arc from a snapshot's `fresh_events` — the
/// take-drain event batch of the input that just resolved, so the flourish
/// fires exactly once per exchange, at the rebuild that first shows its
/// outcome. Event order is kept and equal-length beats slice up
/// [`ARC_HOLD_TICKS`]; consecutive same-pose runs collapse (a MultiHit
/// rite's N `Damage` events are ONE recoil, not a strobe).
///
/// `caster_strikes`: champions with staged arc frames
/// ([`champion_arc_paths`]) turn their own `RiteUsed` into a [`ArcPose::Strike`]
/// beat; frame-less combatants (bosses, unknown species) never pose and get
/// one brief flash beat iff they took damage. `side` is the ENGINE side
/// (events carry engine sides; see [`battle_row`] for the me/foe mapping).
///
/// Not wired: ailment chip (`AilmentChip`) never flashes — end-of-turn burn
/// ticks firing an attack-recoil pose read as a new hit, so only direct
/// `Damage` events count. All other event kinds are ignored by design.
pub(crate) fn arc_plan(
    events: &[BattleEvent],
    side: u8,
    caster_strikes: bool,
) -> Vec<(ArcPose, u16)> {
    if !caster_strikes {
        let hurt = events
            .iter()
            .any(|e| matches!(e, BattleEvent::Damage { side: s, .. } if *s == side));
        return if hurt {
            vec![(ArcPose::Hit, FLASH_TICKS)]
        } else {
            Vec::new()
        };
    }
    let mut poses: Vec<ArcPose> = Vec::new();
    for e in events {
        let pose = match e {
            BattleEvent::RiteUsed { side: s, .. } if *s == side => Some(ArcPose::Strike),
            BattleEvent::Damage { side: s, .. } if *s == side => Some(ArcPose::Hit),
            _ => None,
        };
        if let Some(p) = pose {
            if poses.last() != Some(&p) {
                poses.push(p);
            }
        }
    }
    if poses.is_empty() {
        return Vec::new();
    }
    let n = poses.len() as u16;
    let each = ARC_HOLD_TICKS / n;
    let mut plan: Vec<(ArcPose, u16)> = poses.into_iter().map(|p| (p, each)).collect();
    // The division remainder rides the first beat, keeping the total exact.
    plan[0].1 += ARC_HOLD_TICKS - each * n;
    plan
}

/// The cycle position a global tick shows: alternating 0/1 plateaus
/// (idle/idle2). Single-frame sets (the 4.3 pilot grid has no `idle2` cell)
/// hold frame 0 forever via [`ChampionAnim::frame_at`].
pub(crate) fn anim_phase(tick: u64) -> u64 {
    (tick / IDLE_FRAME_TICKS) % 2
}

/// A battle portrait with staged frames (cards D5 + D6), resolved by
/// [`champion_anim_paths`] / [`champion_arc_paths`] from
/// `sprites/anim/{id:03}-{slug}/`. All handles are loaded at spawn, so the
/// first swap never draws a blank panel while a fetch is in flight.
///
/// Card D6 combat arc: `battle_screen` stamps `arc` at spawn from the
/// snapshot's `fresh_events` (see [`arc_plan`]); [`advance_champion_anim`]
/// plays the plan one frame at a time, then falls back to the D5 idle loop.
/// The `Mutex` is the single point where per-frame arc mutation crosses the
/// locked D5 wiring — `input::process_inputs`' query borrows `&ChampionAnim`
/// immutably, and no other thread ever holds the lock. Pure frame-count
/// countdown, never wall time, so ui-shots stays byte-deterministic.
///
/// Static-sprite combatants (bosses — no frames by design — and any
/// staged-table species without an anim set) spawn this marker too, with
/// `idle` = their static sprite and every extra handle `None`: the idle
/// loop degenerates to a no-swap, and a `Hit` beat with no `hit` handle
/// degrades to the tint flash alone — the boss white-flash feedback.
#[derive(Component)]
pub(crate) struct ChampionAnim {
    idle: Handle<Image>,
    idle2: Option<Handle<Image>>,
    strike: Option<Handle<Image>>,
    hit: Option<Handle<Image>>,
    /// Remaining flourish beats, front = current. Empty = idle loop.
    arc: Mutex<Vec<(ArcPose, u16)>>,
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

/// Advance every battle portrait exactly one app frame: card D6 arc beats
/// first (pose swap + hit flash, ticking the plan down), then the card D5
/// idle loop for anything at rest. Called from `input::process_inputs` —
/// the one per-frame system every shell binary (windowed, `--smoke`,
/// ui-shots) already registers — which is what lets this run with zero
/// changes to `main.rs`, `screens.rs`, or the harness. Mutation is guarded,
/// so plateaus cost no change-detection work.
pub(crate) fn advance_champion_anim(
    tick: &mut crate::state::AnimTick,
    anims: &mut Query<(&ChampionAnim, &mut ImageNode)>,
) {
    tick.0 = tick.0.wrapping_add(1);
    let phase = anim_phase(tick.0);
    for (anim, mut image) in anims.iter_mut() {
        let (want, flashed) = {
            let mut arc = anim.arc.lock().expect("champion arc mutex poisoned");
            let head = arc.first_mut().map(|(pose, left)| {
                *left -= 1;
                (*pose, *left)
            });
            if matches!(head, Some((_, 0))) {
                arc.remove(0);
            }
            match head {
                Some((pose, _)) => {
                    let frame = match pose {
                        ArcPose::Strike => anim.strike.as_ref(),
                        ArcPose::Hit => anim.hit.as_ref(),
                    }
                    .unwrap_or(&anim.idle)
                    .clone();
                    (frame, matches!(pose, ArcPose::Hit))
                }
                None => (anim.frame_at(phase), false),
            }
        };
        if image.image != want {
            image.image = want;
        }
        let tint = if flashed { flash_tint() } else { Color::WHITE };
        if image.color != tint {
            image.color = tint;
        }
    }
}

/// Idle-loop frame paths for a species with a 4.5 anim set, mirroring the
/// ANIM table in `tools/artbake/stage_sprites.py`. Champions (ids 1..=12)
/// all staged `01_idle.png`; every batch-grid champion also staged
/// `04_idle2.png`, while Squire Audric's 4.3 pilot sheet is the
/// combat-family grid (idle/step/strike/hit/defend/pray) with no idle2, so
/// his loop holds the single idle frame. Bosses and unknown names get
/// `None` → the static [`champion_sprite_path`] render. Only idle/idle2
/// auto-play: pray/step/look/bow never auto-play in battle, and the card
/// D6 strike/hit arc frames resolve through [`champion_arc_paths`].
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

/// Combat-arc frame paths (card D6) — `(strike, hit)` from the per-frame
/// D6 generations (`07_strike.png`, `08_hit.png`; the 4.3 hybrid decision
/// generates arc frames per-frame, never as sheet slices — Audric's pilot
/// 03/04 cells stay pilot history). Champions 1..=12 are staged; bosses
/// and unknown names get `None` → no pose beats, flash alone on hit.
fn champion_arc_paths(species_name: &str) -> Option<(String, String)> {
    let sp = data::species::SPECIES
        .iter()
        .find(|sp| sp.name == species_name)?;
    if !matches!(sp.id, 1..=12) {
        return None;
    }
    let dir = format!("sprites/anim/{:03}-{}", sp.id, slugify(sp.name));
    Some((format!("{dir}/07_strike.png"), format!("{dir}/08_hit.png")))
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
/// while bosses and unknowns keep this static path unchanged. Card D6: the
/// panel's fresh-events flourish (`arc`) is stamped at spawn either way —
/// champions preload their strike/hit frames, frame-less statics degrade
/// to the tint flash (`ChampionAnim` doc).
fn champion_sprite(
    cmd: &mut Commands,
    species_name: &str,
    assets: Option<&AssetServer>,
    arc: Vec<(ArcPose, u16)>,
) -> Option<Entity> {
    let assets = assets?;
    let node = Node {
        width: Val::Px(128.0),
        height: Val::Px(128.0),
        ..default()
    };
    if let Some((idle, idle2)) = champion_anim_paths(species_name) {
        let idle = assets.load(idle);
        let (strike, hit) = champion_arc_paths(species_name)
            .map(|(s, h)| (Some(assets.load(s)), Some(assets.load(h))))
            .unwrap_or((None, None));
        let anim = ChampionAnim {
            idle: idle.clone(),
            idle2: idle2.map(|pose| assets.load(pose)),
            strike,
            hit,
            arc: Mutex::new(arc),
        };
        return Some(cmd.spawn((ImageNode::new(idle), node, anim)).id());
    }
    let sprites = champion_sprite_path(species_name)?;
    let sprite = assets.load(sprites);
    let anim = ChampionAnim {
        idle: sprite.clone(),
        idle2: None,
        strike: None,
        hit: None,
        arc: Mutex::new(arc),
    };
    Some(cmd.spawn((ImageNode::new(sprite), node, anim)).id())
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

    /// The card-D6 arc resolver: every champion's staged per-frame strike/hit
    /// pair resolves to its exact asset paths; bosses and unknown names get
    /// `None` → pose-less, flash-only hurt feedback.
    #[test]
    fn arc_paths_resolve_for_all_twelve_champions_only() {
        assert_eq!(
            champion_arc_paths("Squire Audric"),
            Some((
                "sprites/anim/001-squire-audric/07_strike.png".to_string(),
                "sprites/anim/001-squire-audric/08_hit.png".to_string(),
            ))
        );
        for sp in data::species::SPECIES.iter().filter(|sp| matches!(sp.id, 1..=12)) {
            assert!(
                champion_arc_paths(sp.name).is_some(),
                "champion {} has staged arc frames",
                sp.name
            );
        }
        for sp in data::species::SPECIES.iter().filter(|sp| sp.id >= 100) {
            assert_eq!(
                champion_arc_paths(sp.name),
                None,
                "boss {} stays flash-only",
                sp.name
            );
        }
        assert_eq!(champion_arc_paths("Champion #55"), None);
        assert_eq!(champion_arc_paths("Vigo the Carpathian"), None);
    }

    use crusade_core::types::Volatile;

    fn rite(side: u8) -> BattleEvent {
        BattleEvent::RiteUsed { side, rite_id: 7 }
    }

    fn dmg(side: u8) -> BattleEvent {
        BattleEvent::Damage {
            side,
            amount: 12,
            crit: false,
            effectiveness_pct: 100,
        }
    }

    /// A plain exchange — we cast, the foe is hurt, the foe answers, we are
    /// hurt — plans both panels' beats in event order, sliced from exactly
    /// ARC_HOLD_TICKS: our panel plays strike-then-hit, theirs hit-then-strike.
    #[test]
    fn arc_plan_tracks_both_sides_of_an_exchange() {
        let events = vec![
            BattleEvent::TurnStart { turn: 2 },
            rite(0),
            dmg(1),
            rite(1),
            dmg(0),
        ];
        let mine = arc_plan(&events, 0, true);
        let theirs = arc_plan(&events, 1, true);
        assert_eq!(
            mine.iter().map(|b| b.0).collect::<Vec<_>>(),
            vec![ArcPose::Strike, ArcPose::Hit]
        );
        assert_eq!(
            theirs.iter().map(|b| b.0).collect::<Vec<_>>(),
            vec![ArcPose::Hit, ArcPose::Strike]
        );
        assert_eq!(mine.iter().map(|b| b.1).sum::<u16>(), ARC_HOLD_TICKS);
        assert_eq!(theirs.iter().map(|b| b.1).sum::<u16>(), ARC_HOLD_TICKS);
    }

    /// A MultiHit rite's consecutive Damage events collapse into one recoil
    /// (no strobe), and event kinds outside casts/damage never plan a beat.
    #[test]
    fn arc_plan_dedupes_multihit_and_ignores_other_events() {
        let events = vec![
            rite(0),
            dmg(1),
            dmg(1),
            dmg(1),
            BattleEvent::AilmentChip { side: 0, amount: 5 },
            BattleEvent::Missed { side: 1 },
            BattleEvent::VolatileApplied {
                side: 1,
                volatile_: Volatile::Flinch,
            },
        ];
        let theirs = arc_plan(&events, 1, true);
        assert_eq!(theirs, vec![(ArcPose::Hit, ARC_HOLD_TICKS)]);
        // Side 0 cast and took only chip damage: chip is not a hit beat
        // (documented in arc_plan), so the strike stands alone.
        assert_eq!(arc_plan(&events, 0, true), vec![(ArcPose::Strike, ARC_HOLD_TICKS)]);
        // Chip alone plans nothing.
        assert!(arc_plan(&[BattleEvent::AilmentChip { side: 0, amount: 5 }], 0, true).is_empty());
        // Uneven slicing keeps the exact hold: remainder rides beat one.
        let events = vec![rite(0), dmg(1), rite(1), dmg(0), rite(0), dmg(1)];
        let three = arc_plan(&events, 0, true);
        assert_eq!(
            three.iter().map(|b| b.0).collect::<Vec<_>>(),
            vec![ArcPose::Strike, ArcPose::Hit, ArcPose::Strike]
        );
        assert_eq!(three.iter().map(|b| b.1).sum::<u16>(), ARC_HOLD_TICKS);
        // Two cast/hit rounds = four alternating beats (no consecutive
        // dupes, so no collapse): 45 / 4 = 11 beats + remainder 1 on beat 1.
        let events = vec![rite(0), dmg(1), rite(1), dmg(0), rite(0), dmg(0)];
        let four = arc_plan(&events, 0, true);
        assert_eq!(four.iter().map(|b| b.1).collect::<Vec<_>>(), vec![12, 11, 11, 11]);
        assert_eq!(four.iter().map(|b| b.1).sum::<u16>(), ARC_HOLD_TICKS);
    }

    /// Boss path (`caster_strikes == false`): no poses ever; any direct
    /// damage collapses to exactly one FLASH_TICKS hit beat.
    #[test]
    fn arc_plan_bosses_flash_only() {
        let events = vec![rite(1), dmg(1), dmg(1)];
        assert_eq!(arc_plan(&events, 1, false), vec![(ArcPose::Hit, FLASH_TICKS)]);
        assert!(arc_plan(&[rite(1)], 1, false).is_empty());
        assert!(arc_plan(&[dmg(0)], 1, false).is_empty());
    }

    #[test]
    fn slugify_matches_staged_slugs() {
        assert_eq!(slugify("Squire Audric"), "squire-audric");
        assert_eq!(slugify("Vigil-Knight Wymar"), "vigil-knight-wymar");
        assert_eq!(slugify("Glutton Maw"), "glutton-maw");
    }
}

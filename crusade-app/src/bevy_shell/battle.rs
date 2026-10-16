//! The in-battle screen: me/foe panels (species, level, typing, resolve bar,
//! ailment, stage badges, reserves), the chronicle, and the action rows —
//! rites from `my_rites`, switches from `can_switch_to`, Concede. The
//! forced-switch phase draws the same screen under a modal overlay.
//!
//! Layout budget (1280x720): the lower region is engineered to always fit —
//! rites in a FIXED two-column grid of compact cells, switches as one chip
//! row, the chronicle height-capped at [`CHRONICLE_PANEL_LINES`] lines, and
//! panel paddings/gaps tightened from `style::panel_node`'s roomier recipe.
//! Mirror matches (both actives the same species) tag the foe's name line
//! with a "Foe " prefix — text only, never a sprite tint (palette canon).
//!
//! Portrait animation: the card D5 idle loop runs off the frame counter;
//! the card D6/E4 combat arc (three-beat windup/strike/followthrough rite
//! arcs, hit poses, boss white-flash) is stamped per rebuild from the
//! snapshot's `fresh_events` and counts down in frames — presentation
//! only, never game state, fully deterministic. Card E4 also stages the
//! battle-over verdict poses (`11_victory`/`12_defeat`); the row builder
//! lives here ([`verdict_row`]) for the battle-over screens to call.

use std::borrow::Cow;
use std::sync::Mutex;

use bevy::prelude::*;

use crusade_app::{BattleChampionView, ClientInput, RiteCard, ViewSnapshot};
use crusade_core::data;
use crusade_core::engine::BattleEvent;
use crusade_core::types::{Ailment, BattleStat, Secondary, Volatile};

use crate::screens::{action_button, panel, text_node};
use crate::state::{switch_target_idx, switch_target_name, ActionButton, BaseBackground};
use crate::style;
use crate::tooltip;

/// Full in-battle layout for [`Phase::InBattle`].
pub(crate) fn battle_screen(
    cmd: &mut Commands,
    snap: &ViewSnapshot,
    assets: Option<&AssetServer>,
) -> Vec<Entity> {
    let mut kids = vec![
        battle_row(cmd, snap, assets),
        chronicle_panel(cmd, &snap.log_tail),
    ];
    if !snap.my_rites.is_empty() {
        kids.push(rite_grid(cmd, &snap.my_rites));
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
    // Mirror matches (practice/campaign CAN field the same species both
    // sides): tag the foe panel's name line so the two cards never read
    // identically. Name only — palette canon bars sprite tints.
    let mirror = match (snap.me.as_ref(), snap.foe.as_ref()) {
        (Some(me), Some(foe)) => mirror_match(me, foe),
        _ => false,
    };
    let me = snap
        .me
        .as_ref()
        .map(|v| champion_panel(cmd, v, assets, me_plan, false))
        .unwrap_or_else(|| text_node(cmd, "...".into(), style::body_font(), style::DIM));
    let foe = snap
        .foe
        .as_ref()
        .map(|v| champion_panel(cmd, v, assets, foe_plan, mirror))
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

/// Compute one panel's cards-D6/E4 flourish: the plan for this side's
/// engine events, with attack poses only when the displayed species has
/// staged arc frames. Net-duel snapshots (see [`battle_row`]) suppress
/// the arc — the E4 windup/followthrough beats ride the exact same
/// `roster_count == 0` gate as the D6 strike, so a remote echo can never
/// cross-wire a relic-graph swing to the wrong panel.
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

/// True when both actives are the same species (a practice/campaign mirror
/// match): the Foe panel's name line then takes a "Foe " tag.
fn mirror_match(me: &BattleChampionView, foe: &BattleChampionView) -> bool {
    me.species_name == foe.species_name
}

/// One panel's name row: "{Species} - Lv {N}"; a mirror-match foe is
/// prefixed "Foe " so the two cards can never read identically. Text only —
/// the sprite is never tinted (palette canon), and the chronicle's lines
/// stay exactly as the client rendered them.
fn name_line(view: &BattleChampionView, mirror: bool) -> String {
    if mirror {
        format!("Foe {} - Lv {}", view.species_name, view.level)
    } else {
        format!("{} - Lv {}", view.species_name, view.level)
    }
}

// ---------- Hover tips (the tooltip call-site wave) ----------
//
// Every tip below is ASCII, at most two lines at the 480px wrap (budget
// [`TIP_LEN_BUDGET`] chars ~= 66 per line inside the box's padding and
// border), and reads only compile-time tables or the very view row it
/// hangs from -- never game state, never a clock. ONE tip per widget.

/// The two-line budget, pinned by the tests (480px box, 8px padding, 1px
/// border, ~7px advance at the 14px font). Shared with screens.rs' tests;
/// test-only because the prose itself is pinned at the call sites.
#[cfg(test)]
pub(crate) const TIP_LEN_BUDGET: usize = 132;

/// The resolve bar row: current/max, and that faint = faint.
const RESOLVE_TIP: &str =
    "Resolve, current over full: the strength to fight on. At zero the champion faints and must yield the field.";

/// The virtue/sin typing line: what typing means, including the
/// Martial/Spiritual split (crusade-core rites test Valour vs Guard or
/// Piety vs Aegis) and the conqueror matchups of the chart wheel.
const TYPING_TIP: &str =
    "A virtue strikes twofold upon the sin it conquers. Martial rites weigh Valour against Guard; Spiritual, Piety against Aegis.";

/// Shared by every "Send X" switch chip: what the standing reserves mean,
/// and when a switch is owed (a faint) versus freely spent (the deed of
/// its whole turn, per the engine's turn contract).
const SWITCH_TIP: &str =
    "The standing reserves wait in prayer. To call one freely spends the turn's whole deed; after a faint it is owed.";

/// Concede, on a practice field: nothing stands to be lost.
const CONCEDE_TIP_PRACTICE: &str =
    "Yield the field and end the bout; practice asks no penance and yields no shame.";
/// Concede, in a sworn net duel: the attested loss is the whole stake.
const CONCEDE_TIP_DUEL: &str =
    "Yield the field. A sworn duel settles as your attested defeat, and the honor ladder records it.";
/// Concede, mid-crusade: the run ends as a node defeat (client.rs:
/// yielding hands the node to the boss); shards already earned are kept.
const CONCEDE_TIP_CAMPAIGN: &str =
    "Yield the field: the crusade breaks here in defeat; contrition shards already earned are kept.";

/// Pick the honest concede tip. The row KNOWS its context from the
/// snapshot: a live campaign marks itself, and the net-duel driver's
/// documented tell is `roster_count == 0` (practice/campaign battles
/// always stand on a mustered company). Everything else is practice.
fn concede_tip(in_campaign: bool, roster_count: usize) -> &'static str {
    if in_campaign {
        CONCEDE_TIP_CAMPAIGN
    } else if roster_count == 0 {
        CONCEDE_TIP_DUEL
    } else {
        CONCEDE_TIP_PRACTICE
    }
}

/// Hover lore for a battle sprite: the species' short lore line, champions
/// ONLY. A boss's lore IS the Manuscript of Sins' gated allegory
/// (client.rs `SINS_DEFEATED_KEY`), and a battle snapshot carries no
/// defeat mask -- so a boss keeps its allegory until the Hagiography
/// reveals it, same discipline as the sealed manuscript rows. Unknown
/// names (the client's "Champion #N" fallback) never resolve.
fn species_lore_tip(species_name: &str) -> Option<&'static str> {
    let sp = data::species::SPECIES
        .iter()
        .find(|sp| sp.name == species_name)?;
    (sp.id < 100).then_some(sp.lore)
}

/// The composed hover tip for one rite cell: typing and category, what Pow
/// (the wound's weight), Acc (its chance to strike home; 101+ = never
/// misses) and Zeal (the pool of uses) mean, then the rite's carried
/// ailment/status clause if the static table lists one. Reads
/// `data::rite_by_id` (a compile-time table) and the view row only.
fn rite_tip(rite: &RiteCard) -> Cow<'static, str> {
    let mut tip = format!("{} {} -- ", rite.type_display, rite.category_display);
    if rite.power > 0 {
        tip.push_str(&format!("Pow {} weighs the wound, ", rite.power));
    }
    if rite.accuracy > 100 {
        tip.push_str("it never misses, ");
    } else {
        tip.push_str(&format!("Acc {}% strikes home, ", rite.accuracy));
    }
    tip.push_str(&format!("zeal {}/{} left.", rite.zeal_left, rite.zeal_max));
    if let Some(static_rite) = data::rite_by_id(rite.rite_id) {
        for sec in static_rite.secondaries {
            tip.push(' ');
            tip.push_str(&secondary_line(sec));
        }
    }
    tip.into()
}

/// One rite secondary as one terse sentence ("It may (10%) brand the foe
/// (a burning).", "It raises its own Guard by 1 stage."). Chance clauses
/// say "may (p%)"; certainties state the deed plainly.
fn secondary_line(sec: &Secondary) -> String {
    match *sec {
        Secondary::Ailment {
            ailment,
            chance_pct,
        } => {
            let (base, third) = ailment_verb(ailment);
            chance_line(
                chance_pct,
                format!("It {third}."),
                format!("It may ({chance_pct}%) {base}."),
            )
        }
        Secondary::Volatile {
            volatile_,
            chance_pct,
        } => {
            let (base, third) = volatile_verb(volatile_);
            chance_line(
                chance_pct,
                format!("It {third}."),
                format!("It may ({chance_pct}%) {base}."),
            )
        }
        Secondary::StageOnTarget {
            stat,
            delta,
            chance_pct,
        } => stage_line("the foe's", stat, delta, chance_pct),
        Secondary::StageOnSelf {
            stat,
            delta,
            chance_pct,
        } => stage_line("its own", stat, delta, chance_pct),
        Secondary::HealSelf { pct } => format!("It restores {pct}% of its own resolve."),
        Secondary::Drain { pct } => format!("It drinks back {pct}% of the wound."),
        Secondary::Recoil { pct } => format!("The wound recoils {pct}% upon its caster."),
        Secondary::MultiHit => "It strikes two to five times.".to_string(),
        Secondary::HighCrit => "It oft finds the foe's heart (a deeper crit).".to_string(),
    }
}

/// The certainty fork shared by the chance-bearing effects.
fn chance_line(chance_pct: u8, certain: String, maybe: String) -> String {
    if chance_pct >= 100 {
        certain
    } else {
        maybe
    }
}

/// Ailment deeds as verb phrases: base form (after "may (p%)") and
/// third-person (certainty). The gloss in parentheses is the ailment's
/// felt effect from crusade-core's ailment docs.
fn ailment_verb(ailment: Ailment) -> (&'static str, &'static str) {
    match ailment {
        Ailment::Brand => ("brand the foe (a burning)", "brands the foe (a burning)"),
        Ailment::Shackled => ("shackle the foe (a slowing)", "shackles the foe (a slowing)"),
        Ailment::Venom => ("envenom the foe", "envenoms the foe"),
        Ailment::Slumber => ("lull the foe to slumber", "lulls the foe to slumber"),
        Ailment::Frost => ("freeze the foe", "freezes the foe"),
    }
}

/// Volatile deeds, same base/third-person recipe as [`ailment_verb`].
fn volatile_verb(volatile_: Volatile) -> (&'static str, &'static str) {
    match volatile_ {
        Volatile::Confusion => ("sow confusion in the foe", "sows confusion in the foe"),
        Volatile::Flinch => ("make the foe flinch", "makes the foe flinch"),
        Volatile::SeedLeech => (
            "seed the foe (resolve seeps to you)",
            "seeds the foe (resolve seeps to you)",
        ),
    }
}

/// "It raises its own Guard by 1 stage." / "It may (50%) lower the foe's
/// Valour by 2 stages." -- the conjugation fork rides `chance_pct`, so the
/// certainty never wears a "may".
fn stage_line(whose: &str, stat: BattleStat, delta: i8, chance_pct: u8) -> String {
    let n = delta.unsigned_abs();
    let stages = if n == 1 { "stage" } else { "stages" };
    match (chance_pct >= 100, delta > 0) {
        (true, true) => format!("It raises {whose} {} by {n} {stages}.", stat_label(stat)),
        (true, false) => format!("It lowers {whose} {} by {n} {stages}.", stat_label(stat)),
        (false, true) => format!(
            "It may ({chance_pct}%) raise {whose} {} by {n} {stages}.",
            stat_label(stat)
        ),
        (false, false) => format!(
            "It may ({chance_pct}%) lower {whose} {} by {n} {stages}.",
            stat_label(stat)
        ),
    }
}

/// Stat names for the stage lines -- the same short table client.rs keeps
/// private (mirrored here, like the two local `slugify`s), so the shell
/// never reaches across the client boundary for display words.
fn stat_label(stat: BattleStat) -> &'static str {
    match stat {
        BattleStat::Valour => "Valour",
        BattleStat::Piety => "Piety",
        BattleStat::Guard => "Guard",
        BattleStat::Aegis => "Aegis",
        BattleStat::Alacrity => "Alacrity",
    }
}

/// One side's champion card: portrait sprite (when the art lane has the
/// species), then gold edge for ours, ember for the foe. `mirror` marks the
/// foe half of a same-species pairing ([`name_line`]). The card's node
/// recipe is `style::panel_node` tightened for the 720px battle budget.
fn champion_panel(
    cmd: &mut Commands,
    view: &BattleChampionView,
    assets: Option<&AssetServer>,
    arc: Vec<(ArcPose, u16)>,
    mirror: bool,
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
        name_line(view, mirror),
        style::head_font(),
        style::BONE,
    ));
    kids.push(resolve_row(cmd, view.current_resolve, view.max_resolve));
    if !view.types_display.is_empty() {
        let typing = text_node(
            cmd,
            view.types_display.join(" + "),
            style::small_font(),
            style::GOLD,
        );
        tooltip::with_tooltip(&mut cmd.entity(typing), TYPING_TIP);
        kids.push(typing);
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
        Node {
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(10.0)),
            border: UiRect::all(Val::Px(1.0)),
            row_gap: Val::Px(4.0),
            ..default()
        },
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

/// Cards D6/E4: how long one turn's combat-arc flourish holds before the
/// panel falls back to the idle loop. The whole flourish — every beat of
/// the exchange in event order — is sliced out of this one window, so
/// slow multi-event turns never drag it out.
///
/// E4 lengthened the D6 window (45 ticks) to 60 (~1.0 s at the windowed
/// shell's vsync): a lone rite's three-beat arc (windup -> strike ->
/// followthrough) reads 20/20/20 ticks, and a full cast-and-answer turn
/// (three attack beats plus the answer's hit — four beats on each panel)
/// still floors at 15 ticks per beat, the FLASH_TICKS cadence. Uniform
/// slicing is deliberate: beat lengths are a pure function of the beat
/// count, never of wall time, so ui-shots stays byte-deterministic.
pub(crate) const ARC_HOLD_TICKS: u16 = 60;
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

/// One beat of a combat-arc flourish (cards D6 + E4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArcPose {
    /// The panel's champion drawing back before the blow
    /// (`09_windup.png`) — the telegraph beat of the E4 attack arc.
    /// Attacker-side only.
    Windup,
    /// The panel's champion mid-attack (`07_strike.png`) — the contact
    /// beat, when the blow narratively lands.
    Strike,
    /// The panel's champion's weight settling past the target
    /// (`10_followthrough.png`) — the release beat. Attacker-side only.
    Followthrough,
    /// The panel's combatant taking a hit (`08_hit.png` for champions,
    /// tint-flash alone for frame-less bosses).
    Hit,
}

/// Plan one panel's combat arc from a snapshot's `fresh_events` — the
/// take-drain event batch of the input that just resolved, so the flourish
/// fires exactly once per exchange, at the rebuild that first shows its
/// outcome.
///
/// Card E4 arc shape: a staged rite-use expands to THREE back-to-back
/// beats — windup (telegraph) -> strike (contact) -> followthrough
/// (settle) — while a `Damage` event on THIS panel's side stays a single
/// hit beat whose flash fires on the flourish's opening frame, exactly
/// where its chronicle line lands (the defender's flash is NOT re-timed
/// to the attacker's contact frame; windup/followthrough belong to the
/// attacker alone). Event order is kept and equal-length beats slice up
/// [`ARC_HOLD_TICKS`]; consecutive same-pose runs collapse (a MultiHit
/// rite's N `Damage` events are ONE recoil, not a strobe).
///
/// `caster_strikes`: champions with staged arc frames
/// ([`champion_arc_paths`]) turn their own `RiteUsed` into the three-beat
/// arc; frame-less combatants (bosses, unknown species) never pose and
/// get one brief flash beat iff they took damage. `side` is the ENGINE
/// side (events carry engine sides; see [`battle_row`] for the me/foe
/// mapping).
///
/// Not wired: ailment chip (`AilmentChip`) never flashes — end-of-turn
/// burn ticks firing an attack-recoil pose read as a new hit, so only
/// direct `Damage` events count. All other event kinds are ignored by
/// design.
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
    // The E4 telegraph arc: one rite-use expands to three beats; damage
    // stays one beat. Dedupe rides the EXPANDED stream, so a MultiHit's
    // back-to-back damage events collapse to one recoil as in D6, while
    // consecutive casts (W/S/F, W/S/F) never touch — no two adjacent
    // beats can share a pose there.
    const ATTACK_ARC: [ArcPose; 3] =
        [ArcPose::Windup, ArcPose::Strike, ArcPose::Followthrough];
    let mut poses: Vec<ArcPose> = Vec::new();
    for e in events {
        match e {
            BattleEvent::RiteUsed { side: s, .. } if *s == side => {
                for pose in ATTACK_ARC {
                    if poses.last() != Some(&pose) {
                        poses.push(pose);
                    }
                }
            }
            BattleEvent::Damage { side: s, .. } if *s == side => {
                if poses.last() != Some(&ArcPose::Hit) {
                    poses.push(ArcPose::Hit);
                }
            }
            _ => {}
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

/// A battle portrait with staged frames (cards D5, D6 and E4), resolved
/// by [`champion_anim_paths`] / [`champion_arc_paths`] from
/// `sprites/anim/{id:03}-{slug}/`. All handles are loaded at spawn, so the
/// first swap never draws a blank panel while a fetch is in flight.
///
/// Combat arc: `battle_screen` stamps `arc` at spawn from the snapshot's
/// `fresh_events` (see [`arc_plan`]); [`advance_champion_anim`] plays the
/// plan one frame at a time (windup -> strike -> followthrough for the
/// attacker, hit + flash for the defender), then falls back to the D5
/// idle loop. The `Mutex` is the single point where per-frame arc
/// mutation crosses the locked D5 wiring — `input::process_inputs`' query
/// borrows `&ChampionAnim` immutably, and no other thread ever holds the
/// lock. Pure frame-count countdown, never wall time, so ui-shots stays
/// byte-deterministic.
///
/// Static-sprite combatants (bosses — no frames by design — and any
/// staged-table species without an anim set) spawn this marker too, with
/// `idle` = their static sprite and every arc handle `None`: the idle
/// loop degenerates to a no-swap, and every beat with its handle missing
/// degrades to the idle frame — a `Hit` beat thus degrades to the tint
/// flash alone (the boss white-flash feedback; never a broken texture).
/// A champion's anim staging is all-or-nothing per species (the resolvers
/// key off the static species table, not per-file probes), so a pose slot
/// is `None` exactly for combatants that stage NO arc frames.
#[derive(Component)]
pub(crate) struct ChampionAnim {
    idle: Handle<Image>,
    idle2: Option<Handle<Image>>,
    windup: Option<Handle<Image>>,
    strike: Option<Handle<Image>>,
    followthrough: Option<Handle<Image>>,
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

/// Advance every battle portrait exactly one app frame: cards D6/E4 arc
/// beats first (pose swap, ticking the plan down; the hit-flash tint rides
/// the `Hit` beat alone — flashes stay locked to the blow), then the card
/// D5 idle loop for anything at rest. Called from `input::process_inputs`
/// — the one per-frame system every shell binary (windowed, `--smoke`,
/// ui-shots) already registers — which is what lets this run with zero
/// changes to `main.rs`, `screens.rs`, or the harness. Mutation is
/// guarded, so plateaus cost no change-detection work.
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
                        ArcPose::Windup => anim.windup.as_ref(),
                        ArcPose::Strike => anim.strike.as_ref(),
                        ArcPose::Followthrough => anim.followthrough.as_ref(),
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
/// D6/E4 arc frames resolve through [`champion_arc_paths`].
fn champion_anim_paths(species_name: &str) -> Option<(String, Option<String>)> {
    let (id, dir) = champion_anim_dir(species_name)?;
    let idle = format!("{dir}/01_idle.png");
    let idle2 = (id != 1).then(|| format!("{dir}/04_idle2.png"));
    Some((idle, idle2))
}

/// Combat-arc frame paths (cards D6 + E4) for one species: the telegraph
/// windup (`09_windup.png`), the contact strike (`07_strike.png`), the
/// settle followthrough (`10_followthrough.png`) and the hurt frame
/// (`08_hit.png`) — all per-frame generations, never sheet slices (4.3
/// hybrid decision; Audric's pilot 03/04 cells stay pilot history).
/// Champions 1..=12 are staged; bosses (ids 101+) and unknown names get
/// `None` → no pose beats, flash alone on hit (boss canon: no 09..12
/// assets exist for them).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArcFrames {
    windup: String,
    strike: String,
    followthrough: String,
    hit: String,
}

impl ArcFrames {
    fn for_id_dir(dir: String) -> Self {
        ArcFrames {
            windup: format!("{dir}/09_windup.png"),
            strike: format!("{dir}/07_strike.png"),
            followthrough: format!("{dir}/10_followthrough.png"),
            hit: format!("{dir}/08_hit.png"),
        }
    }
}

/// One species' anim-frame set key under `sprites/anim/` — the species id
/// (Audric's pilot sheet has no idle2 cell) plus the `{id:03}-{slug}`
/// directory — or `None` for anything outside the 12 staged champions
/// (bosses 101+, unknown names).
fn champion_anim_dir(species_name: &str) -> Option<(u16, String)> {
    let sp = data::species::SPECIES
        .iter()
        .find(|sp| sp.name == species_name)?;
    if !matches!(sp.id, 1..=12) {
        return None;
    }
    Some((
        sp.id,
        format!("sprites/anim/{:03}-{}", sp.id, slugify(sp.name)),
    ))
}

/// The arc frame set for one species; see [`champion_anim_dir`] for the
/// champion-only gate.
fn champion_arc_paths(species_name: &str) -> Option<ArcFrames> {
    champion_anim_dir(species_name).map(|(_id, dir)| ArcFrames::for_id_dir(dir))
}

/// Battle-over verdict frame paths (card E4): `(11_victory.png,
/// 12_defeat.png)` for IDs 1..=12. Bosses stage no 09..12 frames — canon
/// keeps them on their static battle sprite — and unknown names resolve to
/// `None`, both through the same species-table gate as the arc frames.
fn champion_verdict_paths(species_name: &str) -> Option<(String, String)> {
    let (_id, dir) = champion_anim_dir(species_name)?;
    Some((format!("{dir}/11_victory.png"), format!("{dir}/12_defeat.png")))
}

/// The E4 verdict image a battle-over surface shows for one active
/// champion: staged champions hold their victory/defeat frame by outcome;
/// bosses and unknown-table species keep the plain battle sprite (canon:
/// bosses are static on every surface — verdict rows never reference
/// 09..12 for ids 101+); a wholly unknown name (the client's "Champion
/// #N" defense) draws no portrait at all, never a 404 guess.
fn verdict_image_path(view: &BattleChampionView, victor: bool) -> Option<String> {
    match champion_verdict_paths(&view.species_name) {
        Some((victory, defeat)) => Some(if victor { victory } else { defeat }),
        None => champion_sprite_path(&view.species_name),
    }
}

/// Card E4 verdict imagery for the battle-over surfaces, shared by both
/// battle-over legs (`over_screen` practice/campaign and
/// `net_over_screen`): the winner's active holds `11_victory`, the loser's
/// active holds `12_defeat`, me-left/foe-right like the battle row.
///
/// `won` carries the local viewer's verdict (`Some(true)` = our banner,
/// `Some(false)` = defeat); practice/campaign callers derive it as
/// `snap.battle_result.map(|w| w == 0)`, net callers get it straight from
/// `NetDuelEnd::Finished { won }`. `None` (the field merely fell quiet) —
/// like a headless `assets` (`Option<&AssetServer>` `None` under
/// MinimalPlugins) or a snapshot with no actives — draws nothing and
/// asserts no assets. Mirror matches keep the battle panel's "Foe " tag
/// so the two cards can never read identically.
pub(crate) fn verdict_row(
    cmd: &mut Commands,
    snap: &ViewSnapshot,
    won: Option<bool>,
    assets: Option<&AssetServer>,
) -> Option<Entity> {
    let won = won?;
    let assets = assets?;
    let me = snap.me.as_ref()?;
    let foe = snap.foe.as_ref()?;
    let mirror = mirror_match(me, foe);
    let kids: Vec<Entity> = [
        verdict_panel(cmd, me, won, false, assets),
        verdict_panel(cmd, foe, !won, mirror, assets),
    ]
    .into_iter()
    .flatten()
    .collect();
    if kids.is_empty() {
        return None;
    }
    Some(
        cmd.spawn(Node {
            width: Val::Percent(100.0),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Start,
            ..default()
        })
        .add_children(&kids)
        .id(),
    )
}

/// One verdict card of [`verdict_row`]: the held pose over the name line,
/// framed gold for the victor, plain for the fallen. `None` when the
/// species resolves to no image at all (unknown name).
fn verdict_panel(
    cmd: &mut Commands,
    view: &BattleChampionView,
    victor: bool,
    mirror: bool,
    assets: &AssetServer,
) -> Option<Entity> {
    let path = verdict_image_path(view, victor)?;
    let image = cmd
        .spawn((
            ImageNode::new(assets.load(path)),
            Node {
                width: Val::Px(128.0),
                height: Val::Px(128.0),
                ..default()
            },
        ))
        .id();
    let name = text_node(
        cmd,
        name_line(view, mirror),
        style::small_font(),
        style::BONE,
    );
    let border = if victor { style::GOLD } else { style::PANEL_EDGE };
    Some(
        cmd.spawn((
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(5.0)),
                border: UiRect::all(Val::Px(1.0)),
                row_gap: Val::Px(3.0),
                ..default()
            },
            BackgroundColor(style::PANEL),
            BorderColor::all(border),
        ))
        .add_children(&[image, name])
        .id(),
    )
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
/// while bosses and unknowns keep this static path unchanged. Cards D6/E4:
/// the panel's fresh-events flourish (`arc`) is stamped at spawn either
/// way — champions preload their full arc frame set (windup, strike,
/// followthrough, hit), frame-less statics degrade every beat to the idle
/// frame with the `Hit` tint flash kept (`ChampionAnim` doc).
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
    // The champion-only lore hook (boss allegories stay Manuscript-sealed
    // -- see species_lore_tip).
    let lore = species_lore_tip(species_name);
    if let Some((idle, idle2)) = champion_anim_paths(species_name) {
        let idle = assets.load(idle);
        // Champion anim staging is all-or-nothing per species: the arc
        // resolver's `Some` guarantees the whole 07..12 set was staged
        // (`stage_sprites.py` errors out on a missing pose), so the
        // handles here never guess at absent files.
        let (windup, strike, followthrough, hit) = match champion_arc_paths(species_name) {
            Some(f) => (
                Some(assets.load(f.windup)),
                Some(assets.load(f.strike)),
                Some(assets.load(f.followthrough)),
                Some(assets.load(f.hit)),
            ),
            None => (None, None, None, None),
        };
        let anim = ChampionAnim {
            idle: idle.clone(),
            idle2: idle2.map(|pose| assets.load(pose)),
            windup,
            strike,
            followthrough,
            hit,
            arc: Mutex::new(arc),
        };
        let mut sprite = cmd.spawn((ImageNode::new(idle), node, anim));
        if let Some(tip) = lore {
            tooltip::with_tooltip(&mut sprite, tip);
        }
        return Some(sprite.id());
    }
    let sprites = champion_sprite_path(species_name)?;
    let sprite = assets.load(sprites);
    let anim = ChampionAnim {
        idle: sprite.clone(),
        idle2: None,
        windup: None,
        strike: None,
        followthrough: None,
        hit: None,
        arc: Mutex::new(arc),
    };
    let mut sprite = cmd.spawn((ImageNode::new(sprite), node, anim));
    if let Some(tip) = lore {
        tooltip::with_tooltip(&mut sprite, tip);
    }
    Some(sprite.id())
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
    // Hovering the bar or its numbers explains resolve (and faint = faint).
    let mut row = cmd.spawn(Node {
        align_items: AlignItems::Center,
        column_gap: Val::Px(10.0),
        ..default()
    });
    tooltip::with_tooltip(&mut row, RESOLVE_TIP);
    row.add_children(&[bar, numbers]).id()
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

/// The chronicle's line cap on the battle screen. The client's `LOG_TAIL`
/// already delivers at most 8; this view-side clamp keeps the screen's
/// height budget independent of the caller — the tail is freshest-LAST, so
/// a longer slice elides its oldest lines and keeps the newest.
const CHRONICLE_PANEL_LINES: usize = 8;

/// The battle chronicle: title + the newest [`CHRONICLE_PANEL_LINES`]
/// lines in a height-capped, tightened dark panel. `screens::log_panel`
/// stays as-is for the centered (menu-flow) screens, whose roomier layout
/// budget is not the battle screen's.
fn chronicle_panel(cmd: &mut Commands, lines: &[String]) -> Entity {
    let mut kids = vec![text_node(
        cmd,
        "Chronicle".into(),
        style::small_font(),
        style::GOLD_DIM,
    )];
    let elided = lines.len().saturating_sub(CHRONICLE_PANEL_LINES);
    for line in &lines[elided..] {
        kids.push(text_node(cmd, line.clone(), style::small_font(), style::BONE));
    }
    cmd.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
            border: UiRect::all(Val::Px(1.0)),
            row_gap: Val::Px(3.0),
            ..default()
        },
        BackgroundColor(style::PANEL),
        BorderColor::all(style::PANEL_EDGE),
    ))
    .add_children(&kids)
    .id()
}

/// Up to four rite buttons in a FIXED two-column grid: compact two-line
/// cells (name over the stat strip), so a full kit always costs exactly
/// two short rows — the old wrap-anywhere row let wide labels stack a
/// third row and pushed the command row clean off a 720px viewport.
/// Exhausted rites render as cold, dead slabs, as before.
fn rite_grid(cmd: &mut Commands, rites: &[RiteCard]) -> Entity {
    let buttons: Vec<Entity> = rites
        .iter()
        .map(|rite| {
            let (name, meta) = rite_lines(rite);
            rite_cell(cmd, name, meta, rite)
        })
        .collect();
    cmd.spawn(Node {
        display: Display::Grid,
        grid_template_columns: vec![RepeatedGridTrack::flex(2, 1.0)],
        column_gap: Val::Px(8.0),
        row_gap: Val::Px(6.0),
        ..default()
    })
    .add_children(&buttons)
    .id()
}

/// One rite cell of [`rite_grid`]: live compact button when usable, inert
/// slab (no `Button`, no queued input) when the zeal is spent — the exact
/// live/dead split `action_button`/`disabled_button` had, shrunk. Both
/// halves carry the composed rite tip; on the dead slab it doubles as the
/// "zeal 0/N left" explanation.
fn rite_cell(cmd: &mut Commands, name: String, meta: String, rite: &RiteCard) -> Entity {
    let ink = if rite.usable { style::BONE } else { style::DIM };
    let name_node = text_node(cmd, name, style::body_font(), ink);
    let meta_node = text_node(cmd, meta, style::small_font(), ink);
    let node = Node {
        flex_direction: FlexDirection::Column,
        padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
        border: UiRect::all(Val::Px(1.0)),
        row_gap: Val::Px(2.0),
        ..default()
    };
    let mut cell = cmd.spawn(node);
    if rite.usable {
        cell.insert((
            Button,
            BackgroundColor(style::BUTTON),
            BorderColor::all(style::GOLD_DIM),
            BaseBackground(style::BUTTON),
            ActionButton(ClientInput::PickRite { slot: rite.slot }),
        ));
    } else {
        cell.insert((
            BackgroundColor(style::BUTTON_DISABLED),
            BorderColor::all(style::PANEL_EDGE),
        ));
    }
    tooltip::with_tooltip(&mut cell, rite_tip(rite));
    cell.add_children(&[name_node, meta_node]).id()
}

/// The rite cell's two lines: the name, then the stat strip (Pow shown
/// only for rites that have it — same composition the old label had).
fn rite_lines(rite: &RiteCard) -> (String, String) {
    let meta = if rite.power > 0 {
        format!(
            "{} {} | Pow {} | Acc {} | Zeal {}/{}",
            rite.type_display,
            rite.category_display,
            rite.power,
            rite.accuracy,
            rite.zeal_left,
            rite.zeal_max
        )
    } else {
        format!(
            "{} {} | Acc {} | Zeal {}/{}",
            rite.type_display, rite.category_display, rite.accuracy, rite.zeal_left, rite.zeal_max
        )
    };
    (rite.name.clone(), meta)
}

/// Voluntary switches plus the walk-away concede: ONE row of compact chips
/// (small text, tight padding) so the row cannot force a second line of
/// full-size buttons at the shipped 1280x720 budget.
fn command_row(cmd: &mut Commands, snap: &ViewSnapshot) -> Entity {
    let mut kids: Vec<Entity> = snap
        .can_switch_to
        .iter()
        .map(|target| {
            chip_button(
                cmd,
                format!("Send {}", switch_target_name(target)),
                ClientInput::PickSwitch {
                    party_idx: switch_target_idx(target),
                },
                SWITCH_TIP,
            )
        })
        .collect();
    kids.push(chip_button(
        cmd,
        "Concede".into(),
        ClientInput::Concede,
        concede_tip(snap.campaign.is_some(), snap.roster_count),
    ));
    cmd.spawn(Node {
        column_gap: Val::Px(8.0),
        row_gap: Val::Px(4.0),
        flex_wrap: FlexWrap::Wrap,
        ..default()
    })
    .add_children(&kids)
    .id()
}

/// A compact clickable chip: the [`ActionButton`]/hover-tint wiring of
/// `screens::action_button` with small text and tight padding, plus its one
/// hover tip.
fn chip_button(cmd: &mut Commands, label: String, input: ClientInput, tip: &'static str) -> Entity {
    let text = text_node(cmd, label, style::small_font(), style::BONE);
    let mut chip = cmd.spawn((
        Button,
        Node {
            padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
            border: UiRect::all(Val::Px(1.0)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(style::BUTTON),
        BorderColor::all(style::GOLD_DIM),
        BaseBackground(style::BUTTON),
        ActionButton(input),
    ));
    tooltip::with_tooltip(&mut chip, tip);
    chip.add_children(&[text]).id()
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

    /// The cards-D6/E4 arc resolver: every champion's staged per-frame arc
    /// set (windup / strike / followthrough / hit) resolves to its exact
    /// asset paths; bosses and unknown names get `None` → pose-less,
    /// flash-only hurt feedback.
    #[test]
    fn arc_paths_resolve_for_all_twelve_champions_only() {
        assert_eq!(
            champion_arc_paths("Squire Audric"),
            Some(ArcFrames {
                windup: "sprites/anim/001-squire-audric/09_windup.png".to_string(),
                strike: "sprites/anim/001-squire-audric/07_strike.png".to_string(),
                followthrough: "sprites/anim/001-squire-audric/10_followthrough.png".to_string(),
                hit: "sprites/anim/001-squire-audric/08_hit.png".to_string(),
            })
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

    /// The E4 battle-over resolvers: every champion holds its victory /
    /// defeat frames at the verdict; bosses (ids 101+) stage no 09..12
    /// frames and keep their static battle sprite on the verdict surfaces
    /// (canon: static everywhere); an unknown name draws nothing — never
    /// a 404 guess.
    #[test]
    fn verdict_poses_follow_the_champion_only_gate() {
        assert_eq!(
            champion_verdict_paths("Squire Audric"),
            Some((
                "sprites/anim/001-squire-audric/11_victory.png".to_string(),
                "sprites/anim/001-squire-audric/12_defeat.png".to_string(),
            ))
        );
        let view = |species: &str| BattleChampionView {
            ours: true,
            species_name: species.to_string(),
            level: 25,
            current_resolve: 80,
            max_resolve: 80,
            ailment: None,
            types_display: Vec::new(),
            stages: Vec::new(),
            alive_reserves: 2,
        };
        for sp in data::species::SPECIES.iter().filter(|sp| matches!(sp.id, 1..=12)) {
            let (victory, defeat) =
                champion_verdict_paths(sp.name).expect("champion verdict frames");
            assert!(champion_arc_paths(sp.name).is_some());
            assert_eq!(verdict_image_path(&view(sp.name), true), Some(victory));
            assert_eq!(verdict_image_path(&view(sp.name), false), Some(defeat));
        }
        for sp in data::species::SPECIES.iter().filter(|sp| sp.id >= 100) {
            assert_eq!(
                champion_arc_paths(sp.name),
                None,
                "boss {} stays flash-only",
                sp.name
            );
            assert_eq!(
                champion_verdict_paths(sp.name),
                None,
                "boss {} stages no victory/defeat frames",
                sp.name
            );
            for victor in [true, false] {
                let path = verdict_image_path(&view(sp.name), victor)
                    .expect("bosses keep the static battle sprite");
                // The boss-verdict guard: no 09..12 reference can ever
                // point at a boss panel.
                assert!(path.starts_with("sprites/champions/"), "{path}");
                assert!(
                    !path.contains("anim/")
                        && !path.contains("_victory")
                        && !path.contains("_defeat"),
                    "{path}"
                );
            }
        }
        assert_eq!(champion_verdict_paths("Champion #55"), None);
        assert_eq!(verdict_image_path(&view("Champion #55"), true), None);
        assert_eq!(verdict_image_path(&view("Vigo the Carpathian"), false), None);
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
    /// ARC_HOLD_TICKS: our panel plays windup->strike->followthrough then
    /// takes the counter's hit; theirs is hit, then their own three-beat
    /// answer. Four beats per panel: an even 15-tick slice each.
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
            vec![
                ArcPose::Windup,
                ArcPose::Strike,
                ArcPose::Followthrough,
                ArcPose::Hit
            ]
        );
        assert_eq!(
            theirs.iter().map(|b| b.0).collect::<Vec<_>>(),
            vec![
                ArcPose::Hit,
                ArcPose::Windup,
                ArcPose::Strike,
                ArcPose::Followthrough
            ]
        );
        assert_eq!(mine.iter().map(|b| b.1).sum::<u16>(), ARC_HOLD_TICKS);
        assert_eq!(theirs.iter().map(|b| b.1).sum::<u16>(), ARC_HOLD_TICKS);
        assert_eq!(
            mine.iter().map(|b| b.1).collect::<Vec<_>>(),
            vec![15, 15, 15, 15]
        );
    }

    /// The E4 three-beat arc pinned exactly: one damaging rite reads
    /// telegraph -> contact -> settle on the ATTACKER's panel, 20 ticks a
    /// beat out of the intact 60-tick window, while the defender's plan is
    /// the D6-unchanged single hit beat filling its window — the flash
    /// fires on the flourish's very first frame (the moment the chronicle
    /// line lands), never re-timed onto the attacker's contact frame.
    /// Windup and followthrough belong to the attacker alone.
    #[test]
    fn arc_plan_expands_a_damaging_rite_into_three_beats() {
        let events = vec![rite(0), dmg(1)];
        assert_eq!(
            arc_plan(&events, 0, true),
            vec![
                (ArcPose::Windup, 20),
                (ArcPose::Strike, 20),
                (ArcPose::Followthrough, 20),
            ]
        );
        assert_eq!(
            arc_plan(&events, 1, true),
            vec![(ArcPose::Hit, ARC_HOLD_TICKS)]
        );
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
        // (documented in arc_plan), so the attack arc stands alone.
        assert_eq!(
            arc_plan(&events, 0, true),
            vec![
                (ArcPose::Windup, 20),
                (ArcPose::Strike, 20),
                (ArcPose::Followthrough, 20)
            ]
        );
        // Chip alone plans nothing.
        assert!(arc_plan(&[BattleEvent::AilmentChip { side: 0, amount: 5 }], 0, true).is_empty());
        // Uneven slicing keeps the exact hold: remainder rides beat one.
        let events = vec![rite(0), dmg(1), rite(1), dmg(0), rite(0), dmg(1)];
        let seven = arc_plan(&events, 0, true);
        assert_eq!(
            seven.iter().map(|b| b.0).collect::<Vec<_>>(),
            vec![
                ArcPose::Windup,
                ArcPose::Strike,
                ArcPose::Followthrough,
                ArcPose::Hit,
                ArcPose::Windup,
                ArcPose::Strike,
                ArcPose::Followthrough
            ]
        );
        assert_eq!(seven.iter().map(|b| b.1).sum::<u16>(), ARC_HOLD_TICKS);
        // Two full cast-and-answer rounds: our E4 arcs expand (3+3) plus
        // the two recoils we take — eight beats, no adjacent dupes to
        // collapse — 60 / 8 = 7-tick beats + remainder 4 on beat one.
        let events = vec![rite(0), dmg(1), rite(1), dmg(0), rite(0), dmg(0)];
        let eight = arc_plan(&events, 0, true);
        assert_eq!(
            eight.iter().map(|b| b.1).collect::<Vec<_>>(),
            vec![11, 7, 7, 7, 7, 7, 7, 7]
        );
        assert_eq!(eight.iter().map(|b| b.1).sum::<u16>(), ARC_HOLD_TICKS);
    }

    /// Net-duel replays: the bridge driver's documented `roster_count == 0`
    /// tell (see [`battle_row`]) suppresses EVERY arc beat — the E4
    /// windup/followthrough beats ride the same gate as the D6 strike, so a
    /// remote echo never cross-wires a swing onto the wrong panel. With a
    /// mustered company (practice/campaign) the same events fully expand.
    #[test]
    fn net_replay_snapshots_suppress_all_arc_beats() {
        let events = vec![rite(0), dmg(1), rite(1), dmg(0)];
        let view = |ours: bool| BattleChampionView {
            ours,
            species_name: "Serjeant Bastien".to_string(),
            level: 25,
            current_resolve: 80,
            max_resolve: 80,
            ailment: None,
            types_display: vec!["Patience".to_string()],
            stages: Vec::new(),
            alive_reserves: 2,
        };
        let snap = |roster_count: usize| ViewSnapshot {
            phase: crusade_app::Phase::InBattle,
            roster_count,
            me: Some(view(true)),
            foe: Some(view(false)),
            turn: 2,
            my_rites: Vec::new(),
            can_switch_to: Vec::new(),
            log_tail: Vec::new(),
            battle_result: None,
            campaign: None,
            lobby: None,
            witnesses: Vec::new(),
            sins: Vec::new(),
            fresh_events: events.clone(),
        };
        // The net tell: both panels idle out — no strike, no windup, nothing.
        let duel = snap(0);
        assert!(arc_plan_for_panel(&events, 0, duel.me.as_ref(), &duel).is_empty());
        assert!(arc_plan_for_panel(&events, 1, duel.foe.as_ref(), &duel).is_empty());
        // Same events with a roster present: the full E4 expansion.
        let practice = snap(3);
        assert_eq!(
            arc_plan_for_panel(&events, 0, practice.me.as_ref(), &practice)
                .iter()
                .map(|b| b.0)
                .collect::<Vec<_>>(),
            vec![
                ArcPose::Windup,
                ArcPose::Strike,
                ArcPose::Followthrough,
                ArcPose::Hit
            ]
        );
        // ... but only for a panel whose species staged the frames: a boss
        // panel under a rostered snapshot stays pose-less flash-only.
        let boss_panel = BattleChampionView {
            ours: false,
            species_name: "Brute of Wrath".to_string(),
            ..view(false)
        };
        assert_eq!(
            arc_plan_for_panel(&events, 1, Some(&boss_panel), &practice),
            vec![(ArcPose::Hit, FLASH_TICKS)]
        );
        // No displayed species -> nothing to attribute, no plan at all.
        assert!(arc_plan_for_panel(&events, 0, None, &practice).is_empty());
    }

    /// Boss path (`caster_strikes == false`): no poses ever — windup,
    /// strike and followthrough are attacker beats on staged arcs only —
    /// and any direct damage collapses to exactly one FLASH_TICKS hit beat.
    #[test]
    fn arc_plan_bosses_flash_only() {
        let events = vec![rite(1), dmg(1), dmg(1)];
        let plan = arc_plan(&events, 1, false);
        assert_eq!(plan, vec![(ArcPose::Hit, FLASH_TICKS)]);
        assert!(plan.iter().all(|(pose, _)| {
            !matches!(
                pose,
                ArcPose::Windup | ArcPose::Strike | ArcPose::Followthrough
            )
        }));
        assert!(arc_plan(&[rite(1)], 1, false).is_empty());
        assert!(arc_plan(&[dmg(0)], 1, false).is_empty());
    }

    /// Mirror-match naming: when both sides' actives are the same species
    /// (practice/campaign CAN field e.g. Serjeant Bastien vs Serjeant
    /// Bastien), the Foe panel's name line gains a "Foe " prefix while ours
    /// stays plain; a distinct matchup leaves both lines untouched.
    #[test]
    fn mirror_match_prefixes_the_foe_name_line_only() {
        let view = |ours: bool, species: &str| BattleChampionView {
            ours,
            species_name: species.to_string(),
            level: 25,
            current_resolve: 80,
            max_resolve: 80,
            ailment: None,
            types_display: vec!["Patience".to_string()],
            stages: Vec::new(),
            alive_reserves: 2,
        };
        let mine = view(true, "Serjeant Bastien");
        let mirror_foe = view(false, "Serjeant Bastien");
        let other_foe = view(false, "Squire Audric");
        // Mirror: the foe line is tagged, ours is not.
        assert!(mirror_match(&mine, &mirror_foe));
        assert_eq!(
            name_line(&mirror_foe, true),
            "Foe Serjeant Bastien - Lv 25"
        );
        assert_eq!(name_line(&mine, false), "Serjeant Bastien - Lv 25");
        // Distinct species: no tag anywhere.
        assert!(!mirror_match(&mine, &other_foe));
        assert_eq!(name_line(&other_foe, false), "Squire Audric - Lv 25");
    }

    #[test]
    fn slugify_matches_staged_slugs() {
        assert_eq!(slugify("Squire Audric"), "squire-audric");
        assert_eq!(slugify("Vigil-Knight Wymar"), "vigil-knight-wymar");
        assert_eq!(slugify("Glutton Maw"), "glutton-maw");
    }

    // ---------- Hover-tip composition ----------

    /// Build the RiteCard the client WOULD send for a RiteSlot-ish input
    /// (same expansion recipe as `client.rs::rite_cards`; the client's
    /// private display tables are reproduced via the enums' Debug names,
    /// which read identically: "Humility", "Sin of Wrath", "Martial").
    fn card_for(rite: &crusade_core::types::Rite, zeal_left: u8) -> RiteCard {
        let type_display = match rite.rite_type {
            crusade_core::types::RiteType::Typeless => "Typeless".to_string(),
            crusade_core::types::RiteType::Virtue(v) => format!("{v:?}"),
            crusade_core::types::RiteType::Sin(s) => format!("Sin of {s:?}"),
        };
        RiteCard {
            slot: 0,
            rite_id: rite.id,
            name: rite.name.to_string(),
            type_display,
            category_display: format!("{:?}", rite.category),
            power: rite.power,
            accuracy: rite.accuracy,
            zeal_left,
            zeal_max: rite.zeal,
            usable: zeal_left > 0,
        }
    }

    /// Every hover tip below this budget stays inside its two lines at the
    /// 480px wrap (~66 chars per line in the 14px font after padding).
    fn assert_terse_tip(tip: &str, what: &str) {
        assert!(tip.is_ascii(), "{what} must stay pure ASCII: {tip:?}");
        assert!(
            tip.len() <= TIP_LEN_BUDGET,
            "{what} over the two-line budget ({TIP_LEN_BUDGET}): {tip:?} ({} chars)",
            tip.len()
        );
    }

    /// The rite tip from a RiteSlot-ish input composes typing, category,
    /// Pow/Acc/Zeal meanings, and the carried ailment/status clause --
    /// pinned exactly for a partial-chance ailment rite, a never-miss
    /// self-stage rite, and a certain ailment rite.
    #[test]
    fn rite_tip_composes_typing_stats_and_effects() {
        let from_slot = |rite_id: u16, zeal_left: u8| {
            let slot = crusade_core::types::RiteSlot {
                rite_id,
                zeal_left,
            };
            card_for(
                data::rite_by_id(slot.rite_id).expect("static rite"),
                slot.zeal_left,
            )
        };
        // Humble Rebuke: Humility Spiritual, Pow 55, Acc 95, brand 10%.
        let tip = rite_tip(&from_slot(3, 13));
        assert_eq!(
            tip.as_ref(),
            "Humility Spiritual -- Pow 55 weighs the wound, Acc 95% strikes home, \
             zeal 13/15 left. It may (10%) brand the foe (a burning)."
        );
        assert_terse_tip(&tip, "Humble Rebuke tip");
        // Steady Ward: status rite, Acc 101 (never misses), certain stage.
        let tip = rite_tip(&from_slot(2, 20));
        assert_eq!(
            tip.as_ref(),
            "Typeless Status -- it never misses, zeal 20/20 left. \
             It raises its own Guard by 1 stage."
        );
        assert_terse_tip(&tip, "Steady Ward tip");
        // Litany of Stillness: status rite, certain slumber.
        let tip = rite_tip(&from_slot(5, 15));
        assert_eq!(
            tip.as_ref(),
            "Typeless Status -- Acc 90% strikes home, zeal 15/15 left. \
             It lulls the foe to slumber."
        );
        assert_terse_tip(&tip, "Litany of Stillness tip");
        // A zeal-spent (dead slab) cell still explains itself.
        let tip = rite_tip(&from_slot(1, 0));
        assert!(tip.contains("zeal 0/35 left."), "{tip:?}");
        assert_terse_tip(&tip, "spent Measured Blow tip");
    }

    /// The SAME contract holds for every rite in the static table, not just
    /// the fixtures: every compose lands in ASCII inside the budget. All
    /// offenders are listed at once so wording trims take one round.
    #[test]
    fn every_static_rite_tip_is_ascii_and_terse() {
        assert!(!data::rites::RITES.is_empty());
        let mut worst = 0;
        let mut bad: Vec<String> = Vec::new();
        for rite in data::rites::RITES {
            let tip = rite_tip(&card_for(rite, rite.zeal));
            assert!(tip.is_ascii(), "{} must stay pure ASCII: {tip:?}", rite.name);
            worst = worst.max(tip.len());
            if tip.len() > TIP_LEN_BUDGET {
                bad.push(format!("{} ({} chars)", rite.name, tip.len()));
            }
        }
        assert!(
            bad.is_empty(),
            "{}/{} rites over the two-line budget ({TIP_LEN_BUDGET}, worst {worst}): {}",
            bad.len(),
            data::rites::RITES.len(),
            bad.join("; ")
        );
    }

    /// The concede tip reads the row's own snapshot context: campaign
    /// first, then the net duel's documented `roster_count == 0` tell, then
    /// practice. Each arm keeps its own stake honest.
    #[test]
    fn concede_tip_reads_the_snapshot_context() {
        assert_eq!(concede_tip(true, 4), CONCEDE_TIP_CAMPAIGN);
        // Even should the duel tell ever coincide with a campaign, the
        // campaign wording wins (the ordering IS the pin).
        assert_eq!(concede_tip(true, 0), CONCEDE_TIP_CAMPAIGN);
        assert_eq!(concede_tip(false, 0), CONCEDE_TIP_DUEL);
        assert_eq!(concede_tip(false, 3), CONCEDE_TIP_PRACTICE);
        for tip in [CONCEDE_TIP_PRACTICE, CONCEDE_TIP_DUEL, CONCEDE_TIP_CAMPAIGN] {
            assert_terse_tip(tip, "concede tip");
        }
        assert_terse_tip(SWITCH_TIP, "switch tip");
        assert_terse_tip(RESOLVE_TIP, "resolve tip");
        assert_terse_tip(TYPING_TIP, "typing tip");
    }

    /// The sprite-lore gate: champions carry their table lore onto the
    /// portrait; bosses answer None (their lore is the Manuscript of Sins'
    /// gated allegory -- `SINS_DEFEATED_KEY` -- and the battle snapshot
    /// carries no mask), unknown names answer None.
    #[test]
    fn sprite_lore_is_champions_only() {
        let audric = data::species_by_id(1).expect("Audric");
        assert_eq!(species_lore_tip("Squire Audric"), Some(audric.lore));
        let wrath = data::species_by_id(105).expect("Wrath");
        assert!(!wrath.lore.is_empty(), "the table line exists but stays sealed");
        assert_eq!(species_lore_tip(wrath.name), None);
        assert_eq!(species_lore_tip("Champion #55"), None);
    }
}

//! The rebuild dispatch system, shared node builders, and the non-battle
//! screens (menu, roster, campaign rest, battle over).

use std::borrow::Cow;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bevy::prelude::*;

use crusade_app::{
    ladder_line, ChampionCard, ClientInput, ConversionView, FoeCard, LobbyStage, Phase, SinCard,
    TestimonyCard, ViewSnapshot, WitnessCard,
};
// The house purse's LNURL-pay switchboard reuses the B3a zaps module
// (fetch_pay_params + the 64 KiB body cap) exactly as shipped -- the
// support panel's own additions stay in this file's menu section.
use crusade_net::zaps::{Http, ZapError};
#[cfg(target_arch = "wasm32")]
use crusade_net::zaps;
// The gauntlet's boss order, single-sourced at the client (hoisted out of
// this file — see the comment at `next_boss_species`).
use crusade_app::client::CAMPAIGN_NODE_SPECIES;
use crusade_core::data;
use crusade_core::types::{Sin, SpeciesBase, Typing, Virtue};

use crate::battle;
use crate::state::{
    ActionButton, AlmsInvoice, AlmsPurse, AlmsState, AttestationPost, BaseBackground, GameShell,
    NetBridge, NetCmd, NetDuelEnd, RenderedVersion, SnapshotVersion, UiRoot,
};
use crate::style;
use crate::tooltip;

/// "Support the Order" alms box (ZAP-MONETIZATION.md surface #1): a static,
/// voluntary-donation lightning address. Purely cosmetic — the menu's
/// support panel shows it as copyable text, a QR code, and a wallet hop;
/// it is never an input to the game and no game system reads it back
/// (integrity firewall, §2).
pub(crate) const SUPPORT_LIGHTNING_ADDRESS: &str = "average@minitbits.cash";

/// Repaint the world when (and only when) a handled input bumped the
/// snapshot version: the UI is a pure function of `ClientGame` state, so
/// whole-tree despawn-and-rebuild is both correct and fast enough at 60 fps.
pub fn rebuild_ui(
    mut commands: Commands,
    shell: Res<GameShell>,
    bridge: Res<NetBridge>,
    version: Res<SnapshotVersion>,
    mut rendered: ResMut<RenderedVersion>,
    // Shell-local menu support panel (alms box; no client state involved).
    support: Res<SupportPanel>,
    // Shell-local roster gallery leaf (pager state; Optional -- see the
    // RosterPage doc: the first real pager flip self-installs the default,
    // so host binaries need no registration).
    roster_page: Option<Res<RosterPage>>,
    roots: Query<Entity, With<UiRoot>>,
    // `None` under MinimalPlugins (the headless smoke run): every sprite
    // slot then collapses to the plain text rows it supplements.
    assets: Option<Res<AssetServer>>,
) {
    if rendered.0 == Some(version.0) {
        return;
    }
    rendered.0 = Some(version.0);
    for root in &roots {
        commands.entity(root).despawn(); // children go with it
    }
    let assets = assets.as_deref();
    let (snap, roster, ladder) = {
        let game = shell.game.lock().expect("game mutex poisoned");
        (game.snapshot(), game.roster_cards(), game.ladder_rating())
    };
    // Net-duel mailbox state takes precedence over the client phase (the
    // client sits in BattleSetup for the duel's duration). The driver's
    // snapshots render through the EXISTING battle screens verbatim.
    let net = bridge.shared.as_ref().map(|shared| {
        let s = shared.lock().expect("net mailbox mutex poisoned");
        (s.duel.clone(), s.out.clone(), s.alms.clone())
    });
    let (kids, centered) = match net {
        Some((_, Some(NetDuelEnd::Aborted { notice }), _)) => {
            (duel_aborted_screen(&mut commands, &notice), true)
        }
        Some((Some(duel), out, alms)) => match duel.phase {
            Phase::BattleOver => {
                let won = match out {
                    Some(NetDuelEnd::Finished { won }) => Some(won),
                    _ => None,
                };
                (
                    net_over_screen(&mut commands, &duel, won, alms.as_ref(), assets),
                    true,
                )
            }
            Phase::ForcedSwitchSelect => {
                (battle::forced_switch_screen(&mut commands, &duel, assets), false)
            }
            _ => (battle::battle_screen(&mut commands, &duel, assets), false),
        },
        _ => match snap.phase {
            Phase::Menu => (menu_screen(&mut commands, ladder, assets, &support), true),
            Phase::Roster => (
                roster_screen(
                    &mut commands,
                    &roster,
                    assets,
                    roster_page.as_deref().map_or(0, |p| p.page),
                ),
                false,
            ),
            Phase::BattleSetup => (
                vec![text_node(
                    &mut commands,
                    "Mustering the host...".into(),
                    style::head_font(),
                    style::BONE,
                )],
                true,
            ),
            Phase::InBattle => (battle::battle_screen(&mut commands, &snap, assets), false),
            Phase::ForcedSwitchSelect => {
                (battle::forced_switch_screen(&mut commands, &snap, assets), false)
            }
            Phase::CampaignRest => (rest_screen(&mut commands, &snap, assets), true),
            Phase::BattleOver => (over_screen(&mut commands, &snap, assets), true),
            Phase::ConversionChoice => (conversion_screen(&mut commands, &snap), true),
            Phase::Lobby => (lobby_screen(&mut commands, &snap, ladder), false),
            Phase::Hagiography => (hagiography_screen(&mut commands, &snap, assets), false),
        },
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

fn menu_screen(
    cmd: &mut Commands,
    ladder: Option<u64>,
    assets: Option<&AssetServer>,
    support: &SupportPanel,
) -> Vec<Entity> {
    let title = text_node(cmd, "DEUS VULT".into(), style::title_font(), style::GOLD);
    let subtitle = text_node(
        cmd,
        "- Champions of the Cross -".into(),
        style::head_font(),
        style::GOLD_DIM,
    );
    // The hero art (card D3) kneels beside the title when an asset server
    // exists -- a dignified side piece that adds no height to this already
    // full column, leaving the buttons dominant. Without one (headless
    // smoke) the title stands alone exactly as before.
    let mut kids: Vec<Entity> = match menu_hero(cmd, assets) {
        Some(hero) => {
            let words = cmd
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(14.0),
                    ..default()
                })
                .add_children(&[title, subtitle])
                .id();
            vec![
                cmd.spawn(Node {
                    column_gap: Val::Px(18.0),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .add_children(&[hero, words])
                .id(),
            ]
        }
        None => vec![title, subtitle],
    };
    kids.extend([
        action_button(cmd, "Roster".into(), ClientInput::OpenRoster),
        action_button(
            cmd,
            "Practice Duel (level 25)".into(),
            ClientInput::StartPractice { level: 25 },
        ),
        action_button(cmd, "Campaign".into(), ClientInput::StartCampaign),
        // The alms box (card A3, ZAP-MONETIZATION.md surface #1) sits
        // directly under Campaign: the button toggles its shell-local
        // panel (address, QR code, wallet hop, amount chips) in place,
        // above the reverent helper line.
        support_button(cmd),
    ]);
    if support.open {
        kids.push(support_panel(cmd, support));
    }
    kids.extend([
        text_node(
            cmd,
            "Alms freely given keep the torches lit.".into(),
            style::small_font(),
            style::DIM,
        ),
        // The memorial roll of the revered dead (Hagiography seam).
        hagiography_menu_button(cmd),
        action_button(cmd, "Tournament of Virtue".into(), ClientInput::EnterLobby),
        text_node(cmd, ladder_line(ladder), style::small_font(), style::DIM),
        text_node(
            cmd,
            "Contrition and cold steel.".into(),
            style::small_font(),
            style::DIM,
        ),
    ]);
    kids
}

/// The menu hero: `sprites/scenes/audric-prayer.png` (the squire at dawn
/// prayer from the locked nano-banana-2-1 lane, staged keyed-transparent by
/// `tools/artbake/stage_sprites.py`). 128x128 like the battle portraits.
/// Spawned only when an asset server exists; a 404ing fetch draws nothing
/// and every text/button row is unconditional either way.
fn menu_hero(cmd: &mut Commands, assets: Option<&AssetServer>) -> Option<Entity> {
    let assets = assets?;
    Some(
        cmd.spawn((
            ImageNode::new(assets.load("sprites/scenes/audric-prayer.png")),
            Node {
                width: Val::Px(128.0),
                height: Val::Px(128.0),
                ..default()
            },
        ))
        .id(),
    )
}

// ---------- Support the Order: the menu's alms panel ----------
//
// (A3 expansion; ZAP-MONETIZATION.md surface #1, firewall §2) Everything
// below is shell-local and purely cosmetic: a click toggles a bevy UI
// panel via an entity observer plus a plain resource — NO ClientInput,
// NO Phase, and nothing here can reach a battle, the RNG, matchmaking,
// the ladder, or an attestation. Soft money may change what a player
// SEES, never what a player IS. Alms buy nothing.

/// The chooser's presets: modest amounts, in sats, carried as msat — the
/// same 21/100/1,000 chips as the post-duel alms box (B3b).
const SUPPORT_ALMS_PRESETS: [(u64, &str); 3] = [
    (21_000, "21 sats"),
    (100_000, "100 sats"),
    (1_000_000, "1,000 sats"),
];

/// Shell-local state of the menu's support panel. The whole-tree rebuild
/// reads it on every snapshot-version bump, so it lives in a resource
/// rather than on any despawned node. Handled by the panel's observers
/// only — the client never sees it.
#[derive(Resource, Default)]
pub(crate) struct SupportPanel {
    /// Whether the panel row under the menu's alms button is expanded.
    pub open: bool,
    /// Desktop shell only: a wallet-button click already surfaced its
    /// "scan or copy" note (there is no browser to hand a URI to; the
    /// browser build opens the tab directly and never needs the note).
    #[cfg(not(target_arch = "wasm32"))]
    pub wallet_note: bool,
    /// The amount chooser's latest house-purse invoice.
    pub invoice: SupportInvoice,
    /// Mailbox of the at-most-one in-flight invoice fetch (the B3b
    /// NetShared pattern at menu size): the spawned task writes, the
    /// `support_bridge` system drains. Always `None` in practice on the
    /// desktop shell, which wires no live chips.
    pub shared: Option<Arc<Mutex<SupportShared>>>,
}

/// The chooser's latest house-purse invoice. No failure payload by design
/// (mirroring B3b's `AlmsInvoice`): a failed fetch dims into
/// `Unavailable`, leaving a clean idle/retry cycle — alms never block the
/// game and never surface as errors.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum SupportInvoice {
    /// No amount chip chosen yet.
    #[default]
    Idle,
    /// A bolt11 for `amount_msat` is being requested.
    Fetching { amount_msat: u64 },
    /// The purse answered with this bolt11 for `amount_msat`.
    Ready { amount_msat: u64, bolt11: String },
    /// The fetch failed: chips dim to the fallback note; retry is free.
    Unavailable,
}

/// Written by the one in-flight support invoice fetch; drained once per
/// frame by [`support_bridge`]. Results carry no amount — the resource's
/// `Fetching` arm still identifies it.
#[derive(Default)]
pub(crate) struct SupportShared {
    /// The landed fetch: a bolt11 string, or a plain failure flag.
    pub done: Option<Result<String, ()>>,
}

/// The drain half of the invoice mailbox: a landed fetch repaints the
/// panel via the version bump. No-op whenever no fetch is in flight —
/// which is always, on the desktop shell (its chips are inert slabs).
pub(crate) fn support_bridge(
    mut panel: ResMut<SupportPanel>,
    mut version: ResMut<SnapshotVersion>,
) {
    let Some(shared) = panel.shared.as_ref() else {
        return;
    };
    let done = shared
        .lock()
        .expect("support mailbox mutex poisoned")
        .done
        .take();
    let Some(done) = done else { return };
    panel.invoice = match (&panel.invoice, done) {
        (SupportInvoice::Fetching { amount_msat }, Ok(bolt11)) => SupportInvoice::Ready {
            amount_msat: *amount_msat,
            bolt11,
        },
        _ => SupportInvoice::Unavailable,
    };
    version.0 = version.0.wrapping_add(1);
}

/// An amount chip: `SupportChip(msat)` asks the house purse for an
/// invoice of that many millisatoshis.
#[derive(Component, Clone, Copy)]
struct SupportChip(u64);

/// Which `lightning:` target a support link button hands to the wallet.
#[derive(Component, Clone, Copy)]
enum SupportLink {
    /// The plain address const.
    Address,
    /// The bolt11 last fetched from the house purse.
    Invoice,
}

/// The alms box at the chapel door. Looks like an [`action_button`], but its
/// click is handled by an entity observer instead of the `ClientInput` queue,
/// so the donation path can never touch battles, RNG, ladders, or saves.
fn support_button(cmd: &mut Commands) -> Entity {
    let text = text_node(
        cmd,
        "Support the Order".into(),
        style::body_font(),
        style::BONE,
    );
    cmd.spawn((
        Button,
        style::button_node(),
        BackgroundColor(style::BUTTON),
        BorderColor::all(style::GOLD_DIM),
    ))
    .add_children(&[text])
    .observe(on_support_click)
    .id()
}

/// Almsgiving, not a purchase: nothing unlocks, nothing is owed in return.
/// The button now toggles the shell-local support panel open/shut — the
/// wallet hop itself lives one row deeper, next to the address and code.
fn on_support_click(
    _click: On<Pointer<Click>>,
    mut panel: ResMut<SupportPanel>,
    mut version: ResMut<SnapshotVersion>,
) {
    panel.open = !panel.open;
    version.0 = version.0.wrapping_add(1);
}

/// The expanded alms panel: the address in plain text first — the one row
/// every platform can use by hand — then the scannable code, the wallet
/// hop, and the amount chips. Every row is a fallback for the row above
/// it; failures dim in place, never panic, never block the menu.
fn support_panel(cmd: &mut Commands, panel: &SupportPanel) -> Entity {
    let mut kids = vec![text_node(
        cmd,
        format!("alms for the developers: {SUPPORT_LIGHTNING_ADDRESS}"),
        style::small_font(),
        style::DIM,
    )];
    // The code: `lightning:average@minitbits.cash`, drawn offline from the
    // same const on every platform — it can only be absent if even a
    // version-40 matrix could not hold 32 bytes, which cannot happen.
    match qr_code_node(
        cmd,
        format!("lightning:{SUPPORT_LIGHTNING_ADDRESS}").as_bytes(),
        QR_URI_CELL_PX,
        usize::MAX,
    ) {
        Some(code) => kids.push(code),
        None => kids.push(text_node(
            cmd,
            "(the code could not be drawn - the address above still gives)".into(),
            style::small_font(),
            style::DIM,
        )),
    }
    kids.push(support_link_button(cmd, "Open in your wallet", SupportLink::Address));
    #[cfg(not(target_arch = "wasm32"))]
    if panel.wallet_note {
        kids.push(text_node(
            cmd,
            "this build cannot dial a wallet - scan the code or copy the address by hand".into(),
            style::small_font(),
            style::DIM,
        ));
    }
    // Amount chips: live invoice fetches in the browser build; inert slabs
    // plus the static note on the desktop shell, which ships no HTTPS
    // stack by design (main.rs::NoHttp) — its flow stays the code above.
    kids.push(text_node(
        cmd,
        "or ask the alms purse for an invoice:".into(),
        style::small_font(),
        style::GOLD_DIM,
    ));
    let chips: Vec<Entity> = SUPPORT_ALMS_PRESETS
        .iter()
        .map(|&(msat, label)| support_chip_button(cmd, label, msat))
        .collect();
    kids.push(
        cmd.spawn(Node {
            column_gap: Val::Px(12.0),
            ..default()
        })
        .add_children(&chips)
        .id(),
    );
    #[cfg(not(target_arch = "wasm32"))]
    kids.push(text_node(
        cmd,
        "invoices in the browser build".into(),
        style::small_font(),
        style::DIM,
    ));
    // Invoice states. On the desktop shell the resource never leaves Idle,
    // so this renders nothing; the browser build repaints per state.
    match &panel.invoice {
        SupportInvoice::Idle => {}
        SupportInvoice::Fetching { amount_msat } => kids.push(text_node(
            cmd,
            format!("asking the purse for an invoice of {} sats...", amount_msat / 1000),
            style::small_font(),
            style::DIM,
        )),
        SupportInvoice::Unavailable => kids.push(text_node(
            cmd,
            "the purse did not answer - the address and code above still give freely".into(),
            style::small_font(),
            style::DIM,
        )),
        SupportInvoice::Ready { amount_msat, bolt11 } => {
            kids.push(text_node(
                cmd,
                format!(
                    "{} sats - pay this invoice with any lightning wallet:",
                    amount_msat / 1000
                ),
                style::small_font(),
                style::BONE,
            ));
            // Eye-sized chunks, wrapped to the panel measure (the raw
            // bolt11 would be one unreadable line; bolt11 is pure ASCII
            // per spec, and the shell holds no wallet to pass it to).
            kids.push(
                cmd.spawn((
                    Text::new(chunk_bolt11(bolt11)),
                    style::small_font(),
                    TextColor(style::DIM),
                    Node {
                        max_width: Val::Px(340.0),
                        ..default()
                    },
                ))
                .id(),
            );
            // 3 px/cell cap: up to version 15 (77 modules, ~412 bytes at
            // EC level M) fits the panel; a longer invoice is text-only
            // there — the capped QR would no longer scan anyway.
            match qr_code_node(cmd, bolt11.as_bytes(), QR_INVOICE_CELL_PX, QR_INVOICE_MAX_MODULES)
            {
                Some(code) => kids.push(code),
                None => kids.push(text_node(
                    cmd,
                    "(this invoice is too long for a code here - copy the text above)".into(),
                    style::small_font(),
                    style::DIM,
                )),
            }
            kids.push(support_link_button(
                cmd,
                "Open your wallet (lightning)",
                SupportLink::Invoice,
            ));
        }
    }
    cmd.spawn((
        Node {
            align_items: AlignItems::Center,
            // The 21/100/1,000 chip row is the widest child (~410 px).
            max_width: Val::Px(440.0),
            ..style::panel_node()
        },
        BackgroundColor(style::PANEL),
        BorderColor::all(style::GOLD_DIM),
    ))
    .add_children(&kids)
    .id()
}

/// `lightning:` link button (the plain address, or a fetched invoice).
fn support_link_button(cmd: &mut Commands, label: &str, link: SupportLink) -> Entity {
    let text = text_node(cmd, label.to_string(), style::body_font(), style::BONE);
    cmd.spawn((
        Button,
        style::button_node(),
        BackgroundColor(style::BUTTON),
        BorderColor::all(style::GOLD_DIM),
        link,
    ))
    .add_children(&[text])
    .observe(on_support_link)
    .id()
}

/// A link click hands the payment string to the player's own wallet — the
/// exact A3 pattern (the shell holds no wallet): the browser build asks
/// for a new tab on the `lightning:` URI; the desktop shell has no browser
/// to ask, so it echoes the target to stderr once and surfaces its
/// "scan or copy" note inside the panel.
fn on_support_link(
    click: On<Pointer<Click>>,
    links: Query<&SupportLink>,
    mut panel: ResMut<SupportPanel>,
    mut version: ResMut<SnapshotVersion>,
) {
    let Ok(link) = links.get(click.entity) else {
        return;
    };
    let uri = match link {
        SupportLink::Address => format!("lightning:{SUPPORT_LIGHTNING_ADDRESS}"),
        SupportLink::Invoice => {
            let SupportInvoice::Ready { bolt11, .. } = &panel.invoice else {
                return;
            };
            format!("lightning:{bolt11}")
        }
    };
    #[cfg(target_arch = "wasm32")]
    {
        open_wallet_uri(&uri);
        // A bare wallet hop needs no repaint; sinks keep the observer's
        // signature uniform across targets.
        let _ = (&mut panel, &mut version);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        eprintln!("Support the Order - voluntary alms for the developers: {uri}");
        if !panel.wallet_note {
            panel.wallet_note = true;
            version.0 = version.0.wrapping_add(1);
        }
    }
}


/// An alms chip (the 21/100/1,000-sat chooser precedent of the post-duel
/// alms box). Live in the browser build — a click arms the one in-flight
/// invoice fetch. The desktop shell ships no HTTPS transport
/// (main.rs::NoHttp), so there it is an inert dim slab ahead of the
/// static "browser build" note: the observer is attached but unreachable
/// (a slab spawns no `Button`, so picking never fires on it).
fn support_chip_button(cmd: &mut Commands, label: &str, amount_msat: u64) -> Entity {
    #[cfg(target_arch = "wasm32")]
    {
        let text = text_node(cmd, label.to_string(), style::body_font(), style::BONE);
        cmd.spawn((
            Button,
            style::button_node(),
            BackgroundColor(style::BUTTON),
            BorderColor::all(style::GOLD_DIM),
            SupportChip(amount_msat),
        ))
        .add_children(&[text])
        .observe(on_support_chip)
        .id()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let slab = disabled_button(cmd, label.to_string());
        cmd.entity(slab)
            .insert(SupportChip(amount_msat))
            .observe(on_support_chip);
        slab
    }
}

/// A chooser click arms an invoice fetch. Guards against double fetches
/// exactly like B3b: one invoice at a time, and only from a live chip.
/// The mailbox is created on every platform (the drain system is
/// cross-platform), but only the browser build spawns the fetch itself —
/// the desktop arm provokes nothing the static address cannot pay.
fn on_support_chip(
    click: On<Pointer<Click>>,
    chips: Query<&SupportChip>,
    mut panel: ResMut<SupportPanel>,
    mut version: ResMut<SnapshotVersion>,
) {
    let Ok(chip) = chips.get(click.entity) else {
        return;
    };
    if matches!(panel.invoice, SupportInvoice::Fetching { .. }) {
        return;
    }
    let shared = Arc::new(Mutex::new(SupportShared::default()));
    panel.shared = Some(shared.clone());
    panel.invoice = SupportInvoice::Fetching {
        amount_msat: chip.0,
    };
    version.0 = version.0.wrapping_add(1);
    #[cfg(target_arch = "wasm32")]
    spawn_support_invoice(shared, chip.0);
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Statically reachable but dynamically dead (native chips never
        // carry picking): no fetch exists to serve here.
        let _ = shared;
        panel.invoice = SupportInvoice::Unavailable;
    }
}

// ---------- Support-panel QR codes ----------
//
// Codes are generated offline from shell constants/state with the
// pure-Rust `qrcode` crate (default features off: no image codecs), then
// drawn as bevy UI rects — same code path on native and wasm.

/// Pixels per module for the panel's address code: the `lightning:` URI
/// (~40 chars, a version-3 matrix at EC level M) gets a generous 6 px so
/// a phone camera resolves it at arm's length.
const QR_URI_CELL_PX: f32 = 6.0;

/// Pixels per module cap for a bolt11 code (~300-500 chars -> versions
/// 10..=15 at EC level M): 3 px keeps even the panel cap scannable.
const QR_INVOICE_CELL_PX: f32 = 3.0;

/// An invoice whose matrix exceeds version 15 (77 modules ~ (77+8)*3 =
/// 255 px) no longer fits the panel tastefully at the 3 px cap: the
/// invoice then renders text-only, with the copy affordances it already
/// has. Pinned by the tests below.
const QR_INVOICE_MAX_MODULES: usize = 77;

/// ISO/IEC 18004 quiet zone: four modules of light margin on every side.
/// Rendered as the white pad around the matrix — scannability needs it.
const QR_QUIET_MODULES: usize = 4;

/// The dark-module matrix for `data` at EC level M (`true` = dark,
/// row-major, no quiet zone — the renderer adds that as the white pad).
/// Pure, so the unit tests pin the geometry without a bevy `Commands`.
fn qr_matrix(data: &[u8]) -> Option<(usize, Vec<bool>)> {
    let code =
        qrcode::QrCode::with_error_correction_level(data, qrcode::EcLevel::M).ok()?;
    let width = code.width();
    let dark = code
        .to_colors()
        .iter()
        .map(|c| *c == qrcode::Color::Dark)
        .collect();
    Some((width, dark))
}

/// One QR code as bevy UI rects: a white pad (the quiet zone) around a
/// matrix square, dark modules drawn as absolute-positioned run-length
/// rects — adjacent dark modules in a row merge into one node, so the
/// entity count stays a fraction of the module count, and the bitmap is
/// pixel-exact (same fill, no seams). True black on true white: palette
/// theming is kept off the codes for scan contrast. `None` — the caller
/// shows its text fallback — when the data exceeds `max_modules` or even
/// a version-40 matrix (never for this panel's two payloads).
fn qr_code_node(
    cmd: &mut Commands,
    data: &[u8],
    cell_px: f32,
    max_modules: usize,
) -> Option<Entity> {
    let (width, dark) = qr_matrix(data)?;
    if width > max_modules {
        return None;
    }
    let mut runs: Vec<Entity> = Vec::new();
    for y in 0..width {
        let row = &dark[y * width..(y + 1) * width];
        let mut x = 0;
        while x < width {
            if !row[x] {
                x += 1;
                continue;
            }
            let start = x;
            while x < width && row[x] {
                x += 1;
            }
            runs.push(
                cmd.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(start as f32 * cell_px),
                        top: Val::Px(y as f32 * cell_px),
                        width: Val::Px((x - start) as f32 * cell_px),
                        height: Val::Px(cell_px),
                        ..default()
                    },
                    BackgroundColor(Color::BLACK),
                ))
                .id(),
            );
        }
    }
    let matrix_px = width as f32 * cell_px;
    let quiet_px = QR_QUIET_MODULES as f32 * cell_px;
    let matrix = cmd
        .spawn(Node {
            position_type: PositionType::Relative,
            width: Val::Px(matrix_px),
            height: Val::Px(matrix_px),
            ..default()
        })
        .add_children(&runs)
        .id();
    Some(
        cmd.spawn((
            Node {
                width: Val::Px(matrix_px + 2.0 * quiet_px),
                height: Val::Px(matrix_px + 2.0 * quiet_px),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::WHITE),
        ))
        .add_children(&[matrix])
        .id(),
    )
}

// ---------- Support-panel house-purse invoices ----------

/// The callback's answer: `{"pr": "ln..."}`, or LNURL's shared
/// `status:"ERROR"` shape. Split from the fetch so the contract tests
/// offline against fixed fixtures (the fetch's own validation rides
/// crusade-net's mock-tested `fetch_pay_params`). Only the browser build
/// fetches, so its callers here are the wasm fetch and the unit tests.
#[cfg(any(target_arch = "wasm32", test))]
fn parse_support_invoice(body: &str) -> Result<String, ZapError> {
    // The B3a module's identical wire shape, kept private there.
    #[derive(serde::Deserialize)]
    struct InvoiceResponse {
        #[serde(default)]
        status: Option<String>,
        #[serde(default)]
        reason: Option<String>,
        #[serde(default)]
        pr: Option<String>,
    }
    let resp: InvoiceResponse = serde_json::from_str(body)
        .map_err(|e| ZapError::BadInvoice(format!("invoice json: {e}")))?;
    if resp.status.as_deref() == Some("ERROR") {
        return Err(ZapError::Lnurl(
            resp.reason.unwrap_or_else(|| "unknown".into()),
        ));
    }
    let pr = resp
        .pr
        .ok_or_else(|| ZapError::BadInvoice("missing pr".into()))?;
    let bolt11_shaped = pr.len() >= 6 && pr.get(..2).is_some_and(|p| p.eq_ignore_ascii_case("ln"));
    if !bolt11_shaped {
        return Err(ZapError::BadInvoice("pr is not bolt11-shaped".into()));
    }
    Ok(pr)
}

/// Spawn the invoice fetch on the browser's executor: the same pattern as
/// main.rs::spawn_net_task's wasm arm, for a one-shot op (no channel back;
/// the shared mailbox + [`support_bridge`] carry the result).
#[cfg(target_arch = "wasm32")]
fn spawn_support_invoice(shared: Arc<Mutex<SupportShared>>, amount_msat: u64) {
    wasm_bindgen_futures::spawn_local(async move {
        let done = fetch_support_invoice(amount_msat)
            .await
            .map_err(|err| warn!("support invoice failed ({err}); the static address remains"));
        shared
            .lock()
            .expect("support mailbox mutex poisoned")
            .done = Some(done);
    });
}

/// The house purse's LNURL-pay switchboard, via the shell's one HTTPS
/// transport (main.rs::shell_http, the browser's fetch). Step 1 reuses
/// the zaps module's validating fetch: https-only well-known derivation,
/// 64 KiB cap, `payRequest` tag, same-origin callback, and the advertised
/// min/max — all exactly as B3a/B3b. Step 2 is a plain
/// `callback?amount=<msat>` GET: house alms carry no attestation a
/// kind-9734 could tag, so unlike B3b there is NO zap request here, and
/// crusade-net stays as-is.
#[cfg(target_arch = "wasm32")]
async fn fetch_support_invoice(amount_msat: u64) -> Result<String, ZapError> {
    let http = shell_http();
    let params = zaps::fetch_pay_params(&http, SUPPORT_LIGHTNING_ADDRESS).await?;
    if amount_msat < params.min_sendable_msat || amount_msat > params.max_sendable_msat {
        return Err(ZapError::AmountOutOfRange {
            amount: amount_msat,
            min: params.min_sendable_msat,
            max: params.max_sendable_msat,
        });
    }
    let sep = if params.callback.contains('?') {
        '&'
    } else {
        '?'
    };
    let url = format!("{}{sep}amount={amount_msat}", params.callback);
    let body = http.get_text(&url, zaps::MAX_RESPONSE_BYTES).await?;
    parse_support_invoice(&body)
}

// ---------- The shell's one HTTPS transport (shared by menu + duel alms) ----------
//
// This trio dates from B3b and lived in main.rs; it moved here so BOTH
// shell binaries (crusade-desktop and the ui-shots harness, which builds
// this same `screens` module with its own crate root) resolve it
// identically. Behavior unchanged.

/// The shell's HTTPS transport for the zaps module: the browser's `fetch`
/// on wasm. Redirects are refused outright (the module's cross-origin
/// invariants rest on never following one).
#[cfg(target_arch = "wasm32")]
pub(crate) struct BrowserHttp;

#[cfg(target_arch = "wasm32")]
#[async_trait(?Send)]
impl Http for BrowserHttp {
    async fn get_text(&self, url: &str, max_bytes: u64) -> Result<String, ZapError> {
        let window = web_sys::window().ok_or_else(|| ZapError::Http("no browser window".into()))?;
        let init = web_sys::RequestInit::new();
        // SECURITY: refuse every redirect rather than follow anywhere.
        init.set_redirect(web_sys::RequestRedirect::Error);
        let request = web_sys::Request::new_with_str_and_init(url, &init)
            .map_err(|e| ZapError::Http(format!("request build: {e:?}")))?;
        let value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(|e| ZapError::Http(format!("fetch failed or redirected: {e:?}")))?;
        let response: web_sys::Response = wasm_bindgen::JsCast::dyn_into(value)
            .map_err(|_| ZapError::Http("fetch resolved to a non-Response".into()))?;
        if !response.ok() {
            return Err(ZapError::Http(format!("http status {}", response.status())));
        }
        let text_promise = response
            .text()
            .map_err(|e| ZapError::Http(format!("body read: {e:?}")))?;
        let text = wasm_bindgen_futures::JsFuture::from(text_promise)
            .await
            .map_err(|e| ZapError::Http(format!("body read: {e:?}")))?
            .as_string()
            .ok_or_else(|| ZapError::Http("body was not text".into()))?;
        // A buffered browser fetch cannot stop mid-body; the module cap is
        // still enforced before the response is handed to the parser.
        if text.len() as u64 > max_bytes {
            return Err(ZapError::TooLarge(max_bytes));
        }
        Ok(text)
    }
}

/// The desktop shell ships no HTTPS stack (no TLS client in the dependency
/// tree by design); the alms address renders display-only and invoice
/// requests fail over to the house purse, in-theme. The support panel
/// mirrors this exactly with its inert chips + "browser build" note.
/// (Dead in the ui-shots binary, which builds this module but has no
/// net/alms task — same allowances its shared mods already carry.)
#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub(crate) struct NoHttp;

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl Http for NoHttp {
    async fn get_text(&self, _url: &str, _max_bytes: u64) -> Result<String, ZapError> {
        Err(ZapError::Http(
            "the desktop shell carries no lightning heralds".into(),
        ))
    }
}

/// The one permitted HTTPS transport for this shell (wasm fetch / desktop
/// stub), as a value so generic zaps calls stay monomorphic per target.
#[cfg(target_arch = "wasm32")]
pub(crate) fn shell_http() -> BrowserHttp {
    BrowserHttp
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)] // ui-shots builds this module too; its crate has no caller
pub(crate) fn shell_http() -> NoHttp {
    NoHttp
}

// ---------- Roster ----------

/// Gallery geometry: six cards per leaf as a 2-column x 3-row grid sized to
/// the 1280x720 chrome exactly -- root screen padding 24^2, one 14px root
/// gap, a ~44px header row (title + paging/Back buttons), and two 12px grid
/// row gaps: 48 + 44 + 14 + 3*194 + 24 = 710 of 720. Card width leaves
/// 48+2*600+12 = 1260 of 1280. The whole-tree rebuild renders one leaf at a
/// time (see [`RosterPage`]); anything else would scroll past the viewport.
const ROSTER_PAGE_SIZE: usize = 6;

/// Card outer dimensions (border-box, bevy sizing): 600x194 holds a 176px
/// plate beside a 396px stats column (padding 8, border 1, gap 10).
/// (The vesture chip needs NO budget here: it is an absolute overlay
/// anchored inside the plate frame — a side column proved impossible
/// within 600px without wrapping the stats' rite rows.)
const CARD_W: f32 = 600.0;
const CARD_H: f32 = 194.0;
/// The plate's outer frame: exactly the card's inner height (194 - 2
/// border - 16 padding). The painted ground shows edge-to-edge inside the
/// hairline. (The vesture pick never alters this plate geometry — same
/// frame, same size; only the file inside it changes.)
const PLATE: f32 = 176.0;
/// The image leaves the plate's 1px border on every side.
const PLATE_IMG: f32 = PLATE - 2.0;
/// Stats column width: card inner width (600 - 2 - 16) minus plate + gap.
const CARD_TEXT_W: f32 = 600.0 - 2.0 - 16.0 - PLATE - 10.0;
/// Two columns plus the gap ride under the root's 24px side padding
/// (48 + 1212 = 1260 of 1280).
const GRID_W: f32 = 2.0 * CARD_W + 12.0;

/// Gallery pagination math, kept pure so the boundary table is unit-pinned:
/// (clamped page, page count, slice start, slice end).
fn roster_page_window(card_count: usize, page: usize) -> (usize, usize, usize, usize) {
    let pages = card_count.div_ceil(ROSTER_PAGE_SIZE).max(1);
    let page = page.min(pages - 1);
    let lo = page * ROSTER_PAGE_SIZE;
    let hi = (lo + ROSTER_PAGE_SIZE).min(card_count);
    (page, pages, lo, hi)
}

/// Shell-local roster gallery page. The exact SupportPanel pattern at
/// pager size: read by the whole-tree rebuild, flipped ONLY by the pager
/// observers below -- never a ClientInput, never game state. The resource
/// is deliberately OPTIONAL for binaries: `rebuild_ui` takes
/// `Option<Res<RosterPage>>` and both pager observers self-install the
/// default through deferred commands on the first real flip, so neither
/// main.rs nor the harness needed new wiring (the AnimTick precedent).
#[derive(Resource, Default)]
pub(crate) struct RosterPage {
    pub page: usize,
}

/// A pager flip: -1 for the previous leaf, +1 for the next.
#[derive(Component, Clone, Copy)]
struct RosterPageTurn(i8);

fn roster_screen(
    cmd: &mut Commands,
    cards: &[ChampionCard],
    assets: Option<&AssetServer>,
    page_requested: usize,
) -> Vec<Entity> {
    let (page, pages, lo, hi) = roster_page_window(cards.len(), page_requested);
    let title = text_node(cmd, "The Company".into(), style::head_font(), style::GOLD);
    // The pager cluster rides the header row (shell-local Prev/Next around
    // the leaf counter; Back stays a ClientInput like every other screen).
    // Out-of-range ends are inert slabs, so a live flip can never run off
    // the leaves: Prev exists only right of leaf 1, Next only left of the
    // last leaf.
    let mut pager_kids: Vec<Entity> = Vec::new();
    if page > 0 {
        pager_kids.push(roster_page_button(cmd, "< Prev", -1));
    } else {
        pager_kids.push(disabled_button(cmd, "< Prev".into()));
    }
    pager_kids.push(text_node(
        cmd,
        format!("Page {} of {}", page + 1, pages),
        style::small_font(),
        style::GOLD_DIM,
    ));
    if page + 1 < pages {
        pager_kids.push(roster_page_button(cmd, "Next >", 1));
    } else {
        pager_kids.push(disabled_button(cmd, "Next >".into()));
    }
    let pager = cmd
        .spawn(Node {
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            ..default()
        })
        .add_children(&pager_kids)
        .id();
    let back = action_button(cmd, "Back".into(), ClientInput::CloseToMenu);
    let right = cmd
        .spawn(Node {
            align_items: AlignItems::Center,
            column_gap: Val::Px(12.0),
            ..default()
        })
        .add_children(&[pager, back])
        .id();
    let head = cmd
        .spawn(Node {
            width: Val::Percent(100.0),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            ..default()
        })
        .add_children(&[title, right])
        .id();
    if cards.is_empty() {
        // An empty company draws the note instead of a void grid.
        return vec![
            head,
            text_node(
                cmd,
                "The chapel yard stands empty - muster your first champion.".into(),
                style::body_font(),
                style::DIM,
            ),
        ];
    }
    let card_entities: Vec<Entity> = cards[lo..hi]
        .iter()
        .map(|c| champion_card(cmd, c, assets))
        .collect();
    let grid = cmd
        .spawn(Node {
            width: Val::Px(GRID_W),
            column_gap: Val::Px(12.0),
            row_gap: Val::Px(12.0),
            flex_wrap: FlexWrap::Wrap,
            ..default()
        })
        .add_children(&card_entities)
        .id();
    vec![head, grid]
}

/// A pager button: looks like every [`action_button`], but its click rides
/// an entity observer, so paging can never reach the client (no
/// ClientInput, no Phase -- the gallery itself is pure presentation).
/// Like the menu alms box it carries no ActionButton, so `collect_clicks`
/// never touches it (hover feedback follows that same convention).
fn roster_page_button(cmd: &mut Commands, label: &str, turn: i8) -> Entity {
    let text = text_node(cmd, label.to_string(), style::body_font(), style::BONE);
    cmd.spawn((
        Button,
        style::button_node(),
        BackgroundColor(style::BUTTON),
        BorderColor::all(style::GOLD_DIM),
        RosterPageTurn(turn),
    ))
    .add_children(&[text])
    .observe(on_roster_page_turn)
    .id()
}

/// The pager flip: bump the shell-local leaf and repaint. Bounds are
/// structural (live buttons never spawn at the edges), so the arithmetic
/// saturates; a still-slender guard clamps again at render. When no
/// RosterPage resource exists (any host binary that never registered one),
/// the first flip mints it.
fn on_roster_page_turn(
    click: On<Pointer<Click>>,
    turns: Query<&RosterPageTurn>,
    mut commands: Commands,
    page: Option<ResMut<RosterPage>>,
    mut version: ResMut<SnapshotVersion>,
) {
    let Ok(turn) = turns.get(click.entity) else {
        return;
    };
    match page {
        Some(mut page) => {
            page.page = if turn.0 < 0 {
                page.page.saturating_sub(1)
            } else {
                page.page.saturating_add(1)
            };
        }
        None => {
            commands.insert_resource(RosterPage {
                page: if turn.0 < 0 { 0 } else { 1 },
            });
        }
    }
    version.0 = version.0.wrapping_add(1);
}

/// One company gallery card (~600x194): the painted 176px plate left in a
/// gold-dim frame like a manuscript miniature, then the stats column right
/// -- name and level, virtue/sin typing, resolve, the per-instance
/// Devotion and Grace blocks, and the compact rite list (name with
/// Pow/Acc/Zeal each, up to four). The plate image is the gallery class
/// (un-quantized 512px master downscaled on GPU): it spawns only beside an
/// asset server and a static-table species; a missing file draws nothing
/// over the plate's navy ground and the text column is the card's
/// unconditional floor either way (the same AssetServer discipline as the
/// other sprite seams).
fn champion_card(cmd: &mut Commands, card: &ChampionCard, assets: Option<&AssetServer>) -> Entity {
    let mut kids = Vec::new();
    if let Some(plate) = gallery_frame(cmd, card, assets) {
        kids.push(plate);
    }
    let text = champion_card_stats(cmd, card);
    kids.push(text);
    cmd.spawn((
        Node {
            width: Val::Px(CARD_W),
            height: Val::Px(CARD_H),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(10.0),
            padding: UiRect::all(Val::Px(8.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(style::PANEL),
        BorderColor::all(style::GOLD_DIM),
    ))
    .add_children(&kids)
    .id()
}

// ---------- Roster vesture chip ----------

/// The Vesture chip's hover tip: the gallery plate picker is pure Company
/// chrome — the duel field never leaves the canonical arms (the alt sets
/// ship no animation families; documented, intentional).
const VESTURE_TIP: &str = "The vesture the gallery shows; battle keeps the canonical arms.";

/// Inset of the chip from the plate frame's bottom-left corner.
const VESTURE_INSET: f32 = 4.0;

/// A per-card "Vesture" chip: the house chip recipe (navy fill, gold-dim
/// hairline, small text, tight padding — battle.rs's command-row chips,
/// mirrored here because that helper stays its file's private one) bound
/// to [`ClientInput::CycleVariant`], so a click rings the client's
/// persisted pick through the ordinary `InputQueue`/`process_inputs`
/// pump — one input per click, the repaint bump comes free, and the
/// browser build's store-persist hook fires on the same accepted-input
/// path as every other button. Two rules: only champions carry alt plates
/// (the client refuses anything else), so a non-champion row — corrupt
/// store only — spawns no chip to refuse; and the chip rides ONLY aboard
/// the plate frame itself (spawned as its child by [`gallery_frame`]),
/// keeping headless cards unchanged text-only.
fn vesture_chip(cmd: &mut Commands, card: &ChampionCard) -> Option<Entity> {
    if card.species_id >= 100 {
        return None;
    }
    let text = text_node(cmd, "Vesture".to_string(), style::small_font(), style::BONE);
    let mut chip = cmd.spawn((
        Button,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(VESTURE_INSET),
            bottom: Val::Px(VESTURE_INSET),
            padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
            border: UiRect::all(Val::Px(1.0)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(style::BUTTON),
        BorderColor::all(style::GOLD_DIM),
        BaseBackground(style::BUTTON),
        ActionButton(ClientInput::CycleVariant {
            species_id: card.species_id,
        }),
    ));
    tooltip::with_tooltip(&mut chip, VESTURE_TIP);
    Some(chip.add_children(&[text]).id())
}

// ---------- Roster gallery hover tips ----------
//
// Same house rules as battle.rs's tips: ASCII, [`battle::TIP_LEN_BUDGET`]
// chars (two lines at the 480px wrap), static tables or the card's own
// view fields only, ONE tip per widget.

/// The devotion row: what the per-instance Devotion block IS (the
/// combat-training axis; crusade-core caps 63/measure, 130 in sum).
const DEVOTION_TIP: &str =
    "Devotion: strength honed by training and trial, listed resolve through alacrity -- at most 63 in one measure, 130 in sum.";

/// The grace row: the given (never-trained) axis, 0..=15 per measure, in
/// the same resolve-through-alacrity order as devotion above it.
const GRACE_TIP: &str =
    "Grace: heaven's allotment, given and never trained -- from 0 to 15 in each measure, in the same order as devotion.";

/// The virtue label's hover tip: the card's virtue/sin typing read against
/// the campaign gauntlet -- for each virtue, the sin it conquers (chart:
/// 2x on `opposed_sin`) and the boss spawn bearing that sin. Composed from
/// the static species/chart tables keyed off the card's `species_id`;
/// None for unknown ids and sin-spawn (which never muster into a company).
fn virtue_matchup_tip(species_id: u16) -> Option<Cow<'static, str>> {
    let sp = data::species_by_id(species_id)?;
    let Typing::Virtues(virtues) = sp.typing else {
        return None;
    };
    let clauses: Vec<String> = virtues
        .iter()
        .flatten()
        .map(|v| {
            let sin = v.opposed_sin();
            let boss = sin_boss(sin).map_or("that sin", |b| b.name);
            format!("{} conquers {} ({})", virtue_name(*v), sin_name(sin), boss)
        })
        .collect();
    if clauses.is_empty() {
        return None;
    }
    Some(format!("In the gauntlet: {}.", clauses.join("; ")).into())
}

/// The sin-spawn bearing `sin` in the static table. Engine data pins
/// exactly one boss per sin (species.rs tests prove the cover), so the
/// fallback above "that sin" is unreachable -- a table drift must never
/// panic the shell.
fn sin_boss(sin: Sin) -> Option<&'static SpeciesBase> {
    data::all_species()
        .iter()
        .find(|sp| sp.typing == Typing::Sin(sin))
}

/// Virtue display names -- the same short table client.rs keeps private
/// (mirrored like the local `slugify`), so the shell never reaches across
/// the client boundary for display words.
fn virtue_name(v: Virtue) -> &'static str {
    match v {
        Virtue::Humility => "Humility",
        Virtue::Charity => "Charity",
        Virtue::Chastity => "Chastity",
        Virtue::Diligence => "Diligence",
        Virtue::Patience => "Patience",
        Virtue::Temperance => "Temperance",
        Virtue::Kindness => "Kindness",
    }
}

/// Sin display names; same mirror discipline as [`virtue_name`].
fn sin_name(s: Sin) -> &'static str {
    match s {
        Sin::Pride => "Pride",
        Sin::Envy => "Envy",
        Sin::Lust => "Lust",
        Sin::Sloth => "Sloth",
        Sin::Wrath => "Wrath",
        Sin::Gluttony => "Gluttony",
        Sin::Greed => "Greed",
    }
}

/// The gallery plate's hover lore: the species' short lore line, champions
/// ONLY -- same gate as battle.rs's portrait tip, because a boss's lore is
/// the Manuscript of Sins' reveal-gated allegory and this screen carries
/// no defeat mask. Roster stores hold champions, so the gate never bites
/// in practice; it exists so a corrupt store can never leak the reward
/// text through this binding either.
fn plate_lore(sp: &SpeciesBase) -> Option<&'static str> {
    (sp.id < 100).then_some(sp.lore)
}

/// The stats column of a gallery card (unconditional: every row here is
/// plain text on the panel, image or no image).
fn champion_card_stats(cmd: &mut Commands, card: &ChampionCard) -> Entity {
    let mut kids = vec![
        text_node(
            cmd,
            format!("{} - Lv {}", card.species_name, card.level),
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
        let typing = text_node(
            cmd,
            card.types_display.join(" + "),
            style::small_font(),
            style::GOLD_DIM,
        );
        // The virtue label carries the champion's matchups against the
        // gauntlet's seven sin-bosses (composed from the static typing
        // wheel, or skipped for a species the table cannot name).
        if let Some(tip) = virtue_matchup_tip(card.species_id) {
            tooltip::with_tooltip(&mut cmd.entity(typing), tip);
        }
        kids.push(typing);
    }
    // The fuller blocks: Devotion (trained) over Grace (given). Pre-
    // gallery documents deserialize these as empty strings; the rows then
    // hide rather than render a hollow label. Each live row carries its
    // one hover tip explaining the axis.
    if !card.devotion_display.is_empty() {
        let row = text_node(
            cmd,
            format!("devotion: {}", card.devotion_display),
            style::small_font(),
            style::DIM,
        );
        tooltip::with_tooltip(&mut cmd.entity(row), DEVOTION_TIP);
        kids.push(row);
    }
    if !card.grace_display.is_empty() {
        let row = text_node(
            cmd,
            format!("grace:    {}", card.grace_display),
            style::small_font(),
            style::DIM,
        );
        tooltip::with_tooltip(&mut cmd.entity(row), GRACE_TIP);
        kids.push(row);
    }
    for rite in card.rites.iter().take(4) {
        kids.push(text_node(
            cmd,
            format!(
                "- {}  Pow{} Acc{} Z{}/{}",
                rite.name, rite.power, rite.accuracy, rite.zeal_left, rite.zeal_max
            ),
            style::small_font(),
            style::DIM,
        ));
    }
    cmd.spawn(Node {
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(1.0),
        width: Val::Px(CARD_TEXT_W),
        ..default()
    })
    .add_children(&kids)
    .id()
}

/// The card's 176px gallery plate: a navy well with a gold-dim hairline
/// holding the staged 512px portrait at 174px (`sprites/gallery/
/// {id:03}-{slug}.png`, the un-quantized gallery class staged by
/// `tools/artbake/stage_sprites.py`'s gallery pass from each cell's raw
/// 1024 master) — or, when the card's vesture pick is 1/2, that
/// champion's `--alt-a`/`--alt-b` wardrobe plate (the E5 alt masters,
/// staged to the same 512px class). Spawned only when an asset server
/// exists AND the id is a static-table species (whose plates the art lane
/// always ships); a 404ing fetch (partial wasm deploy) draws nothing into
/// the well over the card's unconditional text rows. On native, a missing
/// alt file falls back to the base plate; the browser build trusts the
/// pick (checking the filesystem pre-load is not a wasm move).
fn gallery_frame(cmd: &mut Commands, card: &ChampionCard, assets: Option<&AssetServer>) -> Option<Entity> {
    let sp = data::species_by_id(card.species_id)?;
    let assets = assets?;
    let path = gallery_plate_path(sp, card.variant);
    #[cfg(not(target_arch = "wasm32"))]
    let path = if card.variant != 0 && !gallery_asset_exists(&path) {
        gallery_sprite_path(sp)
    } else {
        path
    };
    let image = cmd
        .spawn((
            ImageNode::new(assets.load(path)),
            Node {
                width: Val::Px(PLATE_IMG),
                height: Val::Px(PLATE_IMG),
                ..default()
            },
        ))
        .id();
    // The plate tells the champion's origin line on hover (bosses sealed
    // — whichever vesture it wears, the origin story is one). The tip
    // rides the IMAGE entity, not the frame: the frame also parents the
    // vesture chip below, and house rule is ONE tip per hovered widget —
    // pointer onto the chip (its topmost sibling) Out's the image and
    // Over's the chip, so each surface shows its own words.
    if let Some(tip) = plate_lore(sp) {
        tooltip::with_tooltip(&mut cmd.entity(image), tip);
    }
    let frame = cmd
        .spawn((
            Node {
                width: Val::Px(PLATE),
                height: Val::Px(PLATE),
                border: UiRect::all(Val::Px(1.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(style::NAVY),
            BorderColor::all(style::GOLD_DIM),
        ))
        .add_children(&[image])
        .id();
    // The vesture chip rides the plate's bottom-left corner, absolute, as
    // the frame's LAST child so it paints over the art's dark ground
    // strip. Card geometry stays exactly the E1 recipe; headless cards
    // (no asset server, no frame) stay unchanged text-only.
    if let Some(chip) = vesture_chip(cmd, card) {
        cmd.entity(frame).add_children(&[chip]);
    }
    Some(frame)
}

/// Staged gallery-plate asset path for a static-table species
/// (`sprites/gallery/{id:03}-{slug}.png` -- the same NNN-slug keying as
/// the battle sprites, from stage_sprites.py's gallery pass).
fn gallery_sprite_path(sp: &SpeciesBase) -> String {
    format!("sprites/gallery/{:03}-{}.png", sp.id, slugify(sp.name))
}

/// The plate a given vesture pick shows: 0 (or any pick for a sin-spawn —
/// they stage no alts) is the base plate; 1/2 the champion's `--alt-a` /
/// `--alt-b` wardrobe plate. Out-of-ring values clamp onto the base, the
/// same degrade the client's `variant_of` applies to a torn setting.
fn gallery_plate_path(sp: &SpeciesBase, variant: u8) -> String {
    if sp.id >= 100 {
        return gallery_sprite_path(sp);
    }
    match variant {
        1 => format!("{}--alt-a.png", gallery_sprite_path(sp).trim_end_matches(".png")),
        2 => format!("{}--alt-b.png", gallery_sprite_path(sp).trim_end_matches(".png")),
        _ => gallery_sprite_path(sp),
    }
}

/// Native-only existence probe for the vesture fallback: does the staged
/// plate sit under the shell's asset root? The root resolution mirrors
/// bevy's `FileAssetReader` exactly (`BEVY_ASSET_ROOT`, then runtime
/// `CARGO_MANIFEST_DIR`, then the executable's own directory) plus the
/// default `assets` file path, so a partial deploy draws the base plate
/// instead of an empty well.
#[cfg(not(target_arch = "wasm32"))]
fn gallery_asset_exists(asset_path: &str) -> bool {
    let base = std::env::var("BEVY_ASSET_ROOT")
        .or_else(|_| std::env::var("CARGO_MANIFEST_DIR"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
                .unwrap_or_default()
        });
    base.join("assets").join(asset_path).is_file()
}

// ---------- Campaign rest ----------

fn rest_screen(cmd: &mut Commands, snap: &ViewSnapshot, assets: Option<&AssetServer>) -> Vec<Entity> {
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
                "Bosses defeated: {}/7 - contrition shards earned: {}",
                c.defeated, c.shards_earned
            ),
            style::body_font(),
            style::BONE,
        ));
        // The gauntlet has a face (card D3): the next sin-boss's staged
        // portrait over a caption line. Absent art or a cleared gauntlet
        // keeps the screen exactly as it was.
        if let Some(face) = next_boss_panel(cmd, c.node, assets) {
            kids.push(face);
        }
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

/// The next sin-boss's face on the rest screen: the staged 128x128 battle
/// sprite over a one-line caption in a small panel, so the gauntlet has a
/// face while the company mends. `None` once the gauntlet is cleared (node
/// 7) or if the mirrored table ever pointed off the static species table --
/// the node then renders nothing. Same AssetServer discipline as the other
/// sprite seams: the `ImageNode` spawns only when an asset server exists,
/// and a 404ing fetch (partial wasm deploy) simply draws nothing over the
/// caption; every pre-existing text row is unconditional.
fn next_boss_panel(cmd: &mut Commands, node: u8, assets: Option<&AssetServer>) -> Option<Entity> {
    let sp = next_boss_species(node)?;
    let mut kids = Vec::new();
    if let Some(assets) = assets {
        kids.push(
            cmd.spawn((
                ImageNode::new(assets.load(boss_sprite_path(sp))),
                Node {
                    width: Val::Px(128.0),
                    height: Val::Px(128.0),
                    ..default()
                },
            ))
            .id(),
        );
    }
    kids.push(text_node(
        cmd,
        format!("Node {}/7 - {} waits beyond the rest.", node + 1, sp.name),
        style::small_font(),
        style::GOLD_DIM,
    ));
    Some(
        cmd.spawn((
            Node {
                align_items: AlignItems::Center,
                ..style::panel_node()
            },
            BackgroundColor(style::PANEL),
            BorderColor::all(style::GOLD_DIM),
        ))
        .add_children(&kids)
        .id(),
    )
}

/// The gauntlet order is single-sourced in the client:
/// `crusade_app::client::CAMPAIGN_NODE_SPECIES` (imported above). Card D3
/// noted a mirror here would be the interim and "pub the client's const"
/// the cleaner end-state; the Manuscript of Sins work hoisted it, so the
/// portraits below — keyed off `CampaignView.node` — and the client's
/// mechanics/defeat-mask bits now provably share one table. The tests
/// still pin it to the static species table and the staged sprite slugs.
/// Presentation-only consumption: if this screen misread it, the worst
/// case is a wrong portrait, never wrong mechanics.

/// The sin-boss species waiting at gauntlet `node` (0-based), or `None`
/// once the gauntlet is cleared.
fn next_boss_species(node: u8) -> Option<&'static SpeciesBase> {
    let id = *CAMPAIGN_NODE_SPECIES.get(usize::from(node))?;
    data::species_by_id(id)
}

/// Staged battle-sprite asset path for a static-table species
/// (`sprites/champions/{id:03}-{slug}.png` -- the same files
/// `battle.rs::champion_sprite` draws, staged by
/// `tools/artbake/stage_sprites.py`).
fn boss_sprite_path(sp: &SpeciesBase) -> String {
    format!("sprites/champions/{:03}-{}.png", sp.id, slugify(sp.name))
}

/// Asset-slug form of a species name: lowercase ASCII, with every run of
/// non-alphanumerics collapsed to one dash. Same recipe as
/// `battle.rs::slugify` (kept private there), so both files resolve the
/// same staged slugs.
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

// ---------- Battle over ----------

fn over_screen(
    cmd: &mut Commands,
    snap: &ViewSnapshot,
    assets: Option<&AssetServer>,
) -> Vec<Entity> {
    let (banner, color) = match snap.battle_result {
        Some(0) => ("Victory - Deo Gratias", style::GOLD),
        Some(_) => ("Defeat - Pray and Regroup", style::HP_RED),
        None => ("The field falls quiet.", style::BONE),
    };
    let mut kids = vec![text_node(cmd, banner.into(), style::title_font(), color)];
    // The verdict holds: victor's pose framed in gold, the fallen plain.
    if let Some(row) = battle::verdict_row(cmd, snap, snap.battle_result.map(|w| w == 0), assets) {
        kids.push(row);
    }
    if let Some(c) = snap.campaign {
        kids.push(text_node(
            cmd,
            format!(
                "Nodes cleared: {}/7 - contrition shards earned: {}",
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

// ---------- Tournament of Virtue (lobby) ----------

/// The lobby screen: identity + heralds, the notice line, the open
/// challenges with per-challenge accept buttons, and the control row.
fn lobby_screen(cmd: &mut Commands, snap: &ViewSnapshot, ladder: Option<u64>) -> Vec<Entity> {
    let Some(lobby) = snap.lobby.as_ref() else {
        // The phase guarantees a lobby view; stay defensive, never panic.
        return vec![text_node(
            cmd,
            "Seeking the heralds...".into(),
            style::head_font(),
            style::BONE,
        )];
    };
    let idle = lobby.stage == LobbyStage::Idle;
    let mustered = snap.roster_count > 0;
    let mut kids = vec![
        text_node(
            cmd,
            "TOURNAMENT OF VIRTUE".into(),
            style::head_font(),
            style::GOLD,
        ),
        text_node(
            cmd,
            if lobby.key_label.is_empty() {
                "Your seal is being struck.".to_string()
            } else {
                format!("Your seal: {}", lobby.key_label)
            },
            style::small_font(),
            style::GOLD_DIM,
        ),
        text_node(cmd, ladder_line(ladder), style::small_font(), style::DIM),
        text_node(
            cmd,
            if lobby.relays.is_empty() {
                "Awaiting word from the heralds...".to_string()
            } else {
                format!("Heralds: {}", lobby.relays.join(", "))
            },
            style::small_font(),
            style::DIM,
        ),
        text_node(
            cmd,
            if mustered {
                format!("Your company stands {} strong.", snap.roster_count)
            } else {
                "Your company is empty - no champion may take the field.".to_string()
            },
            style::small_font(),
            style::BONE,
        ),
    ];
    if !lobby.notice.is_empty() {
        kids.push(text_node(
            cmd,
            lobby.notice.clone(),
            style::body_font(),
            style::GOLD,
        ));
    }

    // Open challenges: accept buttons are live only at rest and with a
    // mustered company.
    let mut rows = vec![text_node(
        cmd,
        "Open challenges".into(),
        style::body_font(),
        style::GOLD_DIM,
    )];
    if lobby.challenges.is_empty() {
        rows.push(text_node(
            cmd,
            if idle {
                "No challenges hang on the boards.".to_string()
            } else {
                "The boards are being read...".to_string()
            },
            style::small_font(),
            style::DIM,
        ));
    }
    for ch in &lobby.challenges {
        let label = format!(
            "Answer {} - {} champion(s) sworn",
            ch.host_label, ch.party_size
        );
        if idle && mustered {
            rows.push(action_button(
                cmd,
                label,
                ClientInput::AcceptChallenge {
                    event_id: ch.event_id.clone(),
                },
            ));
        } else {
            rows.push(disabled_button(cmd, label));
        }
    }
    kids.push(panel(cmd, &rows));

    // Controls: post / refresh collapse to inert slabs while the stage is
    // busy (with the status text already shown in the notice line).
    let post_label = match lobby.stage {
        LobbyStage::Idle => "Issue a challenge".to_string(),
        LobbyStage::Posting => "Issuing the challenge...".to_string(),
        LobbyStage::AwaitingAccept => "Awaiting a champion's answer...".to_string(),
        LobbyStage::Accepting => "Answering a challenge...".to_string(),
        LobbyStage::Listing => "Scanning the lists...".to_string(),
        LobbyStage::Connecting => "Reaching the heralds...".to_string(),
    };
    let mut buttons = Vec::new();
    if idle && mustered {
        buttons.push(action_button(cmd, post_label, ClientInput::PostChallenge));
    } else {
        buttons.push(disabled_button(cmd, post_label));
    }
    if idle {
        buttons.push(action_button(
            cmd,
            "Scan the lists".to_string(),
            ClientInput::RefreshChallenges,
        ));
    } else {
        buttons.push(disabled_button(cmd, "Scan the lists".to_string()));
    }
    buttons.push(action_button(
        cmd,
        "Withdraw to the chapel yard".to_string(),
        ClientInput::LeaveLobby,
    ));
    kids.extend(buttons);
    kids
}

// ---------- Net duel settlement ----------

/// Settled net duel: the driver snapshot supplies the chronicle; `won`
/// came through the mailbox with the final frame. `alms` carries the B3b
/// post-settlement state (attestation publish receipt + the voluntary
/// alms box). Re-rendering here can never re-publish anything: the publish
/// happened once in the net task, which only stamps its receipt here.
fn net_over_screen(
    cmd: &mut Commands,
    snap: &ViewSnapshot,
    won: Option<bool>,
    alms: Option<&AlmsState>,
    assets: Option<&AssetServer>,
) -> Vec<Entity> {
    let (banner, color) = match won {
        Some(true) => ("Victory - Deo Gratias", style::GOLD),
        Some(false) => ("Defeat - Pray and Regroup", style::HP_RED),
        None => ("The field falls quiet.", style::BONE),
    };
    let mut kids = vec![
        text_node(cmd, banner.into(), style::title_font(), color),
        text_node(
            cmd,
            if won.is_some() {
                "The transcript is sealed and the honor ladder has recorded the standing."
            } else {
                "Sealing the result with the heralds..."
            }
            .to_string(),
            style::small_font(),
            style::DIM,
        ),
        log_panel(
            cmd,
            "Last rites of the duel",
            // 4 freshest lines: the net surface also carries banner + seal +
            // verdict poses + attestation + alms; chronicles longer than
            // this overflowed 720px (measured: banner clipped, Amen flush).
            &snap.log_tail[snap.log_tail.len().saturating_sub(4)..],
        ),
    ];
    // The verdict holds: victor's pose framed in gold, the fallen plain.
    if let Some(row) = battle::verdict_row(cmd, snap, won, assets) {
        kids.push(row);
    }
    if let Some(alms) = alms {
        // (B3b, a) the one-time attestation publish's receipt.
        if let Some(post) = alms.attestation {
            let line = match post {
                AttestationPost::Pending => {
                    "The duel's attestation is being carried to the heralds..."
                }
                AttestationPost::Published => "The duel's attestation rests with the heralds.",
                AttestationPost::Failed => {
                    "The heralds did not take the attestation; the sealed transcript stands."
                }
            };
            kids.push(text_node(cmd, line.into(), style::small_font(), style::DIM));
        }
        // (B3b, b) alms for the victor: loser-optional, never owed, and
        // never offered to the winner (alms are given, not taken).
        if won == Some(false) {
            if let Some(alms_box) = alms_panel(cmd, alms) {
                kids.push(alms_box);
            }
        }
    }
    kids.push(action_button(
        cmd,
        "Amen".into(),
        ClientInput::AcknowledgeResult,
    ));
    kids
}

// ---------- Post-duel alms (card B3b; docs/ZAP-MONETIZATION.md surface #2) ----------
//
// INTEGRITY FIREWALL (ZAP-MONETIZATION §2): everything below renders task
// mailbox text and, on click, sends ONLY NetCmd::AlmsInvoice to the net
// task — battle RNG, matchmaking, the ladder, and the attestation itself
// are settled and signed before any of it, and observe none of it back.
// Alms may change what a player SEES, never what a player IS.

/// The chooser's presets: modest amounts, in sats, carried as msat.
const ALMS_PRESETS: [(u64, &str); 3] = [
    (21_000, "21 sats"),
    (100_000, "100 sats"),
    (1_000_000, "1,000 sats"),
];

/// A preset alms-amount button, carrying millisatoshis.
#[derive(Component, Clone, Copy)]
struct AlmsChoice(u64);

/// Which `lightning:` target an alms link button hands to the wallet.
#[derive(Component, Clone, Copy)]
enum AlmsLink {
    /// The house fallback: the menu's "Support the Order" address.
    House,
    /// The bolt11 last fetched from the victor's purse.
    VictorInvoice,
}

/// "You may offer alms to the victor." Renders the settled duel's alms
/// state: purse resolution states, the amount chooser, and any fetched
/// invoice as copyable text plus a `lightning:` affordance — the exact
/// payment pattern of the menu's alms button.
fn alms_panel(cmd: &mut Commands, alms: &AlmsState) -> Option<Entity> {
    let mut kids = vec![text_node(
        cmd,
        "You may offer alms to the victor.".into(),
        style::body_font(),
        style::GOLD_DIM,
    )];
    match &alms.purse {
        AlmsPurse::NotOffered => return None,
        AlmsPurse::Resolving => {
            kids.push(text_node(
                cmd,
                "the victor's alms purse: resolving...".into(),
                style::small_font(),
                style::DIM,
            ));
        }
        AlmsPurse::House => {
            kids.push(text_node(
                cmd,
                "The victor keeps no alms purse - your alms go to the Crusade's works.".into(),
                style::small_font(),
                style::BONE,
            ));
            kids.push(text_node(
                cmd,
                format!("alms for the works: {SUPPORT_LIGHTNING_ADDRESS}"),
                style::small_font(),
                style::DIM,
            ));
            kids.push(alms_link_button(
                cmd,
                "Support the Crusade's works",
                AlmsLink::House,
            ));
        }
        AlmsPurse::Victor(address) => {
            kids.push(text_node(
                cmd,
                format!("the victor's purse: {address}"),
                style::small_font(),
                style::DIM,
            ));
            kids.push(text_node(
                cmd,
                "Give what you will - nothing is owed:".into(),
                style::small_font(),
                style::GOLD_DIM,
            ));
            let choices: Vec<Entity> = ALMS_PRESETS
                .iter()
                .map(|&(msat, label)| alms_choice_button(cmd, label, msat))
                .collect();
            kids.push(
                cmd.spawn(Node {
                    column_gap: Val::Px(12.0),
                    ..default()
                })
                .add_children(&choices)
                .id(),
            );
            match &alms.invoice {
                AlmsInvoice::Idle => {}
                AlmsInvoice::Fetching { amount_msat } => kids.push(text_node(
                    cmd,
                    format!(
                        "asking the purse for an invoice of {} sats...",
                        amount_msat / 1000
                    ),
                    style::small_font(),
                    style::DIM,
                )),
                AlmsInvoice::Ready {
                    amount_msat,
                    bolt11,
                } => {
                    kids.push(text_node(
                        cmd,
                        format!(
                            "{} sats - pay this invoice with any lightning wallet:",
                            amount_msat / 1000
                        ),
                        style::small_font(),
                        style::BONE,
                    ));
                    kids.push(text_node(
                        cmd,
                        chunk_bolt11(bolt11),
                        style::small_font(),
                        style::DIM,
                    ));
                    kids.push(alms_link_button(
                        cmd,
                        "Open your wallet (lightning)",
                        AlmsLink::VictorInvoice,
                    ));
                }
            }
        }
    }
    Some(panel(cmd, &kids))
}

/// Chooser button. Its click rides an entity observer outside the
/// `ClientInput` queue — mirroring the menu's alms button, so the alms
/// path provably cannot touch battles, RNG, ladders, or saves.
fn alms_choice_button(cmd: &mut Commands, label: &str, amount_msat: u64) -> Entity {
    let text = text_node(cmd, label.to_string(), style::body_font(), style::BONE);
    cmd.spawn((
        Button,
        style::button_node(),
        BackgroundColor(style::BUTTON),
        BorderColor::all(style::GOLD_DIM),
        AlmsChoice(amount_msat),
    ))
    .add_children(&[text])
    .observe(on_alms_choice)
    .id()
}

/// `lightning:` link button (invoice or house address).
fn alms_link_button(cmd: &mut Commands, label: &str, link: AlmsLink) -> Entity {
    let text = text_node(cmd, label.to_string(), style::body_font(), style::BONE);
    cmd.spawn((
        Button,
        style::button_node(),
        BackgroundColor(style::BUTTON),
        BorderColor::all(style::GOLD_DIM),
        link,
    ))
    .add_children(&[text])
    .observe(on_alms_link)
    .id()
}

/// A chooser click asks the task for an invoice. Guards against double
/// fetches and stray clicks: one invoice at a time, and only while a
/// resolved victor purse is on screen.
fn on_alms_choice(
    click: On<Pointer<Click>>,
    choices: Query<&AlmsChoice>,
    mut bridge: ResMut<NetBridge>,
) {
    let Ok(choice) = choices.get(click.entity) else {
        return;
    };
    let sendable = bridge.shared.as_ref().is_some_and(|shared| {
        let s = shared.lock().expect("net mailbox mutex poisoned");
        matches!(&s.alms, Some(a) if matches!(a.purse, AlmsPurse::Victor(_))
            && !matches!(a.invoice, AlmsInvoice::Fetching { .. }))
    });
    if sendable {
        let _ = bridge.send(NetCmd::AlmsInvoice {
            amount_msat: choice.0,
        });
    }
}

/// A link click hands the payment string to the player's own wallet.
fn on_alms_link(click: On<Pointer<Click>>, links: Query<&AlmsLink>, bridge: Res<NetBridge>) {
    let Ok(link) = links.get(click.entity) else {
        return;
    };
    let uri = match link {
        AlmsLink::House => format!("lightning:{SUPPORT_LIGHTNING_ADDRESS}"),
        AlmsLink::VictorInvoice => {
            let bolt11 = bridge.shared.as_ref().and_then(|shared| {
                let s = shared.lock().expect("net mailbox mutex poisoned");
                match &s.alms {
                    Some(AlmsState {
                        invoice: AlmsInvoice::Ready { bolt11, .. },
                        ..
                    }) => Some(bolt11.clone()),
                    _ => None,
                }
            });
            let Some(bolt11) = bolt11 else { return };
            format!("lightning:{bolt11}")
        }
    };
    open_wallet_uri(&uri);
}

/// Hand a `lightning:` payment string to the player's own wallet — the
/// exact pattern of the menu's alms button (the shell holds no wallet):
/// native logs so the on-screen text stays the copy affordance; wasm asks
/// the browser to open the URI in a new tab.
fn open_wallet_uri(uri: &str) {
    info!("alms, freely given: {uri}");
    #[cfg(target_arch = "wasm32")]
    {
        let Some(window) = web_sys::window() else {
            return;
        };
        if let Err(err) = window.open_with_url_and_target(uri, "_blank") {
            warn!("could not open the wallet ({err:?}); the text on screen remains");
        }
    }
}

/// Eye-sized display chunks for a bolt11 — pure ASCII by spec, so byte
/// chunks never split a code point.
fn chunk_bolt11(bolt11: &str) -> String {
    bolt11
        .as_bytes()
        .chunks(40)
        .map(|c| std::str::from_utf8(c).expect("bolt11 is pure ASCII"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A duel that ended out of band (peer fled, link lost): in-theme abort,
/// never a panic, never a recorded standing.
fn duel_aborted_screen(cmd: &mut Commands, notice: &str) -> Vec<Entity> {
    vec![
        text_node(
            cmd,
            "The field is abandoned.".into(),
            style::title_font(),
            style::HP_RED,
        ),
        text_node(cmd, notice.to_string(), style::body_font(), style::BONE),
        text_node(
            cmd,
            "No standing is recorded for an unfinished contest.".into(),
            style::small_font(),
            style::DIM,
        ),
        action_button(cmd, "Amen".into(), ClientInput::AcknowledgeResult),
    ]
}

// ---------- Hagiography (witness roll + Manuscript of Sins) ----------
//
// The memorial roll of the revered non-combatant dead: pure lore, drawn
// from `crusade_core::data::all_witnesses()` through
// `ViewSnapshot::witnesses`. Beneath it, the Manuscript of Sins: the
// seven sin-bosses of the campaign gauntlet via `ViewSnapshot::sins`,
// each entry sealed (name only, dimmed) until its node has been cleared.
// The only button is the way back — nothing on this screen is an input to
// any game system.

/// The Hagiography screen: a single-column memorial scroll. One panel per
/// witness — "name, epithet" as the heading over the vignette reading —
/// with generous room around each, flowing from the top like the roster so
/// the roll may lengthen without crowding. The Manuscript of Sins follows
/// as one register panel of seven rows (a sin-boss revealed only reads its
/// full allegory; an unfaced one keeps its name dimmed beside its icon).
/// Back rides the header row, roster-style: with the manuscript appended
/// the scroll can outgrow a 720 window, so the one exit must never wait
/// below the fold.
fn hagiography_screen(
    cmd: &mut Commands,
    snap: &ViewSnapshot,
    assets: Option<&AssetServer>,
) -> Vec<Entity> {
    let title = text_node(cmd, "Hagiography".into(), style::head_font(), style::GOLD);
    let back = action_button(cmd, "Back".into(), ClientInput::CloseToMenu);
    // Roster-style header: title left, Back right, within the same 760px
    // measure both sections' panels use.
    let head = cmd
        .spawn(Node {
            width: Val::Px(760.0),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            ..default()
        })
        .add_children(&[title, back])
        .id();
    let mut kids = vec![
        head,
        text_node(
            cmd,
            "- The Witness Roll -".into(),
            style::small_font(),
            style::GOLD_DIM,
        ),
    ];
    if snap.witnesses.is_empty() {
        // The phase guarantees the roll; stay defensive, never panic.
        kids.push(text_node(
            cmd,
            "The roll is being read...".into(),
            style::small_font(),
            style::DIM,
        ));
    }
    for card in &snap.witnesses {
        kids.push(witness_panel(cmd, card));
    }
    kids.push(text_node(
        cmd,
        "They kept the faith without sword or rite; the Order remembers.".into(),
        style::small_font(),
        style::DIM,
    ));
    kids.push(text_node(
        cmd,
        "- The Manuscript of Sins -".into(),
        style::small_font(),
        style::GOLD_DIM,
    ));
    if snap.sins.is_empty() {
        // The phase guarantees the seven rows; stay defensive, never panic.
        kids.push(text_node(
            cmd,
            "The manuscript is being read...".into(),
            style::small_font(),
            style::DIM,
        ));
    } else {
        kids.push(manuscript_panel(cmd, &snap.sins, assets));
    }
    kids.push(text_node(
        cmd,
        "- The Roll of Worthy Adversaries -".into(),
        style::small_font(),
        style::GOLD_DIM,
    ));
    if snap.foes.is_empty() {
        // The phase guarantees the fourteen rows; stay defensive.
        kids.push(text_node(
            cmd,
            "The roll is being read...".into(),
            style::small_font(),
            style::DIM,
        ));
    } else {
        kids.push(adversary_roll_panel(cmd, &snap.foes));
    }
    kids.push(text_node(
        cmd,
        "- Testimonies of the Baptized -".into(),
        style::small_font(),
        style::GOLD_DIM,
    ));
    if snap.testimonies.is_empty() {
        kids.push(text_node(
            cmd,
            "The testimonies are being read...".into(),
            style::small_font(),
            style::DIM,
        ));
    } else {
        kids.push(testimony_panel(cmd, &snap.testimonies));
    }
    kids
}

/// One witness entry: a gold-bordered panel with the name and epithet in
/// gold over the vignette in bone, wrapped to a measured line so the long
/// memorial reading stays legible.
fn witness_panel(cmd: &mut Commands, card: &WitnessCard) -> Entity {
    let heading = text_node(
        cmd,
        format!("{}, {}", card.name, card.epithet),
        style::body_font(),
        style::GOLD,
    );
    // Text entities take a Node too: bounding the width is what makes the
    // vignette wrap (the default linebreak mode is word-boundary).
    let body = cmd
        .spawn((
            Text::new(card.vignette.clone()),
            style::small_font(),
            TextColor(style::BONE),
            Node {
                max_width: Val::Px(720.0),
                ..default()
            },
        ))
        .id();
    cmd.spawn((
        Node {
            max_width: Val::Px(760.0),
            row_gap: Val::Px(10.0),
            ..style::panel_node()
        },
        BackgroundColor(style::PANEL),
        BorderColor::all(style::GOLD_DIM),
    ))
    .add_children(&[heading, body])
    .id()
}

/// The Manuscript of Sins as ONE register panel holding the seven sin-boss
/// rows: the book is a single artifact, so the rows share one gold-dim
/// border rather than seven. Row gating is doubled here even though the
/// client already strips sealed rows' text: sin and lore render only for
/// `defeated` cards, so a mis-built snapshot cannot leak the allegories.
fn manuscript_panel(cmd: &mut Commands, sins: &[SinCard], assets: Option<&AssetServer>) -> Entity {
    let rows: Vec<Entity> = sins
        .iter()
        .map(|card| sin_row(cmd, card, assets))
        .collect();
    // A touch tighter than the stock panel recipe (row_gap 8 -> 4, vertical
    // padding 14 -> 8): seven icon rows must still close their border
    // inside a 720 window.
    cmd.spawn((
        Node {
            max_width: Val::Px(760.0),
            row_gap: Val::Px(4.0),
            padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
            ..style::panel_node()
        },
        BackgroundColor(style::PANEL),
        BorderColor::all(style::GOLD_DIM),
    ))
    .add_children(&rows)
    .id()
}

/// One sin-boss row. Revealed: a two-line entry — 32px icon with "name,
/// Sin of X" in gold, then the lore reading in bone (wrapped to the same
/// measure as the witness vignettes). Sealed: one dimmed line, icon tinted
/// down to match — "{name} - Unfaced"; the dignity of "not yet witnessed",
/// with neither sin nor lore shown.
fn sin_row(cmd: &mut Commands, card: &SinCard, assets: Option<&AssetServer>) -> Entity {
    if card.defeated {
        let mut head_kids: Vec<Entity> = Vec::new();
        head_kids.extend(sin_icon(cmd, card.species_id, true, assets));
        head_kids.push(text_node(
            cmd,
            format!("{}, Sin of {}", card.name, card.sin),
            style::body_font(),
            style::GOLD,
        ));
        let heading = cmd
            .spawn(Node {
                align_items: AlignItems::Center,
                column_gap: Val::Px(10.0),
                ..default()
            })
            .add_children(&head_kids)
            .id();
        let lore = cmd
            .spawn((
                Text::new(card.lore.clone()),
                style::small_font(),
                TextColor(style::BONE),
                Node {
                    max_width: Val::Px(700.0),
                    ..default()
                },
            ))
            .id();
        return cmd
            .spawn(Node {
                row_gap: Val::Px(4.0),
                flex_direction: FlexDirection::Column,
                ..default()
            })
            .add_children(&[heading, lore])
            .id();
    }
    let mut kids: Vec<Entity> = Vec::new();
    kids.extend(sin_icon(cmd, card.species_id, false, assets));
    kids.push(text_node(
        cmd,
        format!("{} - Unfaced", card.name),
        style::body_font(),
        style::DIM,
    ));
    cmd.spawn(Node {
        align_items: AlignItems::Center,
        column_gap: Val::Px(10.0),
        ..default()
    })
    .add_children(&kids)
    .id()
}

/// The boss's 32x32 icon in the manuscript — the same
/// `sprites/icons/{id:03}.png` seam as [`champion_icon`] (D2): spawned
/// only when an asset server exists AND the id is a static-table species;
/// a 404ing fetch draws nothing over the row's unconditional text. Sealed
/// entries tint the icon down to the row's dimmed tone.
fn sin_icon(
    cmd: &mut Commands,
    species_id: u16,
    revealed: bool,
    assets: Option<&AssetServer>,
) -> Option<Entity> {
    data::species_by_id(species_id)?;
    let assets = assets?;
    let mut image = ImageNode::new(assets.load(format!("sprites/icons/{species_id:03}.png")));
    if !revealed {
        image.color = Color::srgb(0.30, 0.30, 0.38);
    }
    Some(
        cmd.spawn((
            image,
            Node {
                width: Val::Px(32.0),
                height: Val::Px(32.0),
                ..default()
            },
        ))
        .id(),
    )
}

/// The menu's Hagiography button, same look as every [`action_button`].
/// Wired into `menu_screen` between the alms address line and the
/// "Tournament of Virtue" button.
pub fn hagiography_menu_button(cmd: &mut Commands) -> Entity {
    action_button(cmd, "Hagiography".into(), ClientInput::ViewHagiography)
}

/// The Roll of Worthy Adversaries as ONE register panel holding the
/// fourteen rows (ten archetypes, then the four named commanders): the
/// book is a single artifact, same as the Manuscript. Row gating is
/// doubled here as with [`manuscript_panel`]: lore renders only for
/// `defeated` cards, so a mis-built snapshot cannot leak the readings.
fn adversary_roll_panel(cmd: &mut Commands, foes: &[FoeCard]) -> Entity {
    let rows: Vec<Entity> = foes.iter().map(|card| adversary_row(cmd, card)).collect();
    cmd.spawn((
        Node {
            max_width: Val::Px(760.0),
            row_gap: Val::Px(4.0),
            padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
            ..style::panel_node()
        },
        BackgroundColor(style::PANEL),
        BorderColor::all(style::GOLD_DIM),
    ))
    .add_children(&rows)
    .id()
}

/// One adversary row. Revealed: name in gold over the chronicle reading in
/// bone. Sealed: one dimmed line — "{name} - Unmet"; the man is not yet
/// witnessed, and no lore is shown.
fn adversary_row(cmd: &mut Commands, card: &FoeCard) -> Entity {
    if card.defeated {
        let heading = text_node(cmd, card.name.clone(), style::body_font(), style::GOLD);
        let lore = cmd
            .spawn((
                Text::new(card.lore.clone()),
                style::small_font(),
                TextColor(style::BONE),
                Node {
                    max_width: Val::Px(700.0),
                    ..default()
                },
            ))
            .id();
        return cmd
            .spawn(Node {
                row_gap: Val::Px(4.0),
                flex_direction: FlexDirection::Column,
                ..default()
            })
            .add_children(&[heading, lore])
            .id();
    }
    text_node(
        cmd,
        format!("{} - Unmet", card.name),
        style::body_font(),
        style::DIM,
    )
}

/// The Testimonies of the Baptized as ONE register panel: ten rows, one
/// per convertible archetype. Gating doubled as ever: testimony renders
/// only for `baptized` cards.
fn testimony_panel(cmd: &mut Commands, rows: &[TestimonyCard]) -> Entity {
    let rows: Vec<Entity> = rows.iter().map(|card| testimony_row(cmd, card)).collect();
    cmd.spawn((
        Node {
            max_width: Val::Px(760.0),
            row_gap: Val::Px(6.0),
            padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
            ..style::panel_node()
        },
        BackgroundColor(style::PANEL),
        BorderColor::all(style::GOLD_DIM),
    ))
    .add_children(&rows)
    .id()
}

/// One testimony row. Baptized: the office in gold over the man's own
/// account in bone. Sealed: one dimmed line — "{name} - Not yet called".
fn testimony_row(cmd: &mut Commands, card: &TestimonyCard) -> Entity {
    if card.baptized {
        let heading = text_node(
            cmd,
            format!("{}, baptized into Christ", card.name),
            style::body_font(),
            style::GOLD,
        );
        let body = cmd
            .spawn((
                Text::new(card.testimony.clone()),
                style::small_font(),
                TextColor(style::BONE),
                Node {
                    max_width: Val::Px(700.0),
                    ..default()
                },
            ))
            .id();
        return cmd
            .spawn(Node {
                row_gap: Val::Px(4.0),
                flex_direction: FlexDirection::Column,
                ..default()
            })
            .add_children(&[heading, body])
            .id();
    }
    text_node(
        cmd,
        format!("{} - Not yet called", card.name),
        style::body_font(),
        style::DIM,
    )
}

/// The post-battle Call to Repentance screen (M7b, docs/CONVERSION.md): a
/// yielded adversary stands disarmed; the player owes one choice per
/// pending call. No battle chrome — the duel is over; this is the counsel
/// after, centered like the other interstitial screens.
fn conversion_screen(cmd: &mut Commands, snap: &ViewSnapshot) -> Vec<Entity> {
    let mut kids = vec![text_node(
        cmd,
        "A Foe Yields".into(),
        style::head_font(),
        style::GOLD,
    )];
    match snap.conversion.as_ref() {
        Some(call) => {
            kids.push(text_node(
                cmd,
                format!(
                    "The {} (level {}) stands disarmed and breathing. The Call may be made once: to repent and be baptized into Christ, or to be released with his soul commended unto God.",
                    call.species_name, call.foe_level
                ),
                style::body_font(),
                style::BONE,
            ));
            kids.push(text_node(
                cmd,
                format!(
                    "The call's promise: {} in a hundred ({} call(s) remain).",
                    call.chance_pct, call.pending
                ),
                style::small_font(),
                style::DIM,
            ));
            kids.push(action_button(
                cmd,
                "Call to Repentance".into(),
                ClientInput::CallToRepentance,
            ));
            kids.push(action_button(
                cmd,
                "Release his soul".into(),
                ClientInput::ReleaseSoul,
            ));
        }
        // The phase guarantees a pending call; stay defensive, never panic.
        None => kids.push(text_node(
            cmd,
            "The field is quiet.".into(),
            style::small_font(),
            style::DIM,
        )),
    }
    kids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crusade_core::types::Typing;

    /// The gauntlet table (hoisted to the client as the single source)
    /// must be the seven distinct sin-spawn
    /// species from the static table -- the anchor that pins the imported
    /// `client.rs::CAMPAIGN_NODE_SPECIES` to the staged files from
    /// stage_sprites.py. (D1/D2 keep their sprite tests in battle.rs.)
    #[test]
    fn gauntlet_mirror_covers_each_sin_boss_once() {
        let mut ids = CAMPAIGN_NODE_SPECIES.to_vec();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 7, "seven distinct gauntlet nodes");
        for id in ids {
            let sp = data::species_by_id(id).expect("node id in the static table");
            assert!(
                matches!(sp.typing, Typing::Sin(_)),
                "node species {id} ({}) must be a sin-spawn",
                sp.name
            );
        }
    }

    /// A cleared gauntlet (node 7) offers no next boss: the rest screen
    /// then shows neither face nor caption.
    #[test]
    fn cleared_gauntlet_offers_no_boss() {
        assert!(next_boss_species(7).is_none());
        assert!(next_boss_species(200).is_none());
    }

    /// The mirror must resolve to exactly the staged file paths
    /// (`tools/artbake/stage_sprites.py` SUBJECTS), in gauntlet order.
    #[test]
    fn gauntlet_paths_match_staged_slugs() {
        let expected = [
            "sprites/champions/103-fiend-of-lust.png",
            "sprites/champions/106-glutton-maw.png",
            "sprites/champions/107-grasper-of-greed.png",
            "sprites/champions/104-sleeper-of-sloth.png",
            "sprites/champions/102-wretch-of-envy.png",
            "sprites/champions/105-brute-of-wrath.png",
            "sprites/champions/101-prince-of-pride.png",
        ];
        for (node, want) in expected.iter().enumerate() {
            let sp = next_boss_species(node as u8).expect("gauntlet node has a boss");
            assert_eq!(boss_sprite_path(sp), *want, "node {node}");
        }
    }

    // ---------- Roster gallery ----------

    /// A ChampionCard mirroring the recipe `client.rs::roster_cards` runs
    /// on a stored instance (zeroed devotion/grace displays, typing in the
    /// client's join style). Purely a fixture: the composer reads
    /// `species_id` only, but its input must be a REAL card shape.
    fn mirror_gallery_card(species_id: u16) -> ChampionCard {
        let sp = data::species_by_id(species_id).expect("species in the static table");
        let types_display = match sp.typing {
            Typing::Virtues([a, b]) => {
                let mut v = Vec::new();
                if let Some(a) = a {
                    v.push(virtue_name(a).to_string());
                }
                if let Some(b) = b {
                    v.push(virtue_name(b).to_string());
                }
                vec![v.join("+")]
            }
            Typing::Sin(s) => vec![format!("Sin of {}", sin_name(s))],
            Typing::Valor => vec!["Valor".to_string()],
        };
        ChampionCard {
            id: format!("mirror-{species_id}"),
            species_id,
            species_name: sp.name.to_string(),
            level: 25,
            current_resolve: 80,
            max_resolve: 80,
            ailment: None,
            types_display,
            rites: Vec::new(),
            devotion_display: "R0 V0 P0 G0 Ae0 Al0".to_string(),
            grace_display: "R0 V0 P0 G0 Ae0 Al0".to_string(),
            variant: 0,
        }
    }

    /// The virtue label's gauntlet matchup tip, composed from a mirror
    /// roster card: exact text for a dual-virtue champion (Raimbaud's
    /// Patience+Kindness) and a single-virtue one (Audric's Humility);
    /// every champion is terse ASCII; a sin-spawn id and an unknown id
    /// answer None (never mustered, never tipped).
    #[test]
    fn virtue_matchup_tip_names_the_gauntlet_bosses() {
        let raimbaud = mirror_gallery_card(10);
        assert_eq!(raimbaud.types_display, vec!["Patience+Kindness".to_string()]);
        let tip = virtue_matchup_tip(raimbaud.species_id).expect("a champion has virtues");
        assert_eq!(
            tip.as_ref(),
            "In the gauntlet: Patience conquers Wrath (Brute of Wrath); \
             Kindness conquers Greed (Grasper of Greed)."
        );
        assert!(tip.is_ascii());
        assert!(tip.len() <= battle::TIP_LEN_BUDGET);

        let audric = mirror_gallery_card(1);
        let tip = virtue_matchup_tip(audric.species_id).expect("Audric");
        assert_eq!(
            tip.as_ref(),
            "In the gauntlet: Humility conquers Pride (Prince of Pride)."
        );

        for id in 1..=12u16 {
            let card = mirror_gallery_card(id);
            let tip = virtue_matchup_tip(card.species_id).expect("champion virtues");
            assert!(tip.is_ascii(), "{id}");
            assert!(
                tip.len() <= battle::TIP_LEN_BUDGET,
                "{id} over budget: {tip}"
            );
        }
        assert!(virtue_matchup_tip(101).is_none(), "sin-spawn never muster");
        assert!(virtue_matchup_tip(999).is_none(), "unknown ids stay untipped");
    }

    /// The roster's tooltip consts hold the house contract (ASCII, two
    /// lines), and the plate lore gate mirrors the battle portrait's:
    /// champions quote their table lore; a boss's stays Manuscript-sealed.
    #[test]
    fn roster_tips_hold_the_contract() {
        for tip in [DEVOTION_TIP, GRACE_TIP, VESTURE_TIP] {
            assert!(tip.is_ascii(), "{tip:?}");
            assert!(!tip.is_empty(), "wired tips are never hollow");
            assert!(tip.len() <= battle::TIP_LEN_BUDGET, "{tip:?}");
        }
        let audric = data::species_by_id(1).expect("Audric");
        assert_eq!(plate_lore(audric), Some(audric.lore));
        let pride = data::species_by_id(101).expect("Pride");
        assert_eq!(plate_lore(pride), None);
        // And every champion plate tip itself fits the box.
        for id in 1..=12u16 {
            let sp = data::species_by_id(id).expect("champion species");
            let tip = plate_lore(sp).expect("champion lore");
            assert!(tip.len() <= battle::TIP_LEN_BUDGET, "{}: {tip}", sp.name);
        }
    }

    /// The gallery plate recipe must resolve to exactly the staged files
    /// (`tools/artbake/stage_sprites.py` gallery pass: one un-quantized
    /// 512px plate per static-table species), in species-table order.
    #[test]
    fn gallery_paths_match_staged_slugs() {
        let expected = [
            "sprites/gallery/001-squire-audric.png",
            "sprites/gallery/002-cleric-mabel.png",
            "sprites/gallery/003-serjeant-bastien.png",
            "sprites/gallery/004-hospitaller-ysolt.png",
            "sprites/gallery/005-wayfarer-aelis.png",
            "sprites/gallery/006-canoness-hrothwyn.png",
            "sprites/gallery/007-vexillor-anchetil.png",
            "sprites/gallery/008-brother-aldwyn.png",
            "sprites/gallery/009-cantor-sabina.png",
            "sprites/gallery/010-chevalier-raimbaud.png",
            "sprites/gallery/011-vigil-knight-wymar.png",
            "sprites/gallery/012-friar-osbert.png",
            "sprites/gallery/101-prince-of-pride.png",
            "sprites/gallery/102-wretch-of-envy.png",
            "sprites/gallery/103-fiend-of-lust.png",
            "sprites/gallery/104-sleeper-of-sloth.png",
            "sprites/gallery/105-brute-of-wrath.png",
            "sprites/gallery/106-glutton-maw.png",
            "sprites/gallery/107-grasper-of-greed.png",
        ];
        assert_eq!(expected.len(), data::all_species().len());
        for (sp, want) in data::all_species().iter().zip(expected.iter()) {
            assert_eq!(gallery_sprite_path(sp), *want, "species {}", sp.id);
        }
    }

    /// The vesture-plate ring, pinned against the staged `--alt-{a,b}`
    /// files (E5 wardrobe reads, 512px gallery class): picks 1/2 swap the
    /// plate file; 0, out-of-ring values, and every sin-spawn id read the
    /// base plate (alts exist for the twelve champions only). The chip is
    /// an absolute overlay, so the card geometry contract keeps the E1
    /// numbers exactly (plate + stats + chrome = CARD_W).
    #[test]
    fn gallery_plate_paths_follow_the_vesture_ring() {
        let bastien = data::species_by_id(3).expect("Bastien");
        assert_eq!(
            gallery_plate_path(bastien, 0),
            "sprites/gallery/003-serjeant-bastien.png"
        );
        assert_eq!(
            gallery_plate_path(bastien, 1),
            "sprites/gallery/003-serjeant-bastien--alt-a.png"
        );
        assert_eq!(
            gallery_plate_path(bastien, 2),
            "sprites/gallery/003-serjeant-bastien--alt-b.png"
        );
        assert_eq!(
            gallery_plate_path(bastien, 9),
            "sprites/gallery/003-serjeant-bastien.png",
            "out-of-ring values clamp to the base plate"
        );
        let pride = data::species_by_id(101).expect("Pride");
        for variant in 0..=2u8 {
            assert_eq!(
                gallery_plate_path(pride, variant),
                "sprites/gallery/101-prince-of-pride.png",
                "sin-spawn keep one vesture (variant {variant})"
            );
        }
        // Every champion's ring resolves to its own slugged alt pair.
        for id in 1..=12u16 {
            let sp = data::species_by_id(id).expect("champion species");
            let base = gallery_sprite_path(sp);
            assert_eq!(
                gallery_plate_path(sp, 1),
                format!("{}--alt-a.png", base.trim_end_matches(".png"))
            );
        }
        assert_eq!(
            PLATE + CARD_TEXT_W + 10.0 + 2.0 + 16.0,
            CARD_W,
            "E1 card geometry holds: plate + gap + stats fill the card exactly"
        );
    }

    /// Pager math: six-card leaves; the window clamps an out-of-range page
    /// back onto the last leaf, and an empty company reads one leaf of
    /// nothing (never a page 0-of-0).
    #[test]
    fn roster_paging_windows_are_six_card_leaves() {
        assert_eq!(roster_page_window(0, 0), (0, 1, 0, 0));
        assert_eq!(roster_page_window(6, 0), (0, 1, 0, 6));
        assert_eq!(roster_page_window(7, 0), (0, 2, 0, 6));
        assert_eq!(roster_page_window(7, 1), (1, 2, 6, 7));
        // The design case: a full twenty-card company, four leaves.
        assert_eq!(roster_page_window(20, 0), (0, 4, 0, 6));
        assert_eq!(roster_page_window(20, 3), (3, 4, 18, 20));
        // Stale/overshooting leaf requests clamp onto the last leaf.
        assert_eq!(roster_page_window(20, 9), (3, 4, 18, 20));
        assert_eq!(roster_page_window(3, 4), (0, 1, 0, 3));
    }

    // ---------- Support-panel codes & invoice parsing ----------

    /// The menu's `lightning:` URI QR, pinned exactly: 32 bytes of
    /// byte-mode payload at EC level M encode into a version-3 matrix,
    /// 29 modules wide (26-M bytes is too small, 42-M suffices) -- the
    /// known-input module count behind the panel's generous 6 px lane.
    /// The finder assertions pin ISO/IEC 18004 invariants: a 7x7 dark
    /// ring / light moat / dark 3x3 eye at three corners, true of every
    /// QR code regardless of version or mask choice.
    #[test]
    fn support_uri_qr_is_a_version3_matrix() {
        let uri = format!("lightning:{SUPPORT_LIGHTNING_ADDRESS}");
        let (width, dark) = qr_matrix(uri.as_bytes()).expect("the URI must fit a QR matrix");
        assert_eq!(width, 29, "expected a version-3 (29x29) matrix");
        assert_eq!(dark.len(), width * width);
        let at = |x: usize, y: usize| dark[y * width + x];
        for (cx, cy) in [(0, 0), (width - 7, 0), (0, width - 7)] {
            for i in 0..7 {
                assert!(at(cx + i, cy), "finder ring, corner ({cx},{cy})");
                assert!(at(cx + i, cy + 6), "finder ring, corner ({cx},{cy})");
                assert!(at(cx, cy + i), "finder ring, corner ({cx},{cy})");
                assert!(at(cx + 6, cy + i), "finder ring, corner ({cx},{cy})");
            }
            for i in 1..6 {
                assert!(!at(cx + i, cy + 1), "finder moat, corner ({cx},{cy})");
                assert!(!at(cx + i, cy + 5), "finder moat, corner ({cx},{cy})");
                assert!(!at(cx + 1, cy + i), "finder moat, corner ({cx},{cy})");
                assert!(!at(cx + 5, cy + i), "finder moat, corner ({cx},{cy})");
            }
            for dy in 2..5 {
                for dx in 2..5 {
                    assert!(at(cx + dx, cy + dy), "finder eye, corner ({cx},{cy})");
                }
            }
        }
        // The scannability margin: ISO/IEC 18004 requires a four-module
        // quiet zone, which the white pad renders exactly.
        assert_eq!(QR_QUIET_MODULES, 4, "quiet zone must stay the ISO 4 modules");
    }

    /// The invoice lane's size policy, pinned: a too-long bolt11 (600
    /// bytes needs more than a version-15 matrix at EC level M) exceeds
    /// the panel cap, which is exactly what sends it text-only -- and a
    /// short chip-amount invoice stays inside.
    #[test]
    fn invoice_qr_size_policy_is_pinned() {
        let oversized = [b'x'; 600];
        let (width, _) = qr_matrix(&oversized).expect("600 bytes still fit a QR");
        assert!(
            width > QR_INVOICE_MAX_MODULES,
            "a version-15+ ({width} modules) matrix must cap out of the QR lane"
        );
        let short = [b'l'; 200];
        let (width, _) = qr_matrix(&short).expect("200 bytes fit");
        assert!(width <= QR_INVOICE_MAX_MODULES, "a short invoice stays in-lane");
    }

    /// The invoice callback's contract, pinned offline against fixed
    /// bodies: happy path, LNURL ERROR, and the bad shapes. (The fetch
    /// producing these bodies is browser `fetch` under main.rs's
    /// redirect-refusing BrowserHttp, and step 1's validation is
    /// crusade-net's already mock-tested `fetch_pay_params`.)
    #[test]
    fn support_invoice_parsing_offline_fixtures() {
        let pr = parse_support_invoice(r#"{"pr": "lnbc210n1pjexample"}"#).expect("fixture parses");
        assert_eq!(pr, "lnbc210n1pjexample");
        let err = parse_support_invoice(r#"{"status":"ERROR","reason":"amount too small"}"#)
            .unwrap_err();
        assert!(matches!(err, ZapError::Lnurl(ref r) if r == "amount too small"));
        for bad in [
            r#"{"pr": 42}"#,                    // wrong type
            r#"{"successAction":{}}"#,          // missing pr
            r#"{"pr": "not-a-bolt11-string"}"#, // wrong prefix
            "certainly not json",
        ] {
            assert!(
                matches!(parse_support_invoice(bad), Err(ZapError::BadInvoice(_))),
                "{bad} must be BadInvoice"
            );
        }
    }
}

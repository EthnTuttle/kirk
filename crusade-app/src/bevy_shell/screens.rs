//! The rebuild dispatch system, shared node builders, and the non-battle
//! screens (menu, roster, campaign rest, battle over).

use bevy::prelude::*;

use crusade_app::{ladder_line, ChampionCard, ClientInput, LobbyStage, Phase, ViewSnapshot};
use crusade_core::data;

use crate::battle;
use crate::state::{
    ActionButton, AlmsInvoice, AlmsPurse, AlmsState, AttestationPost, BaseBackground, GameShell,
    NetBridge, NetCmd, NetDuelEnd, RenderedVersion, SnapshotVersion, UiRoot,
};
use crate::style;

/// "Support the Order" alms box (ZAP-MONETIZATION.md surface #1): a static,
/// voluntary-donation lightning address. Purely cosmetic — the menu shows it
/// and hands it to the player's own wallet on click; it is never an input to
/// the game and no game system reads it back (integrity firewall, §2).
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
                    net_over_screen(&mut commands, &duel, won, alms.as_ref()),
                    true,
                )
            }
            Phase::ForcedSwitchSelect => {
                (battle::forced_switch_screen(&mut commands, &duel, assets), false)
            }
            _ => (battle::battle_screen(&mut commands, &duel, assets), false),
        },
        _ => match snap.phase {
            Phase::Menu => (menu_screen(&mut commands, ladder), true),
            Phase::Roster => (roster_screen(&mut commands, &roster, assets), false),
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
            Phase::CampaignRest => (rest_screen(&mut commands, &snap), true),
            Phase::BattleOver => (over_screen(&mut commands, &snap), true),
            Phase::Lobby => (lobby_screen(&mut commands, &snap, ladder), false),
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

fn menu_screen(cmd: &mut Commands, ladder: Option<u64>) -> Vec<Entity> {
    vec![
        text_node(cmd, "DEUS VULT".into(), style::title_font(), style::GOLD),
        text_node(
            cmd,
            "- Champions of the Cross -".into(),
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
        // The alms box (card A3, ZAP-MONETIZATION.md surface #1) sits
        // directly under Campaign: button, reverent helper line, then the
        // address itself in plain text so any platform can copy it into a
        // wallet by hand.
        support_button(cmd),
        text_node(
            cmd,
            "Alms freely given keep the torches lit.".into(),
            style::small_font(),
            style::DIM,
        ),
        text_node(
            cmd,
            format!("alms for the developers: {SUPPORT_LIGHTNING_ADDRESS}"),
            style::small_font(),
            style::DIM,
        ),
        action_button(cmd, "Tournament of Virtue".into(), ClientInput::EnterLobby),
        text_node(cmd, ladder_line(ladder), style::small_font(), style::DIM),
        text_node(
            cmd,
            "Contrition and cold steel.".into(),
            style::small_font(),
            style::DIM,
        ),
    ]
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
/// The address is always visible below the button, so on any platform the
/// player can type it into their own wallet by hand. Native echoes it once
/// to stderr; wasm also asks the browser to hand `lightning:{addr}` to a
/// wallet in a new tab.
fn on_support_click(_click: On<Pointer<Click>>) {
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("Support the Order - voluntary alms for the developers: {SUPPORT_LIGHTNING_ADDRESS}");
    #[cfg(target_arch = "wasm32")]
    open_lightning_uri();
}

/// Browser-only hop to the player's own wallet via the `lightning:` URI
/// scheme, mirroring the `persist.rs` wasm/native split (no new deps; popup
/// failures degrade to the always-visible address line in the menu).
#[cfg(target_arch = "wasm32")]
fn open_lightning_uri() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let uri = format!("lightning:{SUPPORT_LIGHTNING_ADDRESS}");
    if let Err(err) = window.open_with_url_and_target(&uri, "_blank") {
        warn!("could not open lightning URI ({err:?}); address is shown in the menu");
    }
}

// ---------- Roster ----------

fn roster_screen(
    cmd: &mut Commands,
    cards: &[ChampionCard],
    assets: Option<&AssetServer>,
) -> Vec<Entity> {
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
    let card_entities: Vec<Entity> = cards
        .iter()
        .map(|c| champion_card(cmd, c, assets))
        .collect();
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

fn champion_card(cmd: &mut Commands, card: &ChampionCard, assets: Option<&AssetServer>) -> Entity {
    let mut kids = Vec::new();
    if let Some(icon) = champion_icon(cmd, card.species_id, assets) {
        kids.push(icon);
    }
    kids.push(text_node(
        cmd,
        format!("{} - Lv {}", card.species_name, card.level),
        style::body_font(),
        style::BONE,
    ));
    kids.push(text_node(
        cmd,
        format!("resolve {}/{}", card.current_resolve, card.max_resolve),
        style::small_font(),
        style::GOLD,
    ));
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

/// The 32x32 roster icon at the top of a champion card — the section 2
/// seam-2 `ImageNode`, keyed by the card's own `species_id`
/// (`sprites/icons/{id:03}.png`, auto-derived from the battle sprite by
/// `tools/artbake/stage_sprites.py`). Spawned only when an asset server
/// exists AND the id is a static-table species (whose icon the art lane
/// always ships); a 404ing icon (partial wasm deploy) draws nothing over
/// the card's unconditional text rows.
fn champion_icon(cmd: &mut Commands, species_id: u16, assets: Option<&AssetServer>) -> Option<Entity> {
    data::species_by_id(species_id)?;
    let assets = assets?;
    Some(
        cmd.spawn((
            ImageNode::new(assets.load(format!("sprites/icons/{species_id:03}.png"))),
            Node {
                width: Val::Px(32.0),
                height: Val::Px(32.0),
                ..default()
            },
        ))
        .id(),
    )
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
                "Bosses defeated: {}/7 - contrition shards earned: {}",
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
        Some(0) => ("Victory - Deo Gratias", style::GOLD),
        Some(_) => ("Defeat - Pray and Regroup", style::HP_RED),
        None => ("The field falls quiet.", style::BONE),
    };
    let mut kids = vec![text_node(cmd, banner.into(), style::title_font(), color)];
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
        log_panel(cmd, "Last rites of the duel", &snap.log_tail),
    ];
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

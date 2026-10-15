//! The rebuild dispatch system, shared node builders, and the non-battle
//! screens (menu, roster, campaign rest, battle over).

use bevy::prelude::*;

use crusade_app::{
    ladder_line, ChampionCard, ClientInput, LobbyStage, Phase, ViewSnapshot, WitnessCard,
};
use crusade_core::data;
use crusade_core::types::SpeciesBase;

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
            Phase::Menu => (menu_screen(&mut commands, ladder, assets), true),
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
            Phase::CampaignRest => (rest_screen(&mut commands, &snap, assets), true),
            Phase::BattleOver => (over_screen(&mut commands, &snap), true),
            Phase::Lobby => (lobby_screen(&mut commands, &snap, ladder), false),
            Phase::Hagiography => (hagiography_screen(&mut commands, &snap), false),
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

fn menu_screen(cmd: &mut Commands, ladder: Option<u64>, assets: Option<&AssetServer>) -> Vec<Entity> {
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

/// MIRROR of `client.rs::CAMPAIGN_NODE_SPECIES` (the seven-node gauntlet
/// order: Lust, Gluttony, Greed, Sloth, Envy, Wrath, Pride last): the
/// client's const is private, and `CampaignView.node` is the index the
/// snapshot already carries, so the shell re-derives the next boss's
/// species id here rather than touching the view plumbing -- the same
/// shell-side reconstruction discipline as `battle.rs::champion_sprite_path`.
/// Presentation-only: the order is locked design, the tests below pin it to
/// the static species table, and if this copy ever did drift from the
/// client's, the worst case is a wrong portrait on this screen, never wrong
/// mechanics.
const CAMPAIGN_NODE_SPECIES: [u16; 7] = [103, 106, 107, 104, 102, 105, 101];

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

// ---------- Hagiography (witness roll) ----------
//
// The memorial roll of the revered non-combatant dead: pure lore, drawn
// from `crusade_core::data::all_witnesses()` through
// `ViewSnapshot::witnesses`. The only button is the way back — nothing on
// this screen is an input to any game system.

/// The Hagiography screen: a single-column memorial scroll. One panel per
/// witness — "name, epithet" as the heading over the vignette reading —
/// with generous room around each, flowing from the top like the roster so
/// the roll may lengthen without crowding.
fn hagiography_screen(cmd: &mut Commands, snap: &ViewSnapshot) -> Vec<Entity> {
    let mut kids = vec![
        text_node(cmd, "Hagiography".into(), style::head_font(), style::GOLD),
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
    kids.push(action_button(cmd, "Back".into(), ClientInput::CloseToMenu));
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

/// The menu's Hagiography button, same look as every [`action_button`].
/// Wired into `menu_screen` between the alms address line and the
/// "Tournament of Virtue" button.
pub fn hagiography_menu_button(cmd: &mut Commands) -> Entity {
    action_button(cmd, "Hagiography".into(), ClientInput::ViewHagiography)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crusade_core::types::Typing;

    /// The mirrored gauntlet table must be the seven distinct sin-spawn
    /// species from the static table -- the anchor that glues the mirror to
    /// `client.rs::CAMPAIGN_NODE_SPECIES` and to the staged files from
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
}

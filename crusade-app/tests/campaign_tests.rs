//! End-to-end tests for the campaign (PvE gauntlet) slice. The UI loop is
//! simulated with ClientGame inputs only: fight (greedy max-power rite) until
//! CampaignRest, continue, and so on until BattleOver.
//!
//! Fixtures follow the mkchamp pattern from crusade-core's engine tests:
//! learnset-legal rites, zero grace/devotion, full zeal, resolve at the
//! computed maximum. The gauntlet is deterministic (per-node fixed seeds);
//! fixtures are over-leveled so any roster id ordering clears it.

use crusade_app::{AppError, ClientGame, ClientInput, Phase, ViewSnapshot};
use crusade_core::data;
use crusade_core::types::{ChampionInstance, DevotionStats, GraceStats, RiteSlot, MAX_LEVEL};
use crusade_data::RosterStore;

fn mkchamp(species_id: u16, level: u8, rite_ids: &[u16]) -> ChampionInstance {
    let sp = data::species_by_id(species_id).expect("fixture species");
    let mut rites: [Option<RiteSlot>; 4] = [None; 4];
    for (i, id) in rite_ids.iter().enumerate() {
        let r = data::rite_by_id(*id).expect("fixture rite");
        assert!(
            sp.learnset
                .iter()
                .any(|e| e.rite_id == *id && e.level <= level),
            "fixture rite {id} not learnable by species {species_id} at {level}"
        );
        rites[i] = Some(RiteSlot {
            rite_id: *id,
            zeal_left: r.zeal,
        });
    }
    let mut c = ChampionInstance {
        species_id,
        level,
        xp: 0,
        grace: GraceStats {
            resolve: 0,
            valour: 0,
            piety: 0,
            guard: 0,
            aegis: 0,
            alacrity: 0,
        },
        devotion: DevotionStats {
            resolve: 0,
            valour: 0,
            piety: 0,
            guard: 0,
            aegis: 0,
            alacrity: 0,
        },
        calling_id: 0,
        rites,
        current_resolve: 0,
        ailment: None,
        converted: false,
        converted_from: None,
        typing_override: None,
    };
    c.current_resolve = data::compute_stats(&c, sp).max_resolve;
    c
}

fn game_with_roster(champs: &[ChampionInstance]) -> ClientGame {
    let mut store = RosterStore::new();
    for c in champs {
        store.add_champion(c).unwrap();
    }
    ClientGame::new(store)
}

/// Six level-50 champions covering six of the seven virtues (Patience and
/// Kindness ride on Raimbaud): every sin boss meets at least one opposed
/// blade, and every slot carries a high-power damaging rite. All rites are
/// learnable well below 50.
fn strong_roster() -> Vec<ChampionInstance> {
    vec![
        mkchamp(10, 50, &[18, 25, 7, 1]), // Raimbaud: Patience 80p / Kindness 60p
        mkchamp(11, 50, &[16, 17, 22, 1]), // Wymar: Diligence 75p+65p / Temperance 65p
        mkchamp(6, 50, &[14, 13, 21, 12]), // Hrothwyn: Chastity 70p+55p (piety 90)
        mkchamp(4, 50, &[11, 4, 27, 1]),  // Ysolt: Charity 60p+45p drain
        mkchamp(5, 50, &[25, 8, 28, 23]), // Aelis: Kindness 60p / 50p multihit
        mkchamp(8, 50, &[3, 9, 2, 1]),    // Aldwyn: Humility 55p+50p
    ]
}

fn switch_idx(target: &str) -> u8 {
    target
        .split(':')
        .next()
        .expect("can_switch_to entries look like '<idx>:<name>'")
        .parse()
        .expect("party index prefix parses")
}

fn assert_illegal(res: Result<(), AppError>) {
    assert!(
        matches!(res, Err(AppError::IllegalTransition(_))),
        "expected IllegalTransition, got {res:?}"
    );
}

/// Highest-power usable damaging rite; ties resolve to the lowest slot.
fn best_attack(snap: &ViewSnapshot) -> Option<u8> {
    let mut best: Option<(u8, u8)> = None; // (slot, power), slots ascend
    for r in snap.my_rites.iter().filter(|r| r.usable && r.power > 0) {
        if best.is_none_or(|(_, p)| r.power > p) {
            best = Some((r.slot, r.power));
        }
    }
    best.map(|(slot, _)| slot)
}

/// One step of the campaign driver loop. Returns false once BattleOver.
fn drive_step(game: &mut ClientGame, snap: &ViewSnapshot) -> bool {
    match snap.phase {
        Phase::BattleOver => return false,
        Phase::InBattle => {
            let input = if let Some(slot) = best_attack(snap) {
                ClientInput::PickRite { slot }
            } else if let Some(t) = snap.can_switch_to.first() {
                ClientInput::PickSwitch {
                    party_idx: switch_idx(t),
                }
            } else {
                ClientInput::Concede // nothing to fight with: end the farce
            };
            game.handle(input).unwrap();
        }
        Phase::ForcedSwitchSelect => {
            let t = snap
                .can_switch_to
                .first()
                .expect("forced switch needs a target");
            let idx = switch_idx(t);
            game.handle(ClientInput::PickSwitch { party_idx: idx })
                .unwrap();
        }
        Phase::CampaignRest => game.handle(ClientInput::ContinueCampaign).unwrap(),
        other => panic!("unexpected phase {other:?} mid-campaign"),
    }
    true
}

#[test]
fn full_gauntlet_clear_victory() {
    let roster = strong_roster();
    let mut game = game_with_roster(&roster);
    assert_eq!(game.snapshot().phase, Phase::Menu);
    assert_eq!(game.snapshot().campaign, None);

    game.handle(ClientInput::StartCampaign).unwrap();

    let mut phases = vec![game.snapshot().phase];
    let mut rests_seen = 0u8;
    let mut turns = 0;
    loop {
        let snap = game.snapshot();
        // The campaign marker rides every snapshot of the run.
        let c = snap
            .campaign
            .expect("campaign view present while the run is live");
        assert!(c.node <= 7 && c.defeated <= 7);
        if snap.phase == Phase::CampaignRest {
            rests_seen += 1;
            // At rest the next node index equals the defeated tally, and the
            // shard total is the triangular sum of 1-based bounties.
            let d = u64::from(c.defeated);
            assert_eq!(c.node, c.defeated);
            assert!(matches!(c.node, 1..=6), "rest happens between nodes");
            assert_eq!(c.shards_earned, d * (d + 1) / 2);
            // Heal preview rides the log tail at rest.
            assert!(
                snap.log_tail
                    .iter()
                    .any(|l| l.contains("rests (resolve") || l.contains("contrition shard")),
                "rest snapshot carries the heal/bounty preview: {:?}",
                snap.log_tail
            );
        }
        if !drive_step(&mut game, &snap) {
            break;
        }
        let phase = game.snapshot().phase;
        if phases.last() != Some(&phase) {
            phases.push(phase);
        }
        turns += 1;
        assert!(turns < 2000, "campaign did not terminate");
    }

    let final_snap = game.snapshot();
    assert_eq!(final_snap.phase, Phase::BattleOver);
    assert_eq!(
        final_snap.battle_result,
        Some(0),
        "the gauntlet ends in victory"
    );
    let c = final_snap
        .campaign
        .expect("victory frame keeps campaign view");
    assert_eq!(c.node, 7, "all seven nodes answered");
    assert_eq!(c.defeated, 7, "seven bosses defeated");
    assert_eq!(c.shards_earned, 28, "1+2+..+7 shards");
    assert_eq!(rests_seen, 6, "six rests between seven nodes");
    // Phase trajectory: Menu -> InBattle -> (CampaignRest/InBattle)* -> BattleOver.
    assert_eq!(phases.first(), Some(&Phase::InBattle));
    assert!(phases.contains(&Phase::CampaignRest));
    assert_eq!(phases.last(), Some(&Phase::BattleOver));
    assert!(
        final_snap
            .log_tail
            .iter()
            .any(|l| l.contains("gauntlet is cleared")),
        "victory line in the log tail: {:?}",
        final_snap.log_tail
    );

    // The one sanctioned store write: contrition shards are credited.
    let inventory = game.store().inventory().unwrap();
    assert_eq!(
        inventory,
        vec![("contrition-shard".to_string(), 28u64)],
        "shard bounty credited to the store"
    );

    // Campaign clones took the wounds: the stored roster must be pristine.
    let listed = game.store().list_champions().unwrap();
    assert_eq!(listed.len(), roster.len());
    for (_, c) in &listed {
        let sp = data::species_by_id(c.species_id).unwrap();
        assert_eq!(
            c.current_resolve,
            data::compute_stats(c, sp).max_resolve,
            "store champion untouched by the gauntlet"
        );
        assert_eq!(c.level, 50, "no level drift / respec loss");
        assert_eq!(c.ailment, None);
    }

    game.handle(ClientInput::AcknowledgeResult).unwrap();
    let snap = game.snapshot();
    assert_eq!(snap.phase, Phase::Menu);
    assert_eq!(snap.campaign, None, "acknowledge closes the campaign view");
}

#[test]
fn party_wipe_at_first_node_ends_campaign_without_shards() {
    // One level-5 champion cannot dent three sin-spawn of levels 10/10/15.
    let mut game = game_with_roster(&[mkchamp(1, 5, &[1, 2])]);
    game.handle(ClientInput::StartCampaign).unwrap();

    let mut rests_seen = 0;
    let mut turns = 0;
    loop {
        let snap = game.snapshot();
        if snap.phase == Phase::CampaignRest {
            rests_seen += 1;
        }
        if !drive_step(&mut game, &snap) {
            break;
        }
        turns += 1;
        assert!(turns < 2000, "campaign did not terminate");
    }

    let snap = game.snapshot();
    assert_eq!(snap.phase, Phase::BattleOver);
    assert_eq!(snap.battle_result, Some(1), "the sin-spawn prevail");
    let c = snap.campaign.expect("loss frame keeps campaign view");
    assert_eq!((c.node, c.defeated, c.shards_earned), (0, 0, 0));
    assert_eq!(rests_seen, 0, "no rest without a cleared node");
    assert!(
        game.store().inventory().unwrap().is_empty(),
        "a wipe at node 1 pays no shards"
    );
    assert!(
        snap.log_tail.iter().any(|l| l.contains("campaign is over")),
        "loss line in the log tail: {:?}",
        snap.log_tail
    );
}

#[test]
fn abandon_mid_gauntlet_keeps_earned_shards_and_roster() {
    let roster = strong_roster();
    let mut game = game_with_roster(&roster);
    game.handle(ClientInput::StartCampaign).unwrap();

    // Drive to the first rest: node 1 bounty is in the store by then.
    let mut turns = 0;
    loop {
        let snap = game.snapshot();
        match snap.phase {
            Phase::CampaignRest => break,
            Phase::BattleOver => panic!("gauntlet ended before the first rest"),
            _ => {
                drive_step(&mut game, &snap);
            }
        }
        turns += 1;
        assert!(turns < 500, "first node did not resolve");
    }

    // From the rest: practice/campaign starts and (below) continue have rules.
    assert_illegal(game.handle(ClientInput::StartPractice { level: 40 }));
    assert_illegal(game.handle(ClientInput::StartCampaign));
    assert_eq!(
        game.store().inventory().unwrap(),
        vec![("contrition-shard".to_string(), 1u64)],
        "node 1 bounty banked"
    );

    // Step into node 2, then walk away mid-battle.
    game.handle(ClientInput::ContinueCampaign).unwrap();
    let snap = game.snapshot();
    assert_eq!(snap.phase, Phase::InBattle);
    let enemy = snap.foe.expect("campaign battle shows the foe");
    assert!(
        enemy.species_name.contains("Glutton"),
        "node 2 is Gluttony's: {}",
        enemy.species_name
    );
    let abandoned = game.handle(ClientInput::AbandonCampaign);
    assert!(abandoned.is_ok(), "abandon is lawful mid-gauntlet");
    let snap = game.snapshot();
    assert_eq!(snap.phase, Phase::Menu);
    assert_eq!(snap.campaign, None);
    assert!(snap.me.is_none() && snap.foe.is_none());

    // Earned shards stay; the roster is as pristine as before the run.
    assert_eq!(
        game.store().inventory().unwrap(),
        vec![("contrition-shard".to_string(), 1u64)]
    );
    let listed = game.store().list_champions().unwrap();
    assert_eq!(listed.len(), roster.len());
    for (_, c) in &listed {
        let sp = data::species_by_id(c.species_id).unwrap();
        assert_eq!(c.current_resolve, data::compute_stats(c, sp).max_resolve);
    }
}

#[test]
fn campaign_illegal_transitions() {
    let mut empty = game_with_roster(&[]);
    assert_illegal(empty.handle(ClientInput::StartCampaign));
    assert_illegal(empty.handle(ClientInput::ContinueCampaign));
    assert_illegal(empty.handle(ClientInput::AbandonCampaign));

    let mut game = game_with_roster(&strong_roster());
    assert_illegal(game.handle(ClientInput::ContinueCampaign));
    assert_illegal(game.handle(ClientInput::AbandonCampaign));

    game.handle(ClientInput::StartCampaign).unwrap();
    assert_illegal(game.handle(ClientInput::StartCampaign));
    assert_illegal(game.handle(ClientInput::ContinueCampaign));

    // AcknowledgeResult clears the campaign after a loss frame.
    game.handle(ClientInput::Concede).unwrap();
    assert_eq!(game.snapshot().phase, Phase::BattleOver);
    game.handle(ClientInput::AcknowledgeResult).unwrap();
    let snap = game.snapshot();
    assert_eq!(snap.phase, Phase::Menu);
    assert_eq!(snap.campaign, None);
}

#[test]
fn over_leveled_roster_champion_is_capped_at_50() {
    // Stored levels above MAX_LEVEL are clamped on campaign entry (the
    // roster keeps its stored level; only the field copy is capped).
    let mut game = game_with_roster(&[mkchamp(3, 55, &[6, 2, 1])]);
    game.handle(ClientInput::StartCampaign).unwrap();
    let snap = game.snapshot();
    assert_eq!(
        snap.me.expect("campaign battle shows me").level,
        MAX_LEVEL,
        "field copy runs at the level cap"
    );
    let listed = game.store().list_champions().unwrap();
    assert_eq!(listed[0].1.level, 55, "stored level is never rewritten");
    game.handle(ClientInput::AbandonCampaign).unwrap();
}

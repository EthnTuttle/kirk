//! End-to-end tests for the ClientGame state machine (synchronous practice
//! mode). Fixtures follow the mkchamp pattern from crusade-core's engine
//! tests: learnset-legal rites, zero grace/devotion, full zeal, resolve at
//! the computed maximum.

use crusade_app::{AppError, ClientGame, ClientInput, Phase};
use crusade_core::data;
use crusade_core::engine::BattleEvent;
use crusade_core::types::{ChampionInstance, DevotionStats, GraceStats, RiteSlot};
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

#[test]
fn full_practice_battle_via_client_only() {
    let mut game = game_with_roster(&[mkchamp(1, 15, &[1, 2]), mkchamp(2, 15, &[1, 4])]);
    game.handle(ClientInput::StartPractice { level: 15 })
        .unwrap();

    let mut turns = 0;
    let final_snap = loop {
        let snap = game.snapshot();
        match snap.phase {
            Phase::BattleOver => break snap,
            Phase::InBattle => {
                // Primary: rite slot 0. Fall back to any usable rite, then to
                // a voluntary switch, so zeal exhaustion can't stall the loop.
                let my = if snap.my_rites.iter().any(|r| r.slot == 0 && r.usable) {
                    ClientInput::PickRite { slot: 0 }
                } else if let Some(r) = snap.my_rites.iter().find(|r| r.usable) {
                    ClientInput::PickRite { slot: r.slot }
                } else if let Some(t) = snap.can_switch_to.first() {
                    ClientInput::PickSwitch {
                        party_idx: switch_idx(t),
                    }
                } else {
                    panic!("no usable rite or switch available in battle");
                };
                game.handle(my).unwrap();
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
            other => panic!("unexpected phase {other:?}"),
        }
        turns += 1;
        assert!(turns < 2000, "practice battle did not terminate");
    };

    assert!(
        matches!(final_snap.battle_result, Some(0) | Some(1)),
        "battle_result must name a winner"
    );
    assert!(!final_snap.log_tail.is_empty(), "log should not be empty");
    assert!(
        final_snap.log_tail.iter().any(|l| l.contains("damage")),
        "log tail should contain at least one damage line: {:?}",
        final_snap.log_tail
    );
    assert!(final_snap.turn >= 1, "turn counter advanced");
}

#[test]
fn menu_roster_transitions_and_illegal_inputs() {
    let mut game = game_with_roster(&[]);
    let snap = game.snapshot();
    assert_eq!(snap.phase, Phase::Menu);
    assert_eq!(snap.roster_count, 0);

    // Battle inputs are illegal outside a battle.
    assert_illegal(game.handle(ClientInput::PickRite { slot: 0 }));
    assert_illegal(game.handle(ClientInput::PickSwitch { party_idx: 0 }));
    assert_illegal(game.handle(ClientInput::Concede));
    assert_illegal(game.handle(ClientInput::AcknowledgeResult));
    assert_illegal(game.handle(ClientInput::CloseToMenu));
    // Empty roster cannot start a practice duel.
    assert_illegal(game.handle(ClientInput::StartPractice { level: 10 }));

    game.handle(ClientInput::OpenRoster).unwrap();
    assert_eq!(game.snapshot().phase, Phase::Roster);
    assert_illegal(game.handle(ClientInput::OpenRoster));

    game.handle(ClientInput::CloseToMenu).unwrap();
    assert_eq!(game.snapshot().phase, Phase::Menu);
}

#[test]
fn roster_snapshot_shows_added_champions() {
    let mut game = game_with_roster(&[]);
    let champ = mkchamp(1, 12, &[1, 2]);
    let id = game.store().add_champion(&champ).unwrap();

    let snap = game.snapshot();
    assert_eq!(snap.roster_count, 1);

    let cards = game.roster_cards();
    assert_eq!(cards.len(), 1);
    let card = &cards[0];
    assert_eq!(card.id, id);
    assert_eq!(card.species_id, 1);
    assert_eq!(card.species_name, "Squire Audric");
    assert_eq!(card.level, 12);
    assert_eq!(card.types_display, vec!["Humility".to_string()]);
    assert_eq!(card.current_resolve, card.max_resolve);
    assert_eq!(card.ailment, None);

    assert_eq!(card.rites.len(), 2);
    let r0 = &card.rites[0];
    assert_eq!(r0.slot, 0);
    assert_eq!(r0.rite_id, 1);
    assert_eq!(r0.name, "Measured Blow");
    assert_eq!(r0.zeal_left, r0.zeal_max);
    assert!(r0.usable);
    assert_eq!(card.rites[1].name, "Steady Ward");
}

#[test]
fn concede_gives_foe_the_win_and_acknowledge_resets() {
    let mut game = game_with_roster(&[mkchamp(1, 10, &[1])]);
    game.handle(ClientInput::StartPractice { level: 10 })
        .unwrap();
    assert_eq!(game.snapshot().phase, Phase::InBattle);

    game.handle(ClientInput::Concede).unwrap();
    let snap = game.snapshot();
    assert_eq!(snap.phase, Phase::BattleOver);
    assert_eq!(
        snap.battle_result,
        Some(1),
        "conceding yields the foe the win"
    );
    assert!(
        snap.fresh_events
            .iter()
            .any(|e| matches!(e, BattleEvent::BattleEnded { winner: 1 })),
        "fresh events include the BattleEnded notification"
    );
    // The result must be acknowledged before anything else is possible.
    assert_illegal(game.handle(ClientInput::PickRite { slot: 0 }));

    game.handle(ClientInput::AcknowledgeResult).unwrap();
    let snap = game.snapshot();
    assert_eq!(snap.phase, Phase::Menu);
    assert!(snap.me.is_none() && snap.foe.is_none());
    assert_eq!(snap.battle_result, None);
    assert!(
        snap.fresh_events.is_empty(),
        "acknowledge clears fresh events"
    );
    assert_eq!(snap.roster_count, 1, "roster survives the battle");
}

#[test]
fn in_battle_input_validation_and_voluntary_switch() {
    // Two distinct species so the active swap is observable.
    let mut game = game_with_roster(&[mkchamp(1, 15, &[1, 2]), mkchamp(2, 15, &[1, 4])]);
    game.handle(ClientInput::StartPractice { level: 15 })
        .unwrap();
    let snap = game.snapshot();
    assert_eq!(snap.phase, Phase::InBattle);
    assert_eq!(snap.turn, 1);

    // Out-of-range rite slot / switch target.
    assert_illegal(game.handle(ClientInput::PickRite { slot: 9 }));
    assert_illegal(game.handle(ClientInput::PickSwitch { party_idx: 5 }));
    // No second battle, no roster browsing while a battle is live.
    assert_illegal(game.handle(ClientInput::StartPractice { level: 15 }));
    assert_illegal(game.handle(ClientInput::OpenRoster));

    // Voluntary switch to party idx 1 consumes a turn and swaps the active.
    let target = snap
        .can_switch_to
        .iter()
        .find(|s| s.starts_with("1:"))
        .expect("second party member is a legal switch target")
        .clone();
    game.handle(ClientInput::PickSwitch {
        party_idx: switch_idx(&target),
    })
    .unwrap();
    let snap = game.snapshot();
    assert_eq!(
        snap.phase,
        Phase::InBattle,
        "switch must not end the battle"
    );
    assert_eq!(snap.turn, 2, "the voluntary switch consumed a turn");
    let expected_name = target.split(':').nth(1).expect("name half of target");
    assert_eq!(
        snap.me.expect("in-battle snapshot shows me").species_name,
        expected_name,
        "active champion is the one we switched to"
    );
}

#[test]
fn fresh_events_are_take_drain_per_snapshot() {
    let mut game = game_with_roster(&[mkchamp(1, 15, &[1, 2])]);
    game.handle(ClientInput::StartPractice { level: 15 })
        .unwrap();
    game.handle(ClientInput::PickRite { slot: 0 }).unwrap();

    let snap = game.snapshot();
    assert!(
        !snap.fresh_events.is_empty(),
        "the first snapshot reports the turn's events"
    );
    assert!(
        snap.fresh_events
            .iter()
            .any(|e| matches!(e, BattleEvent::TurnStart { .. })),
        "turn batch starts with TurnStart"
    );
    let snap2 = game.snapshot();
    assert!(
        snap2.fresh_events.is_empty(),
        "a second snapshot without input yields no fresh events"
    );
    assert!(snap2.log_tail.len() <= 8, "log tail is capped at ~8 lines");
}

/// The practice foe team's mirror-avoidance through the public input path:
/// a company already fielding the whole theme (Bastien, Aldwyn, Raimbaud)
/// must open against three champions OUTSIDE its party — the "Foe" display
/// tag stays the net-duel backstop, not the practice table's excuse.
#[test]
fn practice_foes_never_mirror_the_party() {
    let mut game = game_with_roster(&[
        mkchamp(3, 15, &[1]),
        mkchamp(8, 15, &[1]),
        mkchamp(10, 15, &[1]),
    ]);
    game.handle(ClientInput::StartPractice { level: 15 })
        .unwrap();
    let snap = game.snapshot();
    let facing = snap
        .log_tail
        .iter()
        .find(|l| l.contains("facing"))
        .expect("the opening line names the foes");
    assert_eq!(
        facing
            .split("facing ")
            .nth(1)
            .expect("facing clause")
            .trim_end_matches('.'),
        "Askari Alamdar, Squire Audric, Cleric Mabel",
        "party holding 3+8: the rotated adversary stands, colliders \
         backfill from the pool in id order (a full mirror is impossible: \
         116 cannot be fielded by a roster)"
    );
    let foe = snap.foe.expect("in-battle snapshot shows the foe");
    assert_ne!(
        foe.species_name, "Serjeant Bastien",
        "never spar your own Bastien"
    );

    // Partial overlap: only the collider is replaced, theme order held.
    let mut game = game_with_roster(&[mkchamp(3, 15, &[1])]);
    game.handle(ClientInput::StartPractice { level: 15 })
        .unwrap();
    let facing = game
        .snapshot()
        .log_tail
        .iter()
        .find(|l| l.contains("facing"))
        .expect("the opening line names the foes")
        .clone();
    assert!(
        facing.contains("facing Brother Aldwyn, Askari Alamdar, Squire Audric."),
        "colliders-only replacement keeps theme order: {facing}"
    );
}

/// The vesture ring: CycleVariant on the roster walks base -> alt-a ->
/// alt-b -> base, persists under `roster.variant.<species_id>`, rides the
/// roster card, and survives a document save/load round-trip.
#[test]
fn cycle_variant_rings_persists_and_survives_reload() {
    let mut game = game_with_roster(&[mkchamp(3, 15, &[1])]);
    game.handle(ClientInput::OpenRoster).unwrap();
    assert_eq!(game.roster_cards()[0].variant, 0, "fresh card reads base");

    for want in [1u8, 2, 0] {
        game.handle(ClientInput::CycleVariant { species_id: 3 })
            .unwrap();
        assert_eq!(
            game.store()
                .setting("roster.variant.3")
                .unwrap()
                .expect("the pick persisted"),
            want.to_string(),
            "settings key carries ring position {want}"
        );
        assert_eq!(
            game.roster_cards()[0].variant,
            want,
            "the card rides the ring position"
        );
    }

    // Durable through the automerge document: save bytes, reload, reread.
    let bytes = game.store().save_bytes().expect("document saves");
    let store = RosterStore::load_bytes(&bytes).expect("document loads");
    let reloaded = ClientGame::new(store);
    assert_eq!(
        reloaded.roster_cards()[0].variant,
        0,
        "0 was persisted last and reload agrees"
    );
    let mut reloaded = reloaded;
    reloaded.handle(ClientInput::OpenRoster).unwrap();
    reloaded
        .handle(ClientInput::CycleVariant { species_id: 3 })
        .unwrap();
    let bytes = reloaded.store().save_bytes().expect("document saves");
    let reloaded = ClientGame::new(RosterStore::load_bytes(&bytes).expect("document loads"));
    assert_eq!(reloaded.roster_cards()[0].variant, 1);
}

/// The ring's guards: a phase that shows no gallery refuses, and ids with
/// no alt plates (sin-spawn, unknowns) never write a setting row.
#[test]
fn cycle_variant_guards_phase_and_species() {
    let mut game = game_with_roster(&[mkchamp(3, 15, &[1])]);
    // Menu shows no gallery: the input is illegal there.
    assert_illegal(game.handle(ClientInput::CycleVariant { species_id: 3 }));
    game.handle(ClientInput::OpenRoster).unwrap();
    for bad in [101u16, 999] {
        assert_illegal(game.handle(ClientInput::CycleVariant { species_id: bad }));
        assert!(
            game.store()
                .setting(&format!("roster.variant.{bad}"))
                .unwrap()
                .is_none(),
            "a refused pick writes nothing for {bad}"
        );
    }
    // A live battle also refuses (gallery chrome mutates no battle state).
    game.handle(ClientInput::StartPractice { level: 15 })
        .unwrap();
    assert_illegal(game.handle(ClientInput::CycleVariant { species_id: 3 }));
    // Corrupt-setting degrade: a hand-written garbage value reads as base.
    game = game_with_roster(&[mkchamp(3, 15, &[1])]);
    game.store()
        .set_setting("roster.variant.3", "garbage")
        .unwrap();
    assert_eq!(game.roster_cards()[0].variant, 0);
    game.store().set_setting("roster.variant.3", "9").unwrap();
    assert_eq!(
        game.roster_cards()[0].variant,
        2,
        "out-of-ring values clamp onto the ring"
    );
}

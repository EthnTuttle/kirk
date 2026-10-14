//! Game-trait adapter tests over real, freshly signed nostr event chains.
//!
//! The happy-path chain is fabricated by scripting a REAL crusade-core battle
//! (species 1 "Squire Audric" lvl 10 with rite 1 vs species 101 "Prince of
//! Pride" lvl 10 with rite 42, one champion per side): both sides use rite
//! slot 0 until the acceptor yields from turn 8 on — in practice one side is
//! KO'd within a few turns first. Either way `winner_side` is read back from
//! the scripted engine battle, so the tests do not depend on the length of
//! the transcript. CashuToken fixtures use the same empty-proof
//! JSON trick as kirk's own card_game/commitments tests; commitment hashes in
//! Challenge/Accept are computed from exactly the tokens later revealed, via
//! `kirk::cashu::commitments::TokenCommitment`.

use crusade_bridge::seed::derive_battle_seed_from_reveal;
use crusade_bridge::{BattleActionSubmission, BattleAdapter};
use crusade_core::data;
use crusade_core::engine::{Action, Battle};
use crusade_core::types::{ChampionInstance, DevotionStats, GraceStats, RiteSlot};
use kirk::cashu::commitments::TokenCommitment;
use kirk::error::ValidationErrorType;
use kirk::events::{
    ChallengeAcceptContent, ChallengeContent, FinalContent, MoveContent, MoveType,
    CHALLENGE_ACCEPT_KIND, CHALLENGE_KIND, FINAL_KIND, MOVE_KIND,
};
use kirk::game::traits::Game;
use kirk::CashuToken;
use nostr::{Event as NostrEvent, EventBuilder, EventId, Keys, Timestamp};
use serde::Serialize;

/// Deterministic chain timestamps (the adapter reads chain order, not
/// `created_at`; we still stamp increasing times like a real client).
const BASE_TS: u64 = 1_700_000_000;

// ---------- champion fixtures (mirrors crusade-core engine tests) ----------

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
    };
    c.current_resolve = data::compute_stats(&c, sp).max_resolve;
    c
}

// ---------- token fixtures (same JSON trick as kirk's cashu tests) ----------

fn mock_token(seed: u8) -> CashuToken {
    let token_json = format!(
        r#"{{"token":[{{"mint":"https://mint{seed}.example.com","proofs":[]}}],"memo":"battle_test_{seed}","unit":"sat"}}"#
    );
    serde_json::from_str(&token_json).expect("mock token parses")
}

// ---------- event builders ----------

fn sign(kind: nostr::Kind, content: &impl Serialize, keys: &Keys, secs: u64) -> NostrEvent {
    EventBuilder::new(
        kind,
        serde_json::to_string(content).unwrap(),
        Vec::<nostr::Tag>::new(),
    )
    .custom_created_at(Timestamp::from_secs(BASE_TS + secs))
    .to_event(keys)
    .unwrap()
}

#[allow(clippy::too_many_arguments)]
fn sign_move(
    keys: &Keys,
    prev: EventId,
    secs: u64,
    turn: u32,
    action: Action,
    party_reveal: Option<Vec<ChampionInstance>>,
    revealed_tokens: Option<Vec<CashuToken>>,
) -> NostrEvent {
    let submission = BattleActionSubmission {
        turn,
        action,
        party_reveal,
    };
    let content = MoveContent {
        previous_event_id: prev,
        // A move that carries the committed tokens is a kirk Reveal move.
        move_type: if revealed_tokens.is_some() {
            MoveType::Reveal
        } else {
            MoveType::Move
        },
        move_data: serde_json::to_value(&submission).unwrap(),
        revealed_tokens,
        deadline: None,
    };
    sign(MOVE_KIND, &content, keys, secs)
}

/// A fabricated battle plus its nostr event chain.
struct Fixture {
    keys: [Keys; 2], // [challenge creator (side0), acceptor (side1)]
    parties: [Vec<ChampionInstance>; 2],
    actions: Vec<(Action, Action)>, // engine transcript by turn
    winner_side: u8,
    events: Vec<NostrEvent>,
}

fn build_fixture() -> Fixture {
    let keys = [Keys::generate(), Keys::generate()];
    let tokens = [vec![mock_token(1)], vec![mock_token(2)]];
    let hashes: [Vec<String>; 2] = [
        tokens[0]
            .iter()
            .map(TokenCommitment::hash_single_token)
            .collect(),
        tokens[1]
            .iter()
            .map(TokenCommitment::hash_single_token)
            .collect(),
    ];

    let challenge = sign(
        CHALLENGE_KIND,
        &ChallengeContent {
            game_type: "crusade_battle".to_string(),
            commitment_hashes: hashes[0].clone(),
            game_parameters: serde_json::json!({"game": "deus-vult", "level": 10}),
            expiry: None,
            timeout_config: None,
        },
        &keys[0],
        0,
    );
    let accept = sign(
        CHALLENGE_ACCEPT_KIND,
        &ChallengeAcceptContent {
            challenge_id: challenge.id,
            commitment_hashes: hashes[1].clone(),
        },
        &keys[1],
        10,
    );

    // Seed exactly the way the adapter will derive it. These mock tokens have
    // no proofs, hence no C values; the seed input from each side is empty
    // and the battle id carries the entropy. The token binding under test is
    // the commitment-hash comparison, not the seed.
    let seed =
        derive_battle_seed_from_reveal(&challenge.id.to_hex(), &accept.id.to_hex(), &[], &[])
            .expect("event ids are 32-byte hex");

    let parties = [
        vec![mkchamp(1, 10, &[1])],    // Squire Audric: Measured Blow
        vec![mkchamp(101, 10, &[42])], // Prince of Pride: Tirade of Scorn
    ];

    // Script through the real engine: rite slot 0 on both sides; from turn 8
    // on the acceptor yields. Single-champion parties at these stats need a
    // few turns to KO (and never inside turn 1), so the battle ends naturally
    // before the yield — the yield just guarantees termination either way.
    let mut battle =
        Battle::new(parties[0].clone(), parties[1].clone(), seed).expect("legal fixture battle");
    let mut actions: Vec<(Action, Action)> = Vec::new();
    while battle.winner.is_none() {
        let pair = if battle.turn >= 8 {
            (Action::Rite { slot: 0 }, Action::Yield)
        } else {
            (Action::Rite { slot: 0 }, Action::Rite { slot: 0 })
        };
        battle
            .apply_turn(pair.0, pair.1)
            .expect("scripted turn resolves");
        actions.push(pair);
    }
    let winner_side = battle.winner.expect("yield guarantees a winner");
    assert!(
        actions.len() >= 2,
        "fixture battle must last at least 2 turns for the mid-chain tests"
    );

    // Fabricate Move events in chain order: challenge, accept, then per turn
    // the creator's move followed by the acceptor's, then both Final events.
    let mut events = vec![challenge.clone(), accept.clone()];
    let mut prev = accept.id;
    let mut secs = 20u64;
    let mut reveal_done = [false; 2];
    for (i, (a0, a1)) in actions.iter().enumerate() {
        let turn = (i + 1) as u32;
        for (side, action) in [(0usize, *a0), (1usize, *a1)] {
            let first = !reveal_done[side];
            reveal_done[side] = true;
            let ev = sign_move(
                &keys[side],
                prev,
                secs,
                turn,
                action,
                first.then(|| parties[side].clone()),
                first.then(|| tokens[side].clone()),
            );
            prev = ev.id;
            secs += 10;
            events.push(ev);
        }
    }
    for k in &keys {
        let ev = sign(
            FINAL_KIND,
            &FinalContent {
                game_sequence_root: challenge.id,
                commitment_method: None,
                final_state: serde_json::json!({"winner_side": winner_side}),
            },
            k,
            secs,
        );
        secs += 10;
        events.push(ev);
    }

    Fixture {
        keys,
        parties,
        actions,
        winner_side,
        events,
    }
}

fn error_messages(result: &kirk::error::ValidationResult) -> Vec<String> {
    result.errors.iter().map(|e| e.message.clone()).collect()
}

// ---------- forced-switch fixture (multi-champion, mid-chain faint) ----------

/// True while the engine owes `side` a forced switch (same probe the live
/// DuelSession uses: legal actions degenerate to only-Switch).
fn switch_pending(b: &Battle, side: u8) -> bool {
    let legal = b.legal_actions(side);
    !legal.is_empty() && legal.iter().all(|a| matches!(a, Action::Switch { .. }))
}

/// A fabricated battle whose engine transcript REQUIRES a forced switch:
/// side 0 fields one Prince of Pride (lvl 50, rite 8 "Flail of Penance",
/// MultiHit), side 1 fields two Squire Audrics (lvl 5, rite 1). The prince
/// fells the first Audric mid-chain, which raises ForcedSwitchNeeded on
/// side 1; the second Audric falls right after and decides it.
struct SwitchFixture {
    keys: [Keys; 2],
    winner_side: u8,
    events: Vec<NostrEvent>,
    /// (index into `events`, declared turn, party_idx) of side 1's
    /// forced-switch Move event, for the tamper variant.
    switch_move: (usize, u32, u8),
}

fn build_forced_switch_fixture() -> SwitchFixture {
    let keys = [Keys::generate(), Keys::generate()];
    let tokens = [vec![mock_token(1)], vec![mock_token(2)]];
    let hashes: [Vec<String>; 2] = [
        tokens[0]
            .iter()
            .map(TokenCommitment::hash_single_token)
            .collect(),
        tokens[1]
            .iter()
            .map(TokenCommitment::hash_single_token)
            .collect(),
    ];
    let challenge = sign(
        CHALLENGE_KIND,
        &ChallengeContent {
            game_type: "crusade_battle".to_string(),
            commitment_hashes: hashes[0].clone(),
            game_parameters: serde_json::json!({"game": "deus-vult", "level": 50}),
            expiry: None,
            timeout_config: None,
        },
        &keys[0],
        0,
    );
    let accept = sign(
        CHALLENGE_ACCEPT_KIND,
        &ChallengeAcceptContent {
            challenge_id: challenge.id,
            commitment_hashes: hashes[1].clone(),
        },
        &keys[1],
        10,
    );
    let seed =
        derive_battle_seed_from_reveal(&challenge.id.to_hex(), &accept.id.to_hex(), &[], &[])
            .expect("event ids are 32-byte hex");
    let parties = [
        vec![mkchamp(101, 50, &[8])], // Prince of Pride: Flail of Penance
        vec![mkchamp(1, 5, &[1]), mkchamp(1, 5, &[1])], // two Squire Audrics
    ];

    // Script through the real engine exactly as DuelSession drives it:
    // normal turns pair both actions through apply_turn; a pending forced
    // switch resolves through a lone apply_forced_switch (canonical side
    // order 0 then 1) BEFORE the next pair can run — apply_turn itself
    // rejects anything while a switch is owed.
    let mut battle =
        Battle::new(parties[0].clone(), parties[1].clone(), seed).expect("legal fixture battle");
    let mut records: Vec<Vec<(usize, Action)>> = Vec::new();
    let mut forced_seen = false;
    while battle.winner.is_none() {
        if switch_pending(&battle, 0) || switch_pending(&battle, 1) {
            let s = u8::from(!switch_pending(&battle, 0));
            let idx = (0..battle.sides[usize::from(s)].party.len())
                .find(|&i| {
                    battle.sides[usize::from(s)].alive(i)
                        && i != battle.sides[usize::from(s)].active_idx
                })
                .expect("a pending forced switch has an alive replacement");
            battle
                .apply_forced_switch(s, idx as u8)
                .expect("scripted forced switch resolves");
            records.push(vec![(
                usize::from(s),
                Action::Switch {
                    party_idx: idx as u8,
                },
            )]);
            forced_seen = true;
        } else {
            let pair = (Action::Rite { slot: 0 }, Action::Rite { slot: 0 });
            battle
                .apply_turn(pair.0, pair.1)
                .expect("scripted turn resolves");
            records.push(vec![(0, pair.0), (1, pair.1)]);
        }
        assert!(
            records.len() < 64,
            "forced-switch fixture must terminate quickly"
        );
    }
    assert!(
        forced_seen,
        "fixture battle must raise a forced switch mid-chain"
    );
    let winner_side = battle.winner.expect("scripted battle decided");

    // Fabricate Move events. Record slots number 1..=N consecutively: a
    // turn pair shares its slot number, a forced switch takes its own.
    let mut events = vec![challenge.clone(), accept.clone()];
    let mut prev = accept.id;
    let mut secs = 20u64;
    let mut reveal_done = [false; 2];
    let mut switch_move = (0usize, 0u32, 0u8);
    for (slot, group) in records.iter().enumerate() {
        let turn = (slot + 1) as u32;
        for &(side, action) in group {
            let first = !reveal_done[side];
            reveal_done[side] = true;
            let ev = sign_move(
                &keys[side],
                prev,
                secs,
                turn,
                action,
                first.then(|| parties[side].clone()),
                first.then(|| tokens[side].clone()),
            );
            if let (1, Action::Switch { party_idx }) = (group.len(), action) {
                switch_move = (events.len(), turn, party_idx);
            }
            prev = ev.id;
            secs += 10;
            events.push(ev);
        }
    }
    for k in &keys {
        let ev = sign(
            FINAL_KIND,
            &FinalContent {
                game_sequence_root: challenge.id,
                commitment_method: None,
                final_state: serde_json::json!({"winner_side": winner_side}),
            },
            k,
            secs,
        );
        secs += 10;
        events.push(ev);
    }

    SwitchFixture {
        keys,
        winner_side,
        events,
        switch_move,
    }
}

// ---------- relay-order canon ----------

/// Full verdict of validate_sequence, canonicalized so any relay collection
/// order must compare equal: validity, winner, and the sorted set of error
/// findings (event id + type + message).
fn verdict(
    adapter: &BattleAdapter,
    events: &[NostrEvent],
) -> (bool, Option<nostr::PublicKey>, Vec<String>) {
    let r = adapter.validate_sequence(events).unwrap();
    let mut findings: Vec<String> = r
        .errors
        .iter()
        .map(|e| format!("{}::{:?}::{}", e.event_id, e.error_type, e.message))
        .collect();
    findings.sort();
    (r.is_valid, r.winner, findings)
}

/// The orderings two relays would each produce independently: tail-first
/// and a cross-interleave of the front/back halves of the poll window.
fn relay_orderings(events: &[NostrEvent]) -> Vec<(&'static str, Vec<NostrEvent>)> {
    let mut reversed = events.to_vec();
    reversed.reverse();
    let mid = events.len().div_ceil(2);
    let (lo, hi) = (&events[..mid], &events[mid..]);
    let mut interleaved = Vec::with_capacity(events.len());
    for (i, e) in lo.iter().enumerate() {
        interleaved.push(e.clone());
        if let Some(e) = hi.get(i) {
            interleaved.push(e.clone());
        }
    }
    vec![("reversed", reversed), ("interleaved", interleaved)]
}

/// Every relay collection order must reach the input-order verdict's
/// findings, byte-identical.
fn assert_order_independent(events: &[NostrEvent]) {
    let adapter = BattleAdapter;
    let base = verdict(&adapter, events);
    for (name, permuted) in relay_orderings(events) {
        assert_eq!(
            verdict(&adapter, &permuted),
            base,
            "{name} order changed the verdict"
        );
    }
}

// ---------- tests ----------

/// Happy path: a full, honestly played 3-turn battle chain must validate,
/// replay to the engine-decided winner, and be complete with both Finals.
#[test]
fn happy_chain_replays_to_engine_winner() {
    let fx = build_fixture();
    assert_eq!(fx.events.len(), 2 + 2 * fx.actions.len() + 2);

    let adapter = BattleAdapter;
    assert_eq!(adapter.required_final_events(), 2);

    let result = adapter.validate_sequence(&fx.events).unwrap();
    assert!(
        result.is_valid,
        "honest chain must validate: {:?}",
        error_messages(&result)
    );
    let expected = fx.keys[fx.winner_side as usize].public_key();
    assert_eq!(result.winner, Some(expected));
    assert_eq!(result.forfeited_player, None);

    assert!(adapter.is_sequence_complete(&fx.events).unwrap());
    assert_eq!(
        adapter.determine_winner(&fx.events).unwrap(),
        Some(expected)
    );
}

/// Tampered action: flip the turn-2 creator move to an unequipped rite slot.
/// Same signer, same position in the chain — but the engine rejects the
/// transcript at replay.
#[test]
fn tampered_action_mid_chain_is_invalid() {
    let fx = build_fixture();
    let adapter = BattleAdapter;
    let mut events = fx.events.clone();

    // events = [challenge, accept, t1c, t1a, t2c, ...] -> index 4 is the
    // turn-2 creator move.
    events[4] = sign_move(
        &fx.keys[0],
        events[3].id,
        40,
        2,
        Action::Rite { slot: 1 }, // fixture champions equip slot 0 only
        None,
        None,
    );

    let result = adapter.validate_sequence(&events).unwrap();
    assert!(!result.is_valid, "tampered chain must not validate");
    assert!(result.winner.is_none());
    assert!(
        result
            .errors
            .iter()
            .any(|e| matches!(e.error_type, ValidationErrorType::InvalidMove)
                && e.message.contains("rejected by engine")),
        "expected an engine-rejection error, got {:?}",
        error_messages(&result)
    );
    assert!(!adapter.is_sequence_complete(&events).unwrap());
    assert_eq!(adapter.determine_winner(&events).unwrap(), None);
}

/// Wrong turn numbering: the turn-2 acceptor move declares turn 7.
#[test]
fn wrong_turn_numbering_is_invalid() {
    let fx = build_fixture();
    let adapter = BattleAdapter;
    let mut events = fx.events.clone();

    let turn2_acceptor_action = fx.actions[1].1;
    events[5] = sign_move(
        &fx.keys[1],
        events[4].id,
        50,
        7, // declared turn != chain turn 2
        turn2_acceptor_action,
        None,
        None,
    );

    let result = adapter.validate_sequence(&events).unwrap();
    assert!(!result.is_valid, "wrong turn numbering must not validate");
    assert!(
        result.errors.iter().any(
            |e| matches!(e.error_type, ValidationErrorType::InvalidSequence)
                && e.message.contains("Turn numbering broken")
        ),
        "expected a turn-numbering error, got {:?}",
        error_messages(&result)
    );
}

/// Forged reveal: the creator's first move reveals a token it never
/// committed to (commitment-hash compare must fail, same as card_game).
#[test]
fn forged_token_reveal_is_invalid() {
    let fx = build_fixture();
    let adapter = BattleAdapter;
    let mut events = fx.events.clone();

    // Index 2 is the creator's first move (turn 1, party + token reveal).
    events[2] = sign_move(
        &fx.keys[0],
        events[1].id,
        20,
        1,
        fx.actions[0].0,
        Some(fx.parties[0].clone()),
        Some(vec![mock_token(99)]), // not the committed token
    );

    let result = adapter.validate_sequence(&events).unwrap();
    assert!(!result.is_valid, "forged reveal must not validate");
    assert!(
        result.errors.iter().any(|e| matches!(
            e.error_type,
            ValidationErrorType::InvalidCommitment
        ) && e.message.contains("does not match its commitment")),
        "expected a commitment-mismatch error, got {:?}",
        error_messages(&result)
    );
}

/// A validated, decided chain that is missing the Final events is well-formed
/// but not complete.
#[test]
fn decided_chain_without_finals_is_valid_but_incomplete() {
    let fx = build_fixture();
    let adapter = BattleAdapter;
    let events = &fx.events[..fx.events.len() - 2]; // drop both Final events

    let result = adapter.validate_sequence(events).unwrap();
    assert!(
        result.is_valid,
        "decided chain still validates: {:?}",
        error_messages(&result)
    );
    assert!(result.winner.is_some());
    assert!(!adapter.is_sequence_complete(events).unwrap());
    // determine_winner only needs the replay, not the Finals.
    assert_eq!(
        adapter.determine_winner(events).unwrap(),
        Some(fx.keys[fx.winner_side as usize].public_key())
    );
}

/// Multi-champion chain whose engine transcript requires a forced switch
/// mid-battle: the replay must thread the lone Switch record through
/// apply_forced_switch (a turn pair would be engine-rejected while the
/// switch is owed) and still reach the engine-decided winner.
#[test]
fn forced_switch_chain_replays_to_engine_winner() {
    let fx = build_forced_switch_fixture();
    let adapter = BattleAdapter;

    let result = adapter.validate_sequence(&fx.events).unwrap();
    assert!(
        result.is_valid,
        "honest forced-switch chain must validate: {:?}",
        error_messages(&result)
    );
    assert_eq!(
        fx.winner_side, 0,
        "the level-50 prince must sweep two level-5 squires"
    );
    let expected = fx.keys[0].public_key();
    assert_eq!(result.winner, Some(expected));
    assert!(adapter.is_sequence_complete(&fx.events).unwrap());
    assert_eq!(
        adapter.determine_winner(&fx.events).unwrap(),
        Some(expected)
    );
}

/// Relay collection order must not matter: reversed and interleaved copies
/// of honest chains (with and without a mid-chain forced switch) reach
/// byte-identical verdicts.
#[test]
fn honest_chains_validate_identically_under_any_relay_order() {
    let fx = build_fixture();
    assert_order_independent(&fx.events);
    let fx = build_forced_switch_fixture();
    assert_order_independent(&fx.events);
}

/// Tamper: side 1's forced-switch record re-signed by side 0. Replay faces
/// a side-0 action where canonical order requires side 1's forced switch —
/// invalid, with identical findings under every relay order.
#[test]
fn forced_switch_record_signed_by_wrong_side_is_invalid() {
    let fx = build_forced_switch_fixture();
    let adapter = BattleAdapter;
    let (idx, turn, party_idx) = fx.switch_move;

    let mut tampered = fx.events.clone();
    tampered[idx] = sign_move(
        &fx.keys[0],
        fx.events[idx - 1].id,
        1000,
        turn,
        Action::Switch { party_idx },
        None,
        None,
    );

    let result = adapter.validate_sequence(&tampered).unwrap();
    assert!(
        !result.is_valid,
        "wrong-side forced-switch record must not validate"
    );
    assert!(result.winner.is_none());
    assert!(
        result
            .errors
            .iter()
            .any(|e| matches!(e.error_type, ValidationErrorType::InvalidSequence)),
        "expected a sequencing error, got {:?}",
        error_messages(&result)
    );
    assert!(!adapter.is_sequence_complete(&tampered).unwrap());
    assert_eq!(adapter.determine_winner(&tampered).unwrap(), None);
    assert_order_independent(&tampered);
}

/// Relay feeds can carry foreign events that reference chain ids (kirk's
/// collector links moves by previous_event_id, which anyone can point at).
/// Foreign pubkeys must be skipped silently — the honest verdict stands,
/// with identical findings under every relay order.
#[test]
fn foreign_pubkey_events_are_skipped_silently() {
    let fx = build_fixture();
    let adapter = BattleAdapter;
    let outsider = Keys::generate();
    let mut events = fx.events.clone();

    // A foreign move pointing into the chain, declaring a bogus turn.
    let foreign_move = sign_move(
        &outsider,
        fx.events[1].id,
        997,
        99,
        Action::Yield,
        None,
        None,
    );
    // A foreign move whose content is not even a MoveContent.
    let foreign_garbage = sign(
        MOVE_KIND,
        &serde_json::json!({"bogus": true}),
        &outsider,
        998,
    );
    // A foreign Final event referencing this sequence root.
    let foreign_final = sign(
        FINAL_KIND,
        &FinalContent {
            game_sequence_root: fx.events[0].id,
            commitment_method: None,
            final_state: serde_json::json!({"impostor": true}),
        },
        &outsider,
        999,
    );
    events.insert(0, foreign_garbage);
    events.insert(events.len() / 2, foreign_final);
    events.push(foreign_move);

    let result = adapter.validate_sequence(&events).unwrap();
    assert!(
        result.is_valid,
        "foreign noise must not sway an honest chain: {:?}",
        error_messages(&result)
    );
    assert_eq!(
        result.winner,
        Some(fx.keys[fx.winner_side as usize].public_key())
    );
    assert!(adapter.is_sequence_complete(&events).unwrap());
    assert_order_independent(&events);
}

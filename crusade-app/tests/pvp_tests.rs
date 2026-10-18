//! End-to-end network duels through `NetDuelDriver`: two drivers over a
//! memory transport pair play scripted greedy games, verify transcript and
//! attestation agreement, exercise the forced-switch rounds, update ladder
//! ratings, and surface disconnects as errors (no panic).
//!
//! Fixtures follow the mkchamp pattern from crusade-core's engine tests:
//! learnset-legal rites, zero grace/devotion, full zeal, resolve at the
//! computed maximum.

use crusade_app::pvp::{settle_ladder, NetDuelDriver};

/// Read one player's rating out of the atomic `ladder.state` value.
fn rating_of(store: &RosterStore, pk: &nostr::PublicKey) -> Option<u64> {
    let json = store.setting(crusade_app::pvp::LADDER_STATE_KEY).ok()??;
    serde_json::from_str::<serde_json::Value>(&json)
        .ok()?
        .get("ratings")?
        .get(pk.to_hex())?
        .as_u64()
}
use crusade_app::{AppError, ClientInput, Phase};
use crusade_core::data;
use crusade_core::engine::Action;
use crusade_core::types::{ChampionInstance, DevotionStats, GraceStats, RiteSlot};
use crusade_data::RosterStore;
use crusade_net::memory::memory_pair;
use crusade_net::NetError;
use nostr::Keys;

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

/// Fixed duel seed (in real wiring it arrives from the challenge flow's
/// commit-reveal; here it's just a parameter, same on both replicas).
const SEED: [u8; 32] = [0x5E; 32];
/// Fixed attestation battle id (sha256(challenge||accept) in real wiring).
const BATTLE_ID: [u8; 32] = [0xB1; 32];

/// The app's greedy practice policy as a duel turn input.
fn greedy_input(d: &NetDuelDriver) -> ClientInput {
    match crusade_app::policy::greedy_action(d.battle(), d.side()) {
        Action::Rite { slot } => ClientInput::PickRite { slot },
        Action::Switch { party_idx } => ClientInput::PickSwitch { party_idx },
        Action::Yield => ClientInput::Concede,
    }
}

#[derive(Debug, Default)]
struct DriveStats {
    turns: u32,
    forced_switches: u32,
}

/// Play one side to the end with the greedy policy: forced switches resolve
/// via `my_switch` (first legal target), then `finalize` signs the
/// attestation. Drives ONLY this side — the peer must run concurrently.
async fn drive_to_end(d: &mut NetDuelDriver) -> Result<DriveStats, AppError> {
    let mut stats = DriveStats::default();
    while d.battle_result().is_none() {
        stats.turns += 1;
        assert!(stats.turns < 500, "duel did not terminate");
        if d.pending_forced_switch() {
            let idx = *d
                .legal_switch_targets()
                .first()
                .expect("pending switch implies a target");
            d.my_switch(idx).await?;
            stats.forced_switches += 1;
        } else {
            d.my_turn(greedy_input(d)).await?;
        }
    }
    d.finalize().await?;
    Ok(stats)
}

/// Prince of Pride (host) vs an out-leveled Squire Audric (joiner): the
/// duel converges with identical transcripts, both attestation events
/// verify under each side's own fresh keys with agreeing digests, and the
/// ladder books host win / joiner loss off the default 1000 (+16 / -16
/// exactly).
#[tokio::test]
async fn full_duel_host_vs_join_converges_attests_and_rates() {
    let host_party = vec![mkchamp(101, 50, &[8])];
    let join_party = vec![mkchamp(1, 20, &[1, 2])];
    let (t_host, t_join) = memory_pair();
    let host_keys = Keys::generate();
    let join_keys = Keys::generate();

    let mut host = NetDuelDriver::host(
        host_party.clone(),
        join_party.clone(),
        t_host,
        SEED,
        BATTLE_ID,
        host_keys.public_key(),
        join_keys.public_key(),
        Some(&host_keys),
    )
    .expect("host driver");
    let mut join = NetDuelDriver::join(
        join_party,
        host_party,
        t_join,
        SEED,
        BATTLE_ID,
        host_keys.public_key(),
        host_keys.public_key(),
        Some(&join_keys),
    )
    .expect("join driver");

    let join_task = tokio::spawn(async move {
        drive_to_end(&mut join).await.map(|stats| (stats, join))
    });
    let host_stats = drive_to_end(&mut host).await.expect("host drive");
    let (_join_stats, join) = join_task.await.expect("join task").expect("join drive");

    assert_eq!(
        host.battle_result(),
        Some(0),
        "the over-leveled Prince takes it"
    );
    assert_eq!(
        join.battle_result(),
        Some(0),
        "replicas agree on the winner"
    );
    assert!(host.we_won() == Some(true) && join.we_won() == Some(false));
    assert!(host_stats.turns >= 1, "at least one turn was played");
    assert_eq!(
        host.transcript_hash(),
        join.transcript_hash(),
        "identical transcripts after finalize"
    );

    // Snapshot sanity at battle end (view adapter reuses the in-battle shapes).
    let snap = host.snapshot();
    assert_eq!(snap.phase, Phase::BattleOver);
    assert_eq!(snap.battle_result, Some(0));
    assert!(snap.me.is_some() && snap.foe.is_some());

    // Each side's attestation verifies under ITS OWN fresh pubkey against
    // the shared battle id, and both attestations agree field-for-field.
    let battle_hex = hex::encode(BATTLE_ID);
    let host_event = host.attestation_event().expect("host attestation");
    let join_event = join.attestation_event().expect("join attestation");
    let host_att =
        crusade_bridge::verify_attestation(&host_event, &host_keys.public_key(), &battle_hex)
            .expect("host attestation verifies");
    let join_att =
        crusade_bridge::verify_attestation(&join_event, &join_keys.public_key(), &battle_hex)
            .expect("join attestation verifies");
    assert!(
        crusade_bridge::digests_agree(&host_att, &join_att),
        "both sides attest to the same duel outcome"
    );
    assert_eq!(host_att.digest, hex::encode(host.transcript_hash()));
    assert_eq!(host_att.winner, 0);

    // V12-1 ladder: nothing has moved yet — finalize signed seals but no
    // counter-seal pair has been folded.
    let mut host_store = RosterStore::new();
    let mut join_store = RosterStore::new();

    // Self-only fold (own seal alone, the old honor-scoring position) is
    // inert: a rating needs BOTH seals.
    assert_eq!(
        settle_ladder(&mut host_store, &host_keys.public_key(), &[host_event.clone()])
            .expect("self-only settle"),
        None
    );
    assert_eq!(rating_of(&host_store, &host_keys.public_key()), None);

    // Honest two-client settlement: each side folds the same seal +
    // counter-seal pair into its OWN store, and both agree (host +16,
    // joiner -16 off the default 1000) — the convergent-fold done-test.
    assert_eq!(
        settle_ladder(
            &mut host_store,
            &host_keys.public_key(),
            &[host_event.clone(), join_event.clone()]
        )
        .expect("host settle"),
        Some(1016)
    );
    assert_eq!(
        settle_ladder(
            &mut join_store,
            &join_keys.public_key(),
            &[join_event.clone(), host_event.clone()]
        )
        .expect("join settle"),
        Some(984)
    );
    assert_eq!(rating_of(&host_store, &host_keys.public_key()), Some(1016));
    assert_eq!(rating_of(&join_store, &join_keys.public_key()), Some(984));

    // Replay: folding the same pair again cannot move anything.
    assert_eq!(
        settle_ladder(
            &mut host_store,
            &host_keys.public_key(),
            &[host_event.clone(), join_event.clone()]
        )
        .expect("replay settle"),
        Some(1016),
        "refolding the identical stream is idempotent"
    );

    // finalize consumed its protocol step: re-entry is a no-op returning
    // the same attestation.
    let again = host.finalize().await.expect("idempotent");
    assert_eq!(again.as_ref().map(|e| e.id), Some(host_event.id));
}

/// The Prince's Flail of Penance KOs level-5 Audrics while their mate waits:
/// the joiner owes forced switches, the announce/await rounds carry them in
/// canonical order, and both replicas finish with identical transcripts.
#[tokio::test]
async fn forced_switch_duel_over_net_completes() {
    let host_party = vec![mkchamp(101, 50, &[8])];
    let join_party = vec![mkchamp(1, 5, &[1]), mkchamp(1, 5, &[1])];
    let (t_host, t_join) = memory_pair();

    let mut host = NetDuelDriver::host(
        host_party.clone(),
        join_party.clone(),
        t_host,
        SEED,
        BATTLE_ID,
        Keys::generate().public_key(),
        Keys::generate().public_key(),
        None,
    )
    .expect("host driver");
    let mut join = NetDuelDriver::join(
        join_party,
        host_party,
        t_join,
        SEED,
        BATTLE_ID,
        Keys::generate().public_key(),
        Keys::generate().public_key(),
        None,
    )
    .expect("join driver");

    let join_task = tokio::spawn(async move { drive_to_end(&mut join).await.map(|stats| (stats, join)) });
    drive_to_end(&mut host).await.expect("host drive");
    let (join_stats, join) = join_task.await.expect("join task").expect("join drive");

    assert!(
        join_stats.forced_switches >= 1,
        "the joiner must have announced at least one forced switch"
    );
    assert_eq!(host.battle_result(), Some(0));
    assert_eq!(host.transcript_hash(), join.transcript_hash());
    // No keys at construction: the duel still settles, just without a
    // signed attestation.
    assert!(host.attestation_event().is_none());
    assert!(join.attestation_event().is_none());
}

/// When the peer's transport dies mid-duel, the host's next protocol read
/// surfaces NetError::Closed as AppError::Net — no panic, no hang.
#[tokio::test]
async fn disconnect_mid_duel_surfaces_closed_no_panic() {
    let host_party = vec![mkchamp(1, 50, &[1, 2])];
    let join_party = vec![mkchamp(3, 50, &[1, 6])];
    let (t_host, t_join) = memory_pair();

    let mut host = NetDuelDriver::host(
        host_party.clone(),
        join_party.clone(),
        t_host,
        SEED,
        BATTLE_ID,
        Keys::generate().public_key(),
        Keys::generate().public_key(),
        None,
    )
    .expect("host driver");
    let mut join = NetDuelDriver::join(
        join_party,
        host_party,
        t_join,
        SEED,
        BATTLE_ID,
        Keys::generate().public_key(),
        Keys::generate().public_key(),
        None,
    )
    .expect("join driver");

    let join_task = tokio::spawn(async move {
        // Two honest turns, then vanish (transport drops with the driver)
        // while the host's third turn is mid-handshake.
        for _ in 0..2 {
            let input = greedy_input(&join);
            join.my_turn(input).await.expect("join turn");
        }
        drop(join);
    });

    // Bulked-up mirror match: no faints within two turns, so no forced
    // switches can interleave with the scripted disconnect point.
    for n in 1..=2 {
        assert!(!host.pending_forced_switch(), "no switch owed on turn {n}");
        host.my_turn(greedy_input(&host))
            .await
            .unwrap_or_else(|e| panic!("host turn {n}: {e}"));
    }
    // Ensure the peer is really gone, then step into the void.
    join_task.await.expect("join task");
    let err = host
        .my_turn(greedy_input(&host))
        .await
        .expect_err("third turn hits the closed link");
    assert_eq!(err, AppError::Net(NetError::Closed));
}

/// Sign one kind-9265 attestation for the settle battery below (host is
/// always `host_keys`; the maps `winner` to a player).
fn seal(keys: &Keys, host: &Keys, peer: &Keys, battle: [u8; 32], winner: u8) -> nostr::Event {
    crusade_bridge::sign_attestation(
        keys,
        &crusade_bridge::Attestation {
            digest: hex::encode([0xD1; 32]),
            battle: hex::encode(battle),
            winner,
            host: Some(host.public_key().to_hex()),
            peer: Some(peer.public_key().to_hex()),
        },
    )
    .expect("seal signs")
}

/// V12-1 store-level settle battery: only a verified seal + counter-seal
/// pair moves the persisted rating; forge/self/replay shapes are inert,
/// and a verified second duel rolls forward off the stored checkpoint.
#[test]
fn settle_ladder_writes_only_on_verified_pairs() {
    let host_keys = Keys::generate();
    let join_keys = Keys::generate();
    let pair = |battle: [u8; 32]| {
        vec![
            seal(&host_keys, &host_keys, &join_keys, battle, 0),
            seal(&join_keys, &host_keys, &host_keys, battle, 0),
        ]
    };

    // Verified pair, host won: +16/-16 off the defaults, persisted.
    let mut host_store = RosterStore::new();
    let mut join_store = RosterStore::new();
    let events = pair([0xE1; 32]);
    assert_eq!(
        settle_ladder(&mut host_store, &host_keys.public_key(), &events).unwrap(),
        Some(1016)
    );
    assert_eq!(
        settle_ladder(&mut join_store, &join_keys.public_key(), &events).unwrap(),
        Some(984)
    );

    // Forge: tamper the counter-seal after signing. Store untouched.
    let mut bad = events.clone();
    bad[1].content = bad[1].content.replacen(":0}", ":1}", 1);
    let before = rating_of(&host_store, &host_keys.public_key());
    assert_eq!(
        settle_ladder(&mut host_store, &host_keys.public_key(), &bad).unwrap(),
        Some(1016),
        "the stored checkpoint still seeds the fold, but the forged pair adds nothing"
    );
    assert_eq!(rating_of(&host_store, &host_keys.public_key()), before);

    // Self-sign: one key sealing both sides. Untouched.
    let solo = vec![
        seal(&host_keys, &host_keys, &host_keys, [0xE2; 32], 0),
        seal(&host_keys, &host_keys, &host_keys, [0xE2; 32], 0),
    ];
    let mut solo_store = RosterStore::new();
    assert_eq!(
        settle_ladder(&mut solo_store, &host_keys.public_key(), &solo).unwrap(),
        None
    );
    assert!(solo_store
        .setting(crusade_app::pvp::LADDER_STATE_KEY)
        .unwrap()
        .is_none());

    // Replay: the SAME events again — idempotent, no extra movement.
    assert_eq!(
        settle_ladder(&mut host_store, &host_keys.public_key(), &events).unwrap(),
        Some(1016)
    );

    // Missing counter-seal: only our own half. Untouched.
    let half = vec![events[0].clone()];
    let mut half_store = RosterStore::new();
    assert_eq!(
        settle_ladder(&mut half_store, &host_keys.public_key(), &half).unwrap(),
        None
    );
    assert!(half_store
        .setting(crusade_app::pvp::LADDER_STATE_KEY)
        .unwrap()
        .is_none());

    // Rolling: a second honest duel folds off the persisted checkpoint
    // exactly as both battles folded from scratch would rate us.
    let second = pair([0xE3; 32]);
    let rolled = settle_ladder(&mut host_store, &host_keys.public_key(), &second).unwrap();
    let mut all = events.clone();
    all.extend(second.iter().cloned());
    let fresh_store_rating = crusade_bridge::fold_attestations(
        all.iter(),
        &[],
        &std::collections::BTreeSet::new(),
        &std::collections::BTreeMap::new(),
    )
    .rating(&host_keys.public_key());
    assert_eq!(rolled, fresh_store_rating);

    // A rating claim tendered for the WRONG identity moves nothing of ours.
    let stranger = Keys::generate();
    let mut stranger_store = RosterStore::new();
    assert_eq!(
        settle_ladder(&mut stranger_store, &stranger.public_key(), &events).unwrap(),
        None
    );
    assert!(stranger_store
        .setting(crusade_app::pvp::LADDER_STATE_KEY)
        .unwrap()
        .is_none());
}

/// Two devices, one account: each settles a DIFFERENT duel while forked
/// from the same document, the automerge docs merge (ladder.state is one
/// LWW value — the winner's state arrives whole, never a mixed
/// rating/cursor), and each device then re-presents the seals it holds.
/// Both must converge to the genesis fold of both duels.
#[test]
fn two_devices_converge_after_merge_and_resettle() {
    let me = Keys::generate();
    let foe = Keys::generate();
    let battle1 = vec![
        seal(&me, &me, &foe, [0xF1; 32], 0),
        seal(&foe, &me, &me, [0xF1; 32], 0),
    ];
    let battle2 = vec![
        seal(&me, &me, &foe, [0xF2; 32], 0),
        seal(&foe, &me, &me, [0xF2; 32], 0),
    ];

    // One account, two devices forked from the same document.
    let base = RosterStore::new().save_bytes().expect("base doc saves");
    let mut dev_a = RosterStore::load_bytes(&base).expect("device A loads");
    let mut dev_b = RosterStore::load_bytes(&base).expect("device B loads");

    // Each device settles its own duel while offline from the other.
    let ra1 = settle_ladder(&mut dev_a, &me.public_key(), &battle1).unwrap();
    let rb2 = settle_ladder(&mut dev_b, &me.public_key(), &battle2).unwrap();
    assert_eq!(ra1, Some(1016));
    assert_eq!(rb2, Some(1016));

    // Merge both ways: the docs converge to ONE ladder.state blob
    // (automerge LWW on the single atomic key; which side wins is the
    // CRDT's call, but both devices hold the SAME blob afterwards).
    let a_bytes = dev_a.save_bytes().expect("A saves");
    let b_bytes = dev_b.save_bytes().expect("B saves");
    dev_a.merge_bytes(&b_bytes).expect("A merges B");
    dev_b.merge_bytes(&a_bytes).expect("B merges A");
    let sa = dev_a
        .setting(crusade_app::pvp::LADDER_STATE_KEY)
        .unwrap()
        .unwrap();
    let sb = dev_b
        .setting(crusade_app::pvp::LADDER_STATE_KEY)
        .unwrap()
        .unwrap();
    assert_eq!(sa, sb, "merged devices hold the identical state blob");

    // Both re-present every seal either device holds (the retry/rescan
    // lane re-folds full local history; the cursor makes it idempotent).
    let all: Vec<nostr::Event> = battle1.iter().chain(battle2.iter()).cloned().collect();
    let fa = settle_ladder(&mut dev_a, &me.public_key(), &all).unwrap();
    let fb = settle_ladder(&mut dev_b, &me.public_key(), &all).unwrap();
    assert_eq!(fa, fb, "both devices converge after re-settle");

    // And the convergence point IS the genesis fold of both duels.
    let genesis = crusade_bridge::fold_attestations(
        all.iter(),
        &[],
        &std::collections::BTreeSet::new(),
        &std::collections::BTreeMap::new(),
    );
    assert_eq!(fa, genesis.rating(&me.public_key()));
    // The second duel prices the FOE at their folded 984 (the persisted
    // full table — not a hardcoded 1000): expected ~0.546 -> +14.5 ->
    // +15 -> 1031. (The pre-fix rolling path would have said 1033.)
    assert_eq!(fa, Some(1031));
}

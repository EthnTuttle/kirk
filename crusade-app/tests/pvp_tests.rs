//! End-to-end network duels through `NetDuelDriver`: two drivers over a
//! memory transport pair play scripted greedy games, verify transcript and
//! attestation agreement, exercise the forced-switch rounds, update ladder
//! ratings, and surface disconnects as errors (no panic).
//!
//! Fixtures follow the mkchamp pattern from crusade-core's engine tests:
//! learnset-legal rites, zero grace/devotion, full zeal, resolve at the
//! computed maximum.

use crusade_app::pvp::{record_ladder_result, NetDuelDriver};
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
/// via `my_switch` (first legal target), then `finalize` settles into
/// `store`. Drives ONLY this side — the peer must run concurrently.
async fn drive_to_end(
    d: &mut NetDuelDriver,
    store: &mut RosterStore,
) -> Result<DriveStats, AppError> {
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
    d.finalize(store).await?;
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
        Some(&host_keys),
    )
    .expect("host driver");
    let mut join = NetDuelDriver::join(
        join_party,
        host_party,
        t_join,
        SEED,
        BATTLE_ID,
        Some(&join_keys),
    )
    .expect("join driver");

    let mut host_store = RosterStore::new();
    let mut join_store = RosterStore::new();

    let join_task = tokio::spawn(async move {
        drive_to_end(&mut join, &mut join_store)
            .await
            .map(|stats| (stats, join, join_store))
    });
    let host_stats = drive_to_end(&mut host, &mut host_store)
        .await
        .expect("host drive");
    let (_join_stats, join, join_store) = join_task.await.expect("join task").expect("join drive");

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

    // Ladder: both stores moved off the default 1000 by exactly +/-16.
    assert_eq!(host.ladder_rating(), Some(1016));
    assert_eq!(join.ladder_rating(), Some(984));
    assert_eq!(
        host_store.setting("ladder.rating").unwrap().as_deref(),
        Some("1016")
    );
    assert_eq!(
        join_store.setting("ladder.rating").unwrap().as_deref(),
        Some("984")
    );

    // finalize consumed its protocol step: re-entry is a store-untouched no-op.
    host.finalize(&mut host_store).await.expect("idempotent");
    assert_eq!(
        host_store.setting("ladder.rating").unwrap().as_deref(),
        Some("1016")
    );
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
        None,
    )
    .expect("host driver");
    let mut join = NetDuelDriver::join(join_party, host_party, t_join, SEED, BATTLE_ID, None)
        .expect("join driver");

    let mut host_store = RosterStore::new();
    let mut join_store = RosterStore::new();

    let join_task = tokio::spawn(async move {
        drive_to_end(&mut join, &mut join_store)
            .await
            .map(|stats| (stats, join))
    });
    drive_to_end(&mut host, &mut host_store)
        .await
        .expect("host drive");
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
        None,
    )
    .expect("host driver");
    let mut join = NetDuelDriver::join(join_party, host_party, t_join, SEED, BATTLE_ID, None)
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

/// Ladder Elo math with fixed numbers: fresh stores start at 1000, K=32.
#[test]
fn ladder_rating_records_and_updates_exact() {
    // Win vs equal rating: +16 exactly.
    let mut store = RosterStore::new();
    let new = record_ladder_result(&mut store, true, None).expect("record win");
    assert_eq!(new, 1016);
    assert_eq!(
        store.setting("ladder.rating").unwrap().as_deref(),
        Some("1016")
    );
    // Follow-up loss vs 1000 from 1016: expected score ~0.523 -> -16.74,
    // rounds to -17 -> 999.
    let new = record_ladder_result(&mut store, false, Some(1000)).expect("record loss");
    assert_eq!(new, 999);

    // Loss vs equal rating from default: -16 exactly.
    let mut store = RosterStore::new();
    assert_eq!(record_ladder_result(&mut store, false, None).unwrap(), 984);

    // Win vs a 1600 opponent: expected ~0.0306 -> gain ~30.98 -> 1031.
    let mut store = RosterStore::new();
    assert_eq!(
        record_ladder_result(&mut store, true, Some(1600)).unwrap(),
        1031
    );
}

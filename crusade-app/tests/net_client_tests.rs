//! Tournament of Virtue end-to-end over the embedded-relay fixture
//! (same pattern as `lobby.rs` tests / `pvp_tests.rs`): one player posts a
//! challenge, the other lists and answers it, the lobby seals a
//! `LobbyOutcome`, and BOTH sides then run the exact duel the shell runs —
//! `NostrTransport` cloned off the lobby client + `NetDuelDriver`
//! host/join with `snapshot()` emitting the same in-battle `ViewSnapshot`
//! shapes the Bevy renderer draws verbatim. Covered assertions: challenge
//! posted -> listed, accept -> identical outcomes, outcome -> duel phase,
//! full duel -> matching transcripts, signed attestations, ladder writes.

use std::time::Duration;

use crusade_app::lobby::{Lobby, LobbyCommitment, DEFAULT_TIMEOUT};
use crusade_app::pvp::NetDuelDriver;
use crusade_app::{AppError, ClientInput, Phase};
use crusade_core::data;
use crusade_core::engine::Action;
use crusade_core::types::{ChampionInstance, DevotionStats, GraceStats, RiteSlot};
use crusade_data::RosterStore;
use crusade_net::nostr_transport::NostrTransport;
use nostr::Keys;
use nostr_relay_builder::mock::MockRelay;

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

/// The app's greedy practice policy as a duel turn input.
fn greedy_input(d: &NetDuelDriver) -> ClientInput {
    match crusade_app::policy::greedy_action(d.battle(), d.side()) {
        Action::Rite { slot } => ClientInput::PickRite { slot },
        Action::Switch { party_idx } => ClientInput::PickSwitch { party_idx },
        Action::Yield => ClientInput::Concede,
    }
}

/// Play one side to the end exactly as the shell's duel task does.
async fn drive_to_end(d: &mut NetDuelDriver, store: &mut RosterStore) -> Result<(), AppError> {
    let mut turns = 0u32;
    while d.battle_result().is_none() {
        turns += 1;
        assert!(turns < 500, "duel did not terminate");
        if d.pending_forced_switch() {
            let idx = *d
                .legal_switch_targets()
                .first()
                .expect("pending switch implies a target");
            d.my_switch(idx).await?;
        } else {
            d.my_turn(greedy_input(d)).await?;
        }
    }
    d.finalize(store).await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn posted_challenge_flows_into_a_settled_duel() {
    // `NostrTransport.recv` is subscription-push based with a 30s per-
    // message cap; under whole-workspace test churn the localhost mock
    // relay can starve that window once in a long while. Retry the FULL
    // flow once on exactly that liveness error: fresh relay, fresh keys,
    // fresh duel. Divergence/protocol errors are never retried.
    let mut attempt = 0;
    loop {
        match run_lobby_to_settled_duel().await {
            Ok(()) => return,
            Err(AppError::Net(crusade_net::NetError::Closed)) if attempt == 0 => {
                attempt += 1;
                eprintln!("nostr transport link died under load; retrying on a fresh relay");
            }
            Err(err) => panic!("duel flow failed: {err}"),
        }
    }
}

async fn run_lobby_to_settled_duel() -> Result<(), AppError> {
    let relay = MockRelay::run().await.expect("mock relay starts");
    let url = relay.url();

    let host_keys = Keys::generate();
    let join_keys = Keys::generate();
    let host_pk = host_keys.public_key();
    let join_pk = join_keys.public_key();

    let host = Lobby::connect(host_keys.clone(), &[url.clone()])
        .await
        .expect("host connect");
    let joiner = Lobby::connect(join_keys.clone(), &[url])
        .await
        .expect("joiner connect");

    let host_roster = vec![mkchamp(101, 50, &[8])];
    let join_roster = vec![mkchamp(1, 20, &[1, 2])];

    // 1) Challenge posted by the host...
    let host_c = LobbyCommitment::new(host_roster.clone()).expect("host commitment");
    let challenge = host.publish_challenge(&host_c).await.expect("publish");

    // 2) ...and listed for the joiner (posted -> listed).
    let open = joiner
        .await_challenge(Duration::from_secs(15))
        .await
        .expect("joiner lists the challenge");
    assert_eq!(open.event.id, challenge.id);
    assert_eq!(open.host, host_pk);

    // 3) Accept + seal: both sides derive the identical outcome, then the
    //    joiner runs ITS WHOLE DUEL inside the spawned task (spawn needs a
    //    'static future — the lobby + keys move in), mirroring the shell's
    //    one-task-per-session shape.
    let join_c = LobbyCommitment::new(join_roster.clone()).expect("join commitment");

    let join_task = tokio::spawn(async move {
        let accept = joiner
            .accept(&open, &join_c)
            .await
            .expect("accept publishes");
        let outcome = joiner
            .complete(&open.event, &accept, &join_c, DEFAULT_TIMEOUT)
            .await
            .expect("joiner seals");
        let transport = NostrTransport::new(
            joiner.client().clone(),
            join_keys.clone(),
            outcome.peer,
            outcome.battle_id,
        )
        .await?;
        let mut driver = NetDuelDriver::join(
            outcome.our_roster.clone(),
            outcome.their_roster.clone(),
            transport,
            outcome.seed,
            outcome.battle_id,
            Some(&join_keys),
        )
        .expect("join driver");
        // Outcome -> duel phase.
        let snap = driver.snapshot();
        assert_eq!(snap.phase, Phase::InBattle);
        assert!(snap.me.is_some() && snap.foe.is_some());
        assert!(snap.lobby.is_none(), "a duel snapshot carries no lobby");
        let mut store = RosterStore::new();
        drive_to_end(&mut driver, &mut store).await?;
        joiner.disconnect().await;
        Ok::<_, AppError>((driver, store, outcome))
    });

    let accept = host
        .await_accept(&challenge, DEFAULT_TIMEOUT)
        .await
        .expect("accept arrives");
    let host_out = host
        .complete(&challenge, &accept, &host_c, DEFAULT_TIMEOUT)
        .await
        .expect("host seals");

    let mut host_driver = NetDuelDriver::host(
        host_out.our_roster.clone(),
        host_out.their_roster.clone(),
        NostrTransport::new(
            host.client().clone(),
            host_keys.clone(),
            host_out.peer,
            host_out.battle_id,
        )
        .await?,
        host_out.seed,
        host_out.battle_id,
        Some(&host_keys),
    )
    .expect("host driver");

    // 4) Outcome -> duel phase on the host side too.
    let snap = host_driver.snapshot();
    assert_eq!(snap.phase, Phase::InBattle);
    assert!(snap.me.is_some() && snap.foe.is_some());
    assert!(snap.lobby.is_none(), "a duel snapshot carries no lobby");

    // 5) Full duel over the relay; the joiner task runs concurrently.
    let mut host_store = RosterStore::new();
    drive_to_end(&mut host_driver, &mut host_store).await?;
    let (join_driver, _join_store, join_out) = join_task.await.expect("join task")?;

    assert_eq!(host_out.battle_id, join_out.battle_id);
    assert_eq!(host_out.seed, join_out.seed);
    assert!(host_out.we_host && !join_out.we_host);
    assert_eq!(host_out.peer, join_pk);
    assert_eq!(join_out.peer, host_pk);
    assert_eq!(host_out.our_roster, host_roster);
    assert_eq!(host_out.their_roster, join_roster);
    assert_eq!(join_out.our_roster, join_roster);

    assert_eq!(host_driver.battle_result(), Some(0), "the Prince takes it");
    assert_eq!(
        host_driver.transcript_hash(),
        join_driver.transcript_hash(),
        "relay-carried transcripts agree"
    );
    assert_eq!(host_driver.snapshot().phase, Phase::BattleOver);
    assert_eq!(join_driver.snapshot().phase, Phase::BattleOver);
    assert!(
        host_driver.ladder_rating().is_some() && join_driver.ladder_rating().is_some(),
        "finalize ran the ladder math on both sides"
    );

    // 6) Both attestations verify and agree (B3b publishes them; here we
    //    only prove they exist and match, exactly what the shell consumes).
    let battle_hex = hex::encode(host_out.battle_id);
    let host_att = crusade_bridge::verify_attestation(
        &host_driver.attestation_event().expect("host attestation"),
        &host_pk,
        &battle_hex,
    )
    .expect("host attestation verifies");
    let join_att = crusade_bridge::verify_attestation(
        &join_driver.attestation_event().expect("join attestation"),
        &join_pk,
        &battle_hex,
    )
    .expect("join attestation verifies");
    assert!(crusade_bridge::digests_agree(&host_att, &join_att));

    host.disconnect().await;
    relay.shutdown();
    Ok(())
}

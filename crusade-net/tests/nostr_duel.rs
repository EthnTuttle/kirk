//! Integration test for the nostr-relay fallback transport: two
//! NostrTransport instances duel over an embedded mock relay
//! (nostr_relay_builder::mock::MockRelay), running the same scripted
//! battle fixture as tests/duel.rs to completion.

use std::time::Duration;

use crusade_core::data;
use crusade_core::engine::{Action, Battle};
use crusade_core::{ChampionInstance, DevotionStats, GraceStats, RiteSlot};
use crusade_net::nostr_transport::NostrTransport;
use crusade_net::{DuelSession, NetError, Transport};
use nostr::prelude::*;
use nostr_relay_builder::mock::MockRelay;
use nostr_sdk::Client;

// ---------- fixtures (mirrors tests/duel.rs) ----------

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

/// First-alive switch target when `side` owes a forced switch, else None.
fn forced_target(b: &Battle, side: u8) -> Option<u8> {
    let legal = b.legal_actions(side);
    if !legal.is_empty() && legal.iter().all(|a| matches!(a, Action::Switch { .. })) {
        if let Action::Switch { party_idx } = legal[0] {
            return Some(party_idx);
        }
    }
    None
}

// ---------- scripted policy driver (mirrors tests/duel.rs) ----------

struct DriveOut {
    session: DuelSession,
    turns: u32,
}

/// Resolve pending forced switches in CANONICAL side order: side 0's switch
/// round (announce/await + digest exchange) completes fully before side
/// 1's begins, so the two sides' TurnDigest pairings never cross-fire.
async fn resolve_switches<T: Transport>(
    s: &mut DuelSession,
    side: u8,
    t: &T,
) -> Result<(), NetError> {
    for owed in 0..2u8 {
        while let Some(idx) = forced_target(s.battle(), owed) {
            let applied = if owed == side {
                s.announce_forced_switch(t, idx).await?
            } else {
                s.await_forced_switch(t).await?
            };
            debug_assert!(applied, "pending switch for side {owed} must apply");
        }
    }
    Ok(())
}

async fn drive<T: Transport>(mut s: DuelSession, side: u8, t: &T) -> Result<DriveOut, NetError> {
    let mut turns = 0u32;
    while s.battle().winner.is_none() {
        resolve_switches(&mut s, side, t).await?;
        if s.battle().winner.is_some() {
            break;
        }
        let action = s
            .battle()
            .legal_actions(side)
            .into_iter()
            .find(|a| matches!(a, Action::Rite { .. }))
            .unwrap_or(Action::Yield);
        let r = s.run_turn(t, action).await?;
        turns += 1;
        if r.battle_over {
            break;
        }
    }
    // drive() returning Ok proves this side's finalize() succeeded.
    s.finalize(t).await?;
    Ok(DriveOut { session: s, turns })
}

// ---------- nostr wiring ----------

async fn try_transport_pair(relay_url: &str) -> Result<(NostrTransport, NostrTransport), NetError> {
    let keys0 = Keys::generate();
    let keys1 = Keys::generate();
    let client0 = Client::new(&keys0);
    let client1 = Client::new(&keys1);
    client0
        .add_relay(relay_url)
        .await
        .map_err(|e| NetError::Protocol(format!("add_relay: {e}")))?;
    client1
        .add_relay(relay_url)
        .await
        .map_err(|e| NetError::Protocol(format!("add_relay: {e}")))?;
    client0.connect().await;
    client1.connect().await;
    let battle_id = [9u8; 32];
    let t0 = NostrTransport::new(client0, keys0.clone(), keys1.public_key(), battle_id).await?;
    let t1 = NostrTransport::new(client1, keys1.clone(), keys0.public_key(), battle_id).await?;
    Ok((t0, t1))
}

// ---------- tests ----------

/// Two honest sessions play the duel.rs fixture battle end-to-end over a
/// real (embedded) nostr relay; transcripts, winner, and finalize agree.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn full_duel_over_nostr_relay() {
    let relay = MockRelay::run().await.expect("mock relay starts");
    let (t0, t1) = try_transport_pair(&relay.url())
        .await
        .expect("nostr transport pair");

    let party_a = vec![mkchamp(1, 20, &[1, 2])];
    let party_b = vec![mkchamp(3, 20, &[1, 6])];
    let b0 = Battle::new(party_a.clone(), party_b.clone(), [9; 32]).expect("legal fixture battle");
    let b1 = Battle::new(party_a, party_b, [9; 32]).expect("legal fixture battle");
    let s0 = DuelSession::new(b0, 0);
    let s1 = DuelSession::new(b1, 1);

    let side1 = tokio::spawn(async move { drive(s1, 1, &t1).await });
    let out0 = drive(s0, 0, &t0).await.expect("side0 drive");
    let out1 = side1.await.expect("side1 join").expect("side1 drive");

    assert_eq!(
        out0.session.transcript_hash(),
        out1.session.transcript_hash(),
        "transcript hashes must agree after an honest duel"
    );
    let (w0, w1) = (out0.session.battle().winner, out1.session.battle().winner);
    assert_eq!(w0, w1, "replicas must agree on the winner");
    assert!(w0.is_some(), "battle must have ended");
    assert!(
        out0.turns < 100 && out1.turns < 100,
        "battle should end well under 100 turns (got {} / {})",
        out0.turns,
        out1.turns
    );
    relay.shutdown();
}

/// Smoke: one shot send/recv through the embedded relay, exercising the
/// lazy subscription path on its own before the full duel hammers it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn send_recv_roundtrip_over_relay() {
    let relay = MockRelay::run().await.expect("mock relay starts");
    let (t0, t1) = try_transport_pair(&relay.url())
        .await
        .expect("nostr transport pair");

    let give = crusade_net::msg::NetMsg::Commit {
        turn: 1,
        hash: [42u8; 32],
    };
    // Force t1's subscription up before the event lands to assert live
    // delivery (the duel test additionally covers stored-event catch-up).
    let (sent, recv) = tokio::join!(t0.send(&give), async {
        // Small head start so the REQ is in before the publish; the
        // transport itself does not require this (since-tag catch-up), it
        // just keeps this smoke test deterministic.
        tokio::time::sleep(Duration::from_millis(50)).await;
        t1.recv().await
    });
    sent.expect("send");
    let got = recv.expect("recv");
    assert_eq!(got, give);
    relay.shutdown();
}

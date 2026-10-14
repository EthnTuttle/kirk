#![cfg(feature = "iroh-quic")]
//! Integration test: the full commit-reveal duel protocol over a real iroh
//! QUIC connection on localhost. Mirrors tests/duel.rs, swapping
//! MemoryTransport for IrohTransport.

use crusade_core::data;
use crusade_core::engine::{Action, Battle};
use crusade_core::{ChampionInstance, DevotionStats, GraceStats, RiteSlot};
use crusade_net::iroh_transport::DuelEndpoint;
use crusade_net::{DuelSession, NetError, Transport};

// ---------- fixtures (mirrors crusade-core engine/tests.rs mkchamp) ----------

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

// ---------- scripted policy driver ----------

struct DriveOut {
    session: DuelSession,
    turns: u32,
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

/// Play to completion: legal rite slot 0 policy; forced-switch rounds via
/// the announce/await pair; FinalDigest exchange at the end.
async fn drive<T: Transport>(mut s: DuelSession, side: u8, t: &T) -> Result<DriveOut, NetError> {
    let mut turns = 0u32;
    while s.battle().winner.is_none() {
        // Resolve pending forced switches (either side) before the next turn.
        loop {
            let mut progressed = false;
            if let Some(idx) = forced_target(s.battle(), side) {
                if s.announce_forced_switch(t, idx).await? {
                    progressed = true;
                }
            }
            if s.await_forced_switch(t).await? {
                progressed = true;
            }
            if !progressed {
                break;
            }
        }
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
    s.finalize(t).await?;
    Ok(DriveOut { session: s, turns })
}

// ---------- test ----------

/// Two endpoints on 127.0.0.1 exchange tickets, one accepts and the other
/// connects, then a full duel runs over the iroh QUIC link.
///
/// If the sandbox cannot bind UDP at all, the test prints a SKIP note and
/// passes — the machine is expected to offer loopback UDP.
#[tokio::test]
async fn duel_over_iroh_localhost() {
    let ep_a = match DuelEndpoint::bind().await {
        Ok(ep) => ep,
        Err(e) => {
            eprintln!("SKIP duel_over_iroh_localhost: iroh bind unavailable here: {e}");
            println!("SKIP duel_over_iroh_localhost: no UDP in this sandbox ({e})");
            return;
        }
    };
    let ep_b = match DuelEndpoint::bind().await {
        Ok(ep) => ep,
        Err(e) => {
            eprintln!("SKIP duel_over_iroh_localhost: iroh bind unavailable here: {e}");
            println!("SKIP duel_over_iroh_localhost: no UDP in this sandbox ({e})");
            return;
        }
    };

    let ticket_a = ep_a.node_ticket().await.expect("side A ticket");

    let party_a = vec![mkchamp(1, 20, &[1, 2])];
    let party_b = vec![mkchamp(3, 20, &[1, 6])];
    let b0 = Battle::new(party_a.clone(), party_b.clone(), [11; 32]).expect("legal fixture battle");
    let b1 = Battle::new(party_a, party_b, [11; 32]).expect("legal fixture battle");
    let s0 = DuelSession::new(b0, 0);
    let s1 = DuelSession::new(b1, 1);

    // Side B dials side A's ticket; side A's endpoint accepts. A's
    // session drives engine side 0, B's drives side 1.
    let side_b = tokio::spawn(async move {
        let t = ep_b.connect(&ticket_a).await?;
        drive(s1, 1, &t).await
    });
    let t0 = ep_a.accept().await.expect("accept duel connection");
    // drive() returning Ok implies finalize() verified Ok on that side.
    let out0 = drive(s0, 0, &t0).await.expect("side0 drive");
    let out1 = side_b.await.expect("side1 join").expect("side1 drive");

    assert_eq!(
        out0.session.transcript_hash(),
        out1.session.transcript_hash(),
        "transcript hashes must agree after an honest duel over iroh"
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
}

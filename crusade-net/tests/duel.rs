//! Integration tests for the commit-reveal duel protocol over
//! MemoryTransport: happy-path completion, forced-switch rounds, and the
//! two failure detectors (commitment mismatch, transcript divergence).

use async_trait::async_trait;
use crusade_core::data;
use crusade_core::engine::{Action, Battle};
use crusade_core::{ChampionInstance, DevotionStats, GraceStats, RiteSlot};
use crusade_net::memory::{memory_pair, MemoryTransport};
use crusade_net::msg::NetMsg;
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

/// Two identical replicas (one per session side) plus a connected link pair.
fn duel_setup(
    party_a: Vec<ChampionInstance>,
    party_b: Vec<ChampionInstance>,
    seed: [u8; 32],
) -> (DuelSession, DuelSession, MemoryTransport, MemoryTransport) {
    let b0 = Battle::new(party_a.clone(), party_b.clone(), seed).expect("legal fixture battle");
    let b1 = Battle::new(party_a, party_b, seed).expect("legal fixture battle");
    let (t0, t1) = memory_pair();
    (DuelSession::new(b0, 0), DuelSession::new(b1, 1), t0, t1)
}

// ---------- scripted policy driver ----------

struct DriveOut {
    session: DuelSession,
    turns: u32,
    announced: u32,
}

/// First-alive switch target when `side` owes a forced switch, else None
/// (legal_actions narrows to all-Switch exactly when a switch is pending).
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
    let mut announced = 0u32;
    while s.battle().winner.is_none() {
        // Resolve pending forced switches (either side) before the next turn.
        loop {
            let mut progressed = false;
            if let Some(idx) = forced_target(s.battle(), side) {
                if s.announce_forced_switch(t, idx).await? {
                    announced += 1;
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
    Ok(DriveOut {
        session: s,
        turns,
        announced,
    })
}

// ---------- tests ----------

/// Two honest sessions play a full battle; transcripts converge and the
/// FinalDigest exchange succeeds on both sides.
#[tokio::test]
async fn full_duel_over_memory() {
    let party_a = vec![mkchamp(1, 20, &[1, 2])];
    let party_b = vec![mkchamp(3, 20, &[1, 6])];
    let (s0, s1, t0, t1) = duel_setup(party_a, party_b, [9; 32]);

    let side1 = tokio::spawn(async move { drive(s1, 1, &t1).await });
    // drive() returning Ok implies finalize() verified Ok on that side.
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
}

/// side1's Audrics fall to the Prince's Flail of Penance; the first faint
/// raises ForcedSwitchNeeded and the announce/await pair carries it.
#[tokio::test]
async fn forced_switch_protocol() {
    let party_a = vec![mkchamp(101, 50, &[8])];
    let party_b = vec![mkchamp(1, 5, &[1]), mkchamp(1, 5, &[1])];
    let (s0, s1, t0, t1) = duel_setup(party_a, party_b, [7; 32]);

    let side1 = tokio::spawn(async move { drive(s1, 1, &t1).await });
    let out0 = drive(s0, 0, &t0).await.expect("side0 drive");
    let out1 = side1.await.expect("side1 join").expect("side1 drive");

    assert!(
        out1.announced >= 1,
        "side1 must have announced at least one forced switch"
    );
    assert_eq!(
        out0.session.transcript_hash(),
        out1.session.transcript_hash()
    );
    assert_eq!(out0.session.battle().winner, Some(0));
    assert_eq!(out1.session.battle().winner, Some(0));
}

/// Wraps a MemoryTransport and corrupts every outgoing Reveal's action
/// (the Commit has already gone out with the honest hash) — the moral
/// equivalent of a one-byte bit-flip in the serialized action.
struct TamperReveal {
    inner: MemoryTransport,
}

fn flip_action(a: &Action) -> Action {
    match a {
        Action::Rite { slot } => Action::Rite { slot: slot ^ 1 },
        Action::Switch { party_idx } => Action::Switch {
            party_idx: party_idx ^ 1,
        },
        Action::Yield => Action::Rite { slot: 0 },
    }
}

#[async_trait]
impl Transport for TamperReveal {
    async fn send(&self, msg: &NetMsg) -> Result<(), NetError> {
        match msg {
            NetMsg::Reveal { turn, action } => {
                self.inner
                    .send(&NetMsg::Reveal {
                        turn: *turn,
                        action: flip_action(action),
                    })
                    .await
            }
            other => self.inner.send(other).await,
        }
    }
    async fn recv(&self) -> Result<NetMsg, NetError> {
        self.inner.recv().await
    }
}

/// The honest side must reject a reveal that fails to open its commitment,
/// before any engine state is applied.
#[tokio::test]
async fn commitment_mismatch_detected() {
    let party_a = vec![mkchamp(1, 20, &[1, 2])];
    let party_b = vec![mkchamp(3, 20, &[1, 6])];
    let (mut s0, mut s1, t0, t1) = duel_setup(party_a, party_b, [3; 32]);
    let tampered = TamperReveal { inner: t1 };

    // side1 (behind the tampering transport) proceeds past verification and
    // then hangs awaiting a TurnDigest that never arrives; abort it after
    // the honest side has rejected the reveal.
    let side1 = tokio::spawn(async move { s1.run_turn(&tampered, Action::Rite { slot: 0 }).await });

    let err = s0
        .run_turn(&t0, Action::Rite { slot: 0 })
        .await
        .expect_err("forged reveal must be rejected");
    assert_eq!(err, NetError::CommitmentMismatch);
    assert_eq!(s0.battle().turn, 1, "turn must not advance");
    assert!(s0.battle().events().is_empty(), "no events applied");
    assert_eq!(s0.transcript_hash(), [0u8; 32], "digest untouched");

    side1.abort();
    let _ = side1.await;
}

/// Wraps a MemoryTransport and forges a wrong digest on every outgoing
/// TurnDigest; commits and reveals pass through honestly.
struct ForgeDigest {
    inner: MemoryTransport,
}

#[async_trait]
impl Transport for ForgeDigest {
    async fn send(&self, msg: &NetMsg) -> Result<(), NetError> {
        match msg {
            NetMsg::TurnDigest { turn, hash } => {
                let mut forged = *hash;
                forged[0] ^= 0xFF;
                self.inner
                    .send(&NetMsg::TurnDigest {
                        turn: *turn,
                        hash: forged,
                    })
                    .await
            }
            other => self.inner.send(other).await,
        }
    }
    async fn recv(&self) -> Result<NetMsg, NetError> {
        self.inner.recv().await
    }
}

/// Honest reveals on both sides, but side1's outgoing digest is forged:
/// side0's replica state is fine, yet the digest comparison must still
/// catch the divergence this turn.
#[tokio::test]
async fn digest_catching() {
    let party_a = vec![mkchamp(1, 20, &[1, 2])];
    let party_b = vec![mkchamp(3, 20, &[1, 6])];
    let (mut s0, mut s1, t0, t1) = duel_setup(party_a, party_b, [4; 32]);
    let forger = ForgeDigest { inner: t1 };

    // side1's own view is consistent (it receives side0's honest digest),
    // so its run_turn succeeds; only side0 observes the forgery.
    let side1 = tokio::spawn(async move { s1.run_turn(&forger, Action::Rite { slot: 0 }).await });

    let err = s0
        .run_turn(&t0, Action::Rite { slot: 0 })
        .await
        .expect_err("forged digest must be caught");
    assert_eq!(err, NetError::TranscriptDiverged(1));

    let r1 = side1.await.expect("side1 join");
    assert!(r1.is_ok(), "forging side's local view is self-consistent");
}

//! Out-of-order wire tolerance: a transport that delays the sender's Commit
//! by one message (the peer observes Reveal BEFORE Commit, once) must not
//! kill the session — the duel layer stashes the early Reveal and completes
//! the duel with matching transcripts.

use async_trait::async_trait;
use crusade_core::data;
use crusade_core::engine::{Action, Battle};
use crusade_core::{ChampionInstance, DevotionStats, GraceStats, RiteSlot};
use crusade_net::memory::{memory_pair, MemoryTransport};
use crusade_net::msg::NetMsg;
use crusade_net::{DuelSession, NetError, Transport};
use tokio::sync::Mutex;

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

// ---------- shuffle transport ----------

#[derive(Default)]
struct ShuffleState {
    held: Option<NetMsg>,
    done: bool,
}

/// Wraps a MemoryTransport; the FIRST outgoing Commit is held back and
/// flushed only after the next outgoing message, so the peer receives that
/// message (in the duel protocol: the Reveal) one slot before the Commit.
struct ShuffleTransport {
    inner: MemoryTransport,
    state: Mutex<ShuffleState>,
}

impl ShuffleTransport {
    fn new(inner: MemoryTransport) -> Self {
        Self {
            inner,
            state: Mutex::new(ShuffleState::default()),
        }
    }
}

#[async_trait]
impl Transport for ShuffleTransport {
    async fn send(&self, msg: &NetMsg) -> Result<(), NetError> {
        {
            let mut g = self.state.lock().await;
            if !g.done && matches!(msg, NetMsg::Commit { .. }) {
                g.done = true;
                g.held = Some(msg.clone());
                return Ok(());
            }
        }
        self.inner.send(msg).await?;
        let held = self.state.lock().await.held.take();
        if let Some(held) = held {
            self.inner.send(&held).await?;
        }
        Ok(())
    }

    async fn recv(&self) -> Result<NetMsg, NetError> {
        self.inner.recv().await
    }
}

// ---------- scripted policy driver (mirrors tests/duel.rs) ----------

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

/// Resolve pending forced switches in canonical side order (side 0's round
/// fully completes before side 1's begins).
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
    s.finalize(t).await?;
    Ok(DriveOut { session: s, turns })
}

// ---------- test ----------

/// Full scripted duel (species 1 vs 3 fixtures) over a link where side1's
/// first Commit is delivered AFTER its Reveal: the duel must complete Ok
/// with equal transcripts on both sides.
#[tokio::test]
async fn delayed_commit_duel_completes() {
    let party_a = vec![mkchamp(1, 20, &[1, 2])];
    let party_b = vec![mkchamp(3, 20, &[1, 6])];
    let b0 = Battle::new(party_a.clone(), party_b.clone(), [9; 32]).expect("legal fixture battle");
    let b1 = Battle::new(party_a, party_b, [9; 32]).expect("legal fixture battle");
    let (t0, t1) = memory_pair();
    let shuffled = ShuffleTransport::new(t1);

    let side1 = tokio::spawn(async move { drive(DuelSession::new(b1, 1), 1, &shuffled).await });
    let out0 = drive(DuelSession::new(b0, 0), 0, &t0)
        .await
        .expect("side0 drive");
    let out1 = side1.await.expect("side1 join").expect("side1 drive");

    assert_eq!(
        out0.session.transcript_hash(),
        out1.session.transcript_hash(),
        "transcripts agree despite the reordered Commit/Reveal"
    );
    assert_eq!(out0.session.battle().winner, out1.session.battle().winner);
    assert!(
        out0.session.battle().winner.is_some(),
        "battle must have ended"
    );
    assert!(out0.turns < 100 && out1.turns < 100);
}

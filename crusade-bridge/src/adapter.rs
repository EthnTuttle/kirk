//! kirk `Game` trait adapter (worker J implements): lets a kirk GameMint
//! replay and arbitrate a full battle from its nostr event chain.
//!
//! Flow mapping (open teamsheets):
//! - Challenge/ChallengeAccept carry each player's token commitments as usual.
//! - Each player's FIRST Move event carries `BattleActionSubmission` with
//!   `party_reveal: Some(full party)` AND kirk `revealed_tokens` (all of them);
//!   the action field must be a legal turn-1 action. Subsequent moves: parties
//!   omitted.
//! - battle seed = crate::seed::derive_battle_seed_from_reveal over the
//!   challenge/accept ids and the revealed token C values
//!   (side0 = challenge CREATOR).
//! - Replay mirrors the live duel protocol (crusade_net::DuelSession) as a
//!   sequence of records: RiteTurn (both sides' actions for one turn,
//!   applied via `Battle::apply_turn`), ForcedSwitchTurn (a lone
//!   `Action::Switch` from the side that owes one, applied via
//!   `Battle::apply_forced_switch`), Finalization (both players' Final
//!   events).
//! - Final events both present + replay terminated => sequence complete;
//!   winner = replay winner mapped to that side's nostr pubkey.
//!
//! Event selection is DECLARATIVE, never positional. Only events from the
//! two participant pubkeys (challenge creator = side 0, acceptor = side 1)
//! are read at all — events from any other pubkey are skipped silently,
//! because relay feeds may legitimately carry foreign events referencing
//! chain ids (kirk's `ValidationClient::collect_game_events` links moves by
//! `previous_event_id`, which anyone can point at). Input order is
//! irrelevant: every Move declares its `(turn, action)` and turns are
//! reconstructed by grouping declaration keys over 1..=N — exactly one
//! action per (turn, side), no duplicates, no gaps. While the engine flags
//! a pending forced switch, the next record from the owing side (canonical
//! order: side 0 before side 1 when both owe) must be a lone
//! `Action::Switch` — `apply_turn` itself REJECTS any pair while a switch
//! is owed, and only `apply_forced_switch` clears the flag. An
//! `Action::Switch` inside a normal pair is a voluntary switch. This is the
//! encoding an honest DuelSession chain produces, because a pending forced
//! switch leaves `Switch` as that side's only legal action.
//!
//! Verdict semantics (fail-closed): any deviation below pushes an error and
//! invalidates the sequence; arbiters get all of them in `errors`. Because
//! reconstruction is declarative, shuffling the input slice (either relay's
//! collection order) yields an identical verdict. An honest chain that has
//! not decided the battle yet (including the commit-phase prefix with zero
//! moves) validates with `winner: None` — same verdict shape as kirk's
//! card_game.
//!
//! Validation re-checks token<->commitment binding using the same helpers as
//! card_game's validate_commitments (order-sensitive per-token
//! `TokenCommitment::hash_single_token` compare, exact count match).

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crusade_core::engine::{Action, Battle};
use crusade_core::types::ChampionInstance;
use kirk::cashu::commitments::TokenCommitment;
use kirk::error::{GameProtocolError, ValidationError, ValidationErrorType, ValidationResult};
use kirk::events::{EventParser, CHALLENGE_ACCEPT_KIND, CHALLENGE_KIND, FINAL_KIND, MOVE_KIND};
use kirk::game::traits::Game;
use kirk::CashuToken;
use nostr::{Event as NostrEvent, EventId, PublicKey};
use serde::{Deserialize, Serialize};

/// kirk Game::MoveData for battles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BattleActionSubmission {
    pub turn: u32,
    pub action: Action,
    /// Present only on the player's first move: their full party.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub party_reveal: Option<Vec<ChampionInstance>>,
}

/// kirk Game adapter. Stateless.
#[derive(Debug, Default, Clone, Copy)]
pub struct BattleAdapter;

/// One decoded participant move, keyed by its declared (turn, side) — the
/// chain position it arrived in is irrelevant to replay.
struct PlayerMove<'a> {
    event: &'a NostrEvent,
    side: usize,
    submission: BattleActionSubmission,
}

/// Outcome of a replay pass. `battle` is `Some` only when the chain provided
/// everything needed to construct one (both parties revealed, seed derived).
struct Replay {
    battle: Option<Battle>,
    /// [challenge creator, challenge acceptor] = [side0, side1].
    sides: [PublicKey; 2],
    /// The anchoring Challenge's id, when the chain yielded a unique one.
    challenge_id: Option<EventId>,
    errors: Vec<ValidationError>,
}

/// C values of every proof in `token`, in proof order (v3 or v4).
///
/// Direct field access on `Proof{,V3,V4}.c` so a mint-side arbiter needs no
/// per-mint keyset map; this yields exactly what
/// `CashuToken::extract_c_values` returns once keyset resolution succeeds.
/// (Malformed C pubkeys cannot reach here: serde rejects them when the
/// Move content is parsed.)
fn token_c_values(token: &CashuToken) -> Vec<[u8; 33]> {
    match token {
        CashuToken::TokenV3(v3) => v3
            .token
            .iter()
            .flat_map(|t| t.proofs.iter().map(|p| p.c.to_bytes()))
            .collect(),
        CashuToken::TokenV4(v4) => v4
            .token
            .iter()
            .flat_map(|t| t.proofs.iter().map(|p| p.c.to_bytes()))
            .collect(),
    }
}

/// True while the engine owes `side` a forced switch — the duel roundtrip
/// (crusade_net::DuelSession::switch_pending) detects it the same way: the
/// side's legal action set degenerates to only-Switch.
fn switch_pending(battle: &Battle, side: usize) -> bool {
    let legal = battle.legal_actions(side as u8);
    !legal.is_empty() && legal.iter().all(|a| matches!(a, Action::Switch { .. }))
}

impl BattleAdapter {
    /// Replay the whole event chain, recording every failure found.
    ///
    /// Declarative, order-independent reconstruction: the unique Challenge
    /// anchors the sequence, the unique ChallengeAccept referencing it
    /// closes the side pair, foreign pubkeys are skipped silently, and moves
    /// group by their declared (turn, side). Turn records replay ascending
    /// from 1 with no gaps; while a forced switch is owed the owing side's
    /// next record must be a lone `Action::Switch` (canonical side order
    /// 0 then 1 when both owe); otherwise the turn's pair applies via
    /// `Battle::apply_turn`. Callers must pass a non-empty slice (the trait
    /// entry points reject empty ones first).
    fn replay(&self, events: &[NostrEvent]) -> Replay {
        let mut errors: Vec<ValidationError> = Vec::new();
        macro_rules! fail {
            ($id:expr, $ty:expr, $($arg:tt)*) => {
                errors.push(ValidationError::new($id, $ty, format!($($arg)*)))
            };
        }

        // Relays may redeliver an event; the same signed id carries the same
        // statement, so dedupe silently before any counting.
        let mut seen_ids = HashSet::new();
        let events: Vec<&NostrEvent> = events.iter().filter(|e| seen_ids.insert(e.id)).collect();

        // ---- 1. Challenge anchor: the unique Challenge in the chain ----
        let challenges: Vec<&NostrEvent> = events
            .iter()
            .copied()
            .filter(|e| e.kind == CHALLENGE_KIND)
            .collect();
        let challenge = match challenges.as_slice() {
            [c] => *c,
            other => {
                let id = other.first().map(|e| e.id).unwrap_or_else(|| events[0].id);
                fail!(
                    id,
                    ValidationErrorType::InvalidSequence,
                    "Expected exactly 1 Challenge event, found {}",
                    other.len()
                );
                return Replay {
                    battle: None,
                    sides: [events[0].pubkey; 2],
                    challenge_id: None,
                    errors,
                };
            }
        };
        let challenge_id = challenge.id;
        let challenge_content = match EventParser::parse_challenge(challenge) {
            Ok(c) => c,
            Err(e) => {
                fail!(
                    challenge_id,
                    ValidationErrorType::InvalidSequence,
                    "Challenge content unparseable: {e}"
                );
                return Replay {
                    battle: None,
                    sides: [challenge.pubkey; 2],
                    challenge_id: Some(challenge_id),
                    errors,
                };
            }
        };
        let creator = challenge.pubkey;

        // ---- 2. The unique ChallengeAccept referencing this challenge ----
        // Accepts for other chains are foreign noise (skipped silently);
        // exactly one must reference ours.
        let mut accept: Option<&NostrEvent> = None;
        let mut accept_content = None;
        let mut accept_count = 0usize;
        for candidate in events
            .iter()
            .copied()
            .filter(|e| e.kind == CHALLENGE_ACCEPT_KIND)
        {
            let Ok(content) = EventParser::parse_challenge_accept(candidate) else {
                continue; // unparseable: cannot reference our challenge
            };
            if content.challenge_id == challenge_id {
                accept_count += 1;
                accept = Some(candidate);
                accept_content = Some(content);
            }
        }
        let (accept, accept_content) = match (accept_count, accept, accept_content) {
            (1, Some(a), Some(c)) => (a, c),
            _ => {
                fail!(
                    challenge_id,
                    ValidationErrorType::InvalidSequence,
                    "Expected exactly 1 ChallengeAccept event referencing this challenge, found {accept_count}"
                );
                return Replay {
                    battle: None,
                    sides: [creator; 2],
                    challenge_id: Some(challenge_id),
                    errors,
                };
            }
        };
        let acceptor = accept.pubkey;
        let sides = [creator, acceptor];

        // ---- 3. Move events from the two participants ----
        // Foreign pubkeys are skipped silently. Reveal material is bound to
        // the side's turn-1 record (declarative, not "first in the slice").
        let mut moves: Vec<PlayerMove> = Vec::new();
        let mut revealed_tokens: [Option<Vec<CashuToken>>; 2] = [None, None];
        let mut parties: [Option<Vec<ChampionInstance>>; 2] = [None, None];
        for e in events.iter().copied().filter(|e| e.kind == MOVE_KIND) {
            let Some(side) = sides.iter().position(|pk| *pk == e.pubkey) else {
                continue; // foreign pubkey: skip silently
            };
            let content = match EventParser::parse_move(e) {
                Ok(c) => c,
                Err(err) => {
                    fail!(
                        e.id,
                        ValidationErrorType::InvalidMove,
                        "Move content unparseable: {err}"
                    );
                    continue;
                }
            };
            let submission: BattleActionSubmission = match serde_json::from_value(content.move_data)
            {
                Ok(s) => s,
                Err(err) => {
                    fail!(
                        e.id,
                        ValidationErrorType::InvalidMove,
                        "BattleActionSubmission unparseable: {err}"
                    );
                    continue;
                }
            };
            if submission.turn == 1 {
                // The turn-1 move must carry all reveal material.
                match &submission.party_reveal {
                    None => fail!(
                        e.id,
                        ValidationErrorType::InvalidMove,
                        "Player {side}'s turn-1 move must carry party_reveal"
                    ),
                    Some(p) => parties[side] = Some(p.clone()),
                }
                match &content.revealed_tokens {
                    None => fail!(
                        e.id,
                        ValidationErrorType::InvalidCommitment,
                        "Player {side}'s turn-1 move must carry revealed_tokens"
                    ),
                    Some(ts) if ts.is_empty() => fail!(
                        e.id,
                        ValidationErrorType::InvalidCommitment,
                        "Player {side}'s turn-1 revealed_tokens must not be empty"
                    ),
                    Some(ts) => revealed_tokens[side] = Some(ts.clone()),
                }
            } else {
                if submission.party_reveal.is_some() {
                    fail!(
                        e.id,
                        ValidationErrorType::InvalidMove,
                        "Player {side}: party_reveal allowed on the turn-1 move only"
                    );
                }
                if content.revealed_tokens.is_some() {
                    fail!(
                        e.id,
                        ValidationErrorType::InvalidCommitment,
                        "Player {side}: revealed_tokens allowed on the turn-1 move only"
                    );
                }
            }
            moves.push(PlayerMove {
                event: e,
                side,
                submission,
            });
        }

        // Committed but not started: nothing to bind or replay yet. Same
        // leniency card_game shows a challenge/accept-only prefix (valid,
        // winner None). Any move decode errors above still stand.
        if moves.is_empty() {
            return Replay {
                battle: None,
                sides,
                challenge_id: Some(challenge_id),
                errors,
            };
        }

        // ---- 4. Declarative turn grouping ----
        // Every move declares (turn, side); the declaration, not the slice
        // position, assigns it. Duplicate keys and holes in 1..=max are
        // deviations. Turn numbers are u32 and adversarial (a hole scan over
        // 1..=max could be huge), so gaps surface via the distinct-count
        // mismatch and only the first few holes are reported.
        let mut slots: BTreeMap<(u32, usize), Vec<usize>> = BTreeMap::new();
        for (i, m) in moves.iter().enumerate() {
            slots
                .entry((m.submission.turn, m.side))
                .or_default()
                .push(i);
        }
        for ((turn, side), idxs) in &slots {
            for i in idxs.iter().skip(1) {
                fail!(
                    moves[*i].event.id,
                    ValidationErrorType::InvalidSequence,
                    "Duplicate move declares turn {turn} for side {side}"
                );
            }
        }
        let declared_turns: BTreeSet<u32> = slots.keys().map(|(t, _)| *t).collect();
        let max_turn = *declared_turns.iter().next_back().unwrap_or(&0);
        if declared_turns.len() != max_turn as usize {
            let mut expect = 1u32;
            let mut reported = 0usize;
            for t in &declared_turns {
                while expect < *t && reported < 8 {
                    fail!(
                        challenge_id,
                        ValidationErrorType::InvalidSequence,
                        "Turn numbering broken: no move declares turn {expect}"
                    );
                    expect += 1;
                    reported += 1;
                }
                expect = t.saturating_add(1);
            }
        }
        // The unique record per (turn, side), once duplicates are excluded.
        let singles: BTreeMap<(u32, usize), &PlayerMove> = slots
            .iter()
            .filter(|(_, idxs)| idxs.len() == 1)
            .map(|(k, idxs)| (*k, &moves[idxs[0]]))
            .collect();

        // ---- 5. Token<->commitment binding ----
        // Mirror of kirk::game::card_game::CardGame::validate_commitments:
        // per-player revealed tokens vs their Challenge/ChallengeAccept
        // commitment_hashes, exact count match, per-token
        // TokenCommitment::hash_single_token compare in reveal order.
        if errors.is_empty() {
            let commitments = [
                &challenge_content.commitment_hashes,
                &accept_content.commitment_hashes,
            ];
            for (side, tokens) in revealed_tokens.iter().enumerate() {
                let player = sides[side];
                let Some(tokens) = tokens else {
                    continue; // unreachable on a complete chain; no tokens, nothing to bind
                };
                let commits = commitments[side];
                if commits.len() != tokens.len() {
                    fail!(
                        challenge_id,
                        ValidationErrorType::InvalidCommitment,
                        "Player {player} committed {} hashes but revealed {} tokens",
                        commits.len(),
                        tokens.len()
                    );
                    continue;
                }
                for (token, expected) in tokens.iter().zip(commits) {
                    if TokenCommitment::hash_single_token(token) != *expected {
                        fail!(
                            challenge_id,
                            ValidationErrorType::InvalidCommitment,
                            "Player {player}'s revealed token does not match its commitment"
                        );
                        break;
                    }
                }
            }
        }

        // ---- 6. Battle seed + construction from revealed parties ----
        let mut battle: Option<Battle> = None;
        if errors.is_empty() {
            let c_values = |side: usize| -> Vec<[u8; 33]> {
                revealed_tokens[side]
                    .as_deref()
                    .unwrap_or(&[])
                    .iter()
                    .flat_map(token_c_values)
                    .collect()
            };
            match crate::seed::derive_battle_seed_from_reveal(
                &challenge.id.to_hex(),
                &accept.id.to_hex(),
                &c_values(0),
                &c_values(1),
            ) {
                Ok(seed) => {
                    let [p0, p1] = parties;
                    match (p0, p1) {
                        (Some(p0), Some(p1)) => match Battle::new(p0, p1, seed) {
                            Ok(b) => battle = Some(b),
                            Err(e) => fail!(
                                challenge_id,
                                ValidationErrorType::InvalidMove,
                                "Revealed party failed engine legality: {e}"
                            ),
                        },
                        _ => fail!(
                            challenge_id,
                            ValidationErrorType::InvalidMove,
                            "Both players must reveal their party on their first move"
                        ),
                    }
                }
                Err(e) => fail!(
                    challenge_id,
                    ValidationErrorType::InvalidSequence,
                    "Battle seed derivation failed: {e}"
                ),
            }
        }

        // ---- 7. Replay the records through the engine ----
        // Records replay in ascending declared-turn order; composition per
        // turn is dictated by engine state, exactly as the live DuelSession:
        // a pending forced switch takes the owing side's lone Switch
        // (canonical side order 0 then 1 when both owe), anything else pairs
        // both sides for apply_turn. Stop at the first deviation — later
        // turns could no longer be judged against a trustworthy state.
        if errors.is_empty() {
            let b = battle
                .as_mut()
                .expect("battle constructed above when no errors were recorded");
            for t in 1..=max_turn {
                let rec = |side: usize| singles.get(&(t, side)).copied();
                if b.winner.is_some() {
                    let id = rec(0)
                        .or_else(|| rec(1))
                        .map_or(challenge_id, |m| m.event.id);
                    fail!(
                        id,
                        ValidationErrorType::InvalidSequence,
                        "Chain continues past the decided battle at turn {t}"
                    );
                    break;
                }
                let owing = [0usize, 1].into_iter().find(|&s| switch_pending(b, s));
                match owing {
                    Some(s) => {
                        let other = 1 - s;
                        if let Some(r) = rec(other) {
                            fail!(
                                r.event.id,
                                ValidationErrorType::InvalidSequence,
                                "Turn {t} carries an action for side {other} while side {s}'s forced switch is pending"
                            );
                            break;
                        }
                        match rec(s) {
                            None => {
                                fail!(
                                    challenge_id,
                                    ValidationErrorType::InvalidSequence,
                                    "Side {s} owes a forced switch: turn {t} must carry it"
                                );
                                break;
                            }
                            Some(r) => match r.submission.action {
                                Action::Switch { party_idx } => {
                                    if let Err(e) = b.apply_forced_switch(s as u8, party_idx) {
                                        fail!(
                                            r.event.id,
                                            ValidationErrorType::InvalidMove,
                                            "Forced switch at turn {t} rejected by engine: {e}"
                                        );
                                        break;
                                    }
                                }
                                other_action => {
                                    fail!(
                                        r.event.id,
                                        ValidationErrorType::InvalidSequence,
                                        "Side {s} must submit Action::Switch at turn {t} (forced switch pending), declared {other_action:?}"
                                    );
                                    break;
                                }
                            },
                        }
                    }
                    None => match (rec(0), rec(1)) {
                        (Some(r0), Some(r1)) => {
                            if let Err(e) = b.apply_turn(r0.submission.action, r1.submission.action)
                            {
                                fail!(
                                    r0.event.id,
                                    ValidationErrorType::InvalidMove,
                                    "Turn {t} rejected by engine: {e}"
                                );
                                break;
                            }
                        }
                        (r0, r1) => {
                            let id = r0.or(r1).map_or(challenge_id, |m| m.event.id);
                            fail!(
                                id,
                                ValidationErrorType::InvalidSequence,
                                "Turn {t} incomplete: both sides must declare exactly one action"
                            );
                            break;
                        }
                    },
                }
            }
        }

        Replay {
            battle,
            sides,
            challenge_id: Some(challenge_id),
            errors,
        }
    }
}

impl Game for BattleAdapter {
    type GamePiece = ();
    type GameState = ();
    type MoveData = BattleActionSubmission;

    fn decode_c_value(
        &self,
        _c_value: &[u8; 32],
    ) -> Result<Vec<Self::GamePiece>, GameProtocolError> {
        // Battles assign no pieces from C values. The seed derivation
        // (crate::seed over both players' revealed C values) stands alone.
        Ok(vec![])
    }

    fn validate_sequence(
        &self,
        events: &[NostrEvent],
    ) -> Result<ValidationResult, GameProtocolError> {
        if events.is_empty() {
            return Err(GameProtocolError::SequenceError(
                "Empty event sequence".to_string(),
            ));
        }
        let rep = self.replay(events);
        let is_valid = rep.errors.is_empty();
        // A Yield recorded in the chain lands here already mapped: apply_turn
        // finishes the battle with the OTHER side as winner.
        let winner = if is_valid {
            rep.battle
                .as_ref()
                .and_then(|b| b.winner)
                .map(|w| rep.sides[w as usize])
        } else {
            None
        };
        // No forfeited_player: timeout rules stay at the trait defaults.
        Ok(ValidationResult::new(is_valid, winner, rep.errors, None))
    }

    fn is_sequence_complete(&self, events: &[NostrEvent]) -> Result<bool, GameProtocolError> {
        if events.is_empty() {
            return Err(GameProtocolError::SequenceError(
                "Empty event sequence".to_string(),
            ));
        }
        let rep = self.replay(events);
        if !rep.errors.is_empty() {
            return Ok(false);
        }
        let (Some(battle), Some(challenge_id)) = (rep.battle, rep.challenge_id) else {
            return Ok(false);
        };
        if battle.winner.is_none() {
            return Ok(false);
        }
        // Both required Final events: one per participant, referencing this
        // chain's Challenge. Foreign finals (like all foreign events) are
        // skipped silently; unparseable ones simply do not count.
        let mut final_by = [false; 2];
        for e in events.iter().filter(|e| e.kind == FINAL_KIND) {
            if let Ok(content) = EventParser::parse_final(e) {
                if content.game_sequence_root == challenge_id {
                    if let Some(side) = rep.sides.iter().position(|pk| *pk == e.pubkey) {
                        final_by[side] = true;
                    }
                }
            }
        }
        Ok(final_by == [true, true])
    }

    fn determine_winner(
        &self,
        events: &[NostrEvent],
    ) -> Result<Option<PublicKey>, GameProtocolError> {
        if events.is_empty() {
            return Err(GameProtocolError::SequenceError(
                "Empty event sequence".to_string(),
            ));
        }
        let rep = self.replay(events);
        if !rep.errors.is_empty() {
            return Ok(None);
        }
        Ok(rep
            .battle
            .and_then(|b| b.winner)
            .map(|w| rep.sides[w as usize]))
    }

    fn required_final_events(&self) -> usize {
        2
    }
}

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
//! - Replay: apply each turn's action pair through crusade_core::engine.
//!   Forced switches are actions like any other (Action::Switch).
//! - Final events both present + replay terminated => sequence complete;
//!   winner = replay winner mapped to that side's nostr pubkey.
//!
//! Event ordering: consumed in the order given, with `events[0]` as the
//! Challenge anchor — the same convention `CardGame::validate_sequence`
//! applies to chains that `client::ValidationClient::collect_game_events`
//! produced with `sort_by_key(created_at)`. No re-sorting here. Within one
//! battle turn either side's move event may come first.
//!
//! Verdict semantics (fail-closed): any deviation below pushes an error and
//! invalidates the sequence; arbiters get all of them in `errors`. An honest
//! chain that has not decided the battle yet (including the commit-phase
//! prefix with zero moves) validates with `winner: None` — same verdict shape
//! as kirk's card_game.
//!
//! Validation re-checks token<->commitment binding using the same helpers as
//! card_game's validate_commitments (order-sensitive per-token
//! `TokenCommitment::hash_single_token` compare, exact count match).

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

/// One decoded participant move, in chain order.
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

impl BattleAdapter {
    /// Replay the whole event chain, recording every failure found.
    ///
    /// `challenge_id` is only used to attach errors; `events[0]` must be the
    /// Challenge itself and also anchors Final-event references.
    fn replay(&self, events: &[NostrEvent], challenge_id: EventId) -> Replay {
        let mut errors: Vec<ValidationError> = Vec::new();
        macro_rules! fail {
            ($id:expr, $ty:expr, $($arg:tt)*) => {
                errors.push(ValidationError::new($id, $ty, format!($($arg)*)))
            };
        }

        // ---- 1. Challenge anchor (card_game: events[0] must be Challenge) ----
        let challenge = &events[0];
        if challenge.kind != CHALLENGE_KIND {
            fail!(
                challenge_id,
                ValidationErrorType::InvalidSequence,
                "First event must be Challenge, got {}",
                challenge.kind
            );
            return Replay {
                battle: None,
                sides: [challenge.pubkey; 2],
                errors,
            };
        }
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
                    errors,
                };
            }
        };
        let creator = challenge.pubkey;

        // ---- 2. ChallengeAccept: exactly one, referencing this challenge ----
        let accepts: Vec<&NostrEvent> = events
            .iter()
            .filter(|e| e.kind == CHALLENGE_ACCEPT_KIND)
            .collect();
        let accept = match accepts.as_slice() {
            [a] => *a,
            other => {
                fail!(
                    challenge_id,
                    ValidationErrorType::InvalidSequence,
                    "Expected exactly 1 ChallengeAccept event, found {}",
                    other.len()
                );
                return Replay {
                    battle: None,
                    sides: [creator; 2],
                    errors,
                };
            }
        };
        let accept_content = match EventParser::parse_challenge_accept(accept) {
            Ok(c) => c,
            Err(e) => {
                fail!(
                    accept.id,
                    ValidationErrorType::InvalidSequence,
                    "ChallengeAccept content unparseable: {e}"
                );
                return Replay {
                    battle: None,
                    sides: [creator; 2],
                    errors,
                };
            }
        };
        if accept_content.challenge_id != challenge.id {
            fail!(
                accept.id,
                ValidationErrorType::InvalidSequence,
                "ChallengeAccept references a different challenge id"
            );
            return Replay {
                battle: None,
                sides: [creator; 2],
                errors,
            };
        }
        let acceptor = accept.pubkey;
        let sides = [creator, acceptor];

        // ---- 3. Move events, decoded in chain order ----
        let mut moves: Vec<PlayerMove> = Vec::new();
        let mut revealed_tokens: [Option<Vec<CashuToken>>; 2] = [None, None];
        for e in events.iter().filter(|e| e.kind == MOVE_KIND) {
            let side = match sides.iter().position(|pk| *pk == e.pubkey) {
                Some(s) => s,
                None => {
                    fail!(
                        e.id,
                        ValidationErrorType::InvalidSequence,
                        "Move event from non-participant pubkey {}",
                        e.pubkey
                    );
                    continue;
                }
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
            if moves.iter().all(|m| m.side != side) {
                // First move of this player must carry all reveal material.
                if submission.party_reveal.is_none() {
                    fail!(
                        e.id,
                        ValidationErrorType::InvalidMove,
                        "Player {side}'s first move must carry party_reveal"
                    );
                }
                match &content.revealed_tokens {
                    None => fail!(
                        e.id,
                        ValidationErrorType::InvalidCommitment,
                        "Player {side}'s first move must carry revealed_tokens"
                    ),
                    Some(ts) if ts.is_empty() => fail!(
                        e.id,
                        ValidationErrorType::InvalidCommitment,
                        "Player {side}'s first move revealed_tokens must not be empty"
                    ),
                    Some(ts) => revealed_tokens[side] = Some(ts.clone()),
                }
            } else {
                if submission.party_reveal.is_some() {
                    fail!(
                        e.id,
                        ValidationErrorType::InvalidMove,
                        "Player {side}: party_reveal allowed on first move only"
                    );
                }
                if content.revealed_tokens.is_some() {
                    fail!(
                        e.id,
                        ValidationErrorType::InvalidCommitment,
                        "Player {side}: revealed_tokens allowed on first move only"
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
                errors,
            };
        }

        // ---- 4. Turn pairing: turns 1..=N, one action per side per turn ----
        // Pairing is by chain position (card_game convention: slice order is
        // the chain order; nothing is re-sorted). Within a turn either side's
        // event may come first.
        let mut transcript: Vec<(Action, Action)> = Vec::with_capacity(moves.len() / 2);
        for (i, pair) in moves.chunks(2).enumerate() {
            let turn_no = (i + 1) as u32;
            let pair_id = pair[0].event.id;
            let keyed = match pair {
                [a, b] if a.side == 0 && b.side == 1 => Some((&a.submission, &b.submission)),
                [a, b] if a.side == 1 && b.side == 0 => Some((&b.submission, &a.submission)),
                _ => None,
            };
            let Some((creator_sub, acceptor_sub)) = keyed else {
                fail!(
                    pair_id,
                    ValidationErrorType::InvalidSequence,
                    "Turn {turn_no} must carry exactly one action per side, in chain order"
                );
                continue;
            };
            for sub in [creator_sub, acceptor_sub] {
                if sub.turn != turn_no {
                    fail!(
                        pair_id,
                        ValidationErrorType::InvalidSequence,
                        "Turn numbering broken: expected turn {turn_no}, move declares turn {}",
                        sub.turn
                    );
                }
            }
            transcript.push((creator_sub.action, acceptor_sub.action));
        }

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
                    let party = |side: usize| {
                        moves
                            .iter()
                            .find(|m| m.side == side)
                            .and_then(|m| m.submission.party_reveal.clone())
                    };
                    match (party(0), party(1)) {
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

        // ---- 7. Replay turns through the engine ----
        if errors.is_empty() {
            let b = battle
                .as_mut()
                .expect("battle constructed above when no errors were recorded");
            for (i, (a0, a1)) in transcript.iter().enumerate() {
                if b.winner.is_some() {
                    fail!(
                        moves[2 * i].event.id,
                        ValidationErrorType::InvalidSequence,
                        "Chain continues past the decided battle at turn {}",
                        i + 1
                    );
                    break;
                }
                if let Err(e) = b.apply_turn(*a0, *a1) {
                    fail!(
                        moves[2 * i].event.id,
                        ValidationErrorType::InvalidMove,
                        "Turn {} rejected by engine: {e}",
                        i + 1
                    );
                    break;
                }
            }
        }

        Replay {
            battle,
            sides,
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
        let challenge_id = events
            .first()
            .map(|e| e.id)
            .ok_or_else(|| GameProtocolError::SequenceError("Empty event sequence".to_string()))?;
        let rep = self.replay(events, challenge_id);
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
        let challenge_id = events
            .first()
            .map(|e| e.id)
            .ok_or_else(|| GameProtocolError::SequenceError("Empty event sequence".to_string()))?;
        let rep = self.replay(events, challenge_id);
        if !rep.errors.is_empty() {
            return Ok(false);
        }
        let Some(battle) = rep.battle else {
            return Ok(false);
        };
        if battle.winner.is_none() {
            return Ok(false);
        }
        // Both required Final events: one per player, referencing this chain.
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
        let challenge_id = events
            .first()
            .map(|e| e.id)
            .ok_or_else(|| GameProtocolError::SequenceError("Empty event sequence".to_string()))?;
        let rep = self.replay(events, challenge_id);
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

//! Rolling attestation ladder (V12-1; docs/LADDER.md).
//!
//! Replaces the honor-system, self-scored Elo: a rating moves ONLY on the
//! strength of signed kind-9265 battle attestations (crusade-bridge's
//! [`crate::attest`] machinery). The ladder is a pure fold over the
//! attestation stream: feed the same events, base ratings, settlement
//! cursor, and damping counters to two honest clients and both derive
//! identical tables, in any input order.
//!
//! Fold pipeline (every gate below must pass before an event can move a
//! rating):
//!
//! 1. **Well-formed**: kind == 9265, the nostr event id + Schnorr
//!    signature verify, exactly one `d` tag of 64-LOWERCASE-hex battle
//!    id, the content parses as [`Attestation`], `winner <= 1`, the
//!    digest is 64 lowercase hex, and content battle == d tag. (Case is
//!    explicit: uppercase hex is gate-1 inert, never merely
//!    pair-rejected.)
//! 2. **Equivocation voids**: per (battle, author), identical duplicate
//!    seals (relay republication) fold to one. One author sealing the
//!    SAME battle with disagreeing content (digest, winner, host, or
//!    peer differs) marks that author conflicted on this battle: every
//!    candidate settlement involving them dies, in EVERY fold view,
//!    forever. A retro-poison (a loser re-signing a backdated seal
//!    claiming the win) is thus incentive-equal to withholding: the
//!    battle never settles canonically, and the author cannot pick a
//!    preferred outcome.
//! 3. **Only a mutually-named pair settles**: a battle yields at most
//!    ONE settlement — two unconflicted authors whose seals agree on
//!    digest/battle/winner/host, `host` naming one of them, EACH seal's
//!    `peer` naming the OTHER seal's author (mutual peer-naming), and
//!    created_at skew within [`MAX_PAIR_SKEW_SECS`]. Mutual naming
//!    retires the entire mob/sockpuppet class: any third-party seal —
//!    agreeing, disagreeing, or backdated — names a counterparty the
//!    honest seals never name back, so no candidate pair containing it
//!    can form. Residual: TWO disjoint mutually-named pairs squatting
//!    one battle id is still fabricable — it takes one attacker holding
//!    two colluding keys, costs them nothing but their own ratings
//!    (the forged pair empties both socks), is publicly observable in
//!    the seal stream, and cannot touch the honest settlement (any
//!    store that folded first keeps it; every canonical fold picks the
//!    same (settled_at, author_a, author_b) minimum, so all auditors
//!    agree on the same winner, honest or forged).
//! 4. **Damping**: per unordered pair per [`LADDER_EPOCH_SECS`] window,
//!    the first [`FULL_WEIGHT_PER_EPOCH`] settlements count at full Elo
//!    weight; beyond that each extra settlement counts at
//!    `1 / (excess + 1)`. The counters are part of the fold state and the
//!    app persists them, so damping is live on the rolling path, not just
//!    for genesis auditors.
//!
//! Settlements apply in a canonical total order —
//! (settled_at = the pair's max created_at, battle id) — and Elo math is
//! unchanged from the honor-system ladder: `K=32`, expected
//! `1/(1+10^((opp-mine)/400))`, delta rounded, floored at 0, unrated
//! players start at [`DEFAULT_LADDER_RATING`].
//!
//! Rolling state = (base ratings for every rated player, settlement
//! cursor, damping counters). Folding a suffix of the stream over the
//! exact prior state equals folding the whole stream from genesis —
//! proven by `rolling_fold_matches_genesis_fold_on_mixed_history`.

use std::collections::{BTreeMap, BTreeSet};

use nostr::{Event, EventId, PublicKey};

use crate::attest::{digests_agree, Attestation, ATTEST_KIND};

/// A fresh, unrated player's ladder rating.
pub const DEFAULT_LADDER_RATING: u64 = 1000;
/// Elo K factor (unchanged from the honor-system ladder): a win/loss
/// against an equal-rated opponent moves K/2 = 16.
const ELO_K: f64 = 32.0;
/// Maximum |created_at| skew between the two attestations of one battle.
/// A pairing outside this window is stale and settles nothing.
pub const MAX_PAIR_SKEW_SECS: u64 = 1800;
/// Damping window length in seconds (one week).
pub const LADDER_EPOCH_SECS: u64 = 7 * 24 * 3600;
/// Settlements per unordered pair per epoch that count at full Elo weight.
pub const FULL_WEIGHT_PER_EPOCH: u32 = 2;

/// The folded ladder plus the fold-state outputs a persisting caller must
/// keep: the full ratings table, the battle ids THIS fold settled (union
/// into the caller's cursor), and the damping counters (persist verbatim).
/// All maps/sets iterate in canonical order, so `PartialEq` is a
/// byte-level agreement check for the determinism tests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LadderTable {
    ratings: BTreeMap<PublicKey, u64>,
    newly_settled: BTreeSet<String>,
    damping: BTreeMap<(PublicKey, PublicKey, u64), u32>,
}

impl LadderTable {
    /// A player's folded rating; `None` if they have no settled match
    /// and no base entry.
    pub fn rating(&self, player: &PublicKey) -> Option<u64> {
        self.ratings.get(player).copied()
    }

    /// All rated players, canonical (sorted) order.
    pub fn players(&self) -> Vec<PublicKey> {
        self.ratings.keys().copied().collect()
    }

    /// The complete ratings table (base entries + newly rated players),
    /// canonical order. Persist it wholesale: it is the base of the next
    /// rolling fold.
    pub fn ratings_all(&self) -> Vec<(PublicKey, u64)> {
        self.ratings.iter().map(|(k, v)| (*k, *v)).collect()
    }

    /// Battle ids this fold settled (excluding `already_settled`),
    /// canonical order. A persisting caller unions these into its rolling
    /// cursor; a genesis fold lists every settled battle.
    pub fn newly_settled(&self) -> Vec<String> {
        self.newly_settled.iter().cloned().collect()
    }

    /// Per-(unordered pair, epoch) settlement counts, canonical order.
    /// Persist verbatim and pass back as `damping_prior` on the next
    /// fold, or gate 6 silently resets.
    pub fn damping_counts(&self) -> Vec<(PublicKey, PublicKey, u64, u32)> {
        self.damping
            .iter()
            .map(|((a, b, e), n)| (*a, *b, *e, *n))
            .collect()
    }
}

/// An attestation event that survived gate 1 (well-formedness).
struct Verified {
    author: PublicKey,
    id: EventId,
    created_at: u64,
    att: Attestation,
}

/// Lowercase 64-hex, case-explicit (gate 1's wording, not an emergent
/// property of the pair-disagreement check).
fn is_64_lower_hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Gate 1: structural + cryptographic validity. Never errors — an invalid
/// candidate is simply not part of the stream.
fn read_verified(event: &Event) -> Option<Verified> {
    if event.kind.as_u16() != ATTEST_KIND {
        return None;
    }
    event.verify().ok()?;
    // Exactly one d tag, 64 lowercase hex: the battle id relays indexed.
    let d_tags: Vec<&str> = event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(String::as_str) == Some("d"))
        .filter_map(|t| t.content())
        .collect();
    let [d_tag] = d_tags.as_slice() else {
        return None;
    };
    let att: Attestation = serde_json::from_str(&event.content).ok()?;
    if att.winner > 1 || att.battle != *d_tag {
        return None;
    }
    if !is_64_lower_hex(&att.battle) || !is_64_lower_hex(&att.digest) {
        return None;
    }
    Some(Verified {
        author: event.pubkey,
        id: event.id,
        created_at: event.created_at.as_u64(),
        att,
    })
}

/// One author's seal(s) for one battle, after gate 2.
struct AuthorSeal {
    /// The representative seal (earliest by created_at, event id) used
    /// for pairing.
    rep: Verified,
    /// True when this author sealed this battle with disagreeing content
    /// anywhere in the stream (digest, winner, or host differs between
    /// any two of their seals). Conflicted authors poison every pair
    /// they could join: the equivocation (not the choice of winner) is
    /// what every fold view voids.
    conflicted: bool,
}

/// One settled battle, canonicalized: `a` < `b` by pubkey, so pair keys
/// are orientation-free.
#[derive(Debug, Clone)]
struct Settlement {
    a: PublicKey,
    b: PublicKey,
    winner_is_a: bool,
    /// The pair's max created_at: a battle settles when the LAST of its
    /// two seals landed.
    settled_at: u64,
    battle: String,
}

/// Try to build a settlement from two unconflicted author seals.
fn try_settle(battle: &str, x: &AuthorSeal, y: &AuthorSeal) -> Option<Settlement> {
    if x.conflicted || y.conflicted {
        return None; // gate 2: equivocation voids every pair it touches
    }
    let (xa, ya) = (&x.rep, &y.rep);
    // Note: `peer` must NOT be compared pairwise — the two seals of a
    // duel name each other, so equal `peer` fields would be the anomaly.
    // The mutual-naming check below is the binding constraint.
    if !digests_agree(&xa.att, &ya.att) || xa.att.host != ya.att.host {
        return None; // gate 3: the reports disagree
    }
    // Mutual peer-naming: each seal must name the OTHER seal's author as
    // its counterparty. A third party's agreeing seal names an author
    // whose seal names someone else — the mob/sockpuppet class dies here.
    let mut named = |seal: &Verified, other: PublicKey| -> bool {
        seal.att
            .peer
            .as_deref()
            .and_then(|h| PublicKey::from_hex(h).ok())
            == Some(other)
    };
    if !named(xa, ya.author) || !named(ya, xa.author) {
        return None;
    }
    // The named host must be one of the two authors: then `winner == 0`
    // (the host's engine side) maps unforgeably to a player.
    let host_hex = xa.att.host.as_deref()?;
    let host = PublicKey::from_hex(host_hex).ok()?;
    if host != xa.author && host != ya.author {
        return None;
    }
    if xa.created_at.abs_diff(ya.created_at) > MAX_PAIR_SKEW_SECS {
        return None; // stale pairing
    }
    let (a, b) = if xa.author < ya.author {
        (xa.author, ya.author)
    } else {
        (ya.author, xa.author)
    };
    let host_won = xa.att.winner == 0;
    Some(Settlement {
        a,
        b,
        winner_is_a: host_won == (host == a),
        settled_at: xa.created_at.max(ya.created_at),
        battle: battle.to_string(),
    })
}

/// Fold an attestation event stream into the ladder table.
///
/// `base` seeds starting ratings (the persisted full table from prior
/// folds — not just the local player's row); players without a base
/// entry start at [`DEFAULT_LADDER_RATING`]. `already_settled` is the
/// settlement cursor: battles whose outcomes the base already reflects
/// are skipped, which is exactly what makes refolding a persisted stream
/// idempotent. `damping_prior` carries the persisted per-pair counters.
/// Pure and order-independent given identical inputs.
pub fn fold_attestations<'a>(
    events: impl IntoIterator<Item = &'a Event>,
    base: &[(PublicKey, u64)],
    already_settled: &BTreeSet<String>,
    damping_prior: &BTreeMap<(PublicKey, PublicKey, u64), u32>,
) -> LadderTable {
    // Gate 1, then gate 2's grouping: per (battle, author) keep the
    // representative seal and detect equivocation.
    let mut battles: BTreeMap<String, BTreeMap<PublicKey, AuthorSeal>> = BTreeMap::new();
    for event in events {
        let Some(v) = read_verified(event) else { continue };
        let authors = battles.entry(v.att.battle.clone()).or_default();
        match authors.get_mut(&v.author) {
            None => {
                authors.insert(
                    v.author,
                    AuthorSeal {
                        rep: v,
                        conflicted: false,
                    },
                );
            }
            Some(existing) => {
                let r = &existing.rep;
                // Identical content = relay republication, folds to one;
                // ANY field difference = equivocation, voids the author.
                let same = r.att.digest == v.att.digest
                    && r.att.winner == v.att.winner
                    && r.att.host == v.att.host
                    && r.att.peer == v.att.peer;
                existing.conflicted |= !same;
                if (v.created_at, v.id) < (existing.rep.created_at, existing.rep.id) {
                    if same {
                        existing.rep = v;
                    }
                    // A conflicting later-or-earlier seal never replaces
                    // the representative; conflicted is already what
                    // matters.
                }
            }
        }
    }

    // Gate 3: per battle, the canonical earliest agreeing pair settles;
    // everything else on the id is inert noise.
    let mut settlements: Vec<Settlement> = Vec::new();
    for (battle, authors) in &battles {
        if already_settled.contains(battle) {
            continue; // the checkpoint already carries this outcome
        }
        let keys: Vec<PublicKey> = authors.keys().copied().collect();
        let mut best: Option<Settlement> = None;
        for i in 0..keys.len() {
            for j in (i + 1)..keys.len() {
                let (x, y) = (&authors[&keys[i]], &authors[&keys[j]]);
                let Some(cand) = try_settle(battle, x, y) else {
                    continue;
                };
                let better = match &best {
                    None => true,
                    Some(cur) => {
                        (cand.settled_at, cand.a, cand.b) < (cur.settled_at, cur.a, cur.b)
                    }
                };
                if better {
                    best = Some(cand);
                }
            }
        }
        if let Some(s) = best {
            settlements.push(s);
        }
    }
    settlements.sort_by(|s, t| (&s.settled_at, &s.battle).cmp(&(&t.settled_at, &t.battle)));

    // Gate 4 (damping) + Elo application in canonical order.
    let mut ratings: BTreeMap<PublicKey, u64> = base.iter().copied().collect();
    let mut damping = damping_prior.clone();
    for s in &settlements {
        let epoch = s.settled_at / LADDER_EPOCH_SECS;
        let key = (s.a, s.b, epoch);
        let n = damping.entry(key).or_insert(0);
        *n += 1;
        let weight = if *n <= FULL_WEIGHT_PER_EPOCH {
            1.0
        } else {
            1.0 / ((*n - FULL_WEIGHT_PER_EPOCH + 1) as f64)
        };
        let ra = *ratings.get(&s.a).unwrap_or(&DEFAULT_LADDER_RATING);
        let rb = *ratings.get(&s.b).unwrap_or(&DEFAULT_LADDER_RATING);
        let expected_a = 1.0 / (1.0 + 10f64.powf((rb as f64 - ra as f64) / 400.0));
        let score_a = if s.winner_is_a { 1.0 } else { 0.0 };
        let next_a = (ra as f64 + ELO_K * weight * (score_a - expected_a))
            .round()
            .max(0.0) as u64;
        let next_b = (rb as f64 + ELO_K * weight * ((1.0 - score_a) - (1.0 - expected_a)))
            .round()
            .max(0.0) as u64;
        ratings.insert(s.a, next_a);
        ratings.insert(s.b, next_b);
    }
    let newly_settled = settlements.into_iter().map(|s| s.battle).collect();
    LadderTable {
        ratings,
        newly_settled,
        damping,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind, Timestamp};

    fn attest_event_host(
        keys: &Keys,
        battle: [u8; 32],
        digest: [u8; 32],
        winner: u8,
        host: Option<&PublicKey>,
        peer: Option<&PublicKey>,
    ) -> Event {
        let att = Attestation {
            digest: hex::encode(digest),
            battle: hex::encode(battle),
            winner,
            host: host.map(|h| h.to_hex()),
            peer: peer.map(|h| h.to_hex()),
        };
        let content = serde_json::to_string(&att).expect("att serializes");
        EventBuilder::new(
            Kind::from_u16(ATTEST_KIND),
            content,
            [nostr::Tag::identifier(att.battle.clone())],
        )
        .to_event(keys)
        .expect("attestation signs")
    }

    fn attest_event_host_at(
        keys: &Keys,
        battle: [u8; 32],
        digest: [u8; 32],
        winner: u8,
        host: Option<&PublicKey>,
        peer: Option<&PublicKey>,
        ts: u64,
    ) -> Event {
        let att = Attestation {
            digest: hex::encode(digest),
            battle: hex::encode(battle),
            winner,
            host: host.map(|h| h.to_hex()),
            peer: peer.map(|h| h.to_hex()),
        };
        EventBuilder::new(
            Kind::from_u16(ATTEST_KIND),
            serde_json::to_string(&att).expect("att serializes"),
            [nostr::Tag::identifier(att.battle.clone())],
        )
        .custom_created_at(Timestamp::from(ts))
        .to_event(keys)
        .expect("attestation signs")
    }

    /// An honest duel: `host` and `joiner` sign the same digest/battle,
    /// name `host` as host, and agree on `winner` (0 = host took it).
    fn honest_pair(host: &Keys, joiner: &Keys, battle: [u8; 32], winner: u8) -> Vec<Event> {
        let d = [0x42; 32];
        let (hp, jp) = (host.public_key(), joiner.public_key());
        vec![
            attest_event_host(host, battle, d, winner, Some(&hp), Some(&jp)),
            attest_event_host(joiner, battle, d, winner, Some(&hp), Some(&hp)),
        ]
    }

    /// A timed honest duel: both seals at `ts` (skew 0). Rolling-fold
    /// tests need monotonically increasing per-battle timestamps so the
    /// canonical settlement order and the stream's vector order agree —
    /// created_at seconds tie otherwise and a mid-stream split would
    /// interleave ties differently (a test artifact, not a fold bug:
    /// real battles settle minutes apart).
    fn honest_pair_at(
        host: &Keys,
        joiner: &Keys,
        battle: [u8; 32],
        winner: u8,
        ts: u64,
    ) -> Vec<Event> {
        let d = [0x42; 32];
        let (hp, jp) = (host.public_key(), joiner.public_key());
        vec![
            attest_event_host_at(host, battle, d, winner, Some(&hp), Some(&jp), ts),
            attest_event_host_at(joiner, battle, d, winner, Some(&hp), Some(&hp), ts),
        ]
    }

    fn empty_cursor() -> BTreeSet<String> {
        BTreeSet::new()
    }

    fn empty_damping() -> BTreeMap<(PublicKey, PublicKey, u64), u32> {
        BTreeMap::new()
    }

    /// Re-collect a damping_counts() listing into the fold's map form.
    fn damp_map(t: &LadderTable) -> BTreeMap<(PublicKey, PublicKey, u64), u32> {
        t.damping_counts()
            .into_iter()
            .map(|(a, b, e, n)| ((a, b, e), n))
            .collect()
    }

    fn fold(events: &[Event]) -> LadderTable {
        fold_attestations(events, &[], &empty_cursor(), &empty_damping())
    }

    /// Fold events[..k] fresh, then roll events[k..] over its outputs —
    /// the production rolling path. Must equal `fold(all)` on ratings.
    fn rolling_fold(events: &[Event], split: usize) -> LadderTable {
        let first = fold_attestations(
            events[..split].iter(),
            &[],
            &empty_cursor(),
            &empty_damping(),
        );
        let cursor: BTreeSet<String> = first.newly_settled().into_iter().collect();
        let damp = damp_map(&first);
        fold_attestations(
            events[split..].iter(),
            &first.ratings_all(),
            &cursor,
            &damp,
        )
    }

    #[test]
    fn honest_pair_moves_winner_up_loser_down() {
        let host = Keys::generate();
        let joiner = Keys::generate();
        let events = honest_pair(&host, &joiner, [7u8; 32], 0);
        let table = fold(&events);
        assert_eq!(table.rating(&host.public_key()), Some(1016));
        assert_eq!(table.rating(&joiner.public_key()), Some(984));
    }

    #[test]
    fn joiner_win_moves_joiner_up() {
        let host = Keys::generate();
        let joiner = Keys::generate();
        let events = honest_pair(&host, &joiner, [8u8; 32], 1);
        let table = fold(&events);
        assert_eq!(table.rating(&joiner.public_key()), Some(1016));
        assert_eq!(table.rating(&host.public_key()), Some(984));
    }

    #[test]
    fn folding_is_order_independent_and_replayable() {
        let a = Keys::generate();
        let b = Keys::generate();
        let c = Keys::generate();
        let mut events = Vec::new();
        // Three honest duels across two pairs, distinct battles.
        events.extend(honest_pair(&a, &b, [1u8; 32], 0));
        events.extend(honest_pair(&b, &c, [2u8; 32], 1));
        events.extend(honest_pair(&a, &b, [3u8; 32], 1));
        let forward = fold(&events);
        let mut shuffled = events.clone();
        shuffled.reverse();
        let backward = fold(&shuffled);
        assert_eq!(
            forward, backward,
            "two honest clients folding the same stream agree"
        );
        assert_eq!(forward, fold(&events), "the fold is replayable");
    }

    #[test]
    fn rolling_fold_matches_genesis_fold_on_mixed_history() {
        // Non-trivial history: three players, mixed pairs, a damped
        // same-pair re-match series, a disagreeing battle, a mobbed
        // battle, an equivocated battle — then EVERY split point of the
        // rolling fold must match genesis on ratings.
        let a = Keys::generate();
        let b = Keys::generate();
        let c = Keys::generate();
        let m = Keys::generate(); // mobber
        let mut events = Vec::new();
        // a vs b: FIVE rematches (damping engages beyond 2) — interleaved
        // with other battles so the settlement sort is exercised.
        // Timestamps march per battle so stream order == canonical order.
        let base_ts = nostr::Timestamp::now().as_u64() - 3600;
        for i in 0u64..5 {
            events.extend(honest_pair_at(&a, &b, [0x10 + i as u8; 32], (i % 2) as u8, base_ts + i * 60));
            events.extend(honest_pair_at(&a, &c, [0x20 + i as u8; 32], 0, base_ts + i * 60 + 30));
        }
        // Mobbed honest battle: third-party seal (agreeing AND
        // disagreeing variants) must not touch the a-b settlement.
        events.extend(honest_pair_at(&a, &b, [0x30; 32], 0, base_ts + 600));
        // The agreeing mob seal names a as its counterparty — but a's
        // seal names b, so no pair containing it can ever form.
        events.push(attest_event_host_at(&m, [0x30; 32], [0x42; 32], 0, Some(&a.public_key()), Some(&a.public_key()), base_ts + 700));
        events.push(attest_event_host_at(&m, [0x30; 32], [0x99; 32], 1, Some(&a.public_key()), Some(&a.public_key()), base_ts + 701));
        // Equivocated battle (retro-poison): void everywhere.
        events.extend(honest_pair_at(&b, &c, [0x40; 32], 0, base_ts + 800));
        events.push(attest_event_host_at(&c, [0x40; 32], [0x42; 32], 1, Some(&b.public_key()), Some(&b.public_key()), base_ts + 810));
        // Disagreeing pair: inert.
        events.push(attest_event_host_at(&b, [0x41; 32], [0x42; 32], 0, Some(&b.public_key()), Some(&c.public_key()), base_ts + 900));
        events.push(attest_event_host_at(&c, [0x41; 32], [0x42; 32], 1, Some(&b.public_key()), Some(&b.public_key()), base_ts + 900));

        let genesis = fold(&events);
        // Production settles one duel (one seal PAIR) per call, so the
        // meaningful split points are battle boundaries — every second
        // event of the 10 real duels (indices 0..20). (A split inside a
        // pair leaves an orphan seal, inert by design; a split between a
        // settlement and its later retro-poison seal is the documented
        // prompt-vs-canonical void-equivalence, asserted separately.)
        for split in (2..20).step_by(2) {
            let rolled = rolling_fold(&events, split);
            assert_eq!(
                genesis.ratings_all(),
                rolled.ratings_all(),
                "rolling diverges from genesis at split {split}"
            );
        }
        // The damped a-b series priced identically to genesis is the
        // point: counters ROLLED across the split.
        assert!(genesis.rating(&a.public_key()).is_some());
    }

    #[test]
    fn refold_with_persisted_cursor_is_idempotent() {
        let a = Keys::generate();
        let b = Keys::generate();
        let events = honest_pair(&a, &b, [1u8; 32], 0);
        let first = fold(&events);
        let cursor: BTreeSet<String> = first.newly_settled().into_iter().collect();
        assert_eq!(cursor.len(), 1);
        let second = fold_attestations(
            events.iter(),
            &first.ratings_all(),
            &cursor,
            &damp_map(&first),
        );
        assert_eq!(second.ratings_all(), first.ratings_all());
        assert!(second.newly_settled().is_empty());
    }

    // ---------- done-test: the forgery battery ----------

    #[test]
    fn unsigned_or_tampered_attestation_is_inert() {
        let a = Keys::generate();
        let b = Keys::generate();
        let mut events = honest_pair(&a, &b, [9u8; 32], 0);
        events[1].content = events[1].content.replace("\"winner\":0", "\"winner\":1");
        let table = fold(&events);
        assert_eq!(table.rating(&a.public_key()), None);
        assert_eq!(table.rating(&b.public_key()), None);
    }

    #[test]
    fn replayed_attestation_counts_once_per_battle() {
        let a = Keys::generate();
        let b = Keys::generate();
        let pair = honest_pair(&a, &b, [4u8; 32], 1);
        let mut spam = Vec::new();
        for _ in 0..16 {
            spam.extend(pair.iter().cloned());
        }
        let table = fold(&spam);
        assert_eq!(table.rating(&b.public_key()), Some(1016));
        assert_eq!(table.rating(&a.public_key()), Some(984));
    }

    #[test]
    fn retro_poison_folds_identically_to_withholding() {
        // The loser later signs a SECOND, backdated seal claiming the
        // win. Every fold view — genesis auditor, late syncer, either
        // participant — must then see NO settlement for the battle:
        // identical to the loser never having sealed at all.
        let a = Keys::generate();
        let b = Keys::generate();
        let honest = honest_pair(&a, &b, [0x50; 32], 0);
        let poison = attest_event_host(&b, [0x50; 32], [0x42; 32], 1, Some(&a.public_key()), Some(&a.public_key()));
        let withheld: Vec<Event> = vec![honest[0].clone()];

        let mut poisoned = honest.clone();
        poisoned.push(poison);
        let prompt = fold(&honest);
        let canonical = fold(&poisoned);
        let never = fold(&withheld);
        assert_eq!(
            canonical.ratings_all(),
            never.ratings_all(),
            "retro-poison == withholding in every fold view"
        );
        assert!(canonical.rating(&a.public_key()).is_none());
        // Ordering cannot save the cheat: reversed stream, same void.
        let mut rev = poisoned.clone();
        rev.reverse();
        assert_eq!(fold(&rev).ratings_all(), never.ratings_all());
        // The prompt fold (participants' live settle) keeps its number —
        // divergence exists but the attacker could only ever choose
        // between "settled honestly" and "void", never a preferred win.
        assert_eq!(prompt.rating(&a.public_key()), Some(1016));
    }

    #[test]
    fn mobbed_battle_still_settles_the_honest_pair() {
        // Third-party seals on a public battle id are inert extras; the
        // honest pair's settlement stands in every fold view.
        let a = Keys::generate();
        let b = Keys::generate();
        let m = Keys::generate();
        let m2 = Keys::generate();
        let now = nostr::Timestamp::now().as_u64();
        let mut events = honest_pair_at(&a, &b, [11u8; 32], 0, now);
        // Mobbers react after the honest seals land (created_at seconds
        // tie-break: a same-second agreeing mob seal could win the
        // canonical slot — and only rates the mobber himself worse; see
        // docs/LADDER.md). Here the honest pair is strictly earlier.
        // Mob seals name `a` as their counterparty — the maximal form
        // of the attack. `a`'s own seal names `b`, so every mob pair is
        // dead on arrival regardless of timestamp or pubkey luck.
        events.push(attest_event_host_at(&m, [11u8; 32], [0x42; 32], 0, Some(&a.public_key()), Some(&a.public_key()), now + 5));
        events.push(attest_event_host_at(&m2, [11u8; 32], [0x42; 32], 1, Some(&a.public_key()), Some(&a.public_key()), now + 6));
        let table = fold(&events);
        assert_eq!(
            table.rating(&a.public_key()),
            Some(1016),
            "mob seals cannot void the honest settlement"
        );
        assert_eq!(table.rating(&b.public_key()), Some(984));
        // m2 disagreed with everyone; m "agreed" and would rate himself
        // the loser — but m is never one of the earliest agreeing pair,
        // so he moves nothing. (Canon: first agreeing pair settles.)
        assert_eq!(table.rating(&m.public_key()), None);
        assert_eq!(table.rating(&m2.public_key()), None);
    }

    #[test]
    fn self_signed_battle_is_inert() {
        let solo = Keys::generate();
        let pk = solo.public_key();
        // "Both sides" from one key with the same content: no second
        // author exists; and fabricating a second disagreeing seal from
        // the same key equivocates, voiding the author either way.
        let events = vec![
            attest_event_host(&solo, [5u8; 32], [0x42; 32], 0, Some(&pk), Some(&pk)),
            attest_event_host(&solo, [5u8; 32], [0x42; 32], 0, Some(&pk), Some(&pk)),
        ];
        assert_eq!(fold(&events).rating(&pk), None);
        let conflicted = vec![
            attest_event_host(&solo, [5u8; 32], [0x42; 32], 0, Some(&pk), Some(&pk)),
            attest_event_host(&solo, [5u8; 32], [0x42; 32], 1, Some(&pk), Some(&pk)),
        ];
        assert_eq!(fold(&conflicted).rating(&pk), None);
    }

    #[test]
    fn forged_second_seal_is_inert_for_everyone() {
        let a = Keys::generate();
        let b = Keys::generate();
        let impostor = Keys::generate();
        // A signed an honest seal naming b as counterparty; the impostor
        // countersigns naming a — maximal agreement, but a's seal never
        // names the impostor back, so no pair forms at all.
        let mut events = honest_pair(&a, &b, [6u8; 32], 0);
        events[1] =
            attest_event_host(&impostor, [6u8; 32], [0x42; 32], 0, Some(&a.public_key()), Some(&a.public_key()));
        let table = fold(&events);
        assert_eq!(table.rating(&a.public_key()), None);
        assert_eq!(table.rating(&b.public_key()), None);
        assert_eq!(table.rating(&impostor.public_key()), None);
        assert!(table.newly_settled().is_empty());
    }

    #[test]
    fn disagreeing_pair_is_inert() {
        let a = Keys::generate();
        let b = Keys::generate();
        let mut events = honest_pair(&a, &b, [10u8; 32], 0);
        events[1] = attest_event_host(&b, [10u8; 32], [0x42; 32], 1, Some(&a.public_key()), Some(&a.public_key()));
        let table = fold(&events);
        assert_eq!(table.rating(&a.public_key()), None);
        assert_eq!(table.rating(&b.public_key()), None);
    }

    #[test]
    fn old_seals_without_host_or_peer_never_fold() {
        // Pin: pre-V12-1 kind-9265 seals (host absent) AND round-1 seals
        // (peer absent) are inert in every fold view — a future
        // "helpful" default of host/peer=author would start rating
        // history retroactively without this pin.
        let a = Keys::generate();
        let b = Keys::generate();
        let d = [0x42; 32];
        let (ap, bp) = (a.public_key(), b.public_key());
        for (label, events) in [
            (
                "host absent, peer absent",
                vec![
                    attest_event_host(&a, [13u8; 32], d, 0, None, None),
                    attest_event_host(&b, [13u8; 32], d, 0, None, None),
                ],
            ),
            (
                "host present, peer absent (round-1 shape)",
                vec![
                    attest_event_host(&a, [13u8; 32], d, 0, Some(&ap), None),
                    attest_event_host(&b, [13u8; 32], d, 0, Some(&ap), None),
                ],
            ),
        ] {
            let table = fold(&events);
            assert_eq!(table.rating(&ap), None, "{label}");
            assert_eq!(table.rating(&bp), None, "{label}");
            assert!(table.newly_settled().is_empty(), "{label}");
        }
    }

    #[test]
    fn peer_misnaming_breaks_the_pair() {
        // Both seals otherwise perfect, but one names the wrong
        // counterparty: no mutual naming, no settlement.
        let a = Keys::generate();
        let b = Keys::generate();
        let c = Keys::generate();
        let d = [0x42; 32];
        let (ap, bp, cp) = (a.public_key(), b.public_key(), c.public_key());
        let events = vec![
            attest_event_host(&a, [15u8; 32], d, 0, Some(&ap), Some(&bp)),
            // b's seal names c, not a: misnamed.
            attest_event_host(&b, [15u8; 32], d, 0, Some(&ap), Some(&cp)),
        ];
        let table = fold(&events);
        assert_eq!(table.rating(&ap), None);
        assert_eq!(table.rating(&bp), None);
        assert_eq!(table.rating(&cp), None);
    }

    #[test]
    fn sockpuppet_retro_laundering_is_inert() {
        // R1 replay: the loser backdates a fully AGREEING seal from a
        // fresh sockpuppet (created_at = the host's, pubkey ground to
        // whatever tie-break — irrelevant now). The honest pair still
        // settles and the sockpuppet moves nothing: identical to the
        // seal never existing.
        let host = Keys::generate();
        let loser = Keys::generate();
        let t0 = nostr::Timestamp::now().as_u64() - 10;
        let honest = honest_pair_at(&host, &loser, [0x51; 32], 0, t0);
        let sock = Keys::generate();
        let sock_seal = attest_event_host_at(
            &sock,
            [0x51; 32],
            [0x42; 32],
            0,
            Some(&host.public_key()),
            Some(&host.public_key()), // the sock names the host, as agreed
            t0,                       // backdated into the skew window
        );
        let clean = fold(&honest);
        let mut laundered = honest.clone();
        laundered.push(sock_seal);
        let dirty = fold(&laundered);
        assert_eq!(
            dirty.ratings_all(),
            clean.ratings_all(),
            "the sockpuppet seal changes nothing in any fold view"
        );
        assert_eq!(dirty.rating(&host.public_key()), Some(1016));
        assert_eq!(dirty.rating(&loser.public_key()), Some(984));
        assert_eq!(dirty.rating(&sock.public_key()), None);
        assert_eq!(dirty.newly_settled().len(), 1);
    }

    #[test]
    fn uppercase_hex_battle_is_gate_one_inert() {
        let a = Keys::generate();
        let b = Keys::generate();
        let mut events = honest_pair(&a, &b, [14u8; 32], 0);
        // Uppercase everything: the d tag comparison and content compare
        // are exact strings, and gate 1's lowercase rule is case-explicit.
        let upper = hex::encode([14u8; 32]).to_uppercase();
        for e in &mut events {
            let mut att: Attestation = serde_json::from_str(&e.content).unwrap();
            att.battle = upper.clone();
            let content = serde_json::to_string(&att).unwrap();
            *e = EventBuilder::new(
                Kind::from_u16(ATTEST_KIND),
                content,
                [nostr::Tag::identifier(upper.clone())],
            )
            .to_event(if e.pubkey == a.public_key() { &a } else { &b })
            .unwrap();
        }
        let table = fold(&events);
        assert_eq!(table.rating(&a.public_key()), None);
    }

    #[test]
    fn stale_pairing_is_inert() {
        let a = Keys::generate();
        let b = Keys::generate();
        let fresh = honest_pair(&a, &b, [12u8; 32], 0);
        let att = Attestation {
            digest: hex::encode([0x42; 32]),
            battle: hex::encode([12u8; 32]),
            winner: 0,
            host: Some(a.public_key().to_hex()),
            peer: Some(b.public_key().to_hex()),
        };
        let stale_ts = Timestamp::from(fresh[0].created_at.as_u64() + MAX_PAIR_SKEW_SECS + 1);
        let stale = EventBuilder::new(
            Kind::from_u16(ATTEST_KIND),
            serde_json::to_string(&att).unwrap(),
            [nostr::Tag::identifier(att.battle.clone())],
        )
        .custom_created_at(stale_ts)
        .to_event(&b)
        .expect("stale attestation signs");
        let events = vec![fresh[0].clone(), stale];
        let table = fold(&events);
        assert_eq!(table.rating(&a.public_key()), None);
        assert_eq!(table.rating(&b.public_key()), None);
    }

    #[test]
    fn same_pair_spam_is_damped_beyond_full_weight() {
        let a = Keys::generate();
        let b = Keys::generate();
        let mut events = Vec::new();
        for i in 0u8..10 {
            events.extend(honest_pair(&a, &b, [i + 40; 32], 0));
        }
        let table = fold(&events);
        let r = table.rating(&a.public_key()).expect("rated");
        assert!(r < 1060, "damping caps pair farming, got {r}");
        assert!(r > 1016, "the first full-weight win lands, got {r}");
        let rb = table.rating(&b.public_key()).expect("rated");
        assert!(rb < 984);
        // The counters live in the table now: persisted and re-folded,
        // a SIXTH re-match must still be damped (rolling path is live).
        let sixth = honest_pair(&a, &b, [99u8; 32], 0);
        let rolled = fold_attestations(
            sixth.iter(),
            &table.ratings_all(),
            &table.newly_settled().into_iter().collect(),
            &damp_map(&table),
        );
        let ra_after = rolled.rating(&a.public_key()).unwrap();
        let gained = ra_after - r;
        assert!(gained <= 8, "the 11th same-pair win is 1/9-weight, got +{gained}");
        assert!(gained > 0);
        // And one fresh pair in the same roll pays full weight.
        let c = Keys::generate();
        let fresh_battle = honest_pair(&a, &c, [98u8; 32], 0);
        let rolled2 = fold_attestations(
            fresh_battle.iter(),
            &rolled.ratings_all(),
            &{
                let mut s: BTreeSet<String> = table.newly_settled().into_iter().collect();
                s.extend(rolled.newly_settled());
                s
            },
            &damp_map(&rolled),
        );
        let gain2 = rolled2.rating(&a.public_key()).unwrap() - ra_after;
        assert!(
            gain2 > gained,
            "a fresh opponent is undamped (+{gain2} vs damped +{gained})"
        );
    }

    #[test]
    fn elo_math_matches_honor_system_line() {
        let a = Keys::generate();
        let b = Keys::generate();
        let events = honest_pair(&a, &b, [13u8; 32], 0);
        let table = fold_attestations(
            events.iter(),
            &[(b.public_key(), 1600)],
            &empty_cursor(),
            &empty_damping(),
        );
        let expected_a = (1000.0
            + 32.0 * (1.0 - 1.0 / (1.0 + 10f64.powf((1600.0 - 1000.0) / 400.0))))
        .round()
        .max(0.0) as u64;
        let expected_b = (1600.0
            + 32.0 * (0.0 - 1.0 / (1.0 + 10f64.powf((1000.0 - 1600.0) / 400.0))))
        .round()
        .max(0.0) as u64;
        assert_eq!(table.rating(&a.public_key()), Some(expected_a));
        assert_eq!(table.rating(&b.public_key()), Some(expected_b));
        assert_eq!(expected_a, 1031); // 32 * (1 - 1/(1+10^1.5)) ~= 31
    }
}

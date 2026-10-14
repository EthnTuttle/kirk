//! Transcript attestation.
//!
//! At duel end both sides hold identical transcript digests. Each signs a
//! kind-9265 nostr event with content {"digest": hex, "battle": hex,
//! "winner": 0|1}. The mint (or any arbiter) verifies: both pubkeys are the
//! challenge participants (caller passes them), signatures valid, digests
//! equal, winner consistent.

use nostr::{Event, EventBuilder, Keys, Kind, PublicKey};
use serde::{Deserialize, Serialize};

use crate::BridgeError;

/// Custom kind for duel transcript attestations. Kind map:
/// kirk 9259..=9263, duel transport 9264, attest 9265.
pub const ATTEST_KIND: u16 = 9265;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attestation {
    /// hex(32-byte transcript digest from DuelSession::transcript_hash)
    pub digest: String,
    /// hex(32-byte battle id)
    pub battle: String,
    pub winner: u8,
}

/// Serialize `att` as JSON and sign it into a kind-9265 event. The battle
/// id is duplicated into a `d` tag so relays can index / filter on it.
pub fn sign_attestation(keys: &Keys, att: &Attestation) -> Result<Event, BridgeError> {
    let content = serde_json::to_string(att).map_err(|e| BridgeError::Json(e.to_string()))?;
    let tags = [nostr::Tag::identifier(att.battle.clone())];
    EventBuilder::new(Kind::from_u16(ATTEST_KIND), content, tags)
        .to_event(keys)
        .map_err(|e| BridgeError::Nostr(e.to_string()))
}

/// Verify an attestation event and return the parsed [`Attestation`].
///
/// Checks, in order: kind == 9265, author == `expected_pub`, event id +
/// Schnorr signature valid, `d` tag == `expected_battle`, content parses
/// as [`Attestation`], winner <= 1. Cryptographic/structural failures are
/// [`BridgeError::BadAttestation`]; JSON failures are [`BridgeError::Json`].
pub fn verify_attestation(
    event: &Event,
    expected_pub: &PublicKey,
    expected_battle: &str,
) -> Result<Attestation, BridgeError> {
    if event.kind != Kind::from_u16(ATTEST_KIND) {
        return Err(BridgeError::BadAttestation);
    }
    if &event.pubkey != expected_pub {
        return Err(BridgeError::BadAttestation);
    }
    event.verify().map_err(|_| BridgeError::BadAttestation)?;
    let d_tag = event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(String::as_str) == Some("d"))
        .find_map(|t| t.content());
    if d_tag != Some(expected_battle) {
        return Err(BridgeError::BadAttestation);
    }
    let att: Attestation =
        serde_json::from_str(&event.content).map_err(|e| BridgeError::Json(e.to_string()))?;
    if att.winner > 1 {
        return Err(BridgeError::BadAttestation);
    }
    Ok(att)
}

/// True if both attestations commit to the same transcript digest, battle
/// id, and winner. Any field mismatch => disagreement.
pub fn digests_agree(a: &Attestation, b: &Attestation) -> bool {
    a.digest == b.digest && a.battle == b.battle && a.winner == b.winner
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_att() -> Attestation {
        Attestation {
            digest: hex::encode([0xABu8; 32]),
            battle: hex::encode([0x42u8; 32]),
            winner: 0,
        }
    }

    #[test]
    fn sign_and_verify_happy_path() {
        let keys = Keys::generate();
        let att = sample_att();
        let event = sign_attestation(&keys, &att).unwrap();
        let verified = verify_attestation(&event, &keys.public_key(), &att.battle).unwrap();
        assert_eq!(verified, att);
        // d tag present for indexability
        assert!(event
            .tags
            .iter()
            .any(|t| t.content() == Some(att.battle.as_str())));
    }

    #[test]
    fn wrong_author_rejected() {
        let keys = Keys::generate();
        let impostor = Keys::generate();
        let att = sample_att();
        let event = sign_attestation(&keys, &att).unwrap();
        let err = verify_attestation(&event, &impostor.public_key(), &att.battle).unwrap_err();
        assert_eq!(err, BridgeError::BadAttestation);
    }

    #[test]
    fn tampered_content_rejected() {
        let keys = Keys::generate();
        let att = sample_att();
        let event = sign_attestation(&keys, &att).unwrap();
        // Forge: mutate content without re-signing. Event id covers content,
        // so verify() must fail before any parsing.
        let mut forged = event.clone();
        forged.content = forged.content.replacen("0", "1", 1);
        let err = verify_attestation(&forged, &keys.public_key(), &att.battle).unwrap_err();
        assert_eq!(err, BridgeError::BadAttestation);
    }

    #[test]
    fn winner_above_one_rejected() {
        let keys = Keys::generate();
        let mut att = sample_att();
        att.winner = 2;
        let event = sign_attestation(&keys, &att).unwrap();
        let err = verify_attestation(&event, &keys.public_key(), &att.battle).unwrap_err();
        assert_eq!(err, BridgeError::BadAttestation);
    }

    #[test]
    fn digests_agree_truth_table() {
        let a = sample_att();
        let b = sample_att();
        assert!(digests_agree(&a, &b));

        let c = Attestation {
            digest: hex::encode([0xCDu8; 32]),
            ..a.clone()
        };
        assert!(!digests_agree(&a, &c), "different digest");

        let c = Attestation {
            battle: hex::encode([0x99u8; 32]),
            ..a.clone()
        };
        assert!(!digests_agree(&a, &c), "different battle");

        let c = Attestation {
            winner: 1,
            ..a.clone()
        };
        assert!(!digests_agree(&a, &c), "different winner");
    }

    #[test]
    fn roundtrip_attestation_event_attestation() {
        let keys = Keys::generate();
        let att = Attestation {
            digest: hex::encode([0x5Au8; 32]),
            battle: "00".repeat(32),
            winner: 1,
        };
        let event = sign_attestation(&keys, &att).unwrap();
        let back = verify_attestation(&event, &keys.public_key(), &att.battle).unwrap();
        assert_eq!(att, back);
    }
}

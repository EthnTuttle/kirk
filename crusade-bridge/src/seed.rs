//! Battle seed derivation.
//!
//! seed = SHA256( "deus-vult/seed/v0" || len-prefixed each C value of side0
//! (in the order the kirk acceptor revealed them; side0 = challenge creator)
//! || same for side1 || battle_id ). 33-byte C values are compressed points.

use sha2::{Digest, Sha256};

use crate::BridgeError;

/// Domain separator for the battle seed. Versioned so a formula change is
/// a clean break (new prefix, new derivation).
const SEED_DOMAIN: &[u8] = b"deus-vult/seed/v0";

/// battle_id convention: sha256(challenge_event_id || accept_event_id).
///
/// Deterministic commitment of both players' revealed C values plus the
/// battle id into one 32-byte seed. Exact wire into SHA256:
///
/// ```text
/// "deus-vult/seed/v0"
/// || u32 BE (len side0)
/// || for each c in side0 (reveal order): u32 BE 33 || c
/// || u32 BE (len side1)
/// || for each c in side1 (reveal order): u32 BE 33 || c
/// || battle_id (32 bytes, not length-prefixed)
/// ```
///
/// Pure function: no validation, no errors. Side order matters — callers
/// must fix which side is side0 (challenge creator) by convention.
pub fn derive_battle_seed(
    side0_cs: &[[u8; 33]],
    side1_cs: &[[u8; 33]],
    battle_id: [u8; 32],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(SEED_DOMAIN);
    for side in [side0_cs, side1_cs] {
        hasher.update((side.len() as u32).to_be_bytes());
        for c in side {
            hasher.update((c.len() as u32).to_be_bytes());
            hasher.update(c);
        }
    }
    hasher.update(battle_id);
    hasher.finalize().into()
}

/// Reveal-completion entry point. Computes
/// `battle_id = sha256(raw challenge event id || raw accept event id)` —
/// both are 32-byte nostr event ids in hex — then derives the seed from
/// both players' revealed C values via [`derive_battle_seed`]
/// (`creator_cs` = challenge creator = side 0, in reveal order).
///
/// Malformed hex or non-32-byte ids are `BridgeError::Seed`.
pub fn derive_battle_seed_from_reveal(
    challenge_event_id_hex: &str,
    accept_event_id_hex: &str,
    creator_cs: &[[u8; 33]],
    acceptor_cs: &[[u8; 33]],
) -> Result<[u8; 32], BridgeError> {
    fn decode_event_id(id_hex: &str, which: &str) -> Result<[u8; 32], BridgeError> {
        let raw = hex::decode(id_hex)
            .map_err(|e| BridgeError::Seed(format!("{which} event id is not valid hex: {e}")))?;
        <[u8; 32]>::try_from(raw.as_slice()).map_err(|_| {
            BridgeError::Seed(format!(
                "{which} event id must decode to 32 bytes, got {}",
                raw.len()
            ))
        })
    }
    let challenge_id = decode_event_id(challenge_event_id_hex, "challenge")?;
    let accept_id = decode_event_id(accept_event_id_hex, "accept")?;
    let mut hasher = Sha256::new();
    hasher.update(challenge_id);
    hasher.update(accept_id);
    let battle_id: [u8; 32] = hasher.finalize().into();
    Ok(derive_battle_seed(creator_cs, acceptor_cs, battle_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_is_deterministic() {
        let side0 = vec![[0x0Au8; 33], [0x0Bu8; 33]];
        let side1 = vec![[0x0Cu8; 33]];
        let battle_id = [0x77u8; 32];
        let a = derive_battle_seed(&side0, &side1, battle_id);
        let b = derive_battle_seed(&side0, &side1, battle_id);
        assert_eq!(a, b);
    }

    #[test]
    fn seed_is_side_order_sensitive() {
        let side0 = vec![[0x0Au8; 33]];
        let side1 = vec![[0x0Bu8; 33], [0x0Cu8; 33]];
        let battle_id = [0x77u8; 32];
        let a = derive_battle_seed(&side0, &side1, battle_id);
        let b = derive_battle_seed(&side1, &side0, battle_id);
        assert_ne!(a, b, "swapping sides must change the seed");
    }

    /// Golden vector. RECALIBRATION: value produced by this implementation;
    /// update only if the wire format in the doc above deliberately changes.
    #[test]
    fn seed_golden_vector() {
        let side0 = vec![[0x02u8; 33]];
        let side1 = vec![[0x03u8; 33]];
        let battle_id = [0x42u8; 32];
        let seed = derive_battle_seed(&side0, &side1, battle_id);
        assert_eq!(
            hex::encode(seed),
            "392f74cbecef0ca102cb200e0e9249b771908842f641ea802ca6812f5e2a39d7"
        );
    }

    #[test]
    fn from_reveal_matches_manual_two_step() {
        let challenge_hex = "aa".repeat(32);
        let accept_hex = "bb".repeat(32);
        let creator_cs = vec![[0x02u8; 33]];
        let acceptor_cs = vec![[0x03u8; 33], [0x04u8; 33]];

        let got =
            derive_battle_seed_from_reveal(&challenge_hex, &accept_hex, &creator_cs, &acceptor_cs)
                .expect("valid hex ids");

        let mut preimage = hex::decode(&challenge_hex).unwrap();
        preimage.extend(hex::decode(&accept_hex).unwrap());
        let battle_id: [u8; 32] = Sha256::digest(&preimage).into();
        let want = derive_battle_seed(&creator_cs, &acceptor_cs, battle_id);
        assert_eq!(got, want);
    }

    #[test]
    fn from_reveal_rejects_malformed_hex() {
        let ok_hex = "aa".repeat(32);
        let cs = vec![[0x02u8; 33]];

        let bad_chars = derive_battle_seed_from_reveal("zz", &ok_hex, &cs, &cs);
        assert!(matches!(bad_chars, Err(BridgeError::Seed(_))));

        let short = derive_battle_seed_from_reveal(&ok_hex, "abcd", &cs, &cs);
        assert!(matches!(short, Err(BridgeError::Seed(_))));

        let long = derive_battle_seed_from_reveal(&ok_hex, &"aa".repeat(33), &cs, &cs);
        assert!(matches!(long, Err(BridgeError::Seed(_))));
    }
}

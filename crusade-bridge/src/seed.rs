//! Battle seed derivation.
//!
//! seed = SHA256( "deus-vult/seed/v0" || len-prefixed each C value of side0
//! (in the order the kirk acceptor revealed them; side0 = challenge creator)
//! || same for side1 || battle_id ). 33-byte C values are compressed points.

use sha2::{Digest, Sha256};

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
}

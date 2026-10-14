//! Deterministic RNG facade. ChaCha8: identical across platforms incl. wasm.
//! Seed arrives from the kirk commit-reveal HKDF (32 bytes).

use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;

#[derive(Debug, Clone)]
pub struct BattleRng {
    inner: ChaCha8Rng,
}

impl BattleRng {
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self {
            inner: ChaCha8Rng::from_seed(seed),
        }
    }

    /// From a battle seed + monotonically increasing turn counter.
    /// Domain-separated sub-seeding: sha256-free, we mix via ChaCha itself.
    pub fn for_turn(battle_seed: [u8; 32], turn: u32) -> Self {
        let mut s = battle_seed;
        let t = turn.to_le_bytes();
        for i in 0..4 {
            s[i] ^= t[i];
            s[4 + i] ^= t[i];
        }
        Self::from_seed(s)
    }

    /// Inclusive 0..=99 roll for percent checks.
    pub fn pct(&mut self) -> u8 {
        (self.inner.next_u32() % 100) as u8
    }

    /// True with `chance_pct`% probability (chance_pct >= 100 always true).
    pub fn chance(&mut self, chance_pct: u8) -> bool {
        chance_pct >= 100 || self.pct() < chance_pct
    }

    /// Inclusive range, e.g. damage roll 85..=100.
    pub fn range_u32(&mut self, lo: u32, hi: u32) -> u32 {
        debug_assert!(lo <= hi);
        lo + (self.inner.next_u32() % (hi - lo + 1))
    }

    /// Inclusive small range.
    pub fn range_u8(&mut self, lo: u8, hi: u8) -> u8 {
        self.range_u32(lo as u32, hi as u32) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_stream() {
        let mut a = BattleRng::from_seed([7u8; 32]);
        let mut b = BattleRng::from_seed([7u8; 32]);
        for _ in 0..64 {
            assert_eq!(a.pct(), b.pct());
        }
    }

    #[test]
    fn turn_subseeds_differ() {
        let mut a = BattleRng::for_turn([1u8; 32], 0);
        let mut b = BattleRng::for_turn([1u8; 32], 1);
        let sa: Vec<u8> = (0..16).map(|_| a.pct()).collect();
        let sb: Vec<u8> = (0..16).map(|_| b.pct()).collect();
        assert_ne!(sa, sb);
    }
}

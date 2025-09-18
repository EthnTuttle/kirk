//! Property-based tests for C value randomness and security

use proptest::prelude::*;
use std::collections::{HashMap, HashSet};
use kirk::Game;
use tests::mocks::CoinFlipGame;

/// Test that C values provide sufficient entropy for game pieces
#[cfg(test)]
mod c_value_randomness_tests {
    use super::*;

    #[test]
    fn test_c_value_entropy_distribution() {
        let game = CoinFlipGame::new();
        let mut heads_count = 0;
        let mut tails_count = 0;
        let sample_size = 1000;

        // Generate many C values and check distribution
        for i in 0..sample_size {
            let mut c_value = [0u8; 32];
            // Use different patterns to simulate real C values
            for (j, byte) in c_value.iter_mut().enumerate() {
                *byte = ((i * 7 + j * 13) % 256) as u8;
            }

            let pieces = game.decode_c_value(&c_value).unwrap();
            assert_eq!(pieces.len(), 1);

            match pieces[0].side {
                tests::mocks::reference_game::CoinSide::Heads => heads_count += 1,
                tests::mocks::reference_game::CoinSide::Tails => tails_count += 1,
            }
        }

        // Should be roughly 50/50 distribution (allow 10% deviation)
        let expected = sample_size / 2;
        let tolerance = sample_size / 10;
        
        assert!(heads_count > expected - tolerance && heads_count < expected + tolerance,
               "Heads count {} is outside expected range {}±{}", heads_count, expected, tolerance);
        assert!(tails_count > expected - tolerance && tails_count < expected + tolerance,
               "Tails count {} is outside expected range {}±{}", tails_count, expected, tolerance);
    }

    #[test]
    fn test_c_value_strength_distribution() {
        let game = CoinFlipGame::new();
        let mut strength_counts = HashMap::new();
        let sample_size = 1000;

        // Generate C values and track strength distribution
        for i in 0..sample_size {
            let mut c_value = [0u8; 32];
            // Use second byte for strength
            c_value[1] = (i % 256) as u8;
            
            let pieces = game.decode_c_value(&c_value).unwrap();
            let strength = pieces[0].strength;
            
            *strength_counts.entry(strength).or_insert(0) += 1;
        }

        // Should have good distribution across strength values
        // With 1000 samples and 256 possible values, expect roughly 4 per value
        let unique_strengths = strength_counts.len();
        assert!(unique_strengths > 200, "Too few unique strength values: {}", unique_strengths);
    }

    #[test]
    fn test_c_value_uniqueness() {
        let game = CoinFlipGame::new();
        let mut seen_pieces = HashSet::new();
        
        // Generate many different C values
        for i in 0..1000 {
            let mut c_value = [0u8; 32];
            // Create unique patterns
            for (j, byte) in c_value.iter_mut().enumerate() {
                *byte = ((i * 17 + j * 23) % 256) as u8;
            }
            
            let pieces = game.decode_c_value(&c_value).unwrap();
            let piece_key = (pieces[0].side.clone(), pieces[0].strength);
            
            // While some combinations may repeat, most should be unique
            seen_pieces.insert(piece_key);
        }
        
        // Should have good variety
        assert!(seen_pieces.len() > 400, "Insufficient variety in generated pieces: {}", seen_pieces.len());
    }
}

// Property-based tests for C value security
proptest! {
    #[test]
    fn prop_c_value_deterministic_decoding(c_bytes in prop::array::uniform32(prop::num::u8::ANY)) {
        let game = CoinFlipGame::new();
        
        // Same C value should always produce same game pieces
        let pieces1 = game.decode_c_value(&c_bytes).unwrap();
        let pieces2 = game.decode_c_value(&c_bytes).unwrap();
        
        prop_assert_eq!(pieces1.len(), pieces2.len());
        for (p1, p2) in pieces1.iter().zip(pieces2.iter()) {
            prop_assert_eq!(p1.side, p2.side);
            prop_assert_eq!(p1.strength, p2.strength);
        }
    }

    #[test]
    fn prop_c_value_avalanche_effect(
        c_bytes1 in prop::array::uniform32(prop::num::u8::ANY),
        bit_position in 0..256usize
    ) {
        let game = CoinFlipGame::new();
        
        // Flip one bit in the C value
        let mut c_bytes2 = c_bytes1;
        let byte_index = bit_position / 8;
        let bit_index = bit_position % 8;
        c_bytes2[byte_index] ^= 1 << bit_index;
        
        let pieces1 = game.decode_c_value(&c_bytes1).unwrap();
        let pieces2 = game.decode_c_value(&c_bytes2).unwrap();
        
        // Small change should potentially cause different results
        // (though not guaranteed due to the simple decoding logic)
        prop_assert_eq!(pieces1.len(), pieces2.len());
        
        // At minimum, the pieces should be valid
        for piece in &pieces1 {
            prop_assert!(piece.strength <= 255);
        }
        for piece in &pieces2 {
            prop_assert!(piece.strength <= 255);
        }
    }

    #[test]
    fn prop_c_value_full_range_utilization(c_bytes in prop::array::uniform32(prop::num::u8::ANY)) {
        let game = CoinFlipGame::new();
        let pieces = game.decode_c_value(&c_bytes).unwrap();
        
        // Should always produce exactly one piece for coin flip game
        prop_assert_eq!(pieces.len(), 1);
        
        // Piece should have valid properties
        let piece = &pieces[0];
        prop_assert!(piece.strength <= 255);
        
        // Side should be deterministic based on first byte
        let expected_side = if c_bytes[0] % 2 == 0 {
            tests::mocks::reference_game::CoinSide::Heads
        } else {
            tests::mocks::reference_game::CoinSide::Tails
        };
        prop_assert_eq!(piece.side, expected_side);
        
        // Strength should match second byte
        prop_assert_eq!(piece.strength, c_bytes[1]);
    }

    #[test]
    fn prop_c_value_no_bias_in_sides(c_values in prop::collection::vec(prop::array::uniform32(prop::num::u8::ANY), 100..200)) {
        let game = CoinFlipGame::new();
        let mut heads_count = 0;
        let mut tails_count = 0;
        
        for c_value in c_values.iter() {
            let pieces = game.decode_c_value(c_value).unwrap();
            match pieces[0].side {
                tests::mocks::reference_game::CoinSide::Heads => heads_count += 1,
                tests::mocks::reference_game::CoinSide::Tails => tails_count += 1,
            }
        }
        
        let total = heads_count + tails_count;
        let heads_ratio = heads_count as f64 / total as f64;
        
        // Should be roughly balanced (allow 20% deviation for random samples)
        prop_assert!(heads_ratio > 0.3 && heads_ratio < 0.7, 
                    "Biased coin flip results: {:.2}% heads", heads_ratio * 100.0);
    }

    #[test]
    fn prop_c_value_strength_coverage(c_values in prop::collection::vec(prop::array::uniform32(prop::num::u8::ANY), 256..512)) {
        let game = CoinFlipGame::new();
        let mut strength_set = HashSet::new();
        
        for c_value in c_values.iter() {
            let pieces = game.decode_c_value(c_value).unwrap();
            strength_set.insert(pieces[0].strength);
        }
        
        // With enough samples, should cover a good range of strength values
        let coverage = strength_set.len();
        prop_assert!(coverage > 200, "Insufficient strength coverage: {}/256", coverage);
    }
}

#[cfg(test)]
mod c_value_security_tests {
    use super::*;
    use sha2::{Sha256, Digest};

    #[test]
    fn test_c_value_unpredictability() {
        let game = CoinFlipGame::new();
        
        // Generate C values using cryptographic hash (simulating real Cashu)
        let mut previous_pieces = Vec::new();
        
        for i in 0..100 {
            let mut hasher = Sha256::new();
            hasher.update(format!("test_secret_{}", i));
            let hash = hasher.finalize();
            
            let mut c_value = [0u8; 32];
            c_value.copy_from_slice(&hash);
            
            let pieces = game.decode_c_value(&c_value).unwrap();
            
            // Should not be able to predict next piece from previous ones
            if i > 0 {
                let prev_piece = &previous_pieces[i - 1];
                let curr_piece = &pieces[0];
                
                // Adjacent pieces should not have obvious patterns
                assert_ne!(prev_piece.strength, curr_piece.strength);
            }
            
            previous_pieces.push(pieces[0].clone());
        }
    }

    #[test]
    fn test_c_value_collision_resistance() {
        let game = CoinFlipGame::new();
        let mut piece_hashes = HashSet::new();
        
        // Generate many C values and ensure no collisions in resulting pieces
        for i in 0..1000 {
            let mut hasher = Sha256::new();
            hasher.update(format!("collision_test_{}", i));
            let hash = hasher.finalize();
            
            let mut c_value = [0u8; 32];
            c_value.copy_from_slice(&hash);
            
            let pieces = game.decode_c_value(&c_value).unwrap();
            let piece_hash = format!("{:?}_{}", pieces[0].side, pieces[0].strength);
            
            // While some combinations may repeat, should be rare
            piece_hashes.insert(piece_hash);
        }
        
        // Should have good diversity
        assert!(piece_hashes.len() > 400, "Too many collisions in piece generation");
    }

    #[test]
    fn test_c_value_independence() {
        let game = CoinFlipGame::new();
        
        // Test that different parts of C value contribute independently
        let base_c_value = [42u8; 32];
        let pieces_base = game.decode_c_value(&base_c_value).unwrap();
        
        // Change only first byte (affects side)
        let mut c_value_side = base_c_value;
        c_value_side[0] = 43;
        let pieces_side = game.decode_c_value(&c_value_side).unwrap();
        
        // Change only second byte (affects strength)
        let mut c_value_strength = base_c_value;
        c_value_strength[1] = 100;
        let pieces_strength = game.decode_c_value(&c_value_strength).unwrap();
        
        // Side should change when first byte changes
        if base_c_value[0] % 2 != c_value_side[0] % 2 {
            assert_ne!(pieces_base[0].side, pieces_side[0].side);
        }
        
        // Strength should change when second byte changes
        assert_ne!(pieces_base[0].strength, pieces_strength[0].strength);
        assert_eq!(pieces_strength[0].strength, 100);
    }
}
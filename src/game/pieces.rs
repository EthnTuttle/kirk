//! Game piece decoding utilities from Cashu C values

/// Utilities for extracting game pieces from Cashu token C values
///
/// C values provide cryptographic randomness that can be decoded into
/// game-specific pieces like cards, dice rolls, or other random elements.

/// Convert C value bytes to a number in a given range
pub fn c_value_to_range(c_value: &[u8; 32], max: u32) -> u32 {
    // Use first 4 bytes of C value to generate number in range [0, max)
    let bytes = [c_value[0], c_value[1], c_value[2], c_value[3]];
    let num = u32::from_be_bytes(bytes);
    num % max
}

/// Convert C value to a dice roll (1-6)
pub fn c_value_to_dice(c_value: &[u8; 32]) -> u8 {
    (c_value_to_range(c_value, 6) + 1) as u8
}

/// Convert C value to a coin flip (true/false)
pub fn c_value_to_coin_flip(c_value: &[u8; 32]) -> bool {
    c_value[0] % 2 == 0
}

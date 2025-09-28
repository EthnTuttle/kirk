//! Game piece decoding utilities from Cashu C values

use std::fmt;
use crate::error::GameProtocolError;

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

/// Playing card suit enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub enum Suit {
    Clubs = 0,    // Lowest
    Diamonds = 1,
    Hearts = 2,
    Spades = 3,   // Highest
}

impl Suit {
    /// Create suit from numeric value (0-3)
    pub fn from_u8(value: u8) -> Result<Self, GameProtocolError> {
        match value {
            0 => Ok(Suit::Clubs),
            1 => Ok(Suit::Diamonds),
            2 => Ok(Suit::Hearts),
            3 => Ok(Suit::Spades),
            _ => Err(GameProtocolError::InvalidMove(
                format!("Invalid suit value: {}", value)
            )),
        }
    }
    
    /// Get the symbol for this suit
    pub fn symbol(&self) -> &'static str {
        match self {
            Suit::Clubs => "♣",
            Suit::Diamonds => "♦",
            Suit::Hearts => "♥",
            Suit::Spades => "♠",
        }
    }
    
    /// Get the name for this suit
    pub fn name(&self) -> &'static str {
        match self {
            Suit::Clubs => "Clubs",
            Suit::Diamonds => "Diamonds",
            Suit::Hearts => "Hearts",
            Suit::Spades => "Spades",
        }
    }
}

impl fmt::Display for Suit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.symbol())
    }
}

/// Playing card rank enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub enum Rank {
    Two = 2,
    Three = 3,
    Four = 4,
    Five = 5,
    Six = 6,
    Seven = 7,
    Eight = 8,
    Nine = 9,
    Ten = 10,
    Jack = 11,
    Queen = 12,
    King = 13,
    Ace = 14,      // Highest
}

impl Rank {
    /// Create rank from numeric value (0-12, where 0=Two, 12=Ace)
    pub fn from_u8(value: u8) -> Result<Self, GameProtocolError> {
        match value {
            0 => Ok(Rank::Two),
            1 => Ok(Rank::Three),
            2 => Ok(Rank::Four),
            3 => Ok(Rank::Five),
            4 => Ok(Rank::Six),
            5 => Ok(Rank::Seven),
            6 => Ok(Rank::Eight),
            7 => Ok(Rank::Nine),
            8 => Ok(Rank::Ten),
            9 => Ok(Rank::Jack),
            10 => Ok(Rank::Queen),
            11 => Ok(Rank::King),
            12 => Ok(Rank::Ace),
            _ => Err(GameProtocolError::InvalidMove(
                format!("Invalid rank value: {}", value)
            )),
        }
    }
    
    /// Get the numeric value for this rank (2-14)
    pub fn value(&self) -> u8 {
        *self as u8
    }
    
    /// Get the display character for this rank
    pub fn symbol(&self) -> &'static str {
        match self {
            Rank::Two => "2",
            Rank::Three => "3",
            Rank::Four => "4",
            Rank::Five => "5",
            Rank::Six => "6",
            Rank::Seven => "7",
            Rank::Eight => "8",
            Rank::Nine => "9",
            Rank::Ten => "10",
            Rank::Jack => "J",
            Rank::Queen => "Q",
            Rank::King => "K",
            Rank::Ace => "A",
        }
    }
    
    /// Get the name for this rank
    pub fn name(&self) -> &'static str {
        match self {
            Rank::Two => "Two",
            Rank::Three => "Three",
            Rank::Four => "Four",
            Rank::Five => "Five",
            Rank::Six => "Six",
            Rank::Seven => "Seven",
            Rank::Eight => "Eight",
            Rank::Nine => "Nine",
            Rank::Ten => "Ten",
            Rank::Jack => "Jack",
            Rank::Queen => "Queen",
            Rank::King => "King",
            Rank::Ace => "Ace",
        }
    }
}

impl fmt::Display for Rank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.symbol())
    }
}

/// A standard playing card with suit and rank
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct PlayingCard {
    pub suit: Suit,
    pub rank: Rank,
}

impl PlayingCard {
    /// Create a new playing card
    pub fn new(suit: Suit, rank: Rank) -> Self {
        Self { suit, rank }
    }
    
    /// Create a playing card from a C value (52-card deck)
    pub fn from_c_value(c_value: &[u8; 32]) -> Result<Self, GameProtocolError> {
        let card_value = c_value_to_range(c_value, 52);
        let suit = Suit::from_u8((card_value / 13) as u8)?;
        let rank = Rank::from_u8((card_value % 13) as u8)?;
        Ok(Self::new(suit, rank))
    }
    
    /// Get the numeric value for comparison (rank-based, with suit as tiebreaker)
    pub fn comparison_value(&self) -> u16 {
        // Primary comparison by rank, secondary by suit
        (self.rank.value() as u16) * 4 + (self.suit as u16)
    }
    
    /// Get a short string representation (e.g., "A♠", "10♥")
    pub fn short_string(&self) -> String {
        format!("{}{}", self.rank.symbol(), self.suit.symbol())
    }
    
    /// Get a full name representation (e.g., "Ace of Spades")
    pub fn full_name(&self) -> String {
        format!("{} of {}", self.rank.name(), self.suit.name())
    }
}

impl PartialOrd for PlayingCard {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PlayingCard {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Compare by rank first, then by suit
        match self.rank.cmp(&other.rank) {
            std::cmp::Ordering::Equal => self.suit.cmp(&other.suit),
            other => other,
        }
    }
}

impl fmt::Display for PlayingCard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.short_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_suit_from_u8() {
        assert_eq!(Suit::from_u8(0).unwrap(), Suit::Clubs);
        assert_eq!(Suit::from_u8(1).unwrap(), Suit::Diamonds);
        assert_eq!(Suit::from_u8(2).unwrap(), Suit::Hearts);
        assert_eq!(Suit::from_u8(3).unwrap(), Suit::Spades);
        assert!(Suit::from_u8(4).is_err());
    }

    #[test]
    fn test_rank_from_u8() {
        assert_eq!(Rank::from_u8(0).unwrap(), Rank::Two);
        assert_eq!(Rank::from_u8(12).unwrap(), Rank::Ace);
        assert!(Rank::from_u8(13).is_err());
    }

    #[test]
    fn test_suit_ordering() {
        assert!(Suit::Clubs < Suit::Diamonds);
        assert!(Suit::Diamonds < Suit::Hearts);
        assert!(Suit::Hearts < Suit::Spades);
    }

    #[test]
    fn test_rank_ordering() {
        assert!(Rank::Two < Rank::Three);
        assert!(Rank::King < Rank::Ace);
        assert_eq!(Rank::Ace.value(), 14);
        assert_eq!(Rank::Two.value(), 2);
    }

    #[test]
    fn test_playing_card_ordering() {
        let two_clubs = PlayingCard::new(Suit::Clubs, Rank::Two);
        let two_spades = PlayingCard::new(Suit::Spades, Rank::Two);
        let ace_clubs = PlayingCard::new(Suit::Clubs, Rank::Ace);
        
        // Higher rank wins
        assert!(ace_clubs > two_clubs);
        assert!(ace_clubs > two_spades);
        
        // Same rank, higher suit wins
        assert!(two_spades > two_clubs);
    }

    #[test]
    fn test_card_from_c_value() {
        // Test specific C values to ensure deterministic card generation
        let c_value_0 = [0u8; 32]; // Should map to first card (Two of Clubs)
        let card_0 = PlayingCard::from_c_value(&c_value_0).unwrap();
        assert_eq!(card_0.suit, Suit::Clubs);
        assert_eq!(card_0.rank, Rank::Two);
        
        // Test a C value that should map to a different card
        let mut c_value_1 = [0u8; 32];
        c_value_1[0] = 1; // This should give us a different card
        let card_1 = PlayingCard::from_c_value(&c_value_1).unwrap();
        
        // Cards should be different
        assert_ne!(card_0, card_1);
    }

    #[test]
    fn test_card_c_value_distribution() {
        // Test that different C values produce different cards
        let mut cards = std::collections::HashSet::new();
        
        // Generate C values that will map to different cards
        // We need to vary the first 4 bytes to get different results from c_value_to_range
        for i in 0..52u32 {
            let mut c_value = [0u8; 32];
            let bytes = i.to_be_bytes();
            c_value[0] = bytes[0];
            c_value[1] = bytes[1];
            c_value[2] = bytes[2];
            c_value[3] = bytes[3];
            let card = PlayingCard::from_c_value(&c_value).unwrap();
            cards.insert(card);
        }
        
        // We should get exactly 52 unique cards
        assert_eq!(cards.len(), 52);
    }

    #[test]
    fn test_card_display() {
        let ace_spades = PlayingCard::new(Suit::Spades, Rank::Ace);
        assert_eq!(ace_spades.short_string(), "A♠");
        assert_eq!(ace_spades.full_name(), "Ace of Spades");
        assert_eq!(format!("{}", ace_spades), "A♠");
        
        let ten_hearts = PlayingCard::new(Suit::Hearts, Rank::Ten);
        assert_eq!(ten_hearts.short_string(), "10♥");
        assert_eq!(ten_hearts.full_name(), "Ten of Hearts");
    }

    #[test]
    fn test_comparison_value() {
        let two_clubs = PlayingCard::new(Suit::Clubs, Rank::Two);
        let two_spades = PlayingCard::new(Suit::Spades, Rank::Two);
        let ace_clubs = PlayingCard::new(Suit::Clubs, Rank::Ace);
        
        // Higher rank should have higher comparison value
        assert!(ace_clubs.comparison_value() > two_clubs.comparison_value());
        
        // Same rank, higher suit should have higher comparison value
        assert!(two_spades.comparison_value() > two_clubs.comparison_value());
    }

    #[test]
    fn test_c_value_to_range() {
        let c_value = [0u8; 32];
        assert_eq!(c_value_to_range(&c_value, 52), 0);
        
        let c_value_max = [255u8; 32];
        let result = c_value_to_range(&c_value_max, 52);
        assert!(result < 52);
    }

    #[test]
    fn test_existing_utilities() {
        let c_value = [1u8; 32];
        
        // Test dice roll
        let dice = c_value_to_dice(&c_value);
        assert!(dice >= 1 && dice <= 6);
        
        // Test coin flip
        let coin = c_value_to_coin_flip(&c_value);
        assert!(coin == true || coin == false);
    }
}
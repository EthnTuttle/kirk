use bevy::prelude::*;
use nostr::{EventId, Keys, PublicKey};
use std::collections::HashMap;
use chrono::{DateTime, Utc};

// Re-export types from the main kirk library
use kirk::cashu::tokens::GameToken as KirkGameToken;

/// Core ECS Components for Game Entities

/// Player component representing a game participant
#[derive(Component, Debug, Clone)]
pub struct Player {
    /// Player's Nostr public key
    pub pubkey: PublicKey,
    /// Player's Nostr keys for signing events
    pub nostr_keys: Keys,
    /// Current balance of Game tokens
    pub balance_game_tokens: u64,
    /// Current balance of Reward tokens
    pub balance_reward_tokens: u64,
}

/// GameToken component wrapping CDK token with game context
#[derive(Component, Debug, Clone)]
pub struct GameToken {
    /// The underlying Kirk GameToken
    pub inner: KirkGameToken,
    /// Token type (Game or Reward)
    pub token_type: GameTokenType,
    /// Reference to the Player entity that owns this token
    pub owner: Entity,
}

/// Token type enumeration
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameTokenType {
    /// Game tokens used for gameplay commitments
    Game,
    /// Reward tokens issued to winners
    Reward,
}

/// PlayingCard component representing a card derived from a token's C value
#[derive(Component, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PlayingCard {
    /// Card suit
    pub suit: Suit,
    /// Card rank
    pub rank: Rank,
    /// Reference to the GameToken entity this card was derived from
    pub derived_from_token: Entity,
}

/// Card suit enumeration (ordered from lowest to highest)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Suit {
    Clubs,    // Lowest
    Diamonds,
    Hearts,
    Spades,   // Highest
}

/// Card rank enumeration (ordered from lowest to highest)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rank {
    Two = 2,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Ten,
    Jack,
    Queen,
    King,
    Ace,      // Highest
}

impl Suit {
    /// Create suit from u8 value (0-3)
    pub fn from_u8(value: u8) -> Result<Self, String> {
        match value {
            0 => Ok(Suit::Clubs),
            1 => Ok(Suit::Diamonds),
            2 => Ok(Suit::Hearts),
            3 => Ok(Suit::Spades),
            _ => Err(format!("Invalid suit value: {}", value)),
        }
    }
    
    /// Convert suit to display string
    pub fn to_string(&self) -> &'static str {
        match self {
            Suit::Clubs => "♣",
            Suit::Diamonds => "♦",
            Suit::Hearts => "♥",
            Suit::Spades => "♠",
        }
    }
}

impl Rank {
    /// Create rank from u8 value (0-12, where 0=Two, 12=Ace)
    pub fn from_u8(value: u8) -> Result<Self, String> {
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
            _ => Err(format!("Invalid rank value: {}", value)),
        }
    }
    
    /// Convert rank to display string
    pub fn to_string(&self) -> &'static str {
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
}

impl std::fmt::Display for PlayingCard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.rank.to_string(), self.suit.to_string())
    }
}

/// Challenge status enumeration
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChallengeStatus {
    /// Challenge is being created, waiting for tokens to be minted
    WaitingForTokens,
    /// Challenge is ready and published, waiting for acceptance
    WaitingForAccept,
    /// Challenge has been accepted, game is starting
    Accepted,
    /// Challenge has expired without acceptance
    Expired,
    /// Challenge was cancelled by the challenger
    Cancelled,
}

/// Challenge component for tracking game challenges
#[derive(Component, Debug, Clone)]
pub struct Challenge {
    /// Unique challenge identifier (Nostr event ID)
    pub challenge_id: EventId,
    /// Reference to the Player entity who created the challenge
    pub challenger: Entity,
    /// Amount of tokens wagered in this challenge
    pub amount: u64,
    /// Hash commitments for the challenger's tokens
    pub commitment_hashes: Vec<String>,
    /// Challenge expiry timestamp
    pub expiry: u64,
    /// Current status of the challenge
    pub status: ChallengeStatus,
}

/// ECS Components for Game State Management

/// ActiveGame component representing an ongoing game
#[derive(Component, Debug, Clone)]
pub struct ActiveGame {
    /// Reference to the original challenge ID
    pub challenge_id: EventId,
    /// References to the two Player entities participating
    pub players: [Entity; 2],
    /// Current phase of the game
    pub phase: GamePhase,
    /// Map of committed tokens for each player
    pub committed_tokens: HashMap<Entity, Vec<Entity>>, // Player -> GameTokens
    /// Map of revealed cards for each player
    pub revealed_cards: HashMap<Entity, Entity>, // Player -> PlayingCard
    /// Timestamp when the game was created
    pub created_at: u64,
    /// Timestamp of last activity (for timeout detection)
    pub last_activity: u64,
}

/// Game phase enumeration
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GamePhase {
    /// Waiting for token commitments from players
    TokenCommitment,
    /// Waiting for players to reveal their tokens
    TokenReveal,
    /// Waiting for final events from players
    WaitingForFinal,
    /// Waiting for game sequence validation
    WaitingForValidation,
    /// Game is complete with optional winner
    Complete { winner: Option<PublicKey> },
    /// Game was forfeited due to timeout or other issues
    Forfeited { reason: String },
}

/// GameSequence component for tracking event chains
#[derive(Component, Debug, Clone)]
pub struct GameSequence {
    /// Root event ID (usually the Challenge event)
    pub root_event: EventId,
    /// Ordered list of events in the game sequence
    pub events: Vec<nostr::Event>,
    /// Current validation status
    pub validation_status: ValidationStatus,
}

/// Validation status enumeration
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationStatus {
    /// Sequence is still being built
    Pending,
    /// Sequence is complete and valid
    Valid { winner: Option<PublicKey> },
    /// Sequence is invalid
    Invalid { reason: String },
    /// Validation failed due to error
    Error { error: String },
}

/// RewardToken component for P2PK locked rewards
#[derive(Component, Debug, Clone)]
pub struct RewardToken {
    /// The underlying GameToken
    pub inner: GameToken,
    /// Reference to the Player entity this token is locked to (P2PK)
    pub locked_to: Entity,
    /// Reference to the game this reward was issued for
    pub issued_for_game: EventId,
}

/// PendingReward component for reward processing queue
#[derive(Component, Debug, Clone)]
pub struct PendingReward {
    /// Reference to the Player entity who won
    pub winner: Entity,
    /// Amount of reward tokens to mint
    pub amount: u64,
    /// Reference to the GameSequence entity this reward is for
    pub game_sequence: Entity,
    /// Timestamp when the reward was queued
    pub queued_at: DateTime<Utc>,
}

/// Utility function to convert C value to card value (0-51 for 52-card deck)
pub fn c_value_to_range(c_value: &[u8; 32], range: u32) -> u32 {
    // Use the first 4 bytes of the C value to create a u32
    let mut bytes = [0u8; 4];
    bytes.copy_from_slice(&c_value[0..4]);
    let value = u32::from_be_bytes(bytes);
    
    // Map to range [0, range)
    value % range
}

/// Utility function to derive a playing card from a C value
pub fn derive_card_from_c_value(c_value: &[u8; 32]) -> Result<PlayingCard, String> {
    let card_value = c_value_to_range(c_value, 52); // 52 cards in deck
    let suit = Suit::from_u8((card_value / 13) as u8)?;
    let rank = Rank::from_u8((card_value % 13) as u8)?;
    
    Ok(PlayingCard { 
        suit, 
        rank, 
        derived_from_token: Entity::PLACEHOLDER, // Will be set when spawning
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(Rank::Ten < Rank::Jack);
    }

    #[test]
    fn test_card_ordering() {
        let card1 = PlayingCard {
            suit: Suit::Clubs,
            rank: Rank::Ace,
            derived_from_token: Entity::PLACEHOLDER,
        };
        let card2 = PlayingCard {
            suit: Suit::Spades,
            rank: Rank::Two,
            derived_from_token: Entity::PLACEHOLDER,
        };
        
        // Ace of Clubs should be less than Two of Spades (rank takes precedence)
        assert!(card1 > card2);
    }

    #[test]
    fn test_c_value_to_range() {
        let c_value = [0u8; 32];
        let result = c_value_to_range(&c_value, 52);
        assert!(result < 52);
        
        // Test with non-zero value
        let mut c_value = [0u8; 32];
        c_value[0] = 1;
        let result = c_value_to_range(&c_value, 52);
        assert!(result < 52);
    }

    #[test]
    fn test_derive_card_from_c_value() {
        let c_value = [0u8; 32];
        let card = derive_card_from_c_value(&c_value).unwrap();
        assert_eq!(card.suit, Suit::Clubs);
        assert_eq!(card.rank, Rank::Two);
    }

    #[test]
    fn test_suit_from_u8() {
        assert_eq!(Suit::from_u8(0).unwrap(), Suit::Clubs);
        assert_eq!(Suit::from_u8(3).unwrap(), Suit::Spades);
        assert!(Suit::from_u8(4).is_err());
    }

    #[test]
    fn test_rank_from_u8() {
        assert_eq!(Rank::from_u8(0).unwrap(), Rank::Two);
        assert_eq!(Rank::from_u8(12).unwrap(), Rank::Ace);
        assert!(Rank::from_u8(13).is_err());
    }
}
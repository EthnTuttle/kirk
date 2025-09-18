# Kirk Gaming Protocol Examples

This directory contains comprehensive examples demonstrating the Kirk gaming protocol framework.

## Overview

Kirk is a trustless gaming protocol that combines Cashu ecash tokens with Nostr events for cryptographically-secured gameplay. The examples here show:

1. **Complete Game Implementations** - Two different game types showing framework flexibility
2. **C Value Decoding** - How to extract game pieces from Cashu token C values
3. **Full Game Flow** - From challenge creation to reward distribution
4. **Framework Usage** - How to implement your own games

## Example Games

### 1. CoinFlip Game (`CoinFlipGame`)

A simple coin flip game demonstrating basic framework usage:

- **Game Pieces**: Coin side (heads/tails) and strength value
- **C Value Usage**: First byte determines side (even=heads, odd=tails), second byte is strength
- **Winner Determination**: XOR all strength values for randomness, player with correct guess and highest confidence wins
- **Strategy**: Players must commit to their guess and confidence level

```rust
use kirk::examples::games::CoinFlipGame;

let game = CoinFlipGame::new();
let c_value = [42u8, 150u8, /* ... 30 more bytes */];
let pieces = game.decode_c_value(&c_value)?;
// pieces[0].side = Heads (42 is even)
// pieces[0].strength = 150
```

### 2. Dice Game (`DiceGame`)

A more complex dice game showing advanced features:

- **Game Pieces**: Multiple dice with configurable sides
- **C Value Usage**: Multiple bytes from C value generate different dice values
- **Winner Determination**: Player with highest total of kept dice wins
- **Strategy**: Players can choose which dice to keep, reroll others (if enabled)

```rust
use kirk::examples::games::DiceGame;

let game = DiceGame::new(); // 5 dice, 6 sides each
let c_value = [6u8, 12u8, 18u8, 24u8, 30u8, /* ... */];
let pieces = game.decode_c_value(&c_value)?;
// pieces = [1, 1, 1, 1, 1] (each byte % 6 + 1)
```

## Complete Game Flow Example

Here's how a complete game works from start to finish:

### 1. Setup Phase
```rust
// Players create clients with nostr keys and CDK wallets
let player1_client = PlayerClient::new(nostr_client1, cashu_wallet1, keys1);
let player2_client = PlayerClient::new(nostr_client2, cashu_wallet2, keys2);

// Players mint game tokens from the mint
let game_tokens1 = mint.mint_game_tokens(1000).await?; // 1000 sat worth
let game_tokens2 = mint.mint_game_tokens(1000).await?;
```

### 2. Challenge Phase
```rust
// Player 1 creates a challenge
let game = CoinFlipGame::new();
let challenge_id = player1_client.create_challenge(
    &game,
    &game_tokens1,
    Some(3600) // 1 hour expiry
).await?;

// Player 2 accepts the challenge
let accept_id = player2_client.accept_challenge(
    challenge_id,
    &game,
    &game_tokens2
).await?;
```

### 3. Gameplay Phase
```rust
// Player 1 makes their move (reveals tokens and choice)
let move1_id = player1_client.make_move(
    accept_id,
    MoveType::Move,
    CoinFlipMove {
        chosen_side: CoinSide::Heads,
        confidence: 200,
    },
    Some(game_tokens1) // Reveal tokens
).await?;

// Player 2 makes their move
let move2_id = player2_client.make_move(
    move1_id,
    MoveType::Move,
    CoinFlipMove {
        chosen_side: CoinSide::Tails,
        confidence: 150,
    },
    Some(game_tokens2) // Reveal tokens
).await?;
```

### 4. Finalization Phase
```rust
// Both players publish Final events
let final1_id = player1_client.finalize_game(
    challenge_id,
    None, // Single token, no commitment method needed
    serde_json::json!({"final_state": "complete"})
).await?;

let final2_id = player2_client.finalize_game(
    challenge_id,
    None,
    serde_json::json!({"final_state": "complete"})
).await?;
```

### 5. Validation and Rewards
```rust
// Mint collects all events and validates the sequence
let all_events = relay.get_game_events(challenge_id).await?;
let validation_result = game.validate_sequence(&all_events)?;

if validation_result.is_valid {
    if let Some(winner_pubkey) = validation_result.winner {
        // Mint issues P2PK-locked reward tokens to winner
        let reward_tokens = mint.mint_reward_tokens(2000, winner_pubkey).await?;
        
        // Publish reward event to nostr
        mint.publish_game_result(&all_events, winner_pubkey, reward_tokens).await?;
    }
}
```

## C Value Decoding Patterns

The C values from Cashu tokens provide cryptographic randomness for game pieces. Here are common patterns:

### Single Value Extraction
```rust
fn decode_c_value(&self, c_value: &[u8; 32]) -> Result<Vec<Self::GamePiece>, GameProtocolError> {
    // Use first byte for primary value
    let primary_value = c_value[0];
    
    // Use second byte for secondary attribute
    let secondary_value = c_value[1];
    
    Ok(vec![GamePiece {
        primary: primary_value % max_primary,
        secondary: secondary_value,
    }])
}
```

### Multiple Values from Single C Value
```rust
fn decode_c_value(&self, c_value: &[u8; 32]) -> Result<Vec<Self::GamePiece>, GameProtocolError> {
    let mut pieces = Vec::new();
    
    // Extract multiple pieces from different bytes
    for i in 0..num_pieces {
        let byte_index = i % 32;
        let value = c_value[byte_index] % max_value + 1;
        pieces.push(GamePiece { value });
    }
    
    Ok(pieces)
}
```

### Complex Transformations
```rust
fn decode_c_value(&self, c_value: &[u8; 32]) -> Result<Vec<Self::GamePiece>, GameProtocolError> {
    // Combine multiple bytes for larger value space
    let combined = u32::from_be_bytes([c_value[0], c_value[1], c_value[2], c_value[3]]);
    
    // Use different parts of the combined value
    let piece_type = (combined >> 24) % num_types;
    let piece_strength = (combined >> 16) & 0xFF;
    let piece_modifier = combined & 0xFFFF;
    
    Ok(vec![GamePiece {
        piece_type: piece_type as u8,
        strength: piece_strength as u8,
        modifier: piece_modifier,
    }])
}
```

## Implementing Your Own Game

To create a new game, implement the `Game` trait:

```rust
use kirk::{Game, GameProtocolError};
use nostr::{Event as NostrEvent, PublicKey};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone)]
pub struct MyGame {
    // Game configuration
}

#[derive(Debug, Clone)]
pub struct MyGamePiece {
    // Your game piece data
}

#[derive(Debug, Clone)]
pub struct MyGameState {
    // Track game state during validation
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyMoveData {
    // Data for player moves
}

impl Game for MyGame {
    type GamePiece = MyGamePiece;
    type GameState = MyGameState;
    type MoveData = MyMoveData;

    fn decode_c_value(&self, c_value: &[u8; 32]) -> Result<Vec<Self::GamePiece>, GameProtocolError> {
        // Extract game pieces from C value
        todo!()
    }

    fn validate_sequence(&self, events: &[NostrEvent]) -> Result<ValidationResult, GameProtocolError> {
        // Validate the complete game sequence
        todo!()
    }

    fn is_sequence_complete(&self, events: &[NostrEvent]) -> Result<bool, GameProtocolError> {
        // Check if game is finished
        todo!()
    }

    fn determine_winner(&self, events: &[NostrEvent]) -> Result<Option<PublicKey>, GameProtocolError> {
        // Determine the winner
        todo!()
    }

    fn required_final_events(&self) -> usize {
        // How many players must publish Final events
        2 // Usually both players
    }
}
```

## Key Design Principles

### 1. Cryptographic Randomness
- C values provide unbiased randomness that neither player can predict or control
- Use multiple bytes from C values for complex game pieces
- XOR operations can combine randomness from multiple players

### 2. Commit-and-Reveal
- Players first commit to hash of their tokens (hiding the actual values)
- Later reveal the actual tokens during moves
- Prevents players from seeing opponent's pieces before making decisions

### 3. Trustless Validation
- All game logic is deterministic and verifiable
- Mint validates complete sequences using only the revealed information
- Third parties can independently verify game outcomes

### 4. Flexible Framework
- Game trait allows different game types with same protocol
- Associated types provide type safety for game-specific data
- Event-driven architecture supports various game flows

## Running the Examples

To run the example demonstrations:

```bash
# Run the complete game flow demo
cargo run --example games --bin demo_flow

# Run the framework flexibility demo  
cargo run --example games --bin demo_flexibility

# Run all example tests
cargo test --example games
```

## Security Considerations

1. **C Value Entropy**: Ensure C values provide sufficient randomness for your game
2. **Commitment Security**: Use cryptographically secure hash functions
3. **Sequence Integrity**: Validate complete event chains for tampering
4. **Timeout Handling**: Implement timeouts to prevent games from stalling
5. **Fraud Detection**: Validate all revealed tokens match original commitments

## Next Steps

1. Study the example games to understand the patterns
2. Implement your own game using the `Game` trait
3. Test your game with the provided mock infrastructure
4. Deploy with real Cashu mints and Nostr relays
5. Consider advanced features like multi-round games or tournaments
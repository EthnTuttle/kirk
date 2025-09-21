# Kirk Usage Guide

This guide provides practical examples for using Kirk in different roles: Player, Mint Operator, and Validator.

## Table of Contents

- [Quick Start](#quick-start)
- [Player Guide](#player-guide)
- [Mint Operator Guide](#mint-operator-guide)
- [Validator Guide](#validator-guide)
- [Game Implementation Guide](#game-implementation-guide)

## Quick Start

### Dependencies

Add Kirk to your `Cargo.toml`:

```toml
[dependencies]
kirk = "0.1.0"
cdk = "0.12.1"
nostr-sdk = "0.35"
tokio = { version = "1.0", features = ["full"] }
serde_json = "1.0"
```

### Basic Setup

```rust
use kirk::prelude::*;
use nostr_sdk::{Client, Keys};
use cdk::wallet::Wallet;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup Nostr client
    let keys = Keys::generate();
    let nostr_client = Client::new(&keys);
    nostr_client.add_relay("wss://relay.damus.io").await?;
    nostr_client.connect().await;

    // Setup Cashu wallet (implementation depends on your mint)
    let cashu_wallet = Wallet::new(/* mint_url, database */)?;

    // Create player client
    let player = PlayerClient::new(nostr_client, cashu_wallet, keys);

    Ok(())
}
```

## Player Guide

### Creating a Challenge

```rust
use kirk::{PlayerClient, GameToken, GameTokenType};
use kirk::games::CoinFlipGame; // Example game

async fn create_challenge_example(player: &PlayerClient) -> Result<(), GameError> {
    // 1. Mint some Game tokens
    let game_tokens = player.mint_game_tokens(100).await?; // 100 sats worth
    
    // 2. Create a coin flip game instance
    let game = CoinFlipGame::new();
    
    // 3. Create challenge with 1-hour expiry (default)
    let challenge_id = player.create_challenge_default(&game, &game_tokens).await?;
    
    println!("Challenge created: {}", challenge_id);
    Ok(())
}
```

### Accepting a Challenge

```rust
async fn accept_challenge_example(
    player: &PlayerClient, 
    challenge_id: EventId
) -> Result<(), GameError> {
    // 1. Mint tokens to match the challenge
    let game_tokens = player.mint_game_tokens(100).await?;
    
    // 2. Create game instance
    let game = CoinFlipGame::new();
    
    // 3. Accept the challenge
    let accept_id = player.accept_challenge(challenge_id, &game, &game_tokens).await?;
    
    println!("Challenge accepted: {}", accept_id);
    Ok(())
}
```

### Making Moves

```rust
use kirk::events::MoveType;

async fn make_moves_example(
    player: &PlayerClient,
    previous_event_id: EventId,
    game_tokens: Vec<GameToken>
) -> Result<(), GameError> {
    let game = CoinFlipGame::new();
    
    // 1. Make a regular move (reveal tokens immediately)
    let move_data = CoinFlipMove { call: CoinSide::Heads };
    let move_id = player.make_move(
        previous_event_id,
        MoveType::Move,
        move_data,
        Some(game_tokens.clone()) // Reveal tokens
    ).await?;
    
    // 2. Or use commit-reveal for strategic play
    // First, commit without revealing tokens
    let commit_id = player.make_move(
        previous_event_id,
        MoveType::Commit,
        move_data,
        None // Don't reveal yet
    ).await?;
    
    // Later, reveal the tokens
    let reveal_id = player.make_move(
        commit_id,
        MoveType::Reveal,
        move_data,
        Some(game_tokens) // Now reveal tokens
    ).await?;
    
    Ok(())
}
```

### Finalizing Games

```rust
use kirk::cashu::CommitmentMethod;

async fn finalize_game_example(
    player: &PlayerClient,
    game_root: EventId
) -> Result<(), GameError> {
    // If you used multiple tokens with a single commitment,
    // specify the method used
    let commitment_method = Some(CommitmentMethod::MerkleTreeRadix4);
    
    let final_state = serde_json::json!({
        "game_complete": true,
        "player_satisfied": true
    });
    
    let final_id = player.finalize_game(
        game_root,
        commitment_method,
        final_state
    ).await?;
    
    println!("Game finalized: {}", final_id);
    Ok(())
}
```

### Complete Player Example

```rust
use kirk::prelude::*;

async fn complete_player_flow() -> Result<(), GameError> {
    // Setup
    let keys = Keys::generate();
    let nostr_client = Client::new(&keys);
    let cashu_wallet = setup_wallet().await?;
    let player = PlayerClient::new(nostr_client, cashu_wallet, keys);
    
    // Create challenge
    let game = CoinFlipGame::new();
    let tokens = player.mint_game_tokens(100).await?;
    let challenge_id = player.create_challenge_default(&game, &tokens).await?;
    
    // Wait for acceptance (in real app, listen for events)
    let accept_event = wait_for_challenge_accept(challenge_id).await?;
    
    // Make move
    let move_data = CoinFlipMove { call: CoinSide::Heads };
    let move_id = player.make_move(
        accept_event.id,
        MoveType::Move,
        move_data,
        Some(tokens)
    ).await?;
    
    // Finalize
    player.finalize_game(challenge_id, None, serde_json::json!({})).await?;
    
    Ok(())
}
```

## Mint Operator Guide

### Setting Up a Game Mint

```rust
use kirk::{GameMint, SequenceProcessor};
use cdk::mint::Mint;

async fn setup_game_mint() -> Result<GameMint, GameError> {
    // 1. Create CDK mint (implementation specific)
    let cdk_mint = Mint::new(/* mint config */)?;
    
    // 2. Setup Nostr client for the mint
    let mint_keys = Keys::generate();
    let nostr_client = Client::new(&mint_keys);
    nostr_client.add_relay("wss://relay.damus.io").await?;
    nostr_client.connect().await;
    
    // 3. Wrap with GameMint
    let game_mint = GameMint::new(cdk_mint, nostr_client);
    
    Ok(game_mint)
}
```

### Processing Game Sequences

```rust
async fn process_game_sequences(mint: &GameMint) -> Result<(), GameError> {
    let game = CoinFlipGame::new();
    let processor = SequenceProcessor::new(game, mint.clone());
    
    // Listen for Final events to trigger processing
    let mut subscription = mint.nostr_client.subscribe(vec![
        Filter::new().kind(Kind::Custom(9262)) // Final events
    ]).await;
    
    while let Ok(notification) = subscription.recv().await {
        if let RelayPoolNotification::Event(_, event) = notification {
            // Collect full game sequence
            let events = collect_game_sequence(&event).await?;
            
            // Process the sequence
            match processor.process_sequence(&events).await {
                Ok(result) => {
                    if let Some(winner) = result.winner {
                        println!("Game completed, winner: {}", winner);
                        // Rewards are automatically distributed
                    }
                }
                Err(e) => {
                    eprintln!("Game processing failed: {}", e);
                    // Publish ValidationFailure event
                    publish_validation_failure(&mint, &events, &e).await?;
                }
            }
        }
    }
    
    Ok(())
}
```

### Minting Reward Tokens

```rust
async fn mint_rewards_example(
    mint: &GameMint,
    winner: PublicKey,
    burned_amount: u64
) -> Result<(), GameError> {
    // Mint P2PK locked reward tokens
    let reward_tokens = mint.mint_reward_tokens(burned_amount, winner).await?;
    
    // Publish reward event
    let game_sequence = vec![/* game events */];
    mint.publish_game_result(&game_sequence, winner, reward_tokens).await?;
    
    Ok(())
}
```

### Fraud Detection

```rust
async fn detect_fraud_example(
    processor: &SequenceProcessor<CoinFlipGame>,
    events: &[Event]
) -> Result<(), GameError> {
    match processor.validate_and_determine_winner(events).await {
        Ok(Some(winner)) => {
            // Valid game, distribute rewards
            let burned_tokens = extract_burned_tokens(events)?;
            processor.distribute_rewards(winner, &burned_tokens).await?;
        }
        Ok(None) => {
            // Draw or incomplete game
            println!("Game ended in draw or is incomplete");
        }
        Err(GameError::GameValidation(msg)) => {
            // Fraud detected - forfeit cheating player
            let honest_player = determine_honest_player(events)?;
            let all_tokens = extract_all_tokens(events)?;
            processor.distribute_rewards(honest_player, &all_tokens).await?;
        }
        Err(e) => {
            // System error - no rewards distributed
            eprintln!("System error: {}", e);
        }
    }
    
    Ok(())
}
```

## Validator Guide

### Independent Game Verification

```rust
use kirk::ValidationClient;

async fn validate_game_independently() -> Result<(), GameError> {
    // Setup validation client (no wallet needed)
    let keys = Keys::generate();
    let nostr_client = Client::new(&keys);
    let validator = ValidationClient::new(nostr_client);
    
    // Find a game to validate
    let challenge_id = EventId::from_hex("...")?;
    
    // Collect all game events
    let events = validator.collect_game_events(challenge_id).await?;
    
    // Validate the sequence
    let game = CoinFlipGame::new();
    let result = validator.validate_game_sequence(&game, &events).await?;
    
    if result.is_valid {
        if let Some(winner) = result.winner {
            println!("Valid game, winner: {}", winner);
        } else {
            println!("Valid game, ended in draw");
        }
    } else {
        println!("Invalid game detected:");
        for error in result.errors {
            println!("  - {}: {}", error.event_id, error.message);
        }
    }
    
    Ok(())
}
```

### Commitment Verification

```rust
async fn verify_commitments_example(
    validator: &ValidationClient,
    events: &[Event]
) -> Result<(), ValidationError> {
    // Verify all commitments in the game sequence
    let is_valid = validator.verify_commitments(events).await?;
    
    if is_valid {
        println!("All commitments are valid");
    } else {
        println!("Invalid commitments detected");
        
        // Manual verification for detailed analysis
        for event in events {
            if let Ok(content) = parse_move_content(&event.content) {
                if let Some(tokens) = content.revealed_tokens {
                    // Verify each token against its commitment
                    // Implementation depends on finding the original commitment
                }
            }
        }
    }
    
    Ok(())
}
```

### Monitoring Game Activity

```rust
async fn monitor_games() -> Result<(), GameError> {
    let validator = ValidationClient::new(setup_nostr_client().await?);
    
    // Subscribe to all game events
    let filters = vec![
        Filter::new().kind(Kind::Custom(9259)), // Challenges
        Filter::new().kind(Kind::Custom(9260)), // Accepts
        Filter::new().kind(Kind::Custom(9261)), // Moves
        Filter::new().kind(Kind::Custom(9262)), // Finals
        Filter::new().kind(Kind::Custom(9263)), // Rewards
    ];
    
    let mut subscription = validator.nostr_client.subscribe(filters).await;
    
    while let Ok(notification) = subscription.recv().await {
        if let RelayPoolNotification::Event(_, event) = notification {
            match event.kind {
                Kind::Custom(9259) => println!("New challenge: {}", event.id),
                Kind::Custom(9260) => println!("Challenge accepted: {}", event.id),
                Kind::Custom(9261) => println!("Move made: {}", event.id),
                Kind::Custom(9262) => {
                    // Game completed, validate it
                    let events = validator.collect_game_events(event.id).await?;
                    let game = CoinFlipGame::new();
                    let result = validator.validate_game_sequence(&game, &events).await?;
                    println!("Game validation result: {:?}", result);
                }
                Kind::Custom(9263) => println!("Rewards distributed: {}", event.id),
                _ => {}
            }
        }
    }
    
    Ok(())
}
```

## Game Implementation Guide

### Implementing a Custom Game

```rust
use kirk::game::{Game, GameError};
use serde::{Serialize, Deserialize};

// 1. Define your game pieces
#[derive(Clone, Debug)]
pub enum CardSuit {
    Hearts, Diamonds, Clubs, Spades
}

#[derive(Clone, Debug)]
pub struct Card {
    pub suit: CardSuit,
    pub value: u8, // 1-13
}

// 2. Define game state
#[derive(Clone, Debug)]
pub struct PokerGameState {
    pub phase: GamePhase,
    pub pot: u64,
    pub players_folded: Vec<bool>,
}

#[derive(Clone, Debug)]
pub enum GamePhase {
    Preflop, Flop, Turn, River, Showdown
}

// 3. Define move data
#[derive(Serialize, Deserialize)]
pub enum PokerMove {
    Fold,
    Call,
    Raise { amount: u64 },
    AllIn,
}

// 4. Implement the Game trait
pub struct PokerGame {
    pub deck_size: usize,
}

impl Game for PokerGame {
    type GamePiece = Card;
    type GameState = PokerGameState;
    type MoveData = PokerMove;
    
    fn decode_c_value(&self, c_value: &[u8; 32]) -> Result<Vec<Self::GamePiece>, GameError> {
        // Convert C value bytes to cards
        let mut cards = Vec::new();
        
        // Use first 10 bytes for 5 cards (2 bytes each)
        for i in 0..5 {
            let card_bytes = [c_value[i*2], c_value[i*2 + 1]];
            let card_value = u16::from_be_bytes(card_bytes);
            
            let suit = match (card_value >> 14) & 0x3 {
                0 => CardSuit::Hearts,
                1 => CardSuit::Diamonds,
                2 => CardSuit::Clubs,
                3 => CardSuit::Spades,
                _ => unreachable!(),
            };
            
            let value = ((card_value & 0x3FFF) % 13) as u8 + 1;
            
            cards.push(Card { suit, value });
        }
        
        Ok(cards)
    }
    
    fn validate_sequence(&self, events: &[NostrEvent]) -> Result<ValidationResult, GameError> {
        // Implement poker-specific validation logic
        // Check betting rounds, valid moves, etc.
        todo!("Implement poker validation")
    }
    
    fn is_sequence_complete(&self, events: &[NostrEvent]) -> Result<bool, GameError> {
        // Check if game has reached showdown or all but one folded
        todo!("Implement completion check")
    }
    
    fn determine_winner(&self, events: &[NostrEvent]) -> Result<Option<PublicKey>, GameError> {
        // Determine winner based on poker rules
        todo!("Implement winner determination")
    }
    
    fn required_final_events(&self) -> usize {
        2 // Both players should publish Final events
    }
}
```

### Testing Your Game

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use kirk::test_utils::*;
    
    #[tokio::test]
    async fn test_poker_game_flow() {
        let game = PokerGame { deck_size: 52 };
        
        // Test C value decoding
        let c_value = [1u8; 32]; // Test value
        let cards = game.decode_c_value(&c_value).unwrap();
        assert_eq!(cards.len(), 5);
        
        // Test with mock events
        let events = create_mock_poker_game().await;
        let result = game.validate_sequence(&events).unwrap();
        assert!(result.is_valid);
    }
}
```

### Integration with Kirk

```rust
// Use your custom game with Kirk clients
async fn play_poker_game() -> Result<(), GameError> {
    let player = setup_player_client().await?;
    let game = PokerGame { deck_size: 52 };
    
    // Mint tokens for ante
    let tokens = player.mint_game_tokens(1000).await?; // 1000 sats ante
    
    // Create challenge
    let challenge_id = player.create_challenge_default(&game, &tokens).await?;
    
    // Rest of the game flow...
    Ok(())
}
```

## Best Practices

### Security Considerations

1. **Always validate tokens**: Never trust revealed tokens without verification
2. **Use timeouts**: Implement reasonable timeouts for commit-reveal sequences
3. **Verify commitments**: Always check that revealed tokens match commitments
4. **Handle edge cases**: Plan for network failures, invalid events, etc.

### Performance Tips

1. **Batch operations**: Group multiple token operations when possible
2. **Cache game state**: Store intermediate game states to avoid recomputation
3. **Use appropriate commitment methods**: Merkle trees for many tokens, concatenation for few
4. **Optimize C value decoding**: Make game piece extraction efficient

### Error Handling

1. **Graceful degradation**: Handle network failures gracefully
2. **Clear error messages**: Provide helpful error information to users
3. **Retry logic**: Implement appropriate retry mechanisms for transient failures
4. **Logging**: Log important events for debugging and monitoring
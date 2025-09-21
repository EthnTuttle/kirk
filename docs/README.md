# Kirk Documentation

Kirk is a trustless gaming protocol that combines Cashu ecash tokens with Nostr events for cryptographically-secured gameplay.

## Documentation Overview

This documentation provides comprehensive guidance for developers, players, mint operators, and validators using Kirk.

### Quick Start

- **[Usage Guide](USAGE_GUIDE.md)** - Practical examples for all user types
- **[API Documentation](API.md)** - Complete API reference
- **[Examples](../examples/)** - Working code examples

### Technical Documentation

- **[Commitment Algorithms](COMMITMENTS.md)** - Detailed commitment construction algorithms
- **[Security Guide](SECURITY.md)** - Security considerations and best practices
- **[Troubleshooting](TROUBLESHOOTING.md)** - Common issues and solutions

## What is Kirk?

Kirk enables trustless gaming by combining two powerful technologies:

1. **Cashu Ecash**: Provides cryptographic tokens with unblinded signatures (C values) that serve as sources of randomness for game pieces
2. **Nostr Events**: Enables decentralized coordination and verification of game sequences

### Key Features

- **Trustless**: No central game server required - all gameplay is cryptographically verifiable
- **Decentralized**: Uses Nostr's decentralized event system for coordination
- **Flexible**: Trait-based system supports different game implementations
- **Private**: Preserves Cashu's privacy properties where possible
- **Secure**: All moves are cryptographically verifiable using hash commitments

## Architecture Overview

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Game Client   │    │   Nostr Relay   │    │   Cashu Mint    │
│                 │    │                 │    │                 │
│ - Challenge     │◄──►│ - Event Storage │◄──►│ - Token Mint    │
│ - Move          │    │ - Event Query   │    │ - Validation    │
│ - Validation    │    │ - Subscription  │    │ - Rewards       │
└─────────────────┘    └─────────────────┘    └─────────────────┘
```

### Game Flow

1. **Challenge**: Player creates challenge with hash commitments of their tokens
2. **Accept**: Another player accepts with their own commitments
3. **Moves**: Players make sequential moves, revealing tokens as needed
4. **Finalize**: Players publish Final events to complete the game
5. **Validate**: Mint validates the sequence and distributes rewards

## Getting Started

### Installation

Add Kirk to your `Cargo.toml`:

```toml
[dependencies]
kirk = "0.1.0"
cdk = "0.12.1"
nostr-sdk = "0.35"
tokio = { version = "1.0", features = ["full"] }
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

    // Setup Cashu wallet
    let cashu_wallet = Wallet::new(/* mint_url, database */)?;

    // Create player client
    let player = PlayerClient::new(nostr_client, cashu_wallet, keys);

    Ok(())
}
```

### Simple Game Example

```rust
use kirk::games::CoinFlipGame;

async fn play_coin_flip() -> Result<(), GameError> {
    let player = setup_player().await?;
    let game = CoinFlipGame::new();
    
    // Mint tokens for the game
    let tokens = player.mint_game_tokens(100).await?; // 100 sats
    
    // Create challenge
    let challenge_id = player.create_challenge_default(&game, &tokens).await?;
    
    println!("Challenge created: {}", challenge_id);
    // Wait for another player to accept...
    
    Ok(())
}
```

## Core Concepts

### Hash Commitments

Players commit to their tokens using cryptographic hash commitments:

- **Single Token**: `commitment = SHA256(token_hash)`
- **Multiple Tokens**: Use concatenation or Merkle tree methods
- **Binding**: Cannot change tokens after commitment
- **Hiding**: Commitment reveals no information about tokens

### Game Pieces from C Values

Cashu tokens contain C values (unblinded signatures) that provide cryptographic randomness:

```rust
impl Game for CoinFlipGame {
    fn decode_c_value(&self, c_value: &[u8; 32]) -> Result<Vec<CoinSide>, GameError> {
        // Use C value to determine coin side
        let side = if c_value[0] % 2 == 0 {
            CoinSide::Heads
        } else {
            CoinSide::Tails
        };
        Ok(vec![side])
    }
}
```

### Event Types

Kirk uses custom Nostr event kinds:

- **9259**: Challenge - Initial game proposal with commitments
- **9260**: ChallengeAccept - Accept challenge with own commitments  
- **9261**: Move - Game moves (Move/Commit/Reveal types)
- **9262**: Final - Game completion events
- **9263**: Reward - Reward distribution events

### Token Types

- **Game Tokens**: Used for gameplay commitments and burning
- **Reward Tokens**: Distributed to winners, initially P2PK locked

## User Guides

### For Players

See the [Usage Guide](USAGE_GUIDE.md#player-guide) for detailed examples of:
- Creating and accepting challenges
- Making moves with commit-reveal mechanics
- Finalizing games and receiving rewards

### For Mint Operators

See the [Usage Guide](USAGE_GUIDE.md#mint-operator-guide) for:
- Setting up a game mint
- Processing game sequences
- Detecting fraud and distributing rewards

### For Validators

See the [Usage Guide](USAGE_GUIDE.md#validator-guide) for:
- Independent game verification
- Monitoring game activity
- Commitment verification

### For Game Developers

See the [Usage Guide](USAGE_GUIDE.md#game-implementation-guide) for:
- Implementing custom games
- Defining game pieces and rules
- Integration with Kirk framework

## Security

Kirk provides multiple layers of security:

### Cryptographic Security
- SHA256 hash commitments prevent cheating
- Nostr event signatures ensure authenticity
- Cashu token validation prevents double-spending

### Game Integrity
- Event chains create verifiable game history
- Commitment verification ensures players use committed tokens
- Fraud detection automatically handles cheating

### Privacy Protection
- Hash commitments hide token details until reveal
- Cashu's privacy properties are preserved
- Minimal metadata leakage in events

See the [Security Guide](SECURITY.md) for comprehensive security considerations.

## Advanced Topics

### Commitment Construction

Kirk uses standardized algorithms for hash commitments:

- **Token Ordering**: Always sort tokens by hash before commitment
- **Concatenation Method**: Simple concatenation of sorted token hashes
- **Merkle Tree Method**: Radix-4 merkle tree for efficient multi-token commitments

See [Commitment Algorithms](COMMITMENTS.md) for detailed specifications.

### Custom Game Implementation

Implement the `Game` trait to create custom games:

```rust
pub trait Game: Send + Sync {
    type GamePiece: Clone + Debug;
    type GameState: Clone + Debug;
    type MoveData: Serialize + DeserializeOwned;
    
    fn decode_c_value(&self, c_value: &[u8; 32]) -> Result<Vec<Self::GamePiece>, GameError>;
    fn validate_sequence(&self, events: &[NostrEvent]) -> Result<ValidationResult, GameError>;
    fn is_sequence_complete(&self, events: &[NostrEvent]) -> Result<bool, GameError>;
    fn determine_winner(&self, events: &[NostrEvent]) -> Result<Option<PublicKey>, GameError>;
    fn required_final_events(&self) -> usize;
}
```

### P2PK Token Locking

Reward tokens use NUT-11 P2PK locking:

```rust
// Mint P2PK locked reward tokens
let reward_tokens = mint.mint_reward_tokens(amount, winner_pubkey).await?;

// Winner can spend these tokens by providing signature
let unlocked_tokens = winner.unlock_p2pk_tokens(&reward_tokens).await?;
```

## Troubleshooting

Common issues and solutions:

- **Token Validation Failed**: Check token authenticity and spending status
- **Commitment Verification Failed**: Verify commitment method and token ordering
- **Broken Event Chain**: Ensure events properly reference previous events
- **Network Issues**: Use multiple relays for redundancy

See the [Troubleshooting Guide](TROUBLESHOOTING.md) for detailed diagnostic procedures.

## Examples

The `examples/` directory contains working implementations:

- **[Usage Guide](../examples/usage_guide.rs)** - Basic usage patterns
- **[Demo Flow](../examples/demo_flow.rs)** - Complete game flow demonstration
- **[Demo Flexibility](../examples/demo_flexibility.rs)** - Multiple game types
- **[Games](../examples/games/)** - Reference game implementations

## API Reference

Complete API documentation is available in [API.md](API.md), covering:

- Core types and traits
- Client interfaces
- Event structures
- Error handling
- Cashu integration

## Contributing

Kirk is designed to be extensible and welcomes contributions:

1. **Game Implementations**: Create new game types using the Game trait
2. **Client Improvements**: Enhance player, mint, or validator clients
3. **Security Enhancements**: Improve fraud detection and validation
4. **Documentation**: Help improve guides and examples

## License

Kirk is open source software. See the LICENSE file for details.

## Support

- **Documentation**: This documentation covers most use cases
- **Examples**: Working code examples in the `examples/` directory
- **Issues**: Report bugs and request features via GitHub issues
- **Community**: Join discussions about trustless gaming protocols

---

Kirk enables a new paradigm of trustless, decentralized gaming. By combining Cashu's cryptographic tokens with Nostr's decentralized events, it creates games that are provably fair, verifiable by anyone, and require no trusted third parties beyond the cryptographic protocols themselves.
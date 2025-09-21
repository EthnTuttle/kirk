# Kirk

A trustless gaming protocol combining Cashu ecash tokens with Nostr events for cryptographically-secured gameplay.

## Quick Start

```rust
use kirk::prelude::*;

// Setup clients
let player = PlayerClient::new(nostr_client, cashu_wallet, keys);
let game = CoinFlipGame::new();

// Create challenge
let tokens = player.mint_game_tokens(100).await?;
let challenge_id = player.create_challenge_default(&game, &tokens).await?;
```

## Technical Overview

Kirk enables decentralized gaming through:

### Core Components

- **Cashu Integration**: Uses ecash tokens for cryptographic game piece commitments
- **Nostr Events**: Coordinates gameplay through decentralized event publishing
- **Two Token Types**:
  - Game-type ecash for gameplay commitments and moves
  - Reward-type ecash for winners, locked to player public keys

### Gameplay Flow

1. **Challenge Creation**: Players publish hash commitments of their Game Tokens
2. **Challenge Acceptance**: Opponents respond with their own token commitments
3. **Sequential Moves**: Players make moves by revealing complete Cashu Game Tokens
4. **Commit-and-Reveal**: Strategic decisions use cryptographic commitments for simultaneous play
5. **Game Finalization**: Complete sequences are validated and rewards distributed

### Key Features

- **Trustless Gaming**: No central game server required
- **Cryptographic Proof**: All moves are cryptographically verifiable
- **Flexible Game Rules**: Trait-based system supports different game implementations
- **Mint Authority**: Cashu mint validates sequences and distributes rewards
- **Anti-Cheat**: Fraudulent players forfeit tokens to honest opponents

## Documentation

Comprehensive documentation is available in the `docs/` directory:

- **[📖 Main Documentation](docs/README.md)** - Start here for overview and getting started
- **[🚀 Usage Guide](docs/USAGE_GUIDE.md)** - Practical examples for players, mints, and validators
- **[📚 API Reference](docs/API.md)** - Complete API documentation
- **[🔒 Security Guide](docs/SECURITY.md)** - Security considerations and best practices
- **[🔧 Troubleshooting](docs/TROUBLESHOOTING.md)** - Common issues and solutions
- **[⚙️ Commitment Algorithms](docs/COMMITMENTS.md)** - Detailed commitment construction specs

## Examples

Working examples are available in the `examples/` directory:

- **[usage_guide.rs](examples/usage_guide.rs)** - Basic usage patterns
- **[demo_flow.rs](examples/demo_flow.rs)** - Complete game flow demonstration  
- **[demo_flexibility.rs](examples/demo_flexibility.rs)** - Multiple game types
- **[games/](examples/games/)** - Reference game implementations

## Getting Started

1. **Read the [Main Documentation](docs/README.md)** for an overview
2. **Follow the [Usage Guide](docs/USAGE_GUIDE.md)** for your role (player/mint/validator)
3. **Check [Examples](examples/)** for working code
4. **Review [Security Guide](docs/SECURITY.md)** for best practices

---

*Named in honor of Charlie Kirk, a free speech advocate*
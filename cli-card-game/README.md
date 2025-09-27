# Kirk CLI Card Game

A CLI card game demonstrating the Kirk gaming protocol capabilities. This application showcases trustless gameplay using Cashu ecash tokens and Nostr events.

## Features

- **Embedded Services**: Runs with embedded Cashu mint and Nostr relay
- **Bevy ECS Architecture**: Uses Bevy's Entity Component System for game state management
- **Interactive REPL**: Command-line interface for game operations
- **Cryptographic Security**: All moves are cryptographically verifiable
- **Key Derivation**: Deterministic key generation from master seed

## Installation

```bash
cd cli-card-game
cargo build --release
```

## Usage

```bash
# Run with default settings
cargo run --bin kirk-cards

# Run with custom configuration
cargo run --bin kirk-cards -- --wager 200 --timeout 600

# Run with persistent master seed
cargo run --bin kirk-cards -- --seed-file ~/.kirk_master_seed

# Show help
cargo run --bin kirk-cards -- --help
```

## Commands

Once running, use these commands in the REPL:

- `challenge [amount]` - Create a new challenge
- `accept <id>` - Accept an existing challenge  
- `list` - List available challenges
- `status` - Show current game status
- `balance` - Show token balances
- `mint [amount]` - Mint new Game tokens
- `unlock <token_id>` - Unlock Reward tokens
- `help` - Show available commands
- `quit` - Exit the application

## Architecture

The application uses a multi-threaded architecture:

- **REPL Thread**: Interactive command-line interface
- **Bevy ECS World**: Game state management and logic
- **Embedded Mint**: Cashu token operations
- **Embedded Relay**: Nostr event coordination

## Configuration

Command-line options:

- `--config <file>` - Configuration file path
- `--seed-file <file>` - Master seed file for key persistence
- `--wager <amount>` - Default wager amount (default: 100)
- `--timeout <seconds>` - Game timeout (default: 300)
- `--mint-port <port>` - Embedded mint port (default: 3338)
- `--relay-port <port>` - Embedded relay port (default: 7000)

## Development

```bash
# Run in development mode
cargo run

# Run tests
cargo test

# Check code
cargo check
```
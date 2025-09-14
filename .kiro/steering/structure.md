# Project Structure

## Library Organization

```
src/
├── lib.rs                 # Main library interface and public exports
├── events/                # Nostr event types and handling
│   ├── mod.rs            # Event module exports
│   ├── challenge.rs      # Challenge/ChallengeAccept events
│   ├── move_event.rs     # Move/Commit/Reveal events
│   ├── final_event.rs    # Final/FinalForfeit events
│   └── reward.rs         # Reward/ValidationFailure events
├── game/                 # Game trait and implementations
│   ├── mod.rs           # Game module exports
│   ├── traits.rs        # Core game traits (Game, CommitmentValidator)
│   ├── pieces.rs        # Game piece decoding from C values
│   └── validation.rs    # Game sequence validation logic
├── cashu/               # Cashu integration layer
│   ├── mod.rs          # Cashu module exports
│   ├── tokens.rs       # GameToken wrapper and token utilities
│   ├── commitments.rs  # Hash commitment construction
│   └── mint.rs         # GameMint wrapper and operations
├── client/             # Client interfaces
│   ├── mod.rs         # Client module exports
│   ├── player.rs      # PlayerClient for game participation
│   └── validator.rs   # ValidationClient for sequence verification
└── error.rs           # Centralized error types
```

## Key Design Patterns

### Wrapper Pattern
- **GameToken**: Wraps CDK Token with game-specific context
- **GameMint**: Wraps CDK Mint with nostr integration
- **Minimal Extensions**: Only add functionality not available in base libraries

### Trait-Based Flexibility
- **Game Trait**: Defines game-specific logic (piece decoding, validation, completion)
- **CommitmentValidator**: Handles hash commitment verification
- **Associated Types**: GamePiece, GameState, MoveData for type safety

### Event-Driven Architecture
- **Nostr Events**: All game coordination through standardized event types
- **Event Chains**: Sequential events create verifiable game history
- **Content Structures**: Separate content from event handling

## Module Responsibilities

### `events/`
- Define custom Nostr event kinds (9259-9263)
- Implement event content structures with serde serialization
- Provide event builders using rust-nostr's EventBuilder
- Handle event parsing and validation

### `game/`
- Define core Game trait for flexible game implementations
- Implement C value decoding into game pieces
- Provide sequence validation and winner determination
- Handle commitment verification logic

### `cashu/`
- Wrap CDK types with game-specific context
- Implement standardized hash commitment construction
- Provide mint operations for Game and Reward tokens
- Handle P2PK token locking using NUT-11

### `client/`
- PlayerClient for creating challenges, making moves, finalizing games
- ValidationClient for independent sequence verification
- Integrate nostr-sdk and CDK wallet functionality
- Handle commitment creation and token management

## File Naming Conventions

- **Snake Case**: All file and directory names use snake_case
- **Descriptive Names**: Files clearly indicate their purpose (e.g., `move_event.rs`, `commitments.rs`)
- **Module Structure**: Each directory has `mod.rs` for exports and organization

## Import Organization

```rust
// Standard library imports first
use std::collections::HashMap;

// External crate imports
use serde::{Deserialize, Serialize};
use nostr::{Event, EventBuilder, Keys};
use cdk::wallet::Wallet;

// Internal crate imports
use crate::error::GameProtocolError;
use crate::game::traits::Game;
```

## Testing Structure

```
tests/
├── integration/          # Integration tests
│   ├── full_game.rs     # End-to-end game scenarios
│   ├── mint_validation.rs # Mint validation workflows
│   └── event_chains.rs  # Event sequencing tests
├── property/            # Property-based tests
│   ├── commitments.rs   # Commitment security properties
│   └── c_values.rs      # C value randomness properties
└── mocks/               # Mock implementations
    ├── nostr_relay.rs   # Mock Nostr relay
    ├── cashu_mint.rs    # Mock Cashu mint
    └── reference_game.rs # Simple reference game
```

## Configuration Files

- **Cargo.toml**: Dependencies and project metadata
- **README.md**: Project overview and usage examples
- **.gitignore**: Standard Rust gitignore with Cashu/Nostr specifics
- **docs/**: Additional documentation and examples
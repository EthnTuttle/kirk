# Technology Stack

## Language & Runtime
- **Rust**: Primary language for cryptographic security and performance
- **Tokio**: Async runtime for handling concurrent operations

## Core Dependencies

### Cashu Integration
- **CDK 0.12.1**: Cashu Development Kit for ecash token operations
- **NUT-11**: P2PK (Pay-to-Public-Key) for reward token locking

### Nostr Integration  
- **nostr 0.35**: Core Nostr protocol implementation
- **nostr-sdk 0.35**: Client SDK for relay communication and event handling

### Cryptography
- **sha2**: SHA256 hashing for commitments and merkle trees
- **hex**: Hexadecimal encoding/decoding utilities

### Serialization & Data
- **serde**: Serialization framework with derive macros
- **serde_json**: JSON serialization for event content
- **chrono**: Date/time handling with serde support

### Error Handling
- **thiserror**: Structured error types with derive macros
- **anyhow**: Error context and chaining

## Architecture Principles

### Code Reuse Strategy
- **Maximize Existing Libraries**: Use CDK and rust-nostr functionality directly
- **Minimal Extensions**: Only add wrappers when game-specific context is needed
- **Submodule Strategy**: Fork and branch external libraries only if modifications are required

### Integration Patterns
- **Wrapper Types**: GameToken wraps CDK Token, GameMint wraps CDK Mint
- **Trait-Based Games**: Flexible game implementations using Rust traits
- **Event-Driven**: All game coordination through Nostr events

## Common Commands

### Development
```bash
# Build the library
cargo build

# Run tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Check code without building
cargo check

# Format code
cargo fmt

# Run clippy linter
cargo clippy
```

### Testing
```bash
# Run unit tests only
cargo test --lib

# Run integration tests
cargo test --test integration

# Run property-based tests
cargo test property

# Run with specific test filter
cargo test commitment
```

### Documentation
```bash
# Generate and open docs
cargo doc --open

# Generate docs with private items
cargo doc --document-private-items
```

## Custom Event Kinds
- **9259**: Challenge events
- **9260**: ChallengeAccept events  
- **9261**: Move events (Move/Commit/Reveal)
- **9262**: Final events
- **9263**: Reward events

These are contiguous unused kind numbers verified against the NIPs index.
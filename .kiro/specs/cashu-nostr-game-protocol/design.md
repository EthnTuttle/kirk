# Design Document

## Overview

The Cashu-Nostr Game Protocol is a Rust library that combines Cashu ecash tokens with nostr events to create a trustless, cryptographically-secured gaming framework. The system leverages the unblinded signatures (C values) from Cashu tokens as sources of randomness for game pieces, while using nostr's decentralized event system for game coordination and verification.

The architecture consists of three main actors:
- **Players**: Create challenges, accept challenges, make moves, and reveal tokens
- **Mint**: Acts as game authority, validates sequences, and distributes rewards  
- **Validators**: Anyone who can verify game sequences by collecting nostr events

## Architecture

### Core Components

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Game Client   │    │   Nostr Relay   │    │   Cashu Mint    │
│                 │    │                 │    │                 │
│ - Challenge     │◄──►│ - Event Storage │◄──►│ - Token Mint    │
│ - Move          │    │ - Event Query   │    │ - Validation    │
│ - Validation    │    │ - Subscription  │    │ - Rewards       │
└─────────────────┘    └─────────────────┘    └─────────────────┘
```

### Library Structure

```
src/
├── lib.rs                 # Main library interface
├── events/                # Nostr event types and handling
│   ├── mod.rs
│   ├── challenge.rs       # Challenge/ChallengeAccept events
│   ├── move_event.rs      # Move/Commit/Reveal events
│   ├── final_event.rs     # Final/FinalForfeit events
│   └── reward.rs          # Reward/ValidationFailure events
├── game/                  # Game trait and implementations
│   ├── mod.rs
│   ├── traits.rs          # Core game traits
│   ├── pieces.rs          # Game piece decoding from C values
│   └── validation.rs      # Game sequence validation
├── cashu/                 # Cashu integration
│   ├── mod.rs
│   ├── tokens.rs          # Game/Reward token handling
│   ├── commitments.rs     # Hash commitment utilities
│   └── mint.rs            # Mint operations
├── client/                # Client interface
│   ├── mod.rs
│   ├── player.rs          # Player client
│   └── validator.rs       # Validation client
└── error.rs               # Error types
```

## Components and Interfaces

### 1. Nostr Event Types (Reusing rust-nostr)

```rust
use nostr::{Event, EventBuilder, EventId, PublicKey, Kind};

// Reuse rust-nostr's Event directly, only define content structures
// Using contiguous unused kind numbers verified against NIPs index
const CHALLENGE_KIND: Kind = Kind::Custom(9259);
const CHALLENGE_ACCEPT_KIND: Kind = Kind::Custom(9260);
const MOVE_KIND: Kind = Kind::Custom(9261);
const FINAL_KIND: Kind = Kind::Custom(9262);
const REWARD_KIND: Kind = Kind::Custom(9263);
```

#### Event Content Structures (Minimal Extensions)
```rust
#[derive(Serialize, Deserialize)]
pub struct ChallengeContent {
    pub game_type: String,
    pub commitment_hashes: Vec<String>,
    pub game_parameters: serde_json::Value,
    pub expiry: Option<u64>,
}

#[derive(Serialize, Deserialize)]
pub struct ChallengeAcceptContent {
    pub challenge_id: EventId,
    pub commitment_hashes: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct MoveContent {
    pub previous_event_id: EventId,
    pub move_type: MoveType,
    pub move_data: serde_json::Value,
    pub revealed_tokens: Option<Vec<CashuToken>>,
}

#[derive(Serialize, Deserialize)]
pub enum MoveType { Move, Commit, Reveal }

#[derive(Serialize, Deserialize)]
pub struct FinalContent {
    pub game_sequence_root: EventId,
    pub commitment_method: Option<CommitmentMethod>,
    pub final_state: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
pub struct RewardContent {
    pub game_sequence_root: EventId,
    pub winner_pubkey: PublicKey,
    pub reward_tokens: Vec<GameToken>,
    pub unlock_instructions: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub enum CommitmentMethod {
    /// Simple concatenation of token hashes in ascending order
    Concatenation,
    /// Merkle tree with radix 4, tokens ordered ascending by token hash
    MerkleTreeRadix4,
}
```

#### Event Builders (Using rust-nostr EventBuilder)
```rust
impl ChallengeContent {
    /// Create Challenge event using rust-nostr's EventBuilder
    pub fn to_event(&self, keys: &Keys) -> Result<Event, NostrError> {
        EventBuilder::new(CHALLENGE_KIND, serde_json::to_string(self)?, &[])
            .to_event(keys)
    }
}

impl ChallengeAcceptContent {
    pub fn to_event(&self, keys: &Keys) -> Result<Event, NostrError> {
        EventBuilder::new(CHALLENGE_ACCEPT_KIND, serde_json::to_string(self)?, &[])
            .to_event(keys)
    }
}

impl MoveContent {
    pub fn to_event(&self, keys: &Keys) -> Result<Event, NostrError> {
        EventBuilder::new(MOVE_KIND, serde_json::to_string(self)?, &[])
            .to_event(keys)
    }
}

impl FinalContent {
    pub fn to_event(&self, keys: &Keys) -> Result<Event, NostrError> {
        EventBuilder::new(FINAL_KIND, serde_json::to_string(self)?, &[])
            .to_event(keys)
    }
}

impl RewardContent {
    pub fn to_event(&self, keys: &Keys) -> Result<Event, NostrError> {
        EventBuilder::new(REWARD_KIND, serde_json::to_string(self)?, &[])
            .to_event(keys)
    }
}
```

### 2. Game Traits

#### Core Game Trait
```rust
pub trait Game: Send + Sync {
    type GamePiece: Clone + Debug;
    type GameState: Clone + Debug;
    type MoveData: Serialize + DeserializeOwned;
    
    /// Decode C value (32 bytes) into game pieces
    fn decode_c_value(&self, c_value: &[u8; 32]) -> Result<Vec<Self::GamePiece>, GameError>;
    
    /// Validate a sequence of moves
    fn validate_sequence(&self, events: &[NostrEvent]) -> Result<ValidationResult, GameError>;
    
    /// Determine if game sequence is complete
    fn is_sequence_complete(&self, events: &[NostrEvent]) -> Result<bool, GameError>;
    
    /// Determine winner from completed sequence
    fn determine_winner(&self, events: &[NostrEvent]) -> Result<Option<PublicKey>, GameError>;
    
    /// Get required Final event count (1 or 2 players)
    fn required_final_events(&self) -> usize;
}
```

#### Commitment Validation Trait
```rust
pub trait CommitmentValidator {
    /// Validate single token commitment
    fn validate_single_commitment(
        &self, 
        commitment_hash: &str, 
        revealed_token: &CashuToken
    ) -> Result<bool, ValidationError>;
    
    /// Validate multi-token commitment using provided method
    fn validate_multi_commitment(
        &self,
        commitment_hash: &str,
        revealed_tokens: &[CashuToken],
        method: &CommitmentMethod
    ) -> Result<bool, ValidationError>;
}
```

### 3. Cashu Integration (Reusing CDK)

#### Token Wrapper (Minimal Extension)
```rust
// Import CDK types - exact paths to be determined from CDK documentation
use cdk::nuts::{Token, Proof, CurrencyUnit};
use cdk::wallet::Wallet;
use cdk::mint::Mint;

#[derive(Debug, Clone)]
pub enum GameTokenType {
    Game,
    Reward { p2pk_locked: Option<PublicKey> }, // Uses NUT-11 P2PK locking
}

/// Thin wrapper around CDK's Token to add game context
#[derive(Debug, Clone)]
pub struct GameToken {
    pub inner: Token, // Reuse CDK's Token directly
    pub game_type: GameTokenType,
}

impl GameToken {
    /// Create from existing CDK token
    pub fn from_cdk_token(token: Token, game_type: GameTokenType) -> Self {
        Self {
            inner: token,
            game_type,
        }
    }
    
    /// Get underlying CDK token for operations
    pub fn as_cdk_token(&self) -> &Token {
        &self.inner
    }
    
    /// Extract C values from token proofs for game piece generation
    pub fn extract_c_values(&self) -> Vec<[u8; 32]> {
        self.inner.proofs().iter()
            .map(|proof| proof.c) // C value from proof
            .collect()
    }
    
    /// Check if this is a P2PK locked reward token
    pub fn is_p2pk_locked(&self) -> bool {
        matches!(self.game_type, GameTokenType::Reward { p2pk_locked: Some(_) })
    }
}
```

#### Mint Wrapper (Extends CDK Mint)
```rust
use cdk::mint::Mint;
use cdk::nuts::{MintRequest, MeltRequest, SwapRequest};

pub struct GameMint {
    inner: Mint, // Reuse CDK's Mint implementation
    nostr_client: NostrClient,
}

impl GameMint {
    /// Wrap existing CDK mint
    pub fn new(mint: Mint, nostr_client: NostrClient) -> Self {
        Self {
            inner: mint,
            nostr_client,
        }
    }
    
    /// Mint Game tokens using CDK's existing mint operation
    pub async fn mint_game_tokens(&self, amount: u64) -> Result<Vec<GameToken>, GameError> {
        // Use CDK's mint request flow
        let mint_request = MintRequest::new(amount)?;
        let tokens = self.inner.process_mint_request(mint_request).await?;
        
        Ok(tokens.into_iter()
            .map(|t| GameToken::from_cdk_token(t, GameTokenType::Game))
            .collect())
    }
    
    /// Mint P2PK locked Reward tokens for game winner
    pub async fn mint_reward_tokens(
        &self, 
        amount: u64, 
        winner_pubkey: PublicKey
    ) -> Result<Vec<GameToken>, GameError> {
        // Create P2PK locked tokens using NUT-11
        let p2pk_request = MintRequest::new_p2pk(amount, winner_pubkey)?;
        let tokens = self.inner.process_mint_request(p2pk_request).await?;
        
        Ok(tokens.into_iter()
            .map(|t| GameToken::from_cdk_token(
                t, 
                GameTokenType::Reward { p2pk_locked: Some(winner_pubkey) }
            ))
            .collect())
    }
    
    /// Validate tokens using CDK's existing validation
    pub async fn validate_tokens(&self, tokens: &[Token]) -> Result<bool, GameError> {
        for token in tokens {
            if !self.inner.verify_token(token).await? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    
    /// Process swap request using CDK
    pub async fn swap_tokens(&self, swap_request: SwapRequest) -> Result<Vec<Token>, GameError> {
        self.inner.process_swap_request(swap_request).await
            .map_err(GameError::from)
    }
    
    /// Process melt request using CDK
    pub async fn melt_tokens(&self, melt_request: MeltRequest) -> Result<u64, GameError> {
        self.inner.process_melt_request(melt_request).await
            .map_err(GameError::from)
    }
    
    /// Publish game result and reward to nostr
    pub async fn publish_game_result(
        &self,
        game_sequence: &[Event],
        winner: PublicKey,
        reward_tokens: Vec<GameToken>
    ) -> Result<EventId, GameError> {
        let reward_content = RewardContent {
            game_sequence_root: game_sequence[0].id,
            winner_pubkey: winner,
            reward_tokens,
            unlock_instructions: Some("Use NUT-11 P2PK to spend these tokens".to_string()),
        };
        
        let event = reward_content.to_event(&self.nostr_client.keys()).await?;
        self.nostr_client.send_event(event).await
            .map_err(GameError::from)
    }
}
```

### 4. Client Interfaces (Reusing Existing Clients)

#### Player Client (Wraps rust-nostr and CDK clients)
```rust
use cdk::wallet::Wallet;
use nostr_sdk::{Client as NostrClient, Keys, Event, EventId};

pub struct PlayerClient {
    nostr_client: NostrClient,
    cashu_wallet: Wallet, // CDK Wallet
    keys: Keys, // Nostr keys for signing
}

impl PlayerClient {
    /// Create new player client
    pub fn new(nostr_client: NostrClient, cashu_wallet: Wallet, keys: Keys) -> Self {
        Self {
            nostr_client,
            cashu_wallet,
            keys,
        }
    }
    
    /// Create and publish challenge
    pub async fn create_challenge<G: Game>(
        &self,
        game: &G,
        tokens: &[GameToken],
        expiry_seconds: Option<u64> // Optional expiry, defaults to 1 hour
    ) -> Result<EventId, GameError> {
        // Create hash commitments for tokens
        let commitment_hashes = self.create_commitments(tokens)?;
        
        // Default to 1 hour expiry if not specified
        let expiry = expiry_seconds
            .unwrap_or(3600) // 1 hour default
            .checked_add(chrono::Utc::now().timestamp() as u64)
            .ok_or(GameError::InvalidExpiry)?;
        
        let challenge_content = ChallengeContent {
            game_type: G::game_type(),
            commitment_hashes,
            game_parameters: game.get_parameters()?,
            expiry: Some(expiry),
        };
        
        let event = challenge_content.to_event(&self.keys)?;
        self.nostr_client.send_event(event).await
            .map_err(GameError::from)
    }
    
    /// Create challenge with default 1-hour expiry (convenience method)
    pub async fn create_challenge_default<G: Game>(
        &self,
        game: &G,
        tokens: &[GameToken]
    ) -> Result<EventId, GameError> {
        self.create_challenge(game, tokens, None).await
    }
    
    /// Accept existing challenge
    pub async fn accept_challenge<G: Game>(
        &self,
        challenge_id: EventId,
        game: &G,
        tokens: &[GameToken]
    ) -> Result<EventId, GameError> {
        let commitment_hashes = self.create_commitments(tokens)?;
        
        let accept_content = ChallengeAcceptContent {
            challenge_id,
            commitment_hashes,
        };
        
        let event = accept_content.to_event(&self.keys)?;
        self.nostr_client.send_event(event).await
            .map_err(GameError::from)
    }
    
    /// Make a move (commit, reveal, or regular move)
    pub async fn make_move<G: Game>(
        &self,
        previous_event: EventId,
        move_type: MoveType,
        move_data: G::MoveData,
        revealed_tokens: Option<Vec<GameToken>>
    ) -> Result<EventId, GameError> {
        let move_content = MoveContent {
            previous_event_id: previous_event,
            move_type,
            move_data: serde_json::to_value(move_data)?,
            revealed_tokens: revealed_tokens.map(|tokens| 
                tokens.into_iter().map(|gt| gt.inner).collect()
            ),
        };
        
        let event = move_content.to_event(&self.keys)?;
        self.nostr_client.send_event(event).await
            .map_err(GameError::from)
    }
    
    /// Publish final event
    pub async fn finalize_game(
        &self,
        game_root: EventId,
        commitment_method: Option<CommitmentMethod>,
        final_state: serde_json::Value
    ) -> Result<EventId, GameError> {
        let final_content = FinalContent {
            game_sequence_root: game_root,
            commitment_method,
            final_state,
        };
        
        let event = final_content.to_event(&self.keys)?;
        self.nostr_client.send_event(event).await
            .map_err(GameError::from)
    }
    
    /// Create hash commitments for tokens
    fn create_commitments(&self, tokens: &[GameToken]) -> Result<Vec<String>, GameError> {
        if tokens.len() == 1 {
            Ok(vec![TokenCommitment::single(&tokens[0].inner).commitment_hash])
        } else {
            // Use merkle tree for multiple tokens
            let commitment = TokenCommitment::multiple(
                &tokens.iter().map(|gt| gt.inner.clone()).collect::<Vec<_>>(),
                CommitmentMethod::MerkleTreeRadix4
            );
            Ok(vec![commitment.commitment_hash])
        }
    }
}
```

## Data Models

### Hash Commitment Structure

#### Standardized Commitment Construction
All hash commitments follow standardized construction rules to ensure consistency:

1. **Token Ordering**: Input tokens MUST be sorted in ascending order by their hash value
2. **Single Token**: `commitment_hash = SHA256(token_hash)`
3. **Multiple Tokens - Concatenation**: `commitment_hash = SHA256(token1_hash || token2_hash || ... || tokenN_hash)`
4. **Multiple Tokens - Merkle Tree Radix 4**: 
   - Build merkle tree with radix 4 (each node has up to 4 children)
   - Leaf nodes are individual token hashes
   - Internal nodes are `SHA256(child1 || child2 || child3 || child4)`
   - Commitment hash is the merkle root

```rust
#[derive(Debug, Clone)]
pub struct TokenCommitment {
    pub commitment_hash: String,
    pub commitment_type: CommitmentType,
}

#[derive(Debug, Clone)]
pub enum CommitmentType {
    Single,
    Multiple { method: CommitmentMethod },
}

impl TokenCommitment {
    /// Create commitment for single token
    pub fn single(token: &CashuToken) -> Self {
        let token_hash = Self::hash_token(token);
        let commitment_hash = sha256(&token_hash).to_hex();
        Self {
            commitment_hash,
            commitment_type: CommitmentType::Single,
        }
    }
    
    /// Create commitment for multiple tokens using specified method
    pub fn multiple(tokens: &[CashuToken], method: CommitmentMethod) -> Self {
        let mut sorted_tokens = tokens.to_vec();
        sorted_tokens.sort_by_key(|t| Self::hash_token(t));
        
        let commitment_hash = match method {
            CommitmentMethod::Concatenation => Self::concatenation_commitment(&sorted_tokens),
            CommitmentMethod::MerkleTreeRadix4 => Self::merkle_tree_radix4_commitment(&sorted_tokens),
        };
        
        Self {
            commitment_hash,
            commitment_type: CommitmentType::Multiple { method },
        }
    }
    
    /// Verify commitment against revealed tokens
    pub fn verify(&self, tokens: &[CashuToken]) -> Result<bool, ValidationError>;
    
    /// Standard token hashing function
    fn hash_token(token: &CashuToken) -> [u8; 32];
    
    /// Concatenation commitment construction
    fn concatenation_commitment(sorted_tokens: &[CashuToken]) -> String;
    
    /// Merkle tree radix 4 commitment construction
    fn merkle_tree_radix4_commitment(sorted_tokens: &[CashuToken]) -> String;
}
```

### Game Sequence Tracking

#### State Transition Flow
```
WaitingForAccept → InProgress → WaitingForFinal → Complete
                      ↓              ↓              ↓
                  Forfeited ←── Forfeited ←── Forfeited
```

**State Descriptions:**
- **WaitingForAccept**: Challenge published, waiting for another player to accept
- **InProgress**: Both players have committed tokens, actively making moves
- **WaitingForFinal**: Game moves are complete, waiting for Final events to validate
- **Complete**: All Final events received, winner determined, rewards distributed
- **Forfeited**: A player violated rules or timed out, other player wins by forfeit

```rust
#[derive(Debug, Clone)]
pub struct GameSequence {
    pub challenge_id: EventId,
    pub players: [PublicKey; 2],
    pub events: Vec<Event>,
    pub state: SequenceState,
    pub created_at: u64,
    pub last_activity: u64,
}

#[derive(Debug, Clone)]
pub enum SequenceState {
    /// Challenge published, waiting for ChallengeAccept
    WaitingForAccept,
    /// Both players committed, game moves are happening
    InProgress,
    /// All moves complete, waiting for Final events from players
    WaitingForFinal,
    /// All Final events received, game validated and complete
    Complete { winner: Option<PublicKey> },
    /// Player forfeited (timeout, invalid move, etc.)
    Forfeited { winner: PublicKey },
}

impl SequenceState {
    /// Check if the game sequence can accept new moves
    pub fn can_accept_moves(&self) -> bool {
        matches!(self, SequenceState::InProgress)
    }
    
    /// Check if the game is waiting for Final events
    pub fn needs_final_events(&self) -> bool {
        matches!(self, SequenceState::WaitingForFinal)
    }
    
    /// Check if the game is complete (finished or forfeited)
    pub fn is_finished(&self) -> bool {
        matches!(self, SequenceState::Complete { .. } | SequenceState::Forfeited { .. })
    }
}
```

## Error Handling

### Error Types
```rust
#[derive(Debug, thiserror::Error)]
pub enum GameProtocolError {
    #[error("Nostr error: {0}")]
    Nostr(#[from] nostr::Error),
    
    #[error("Cashu error: {0}")]
    Cashu(#[from] cashu::Error),
    
    #[error("Game validation error: {0}")]
    GameValidation(String),
    
    #[error("Invalid commitment: {0}")]
    InvalidCommitment(String),
    
    #[error("Sequence error: {0}")]
    SequenceError(String),
    
    #[error("Mint error: {0}")]
    MintError(String),
    
    #[error("Invalid expiry time")]
    InvalidExpiry,
}
```

### Validation Results
```rust
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub winner: Option<PublicKey>,
    pub errors: Vec<ValidationError>,
    pub forfeited_player: Option<PublicKey>,
}

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub event_id: EventId,
    pub error_type: ValidationErrorType,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum ValidationErrorType {
    InvalidToken,
    InvalidCommitment,
    InvalidSequence,
    InvalidMove,
    TimeoutViolation,
}
```

## Testing Strategy

### Unit Tests
- **Token Commitment Tests**: Verify single and multi-token hash commitments
- **C Value Decoding Tests**: Test game piece extraction from various C values
- **Event Serialization Tests**: Ensure proper nostr event formatting
- **Game Trait Tests**: Validate game implementation interfaces

### Integration Tests  
- **Full Game Sequence Tests**: End-to-end game play simulation
- **Mint Validation Tests**: Complete mint validation workflow
- **Nostr Event Chain Tests**: Verify event sequencing and references
- **Commitment Verification Tests**: Multi-token commitment validation

### Property-Based Tests
- **C Value Randomness Tests**: Verify C values provide sufficient entropy
- **Commitment Security Tests**: Ensure commitments are cryptographically secure
- **Sequence Integrity Tests**: Verify event chains maintain integrity

### Mock Components
- **Mock Nostr Relay**: For testing event publishing/querying
- **Mock Cashu Mint**: For testing token operations
- **Reference Game Implementation**: Simple game for testing framework

## Dependencies and Code Reuse Strategy

### Primary Dependencies
```toml
[dependencies]
# Cashu integration - use latest stable CDK release
cdk = "0.12.1"

# Nostr integration - reuse existing rust-nostr functionality  
nostr = { version = "0.35" }
nostr-sdk = { version = "0.35" }

# Cryptography for commitments
sha2 = "0.10"
hex = "0.4"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Async runtime
tokio = { version = "1.0", features = ["full"] }

# Error handling
thiserror = "1.0"
anyhow = "1.0"

# Time handling
chrono = { version = "0.4", features = ["serde"] }

# Optional: for submodule development if modifications needed
# cdk = { git = "https://github.com/cashubtc/cdk", branch = "main" }
# nostr = { path = "./deps/nostr" }
```

### Code Reuse from CDK
**Reuse Existing:**
- `cdk::wallet::Wallet` for player token operations
- `cdk::mint::Mint` for mint functionality  
- `cdk::nuts::*` for protocol implementations (Token, Proof, etc.)
- NUT-11 P2PK implementation for token locking
- Existing minting, melting, swapping request/response flows

**Extend Only When Necessary:**
- Add GameToken wrapper to distinguish Game vs Reward tokens
- Add game sequence validation logic
- Add nostr event integration for game coordination
- Add commitment construction utilities

### Code Reuse from rust-nostr
**Reuse Existing:**
- `nostr::Event` for all event handling
- `nostr::Keys` for key management
- `nostr::Client` for relay communication
- `nostr::Filter` for event querying
- Event signing, verification, serialization

**Extend Only When Necessary:**
- Define custom event kinds (9259-9263) for game events
- Add game-specific event content structures
- Add event chain validation logic

### Submodule Strategy
If modifications to cashubtc/cdk or rust-nostr/nostr are needed:
1. Fork repositories to project organization
2. Add as git submodules in `deps/` directory
3. Create feature branches for modifications
4. Use path dependencies in Cargo.toml during development
5. Submit upstream PRs for beneficial changes

## Token Lifecycle Management

### Reward Token States (Using NUT-11 P2PK)
```rust
#[derive(Debug, Clone)]
pub enum RewardTokenState {
    P2PKLocked { to_pubkey: PublicKey }, // NUT-11 Pay-to-Public-Key
    Unlocked, // Standard Cashu tokens
}

impl RewardTokenState {
    /// Check if P2PK token can be spent by specific pubkey
    pub fn can_spend(&self, pubkey: &PublicKey) -> bool;
    
    /// Create P2PK locked token using NUT-11
    pub fn create_p2pk_locked(pubkey: PublicKey) -> Self;
}
```

### Token Operations by State
- **Locked Reward Tokens**: Use NUT-11 (P2PK) to lock tokens to winner's pubkey, can only be spent by that pubkey
- **Unlocked Reward Tokens**: Standard Cashu tokens that can be used by anyone, making them generally useful ecash
- **Game Tokens**: Always unlocked, used only for gameplay commitments and burning

### Unlocking Process (Using NUT-11 P2PK)
1. Winner receives P2PK-locked Reward tokens via Reward event (using NUT-11)
2. Winner can spend P2PK tokens by providing valid signature for their pubkey
3. Winner can swap P2PK tokens for standard unlocked tokens through normal Cashu operations
4. Unlocked tokens function as standard ecash for any purpose

## Commitment Construction Algorithms

### Merkle Tree Radix 4 Algorithm
```rust
/// Build merkle tree with radix 4 from sorted token hashes
fn build_merkle_tree_radix4(token_hashes: &[[u8; 32]]) -> [u8; 32] {
    if token_hashes.is_empty() {
        return [0u8; 32]; // Empty tree root
    }
    
    if token_hashes.len() == 1 {
        return token_hashes[0]; // Single leaf is the root
    }
    
    let mut current_level = token_hashes.to_vec();
    
    while current_level.len() > 1 {
        let mut next_level = Vec::new();
        
        // Process nodes in groups of 4
        for chunk in current_level.chunks(4) {
            let mut node_data = Vec::new();
            
            // Concatenate up to 4 child hashes
            for hash in chunk {
                node_data.extend_from_slice(hash);
            }
            
            // Pad with zeros if less than 4 children
            while node_data.len() < 128 { // 4 * 32 bytes
                node_data.push(0);
            }
            
            // Hash the concatenated children to create parent node
            let parent_hash = sha256(&node_data);
            next_level.push(parent_hash);
        }
        
        current_level = next_level;
    }
    
    current_level[0] // Return merkle root
}
```

### Concatenation Algorithm
```rust
/// Simple concatenation of sorted token hashes
fn build_concatenation_commitment(token_hashes: &[[u8; 32]]) -> [u8; 32] {
    let mut concatenated = Vec::new();
    
    for hash in token_hashes {
        concatenated.extend_from_slice(hash);
    }
    
    sha256(&concatenated)
}
```

## Security Considerations

### Cryptographic Security
- **C Value Entropy**: Ensure C values provide sufficient randomness for game pieces
- **Commitment Security**: Use cryptographically secure hash functions for commitments
- **Token Validation**: Verify all Cashu token proofs during validation

### Game Integrity
- **Sequence Verification**: Validate complete event chains for tampering
- **Timing Attacks**: Implement optional timeout mechanisms for commit/reveal
- **Replay Protection**: Ensure tokens can only be burned once per game

### Privacy Considerations
- **Token Unlinkability**: Preserve Cashu's privacy properties where possible
- **Move Privacy**: Support commit/reveal for strategic information hiding
- **Metadata Leakage**: Minimize game-specific information in nostr events
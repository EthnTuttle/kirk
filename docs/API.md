# Kirk API Documentation

Kirk is a trustless gaming protocol that combines Cashu ecash tokens with Nostr events for cryptographically-secured gameplay.

## Table of Contents

- [Core Types](#core-types)
- [Game Traits](#game-traits)
- [Event Types](#event-types)
- [Client Interfaces](#client-interfaces)
- [Cashu Integration](#cashu-integration)
- [Error Handling](#error-handling)

## Core Types

### GameToken

A wrapper around CDK's Token that adds game-specific context.

```rust
pub struct GameToken {
    pub inner: Token,
    pub game_type: GameTokenType,
}

impl GameToken {
    /// Create from existing CDK token
    pub fn from_cdk_token(token: Token, game_type: GameTokenType) -> Self
    
    /// Get underlying CDK token for operations
    pub fn as_cdk_token(&self) -> &Token
    
    /// Extract C values from token proofs for game piece generation
    pub fn extract_c_values(&self) -> Vec<[u8; 32]>
    
    /// Check if this is a P2PK locked reward token
    pub fn is_p2pk_locked(&self) -> bool
}
```

### GameTokenType

Distinguishes between different token purposes.

```rust
pub enum GameTokenType {
    Game,
    Reward { p2pk_locked: Option<PublicKey> },
}
```

### TokenCommitment

Handles cryptographic commitments for game tokens.

```rust
pub struct TokenCommitment {
    pub commitment_hash: String,
    pub commitment_type: CommitmentType,
}

impl TokenCommitment {
    /// Create commitment for single token
    pub fn single(token: &CashuToken) -> Self
    
    /// Create commitment for multiple tokens using specified method
    pub fn multiple(tokens: &[CashuToken], method: CommitmentMethod) -> Self
    
    /// Verify commitment against revealed tokens
    pub fn verify(&self, tokens: &[CashuToken]) -> Result<bool, ValidationError>
}
```

### CommitmentMethod

Specifies how multi-token commitments are constructed.

```rust
pub enum CommitmentMethod {
    /// Simple concatenation of token hashes in ascending order
    Concatenation,
    /// Merkle tree with radix 4, tokens ordered ascending by token hash
    MerkleTreeRadix4,
}
```

## Game Traits

### Game Trait

Core trait that defines game-specific logic.

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

### CommitmentValidator Trait

Handles commitment verification logic.

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

## Event Types

Kirk uses custom Nostr event kinds for game coordination:

- **9259**: Challenge events
- **9260**: ChallengeAccept events
- **9261**: Move events (Move/Commit/Reveal)
- **9262**: Final events
- **9263**: Reward events

### ChallengeContent

Content structure for Challenge events.

```rust
pub struct ChallengeContent {
    pub game_type: String,
    pub commitment_hashes: Vec<String>,
    pub game_parameters: serde_json::Value,
    pub expiry: Option<u64>,
}

impl ChallengeContent {
    /// Create Challenge event using rust-nostr's EventBuilder
    pub fn to_event(&self, keys: &Keys) -> Result<Event, NostrError>
}
```

### ChallengeAcceptContent

Content structure for ChallengeAccept events.

```rust
pub struct ChallengeAcceptContent {
    pub challenge_id: EventId,
    pub commitment_hashes: Vec<String>,
}

impl ChallengeAcceptContent {
    pub fn to_event(&self, keys: &Keys) -> Result<Event, NostrError>
}
```

### MoveContent

Content structure for Move events.

```rust
pub struct MoveContent {
    pub previous_event_id: EventId,
    pub move_type: MoveType,
    pub move_data: serde_json::Value,
    pub revealed_tokens: Option<Vec<CashuToken>>,
}

pub enum MoveType { Move, Commit, Reveal }

impl MoveContent {
    pub fn to_event(&self, keys: &Keys) -> Result<Event, NostrError>
}
```

### FinalContent

Content structure for Final events.

```rust
pub struct FinalContent {
    pub game_sequence_root: EventId,
    pub commitment_method: Option<CommitmentMethod>,
    pub final_state: serde_json::Value,
}

impl FinalContent {
    pub fn to_event(&self, keys: &Keys) -> Result<Event, NostrError>
}
```

### RewardContent

Content structure for Reward events.

```rust
pub struct RewardContent {
    pub game_sequence_root: EventId,
    pub winner_pubkey: PublicKey,
    pub reward_tokens: Vec<GameToken>,
    pub unlock_instructions: Option<String>,
}

impl RewardContent {
    pub fn to_event(&self, keys: &Keys) -> Result<Event, NostrError>
}
```

## Client Interfaces

### PlayerClient

Main interface for players to participate in games.

```rust
pub struct PlayerClient {
    nostr_client: NostrClient,
    cashu_wallet: Wallet,
    keys: Keys,
}

impl PlayerClient {
    /// Create new player client
    pub fn new(nostr_client: NostrClient, cashu_wallet: Wallet, keys: Keys) -> Self
    
    /// Create and publish challenge with configurable expiry
    pub async fn create_challenge<G: Game>(
        &self,
        game: &G,
        tokens: &[GameToken],
        expiry_seconds: Option<u64>
    ) -> Result<EventId, GameError>
    
    /// Create challenge with default 1-hour expiry (convenience method)
    pub async fn create_challenge_default<G: Game>(
        &self,
        game: &G,
        tokens: &[GameToken]
    ) -> Result<EventId, GameError>
    
    /// Accept existing challenge
    pub async fn accept_challenge<G: Game>(
        &self,
        challenge_id: EventId,
        game: &G,
        tokens: &[GameToken]
    ) -> Result<EventId, GameError>
    
    /// Make a move (commit, reveal, or regular move)
    pub async fn make_move<G: Game>(
        &self,
        previous_event: EventId,
        move_type: MoveType,
        move_data: G::MoveData,
        revealed_tokens: Option<Vec<GameToken>>
    ) -> Result<EventId, GameError>
    
    /// Publish final event
    pub async fn finalize_game(
        &self,
        game_root: EventId,
        commitment_method: Option<CommitmentMethod>,
        final_state: serde_json::Value
    ) -> Result<EventId, GameError>
}
```

### ValidationClient

Interface for third-party game sequence verification.

```rust
pub struct ValidationClient {
    nostr_client: NostrClient,
}

impl ValidationClient {
    /// Create new validation client
    pub fn new(nostr_client: NostrClient) -> Self
    
    /// Collect all events for a game sequence
    pub async fn collect_game_events(&self, challenge_id: EventId) -> Result<Vec<Event>, GameError>
    
    /// Validate complete game sequence
    pub async fn validate_game_sequence<G: Game>(
        &self,
        game: &G,
        events: &[Event]
    ) -> Result<ValidationResult, GameError>
    
    /// Verify commitments against revealed tokens
    pub async fn verify_commitments(
        &self,
        events: &[Event]
    ) -> Result<bool, ValidationError>
}
```

## Cashu Integration

### GameMint

Wrapper around CDK Mint with game-specific functionality.

```rust
pub struct GameMint {
    inner: Mint,
    nostr_client: NostrClient,
}

impl GameMint {
    /// Wrap existing CDK mint
    pub fn new(mint: Mint, nostr_client: NostrClient) -> Self
    
    /// Mint Game tokens using CDK's existing mint operation
    pub async fn mint_game_tokens(&self, amount: u64) -> Result<Vec<GameToken>, GameError>
    
    /// Mint P2PK locked Reward tokens for game winner
    pub async fn mint_reward_tokens(
        &self, 
        amount: u64, 
        winner_pubkey: PublicKey
    ) -> Result<Vec<GameToken>, GameError>
    
    /// Validate tokens using CDK's existing validation
    pub async fn validate_tokens(&self, tokens: &[Token]) -> Result<bool, GameError>
    
    /// Process swap request using CDK
    pub async fn swap_tokens(&self, swap_request: SwapRequest) -> Result<Vec<Token>, GameError>
    
    /// Process melt request using CDK
    pub async fn melt_tokens(&self, melt_request: MeltRequest) -> Result<u64, GameError>
    
    /// Publish game result and reward to nostr
    pub async fn publish_game_result(
        &self,
        game_sequence: &[Event],
        winner: PublicKey,
        reward_tokens: Vec<GameToken>
    ) -> Result<EventId, GameError>
}
```

### SequenceProcessor

Handles game sequence validation and reward distribution for mints.

```rust
pub struct SequenceProcessor<G: Game> {
    game: G,
    mint: GameMint,
}

impl<G: Game> SequenceProcessor<G> {
    /// Create new sequence processor
    pub fn new(game: G, mint: GameMint) -> Self
    
    /// Process complete game sequence
    pub async fn process_sequence(&self, events: &[Event]) -> Result<ProcessingResult, GameError>
    
    /// Validate sequence and determine winner
    pub async fn validate_and_determine_winner(&self, events: &[Event]) -> Result<Option<PublicKey>, GameError>
    
    /// Distribute rewards to winner
    pub async fn distribute_rewards(
        &self,
        winner: PublicKey,
        burned_tokens: &[Token]
    ) -> Result<Vec<GameToken>, GameError>
}
```

## Error Handling

### GameProtocolError

Main error type for the library.

```rust
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

### ValidationResult

Result of game sequence validation.

```rust
pub struct ValidationResult {
    pub is_valid: bool,
    pub winner: Option<PublicKey>,
    pub errors: Vec<ValidationError>,
    pub forfeited_player: Option<PublicKey>,
}
```

### ValidationError

Specific validation error information.

```rust
pub struct ValidationError {
    pub event_id: EventId,
    pub error_type: ValidationErrorType,
    pub message: String,
}

pub enum ValidationErrorType {
    InvalidToken,
    InvalidCommitment,
    InvalidSequence,
    InvalidMove,
    TimeoutViolation,
}
```
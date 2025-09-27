# Design Document

## Overview

The CLI Card Game is a demonstration application that showcases the Kirk gaming protocol through a simple two-player card game. The application runs as a single process with multiple independent threads: an embedded Cashu mint, an embedded Nostr relay, a game validator, and a REPL interface for player interaction. This design provides a complete, self-contained gaming environment that demonstrates trustless gameplay without requiring external infrastructure.

The card game itself is simple: each player commits to a Game token, the C value from their token determines their card (Ace through King with suits), and the highest card wins. This simplicity allows focus on the protocol mechanics rather than complex game rules.

## Architecture

### High-Level System Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        CLI Card Game Process                     │
├─────────────────┬─────────────────────────────────┬─────────────┤
│   REPL Thread   │         Bevy ECS World          │  Embedded   │
│                 │                                 │  Relay      │
│                 │  ┌─────────────┬─────────────┐  │  Thread     │
│ - User Commands │  │  Validator  │ Embedded    │  │             │
│ - Game Display  │  │  Systems    │ Mint        │  │ - Event     │
│ - Status Info   │  │             │ Systems     │  │   Storage   │
│                 │  │ - Event Mon │ - CDK Mint  │  │ - Subs      │
│                 │  │ - Game Logic│ - Token Ops │  │             │
│                 │  │ - Validation│ - Rewards   │  │             │
│                 │  └─────────────┴─────────────┘  │             │
│                 │                                 │             │
│                 │    Shared ECS Components:       │             │
│                 │    - GameTokens, Players,       │             │
│                 │    - ActiveGames, Challenges    │             │
└─────────────────┴─────────────────────────────────┴─────────────┘
         │                         │                         │
         │        Commands         │                         │
         │ ──────────────────────► │                         │
         │                         │                         │
         │        Status           │      Nostr Events       │
         │ ◄────────────────────── │ ◄─────────────────────► │
         │                         │                         │
         └─────────────────────────┴─────────────────────────┘
```

### Bevy ECS Integration Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Bevy ECS World                           │
├─────────────────────────────────────────────────────────────┤
│  Components (Data):                                         │
│  - Player, GameToken, PlayingCard, ActiveGame              │
│  - Challenge, GameSequence, RewardToken                    │
│                                                             │
│  Resources (Global State):                                 │
│  - EmbeddedMint, NostrClient, GameConfig                   │
│  - ReplCommandQueue, GameEventQueue                        │
│                                                             │
│  Systems (Logic):                                          │
│  - process_repl_commands                                   │
│  - handle_nostr_events                                     │
│  - validate_game_sequences                                 │
│  - mint_game_tokens                                        │
│  - distribute_rewards                                      │
│  - update_game_states                                      │
└─────────────────────────────────────────────────────────────┘
```

## Components and Interfaces

### 1. REPL Interface Thread

The REPL provides an interactive command-line interface for players to control the game.

#### Core Commands

```rust
pub enum ReplCommand {
    // Game Operations
    Challenge { amount: u64 },           // Create new challenge
    Accept { challenge_id: String },     // Accept existing challenge
    List,                                // List available challenges
    Status,                              // Show current game status
    
    // Token Management
    Balance,                             // Show token balances
    Mint { amount: u64 },               // Mint new Game tokens
    Unlock { token_id: String },        // Unlock Reward tokens
    
    // System Operations
    Help,                               // Show available commands
    Config { key: String, value: String }, // Set configuration
    Quit,                               // Exit application
}
```

#### REPL Implementation

```rust
pub struct ReplInterface {
    command_sender: mpsc::UnboundedSender<ReplCommand>,
    status_receiver: mpsc::UnboundedReceiver<GameStatus>,
    config: ReplConfig,
}

impl ReplInterface {
    pub async fn run(&mut self) -> Result<(), GameProtocolError> {
        loop {
            // Display prompt and current status
            self.display_prompt().await?;
            
            // Read user input
            let input = self.read_input().await?;
            
            // Parse command
            let command = self.parse_command(&input)?;
            
            // Send command to validator
            self.command_sender.send(command)?;
            
            // Display results
            self.display_response().await?;
        }
    }
    
    fn display_prompt(&self) -> Result<(), GameProtocolError> {
        println!("kirk-cards> ");
        Ok(())
    }
    
    fn parse_command(&self, input: &str) -> Result<ReplCommand, GameProtocolError> {
        // Parse user input into structured commands
        match input.trim().split_whitespace().collect::<Vec<_>>().as_slice() {
            ["challenge", amount] => Ok(ReplCommand::Challenge { 
                amount: amount.parse()? 
            }),
            ["accept", id] => Ok(ReplCommand::Accept { 
                challenge_id: id.to_string() 
            }),
            ["list"] => Ok(ReplCommand::List),
            ["status"] => Ok(ReplCommand::Status),
            ["balance"] => Ok(ReplCommand::Balance),
            ["mint", amount] => Ok(ReplCommand::Mint { 
                amount: amount.parse()? 
            }),
            ["help"] => Ok(ReplCommand::Help),
            ["quit"] => Ok(ReplCommand::Quit),
            _ => Err(GameProtocolError::InvalidMove(
                "Unknown command. Type 'help' for available commands.".to_string()
            )),
        }
    }
}
```

### 2. Bevy ECS Components and Resources

The mint and validator share state through Bevy ECS components and resources, eliminating the need for channel communication.

#### ECS Components (Data)
```rust
use bevy::prelude::*;

#[derive(Component)]
pub struct Player {
    pub pubkey: PublicKey,
    pub nostr_keys: Keys,
    pub balance_game_tokens: u64,
    pub balance_reward_tokens: u64,
}

#[derive(Component)]
pub struct GameToken {
    pub inner: CashuToken,
    pub token_type: GameTokenType,
    pub owner: Entity, // Reference to Player entity
}

#[derive(Component)]
pub struct PlayingCard {
    pub suit: Suit,
    pub rank: Rank,
    pub derived_from_token: Entity, // Reference to GameToken entity
}

#[derive(Component)]
pub struct Challenge {
    pub challenge_id: EventId,
    pub challenger: Entity, // Reference to Player entity
    pub amount: u64,
    pub commitment_hashes: Vec<String>,
    pub expiry: u64,
    pub status: ChallengeStatus,
}

#[derive(Component)]
pub struct ActiveGame {
    pub challenge_id: EventId,
    pub players: [Entity; 2], // References to Player entities
    pub phase: GamePhase,
    pub committed_tokens: HashMap<Entity, Vec<Entity>>, // Player -> GameTokens
    pub revealed_cards: HashMap<Entity, Entity>, // Player -> PlayingCard
    pub created_at: u64,
    pub last_activity: u64,
}

#[derive(Component)]
pub struct GameSequence {
    pub root_event: EventId,
    pub events: Vec<Event>,
    pub validation_status: ValidationStatus,
}

#[derive(Component)]
pub struct RewardToken {
    pub inner: GameToken,
    pub locked_to: Entity, // Reference to Player entity (P2PK)
    pub issued_for_game: EventId,
}

#[derive(Component)]
pub struct PendingReward {
    pub winner: Entity, // Reference to Player entity
    pub amount: u64,
    pub game_sequence: Entity, // Reference to GameSequence entity
}
```

#### ECS Resources (Global State)
```rust
#[derive(Resource)]
pub struct MasterKeyManager {
    pub master_seed: [u8; 64],
    pub nostr_keys: Keys,
    pub mint_keys: Keys,
}

impl MasterKeyManager {
    pub fn new() -> Self {
        // Generate master seed
        let master_seed = Self::generate_master_seed();
        
        // Derive Nostr keys from master seed
        let nostr_keys = Self::derive_nostr_keys(&master_seed);
        
        // Derive mint keys from master seed  
        let mint_keys = Self::derive_mint_keys(&master_seed);
        
        Self {
            master_seed,
            nostr_keys,
            mint_keys,
        }
    }
    
    pub fn from_seed(master_seed: [u8; 64]) -> Self {
        let nostr_keys = Self::derive_nostr_keys(&master_seed);
        let mint_keys = Self::derive_mint_keys(&master_seed);
        
        Self {
            master_seed,
            nostr_keys,
            mint_keys,
        }
    }
    
    fn generate_master_seed() -> [u8; 64] {
        use rand::RngCore;
        let mut seed = [0u8; 64];
        rand::thread_rng().fill_bytes(&mut seed);
        seed
    }
    
    fn derive_nostr_keys(master_seed: &[u8; 64]) -> Keys {
        // Use HKDF to derive Nostr keys from master seed
        use hkdf::Hkdf;
        use sha2::Sha256;
        
        let hk = Hkdf::<Sha256>::new(None, master_seed);
        let mut nostr_seed = [0u8; 32];
        hk.expand(b"nostr-client-key", &mut nostr_seed).unwrap();
        
        Keys::from_sk_str(&hex::encode(nostr_seed)).unwrap()
    }
    
    fn derive_mint_keys(master_seed: &[u8; 64]) -> Keys {
        // Use HKDF to derive mint keys from master seed
        use hkdf::Hkdf;
        use sha2::Sha256;
        
        let hk = Hkdf::<Sha256>::new(None, master_seed);
        let mut mint_seed = [0u8; 32];
        hk.expand(b"embedded-mint-key", &mut mint_seed).unwrap();
        
        Keys::from_sk_str(&hex::encode(mint_seed)).unwrap()
    }
    
    pub fn get_player_keys(&self) -> &Keys {
        &self.nostr_keys
    }
    
    pub fn get_mint_keys(&self) -> &Keys {
        &self.mint_keys
    }
}

#[derive(Resource)]
pub struct EmbeddedMint {
    pub inner: GameMint,
}

#[derive(Resource)]
pub struct NostrClient {
    pub client: nostr_sdk::Client,
}

#[derive(Resource)]
pub struct GameConfig {
    pub mint_port: u16,
    pub relay_port: u16,
    pub default_wager: u64,
    pub game_timeout_seconds: u64,
    pub master_seed_file: Option<String>, // Optional file to persist master seed
}

#[derive(Resource)]
pub struct ReplCommandQueue {
    pub commands: VecDeque<ReplCommand>,
}

#[derive(Resource)]
pub struct GameEventQueue {
    pub events: VecDeque<Event>,
}

#[derive(Resource)]
pub struct GameStatusDisplay {
    pub current_status: String,
    pub active_games_count: usize,
    pub pending_challenges: usize,
    pub player_pubkey: String,
    pub mint_pubkey: String,
}
```

#### Card Game Implementation

```rs
pub struct CardGame;

impl Game for CardGame {
    type GamePiece = PlayingCard;
    type GameState = CardGameState;
    type MoveData = CardMove;
    
    fn decode_c_value(&self, c_value: &[u8; 32]) -> Result<Vec<Self::GamePiece>, GameProtocolError> {
        // Convert C value to playing card
        let card_value = c_value_to_range(c_value, 52); // 52 cards in deck
        let suit = Suit::from_u8((card_value / 13) as u8)?;
        let rank = Rank::from_u8((card_value % 13) as u8)?;
        
        Ok(vec![PlayingCard { suit, rank }])
    }
    
    fn validate_sequence(&self, events: &[NostrEvent]) -> Result<ValidationResult, GameProtocolError> {
        // Validate card game sequence
        let mut validator = CardGameValidator::new();
        validator.validate_events(events)
    }
    
    fn is_sequence_complete(&self, events: &[NostrEvent]) -> Result<bool, GameProtocolError> {
        // Check if both players have revealed their cards
        let move_events: Vec<_> = events.iter()
            .filter(|e| e.kind == MOVE_KIND)
            .collect();
            
        // Need exactly 2 move events (one from each player)
        Ok(move_events.len() >= 2)
    }
    
    fn determine_winner(&self, events: &[NostrEvent]) -> Result<Option<PublicKey>, GameProtocolError> {
        // Compare revealed cards to determine winner
        let cards = self.extract_revealed_cards(events)?;
        
        if cards.len() != 2 {
            return Ok(None); // Game not complete
        }
        
        let (player1, card1) = &cards[0];
        let (player2, card2) = &cards[1];
        
        match card1.cmp(card2) {
            std::cmp::Ordering::Greater => Ok(Some(*player1)),
            std::cmp::Ordering::Less => Ok(Some(*player2)),
            std::cmp::Ordering::Equal => {
                // Tie - could implement tie-breaking rules or declare draw
                Ok(None)
            }
        }
    }
    
    fn required_final_events(&self) -> usize {
        2 // Both players must publish Final events
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PlayingCard {
    pub suit: Suit,
    pub rank: Rank,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Suit {
    Clubs,    // Lowest
    Diamonds,
    Hearts,
    Spades,   // Highest
}

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
```

### 3. Bevy ECS Systems (Logic)

Systems handle all game logic, operating on shared ECS components and resources.

#### Core Game Systems
```rust
use bevy::prelude::*;

// System to process REPL commands
pub fn process_repl_commands(
    mut commands: Commands,
    mut repl_queue: ResMut<ReplCommandQueue>,
    mut players: Query<&mut Player>,
    mut challenges: Query<&mut Challenge>,
    mut games: Query<&mut ActiveGame>,
    mint: Res<EmbeddedMint>,
    nostr_client: Res<NostrClient>,
) {
    while let Some(command) = repl_queue.commands.pop_front() {
        match command {
            ReplCommand::Challenge { amount } => {
                // Create new challenge entity
                let challenge_entity = commands.spawn(Challenge {
                    challenge_id: EventId::new(),
                    challenger: /* current player entity */,
                    amount,
                    commitment_hashes: vec![], // Will be filled by mint system
                    expiry: chrono::Utc::now().timestamp() as u64 + 3600,
                    status: ChallengeStatus::WaitingForTokens,
                }).id();
                
                // Trigger token minting
                commands.spawn(MintTokenRequest {
                    amount,
                    for_challenge: challenge_entity,
                });
            }
            ReplCommand::Accept { challenge_id } => {
                // Find and update challenge
                for mut challenge in challenges.iter_mut() {
                    if challenge.challenge_id.to_string() == challenge_id {
                        challenge.status = ChallengeStatus::Accepted;
                        // Create ActiveGame entity
                        commands.spawn(ActiveGame {
                            challenge_id: challenge.challenge_id,
                            players: [challenge.challenger, /* accepter entity */],
                            phase: GamePhase::TokenCommitment,
                            committed_tokens: HashMap::new(),
                            revealed_cards: HashMap::new(),
                            created_at: chrono::Utc::now().timestamp() as u64,
                            last_activity: chrono::Utc::now().timestamp() as u64,
                        });
                        break;
                    }
                }
            }
            // ... handle other commands
        }
    }
}

// System to handle Nostr events
pub fn handle_nostr_events(
    mut commands: Commands,
    mut event_queue: ResMut<GameEventQueue>,
    mut games: Query<&mut ActiveGame>,
    mut sequences: Query<&mut GameSequence>,
    players: Query<&Player>,
    tokens: Query<&GameToken>,
) {
    while let Some(event) = event_queue.events.pop_front() {
        match event.kind {
            CHALLENGE_KIND => {
                let challenge_content: ChallengeContent = serde_json::from_str(&event.content).unwrap();
                // Create Challenge component
                commands.spawn(Challenge {
                    challenge_id: event.id,
                    challenger: /* find player by pubkey */,
                    amount: 0, // Extract from content
                    commitment_hashes: challenge_content.commitment_hashes,
                    expiry: challenge_content.expiry.unwrap_or(0),
                    status: ChallengeStatus::WaitingForAccept,
                });
            }
            MOVE_KIND => {
                let move_content: MoveContent = serde_json::from_str(&event.content).unwrap();
                // Update game state based on move
                for mut game in games.iter_mut() {
                    if /* game matches event */ {
                        match move_content.move_type {
                            MoveType::Reveal => {
                                // Process revealed tokens and create PlayingCard components
                                if let Some(revealed_tokens) = move_content.revealed_tokens {
                                    for token in revealed_tokens {
                                        let card = derive_card_from_token(&token);
                                        commands.spawn(PlayingCard {
                                            suit: card.suit,
                                            rank: card.rank,
                                            derived_from_token: /* token entity */,
                                        });
                                    }
                                }
                            }
                            // ... handle other move types
                        }
                    }
                }
            }
            // ... handle other event types
        }
    }
}

// System to mint game tokens
pub fn mint_game_tokens(
    mut commands: Commands,
    mut mint_requests: Query<&MintTokenRequest>,
    mut challenges: Query<&mut Challenge>,
    mint: Res<EmbeddedMint>,
) {
    for request in mint_requests.iter() {
        // Use CDK mint to create tokens
        let tokens = mint.inner.mint_game_tokens(request.amount).await.unwrap();
        
        // Create GameToken components
        for token in tokens {
            let token_entity = commands.spawn(GameToken {
                inner: token.inner,
                token_type: token.game_type,
                owner: /* player entity */,
            }).id();
            
            // Update challenge with commitment hash
            if let Ok(mut challenge) = challenges.get_mut(request.for_challenge) {
                let commitment = TokenCommitment::single(&token.inner);
                challenge.commitment_hashes.push(commitment.commitment_hash);
                challenge.status = ChallengeStatus::Ready;
            }
        }
        
        // Remove the request
        commands.entity(request.entity).despawn();
    }
}

// System to validate game sequences and distribute rewards
pub fn validate_and_reward_games(
    mut commands: Commands,
    mut games: Query<&mut ActiveGame>,
    sequences: Query<&GameSequence>,
    players: Query<&Player>,
    cards: Query<&PlayingCard>,
    mint: Res<EmbeddedMint>,
    nostr_client: Res<NostrClient>,
) {
    for mut game in games.iter_mut() {
        if game.phase == GamePhase::WaitingForValidation {
            // Find corresponding game sequence
            if let Some(sequence) = sequences.iter().find(|s| s.root_event == game.challenge_id) {
                // Validate using CardGame trait
                let card_game = CardGame;
                let validation_result = card_game.validate_sequence(&sequence.events).unwrap();
                
                if validation_result.is_valid {
                    if let Some(winner) = validation_result.winner {
                        // Create PendingReward component
                        commands.spawn(PendingReward {
                            winner: /* find player entity by pubkey */,
                            amount: /* calculate from burned tokens */,
                            game_sequence: /* sequence entity */,
                        });
                        
                        game.phase = GamePhase::Complete { winner: Some(winner) };
                    }
                }
            }
        }
    }
}

// System to process pending rewards
pub fn process_pending_rewards(
    mut commands: Commands,
    mut pending_rewards: Query<&PendingReward>,
    mut players: Query<&mut Player>,
    mint: Res<EmbeddedMint>,
    nostr_client: Res<NostrClient>,
) {
    for reward in pending_rewards.iter() {
        // Mint P2PK locked reward tokens
        let reward_tokens = mint.inner.mint_reward_tokens(
            reward.amount, 
            players.get(reward.winner).unwrap().pubkey
        ).await.unwrap();
        
        // Create RewardToken components
        for token in reward_tokens {
            commands.spawn(RewardToken {
                inner: token,
                locked_to: reward.winner,
                issued_for_game: /* game event id */,
            });
        }
        
        // Update player balance
        if let Ok(mut player) = players.get_mut(reward.winner) {
            player.balance_reward_tokens += reward.amount;
        }
        
        // Publish reward event to Nostr
        // ... publish RewardContent event
        
        // Remove pending reward
        commands.entity(reward.entity).despawn();
    }
}

// System to update game status display
pub fn update_status_display(
    mut status: ResMut<GameStatusDisplay>,
    challenges: Query<&Challenge>,
    games: Query<&ActiveGame>,
    key_manager: Res<MasterKeyManager>,
) {
    status.pending_challenges = challenges.iter().count();
    status.active_games_count = games.iter().count();
    status.player_pubkey = key_manager.get_player_keys().public_key().to_string();
    status.mint_pubkey = key_manager.get_mint_keys().public_key().to_string();
    status.current_status = format!(
        "Active Games: {}, Pending Challenges: {} | Player: {}... | Mint: {}...", 
        status.active_games_count, 
        status.pending_challenges,
        &status.player_pubkey[..8],
        &status.mint_pubkey[..8]
    );
}
```

### 4. Bevy ECS Application Setup

The main application sets up the Bevy ECS world and runs the systems.

#### Application Structure
```rust
use bevy::prelude::*;

#[derive(Component)]
pub struct MintTokenRequest {
    pub amount: u64,
    pub for_challenge: Entity,
    pub entity: Entity, // Self-reference for cleanup
}

pub struct GameApp {
    app: App,
}

impl GameApp {
    pub fn new() -> Self {
        let mut app = App::new();
        
        // Add Bevy minimal plugins (no rendering, audio, etc.)
        app.add_plugins(MinimalPlugins);
        
        // Add our resources
        app.insert_resource(GameConfig::default());
        app.insert_resource(ReplCommandQueue { commands: VecDeque::new() });
        app.insert_resource(GameEventQueue { events: VecDeque::new() });
        app.insert_resource(GameStatusDisplay {
            current_status: "Starting...".to_string(),
            active_games_count: 0,
            pending_challenges: 0,
        });
        
        // Add systems to different schedules
        app.add_systems(Update, (
            process_repl_commands,
            handle_nostr_events,
            mint_game_tokens,
            validate_and_reward_games,
            process_pending_rewards,
            update_status_display,
        ).chain()); // Chain ensures proper ordering
        
        // Add startup systems
        app.add_systems(Startup, (
            setup_embedded_mint,
            setup_nostr_client,
            create_default_player,
        ));
        
        Self { app }
    }
    
    pub async fn run(&mut self) -> Result<(), GameProtocolError> {
        // Initialize async resources
        self.setup_async_resources().await?;
        
        // Run the ECS world
        loop {
            self.app.update();
            
            // Small delay to prevent busy waiting
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    
    async fn setup_async_resources(&mut self) -> Result<(), GameProtocolError> {
        // Setup master key manager
        let key_manager = self.setup_key_manager().await?;
        
        // Setup embedded mint with derived keys
        let mint = self.create_embedded_mint(&key_manager).await?;
        self.app.insert_resource(EmbeddedMint { inner: mint });
        
        // Setup Nostr client with derived keys
        let nostr_client = self.create_nostr_client(&key_manager).await?;
        self.app.insert_resource(NostrClient { client: nostr_client });
        
        // Insert key manager as resource
        self.app.insert_resource(key_manager);
        
        Ok(())
    }
    
    async fn setup_key_manager(&self) -> Result<MasterKeyManager, GameProtocolError> {
        // Check if we have a saved master seed
        if let Some(config) = self.app.world.get_resource::<GameConfig>() {
            if let Some(seed_file) = &config.master_seed_file {
                if let Ok(seed_data) = tokio::fs::read(seed_file).await {
                    if seed_data.len() == 64 {
                        let mut seed = [0u8; 64];
                        seed.copy_from_slice(&seed_data);
                        println!("Loaded master seed from {}", seed_file);
                        return Ok(MasterKeyManager::from_seed(seed));
                    }
                }
            }
        }
        
        // Generate new master key manager
        let key_manager = MasterKeyManager::new();
        
        // Save master seed if configured
        if let Some(config) = self.app.world.get_resource::<GameConfig>() {
            if let Some(seed_file) = &config.master_seed_file {
                tokio::fs::write(seed_file, &key_manager.master_seed).await?;
                println!("Saved master seed to {}", seed_file);
            }
        }
        
        Ok(key_manager)
    }
    
    async fn create_embedded_mint(&self, key_manager: &MasterKeyManager) -> Result<GameMint, GameProtocolError> {
        // Create CDK mint instance using derived mint keys
        let mint_keys = key_manager.get_mint_keys();
        
        // Initialize CDK mint with derived keys
        // This would use the actual CDK mint setup with the derived keys
        todo!("Implement CDK mint creation with derived keys")
    }
    
    async fn create_nostr_client(&self, key_manager: &MasterKeyManager) -> Result<nostr_sdk::Client, GameProtocolError> {
        // Create Nostr client with derived keys
        let nostr_keys = key_manager.get_player_keys();
        let client = nostr_sdk::Client::new(nostr_keys);
        
        // Connect to embedded relay
        client.add_relay("ws://127.0.0.1:7000", None).await?;
        client.connect().await;
        
        Ok(client)
    }
    
    // Method to inject REPL commands into the ECS world
    pub fn send_repl_command(&mut self, command: ReplCommand) {
        if let Some(mut queue) = self.app.world.get_resource_mut::<ReplCommandQueue>() {
            queue.commands.push_back(command);
        }
    }
    
    // Method to inject Nostr events into the ECS world
    pub fn send_nostr_event(&mut self, event: Event) {
        if let Some(mut queue) = self.app.world.get_resource_mut::<GameEventQueue>() {
            queue.events.push_back(event);
        }
    }
    
    // Method to get current status for REPL display
    pub fn get_status(&self) -> String {
        if let Some(status) = self.app.world.get_resource::<GameStatusDisplay>() {
            status.current_status.clone()
        } else {
            "Unknown".to_string()
        }
    }
}

// Startup systems
fn setup_embedded_mint(mut commands: Commands) {
    // This will be called during app startup
    // Actual mint setup happens in setup_async_resources
}

fn setup_nostr_client(mut commands: Commands) {
    // This will be called during app startup
    // Actual client setup happens in setup_async_resources
}

fn create_default_player(mut commands: Commands, key_manager: Res<MasterKeyManager>) {
    let player_keys = key_manager.get_player_keys().clone();
    commands.spawn(Player {
        pubkey: player_keys.public_key(),
        nostr_keys: player_keys,
        balance_game_tokens: 0,
        balance_reward_tokens: 0,
    });
}
```

### 5. Embedded Nostr Relay Thread

A simple in-memory Nostr relay for event storage and subscription handling.

#### Relay Implementation
```rust
pub struct EmbeddedRelay {
    events: Arc<RwLock<HashMap<EventId, Event>>>,
    subscriptions: Arc<RwLock<HashMap<String, NostrFilter>>>,
    event_sender: broadcast::Sender<Event>,
}

impl EmbeddedRelay {
    pub fn new() -> Self {
        let (event_sender, _) = broadcast::channel(1000);
        
        Self {
            events: Arc::new(RwLock::new(HashMap::new())),
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            event_sender,
        }
    }
    
    pub async fn run(&self, port: u16) -> Result<(), GameProtocolError> {
        // Start WebSocket server for Nostr protocol
        let addr = format!("127.0.0.1:{}", port);
        let listener = TcpListener::bind(&addr).await?;
        
        println!("Embedded Nostr relay listening on {}", addr);
        
        while let Ok((stream, _)) = listener.accept().await {
            let relay = self.clone();
            tokio::spawn(async move {
                relay.handle_connection(stream).await;
            });
        }
        
        Ok(())
    }
    
    async fn handle_connection(&self, stream: TcpStream) {
        // Handle WebSocket connection and Nostr protocol messages
        // This would implement the full Nostr relay protocol (NIP-01)
        // For brevity, showing the concept rather than full implementation
    }
    
    pub async fn store_event(&self, event: Event) -> Result<(), GameProtocolError> {
        // Store event and notify subscribers
        let mut events = self.events.write().await;
        events.insert(event.id, event.clone());
        
        // Broadcast to subscribers
        let _ = self.event_sender.send(event);
        
        Ok(())
    }
    
    pub async fn query_events(&self, filter: &NostrFilter) -> Result<Vec<Event>, GameProtocolError> {
        let events = self.events.read().await;
        let matching_events: Vec<Event> = events
            .values()
            .filter(|event| filter.matches(event))
            .cloned()
            .collect();
            
        Ok(matching_events)
    }
}
```

## Data Models

### Game State Management
```rust
#[derive(Debug, Clone)]
pub struct GameState {
    pub challenge_id: EventId,
    pub players: [PublicKey; 2],
    pub state: GamePhase,
    pub cards: HashMap<PublicKey, Option<PlayingCard>>,
    pub tokens_committed: HashMap<PublicKey, Vec<String>>, // commitment hashes
    pub tokens_revealed: HashMap<PublicKey, Vec<GameToken>>,
    pub created_at: u64,
    pub last_activity: u64,
}

#[derive(Debug, Clone)]
pub enum GamePhase {
    WaitingForAccept,
    CardsRevealed,
    WaitingForFinal,
    Complete { winner: Option<PublicKey> },
    Forfeited { winner: PublicKey },
}

#[derive(Debug, Clone)]
pub struct CardMove {
    pub action: CardAction,
}

#[derive(Debug, Clone)]
pub enum CardAction {
    RevealCard, // Reveal the committed token to show the card
}

#[derive(Debug, Clone)]
pub struct CardGameState {
    pub phase: GamePhase,
    pub revealed_cards: Vec<(PublicKey, PlayingCard)>,
}
```

### Configuration Management
```rust
#[derive(Debug, Clone)]
pub struct GameConfig {
    pub mint_port: u16,
    pub relay_port: u16,
    pub default_wager: u64,
    pub game_timeout_seconds: u64,
    pub auto_accept_challenges: bool,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            mint_port: 3338,
            relay_port: 7000,
            default_wager: 100,
            game_timeout_seconds: 300, // 5 minutes
            auto_accept_challenges: false,
        }
    }
}
```

## Error Handling

### Enhanced Error Types for CLI
```rust
#[derive(Debug, thiserror::Error)]
pub enum CliGameError {
    #[error("Protocol error: {0}")]
    Protocol(#[from] GameProtocolError),
    
    #[error("REPL error: {0}")]
    Repl(String),
    
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("Network error: {0}")]
    Network(String),
    
    #[error("Game logic error: {0}")]
    GameLogic(String),
    
    #[error("Channel communication error: {0}")]
    Channel(String),
}

impl From<mpsc::error::SendError<MintRequest>> for CliGameError {
    fn from(err: mpsc::error::SendError<MintRequest>) -> Self {
        CliGameError::Channel(err.to_string())
    }
}

impl From<oneshot::error::RecvError> for CliGameError {
    fn from(err: oneshot::error::RecvError) -> Self {
        CliGameError::Channel(err.to_string())
    }
}
```

## Testing Strategy

### Unit Tests
- **Card Game Logic**: Test C value to card conversion, winner determination
- **REPL Command Parsing**: Verify command parsing and validation
- **Channel Communication**: Test MPSC channel message handling
- **Event Processing**: Validate Nostr event handling and game state updates

### Integration Tests
- **Full Game Flow**: End-to-end game from challenge to reward distribution
- **Multi-threaded Communication**: Test thread coordination and message passing
- **Error Recovery**: Test handling of network failures and invalid states
- **Timeout Handling**: Verify proper timeout and forfeit mechanics

### System Tests
- **Embedded Services**: Test mint and relay functionality in isolation
- **Performance**: Verify system handles multiple concurrent games
- **Resource Management**: Test memory usage and cleanup
- **Configuration**: Test various configuration scenarios

## Dependencies

### Additional Dependencies for CLI
```toml
[dependencies]
# Existing Kirk dependencies
kirk = { path = "." }

# Bevy ECS (minimal, no rendering)
bevy = { version = "0.12", default-features = false, features = [
    "bevy_core",
    "bevy_ecs", 
    "bevy_reflect",
    "bevy_time",
    "bevy_utils",
    "multi-threaded"
] }

# Key derivation
hkdf = "0.12"       # HMAC-based Key Derivation Function
rand = "0.8"        # For master seed generation

# CLI and REPL
clap = { version = "4.0", features = ["derive"] }
rustyline = "10.0"  # For REPL input handling
colored = "2.0"     # For colored terminal output

# Async runtime
tokio = { version = "1.0", features = ["full", "fs"] }

# WebSocket for embedded relay
tokio-tungstenite = "0.18"

# Configuration
serde = { version = "1.0", features = ["derive"] }
toml = "0.8"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[dev-dependencies]
tempfile = "3.0"
tokio-test = "0.4"
```

### Project Structure
```
cli-card-game/
├── Cargo.toml
├── src/
│   ├── main.rs              # Application entry point
│   ├── repl/
│   │   ├── mod.rs          # REPL interface
│   │   ├── commands.rs     # Command parsing and handling
│   │   └── display.rs      # Output formatting
│   ├── validator/
│   │   ├── mod.rs          # Game validator
│   │   ├── card_game.rs    # Card game implementation
│   │   └── event_handler.rs # Nostr event processing
│   ├── mint/
│   │   ├── mod.rs          # Embedded mint
│   │   ├── channel.rs      # MPSC channel interface
│   │   └── game_authority.rs # Game validation and rewards
│   ├── relay/
│   │   ├── mod.rs          # Embedded Nostr relay
│   │   └── protocol.rs     # Nostr protocol implementation
│   ├── config.rs           # Configuration management
│   └── error.rs            # CLI-specific error types
├── tests/
│   ├── integration/
│   │   ├── full_game.rs    # End-to-end game tests
│   │   └── multi_thread.rs # Thread communication tests
│   └── unit/
│       ├── card_game.rs    # Card game logic tests
│       └── repl.rs         # REPL command tests
└── README.md
```

## Key Management and Security

### Master Key Derivation Strategy

The application uses a single master seed to derive all cryptographic keys, providing several benefits:

1. **Deterministic Key Generation**: All keys can be regenerated from the master seed
2. **Key Separation**: Nostr and mint keys are cryptographically isolated using different derivation contexts
3. **Backup Simplicity**: Only the master seed needs to be backed up to restore all functionality
4. **Security**: Uses HKDF (HMAC-based Key Derivation Function) with SHA-256 for secure key derivation

### Key Derivation Process

```
Master Seed (64 bytes)
    │
    ├─ HKDF(master_seed, "nostr-client-key") → Nostr Keys (32 bytes)
    │                                           │
    │                                           └─ Player Identity & Event Signing
    │
    └─ HKDF(master_seed, "embedded-mint-key") → Mint Keys (32 bytes)
                                                │
                                                └─ Mint Authority & Token Signing
```

### Seed Persistence

- **Optional File Storage**: Master seed can be saved to disk for persistence across sessions
- **Secure Generation**: Uses cryptographically secure random number generation
- **Recovery**: Application can restore full functionality from saved master seed
- **Fresh Start**: If no seed file exists, generates new master seed automatically

This design provides a complete, self-contained demonstration of the Kirk gaming protocol while maintaining clean separation between components, secure key management, and showcasing the key architectural patterns for trustless gaming applications.
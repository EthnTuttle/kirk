# Troubleshooting Guide

This guide helps diagnose and resolve common issues when using Kirk.

## Table of Contents

- [Common Issues](#common-issues)
- [Error Messages](#error-messages)
- [Debugging Tools](#debugging-tools)
- [Performance Issues](#performance-issues)
- [Network Problems](#network-problems)
- [Game-Specific Issues](#game-specific-issues)
- [Recovery Procedures](#recovery-procedures)

## Common Issues

### Token-Related Issues

#### Issue: "Token validation failed"

**Symptoms**: Games fail to start or moves are rejected with token validation errors.

**Possible Causes**:
- Token has already been spent
- Token is not valid for the mint
- Token amount is insufficient
- Token format is corrupted

**Solutions**:

```rust
// 1. Check token validity
async fn diagnose_token_issue(token: &GameToken, mint: &GameMint) -> Result<(), DiagnosticError> {
    // Verify token with mint
    if !mint.validate_tokens(&[token.as_cdk_token().clone()]).await? {
        println!("Token is invalid according to mint");
        
        // Check if token was already spent
        if mint.is_token_spent(token.as_cdk_token()).await? {
            println!("Token has already been spent");
            return Err(DiagnosticError::TokenAlreadySpent);
        }
        
        // Check token format
        if !is_token_format_valid(token) {
            println!("Token format is invalid");
            return Err(DiagnosticError::InvalidTokenFormat);
        }
    }
    
    println!("Token appears valid");
    Ok(())
}

// 2. Get fresh tokens
async fn get_fresh_tokens(player: &PlayerClient, amount: u64) -> Result<Vec<GameToken>, GameError> {
    // Mint new tokens
    let new_tokens = player.mint_game_tokens(amount).await?;
    
    // Verify they're valid
    for token in &new_tokens {
        player.validate_token(token).await?;
    }
    
    Ok(new_tokens)
}
```

#### Issue: "Insufficient token amount"

**Solutions**:
```rust
// Check required amount for game
async fn check_game_requirements(game: &impl Game) -> u64 {
    game.get_minimum_token_amount()
}

// Mint additional tokens if needed
async fn ensure_sufficient_tokens(
    player: &PlayerClient, 
    required_amount: u64
) -> Result<Vec<GameToken>, GameError> {
    let current_balance = player.get_balance().await?;
    
    if current_balance < required_amount {
        let needed = required_amount - current_balance;
        player.mint_game_tokens(needed).await
    } else {
        player.get_available_tokens(required_amount).await
    }
}
```

### Commitment Issues

#### Issue: "Commitment verification failed"

**Symptoms**: Games fail during validation with commitment mismatch errors.

**Diagnosis**:
```rust
async fn diagnose_commitment_issue(
    commitment_hash: &str,
    revealed_tokens: &[CashuToken],
    method: Option<CommitmentMethod>
) -> Result<(), DiagnosticError> {
    println!("Diagnosing commitment issue...");
    
    // 1. Check if tokens are properly formatted
    for (i, token) in revealed_tokens.iter().enumerate() {
        if !is_token_format_valid(token) {
            println!("Token {} has invalid format", i);
            return Err(DiagnosticError::InvalidTokenFormat);
        }
    }
    
    // 2. Try different commitment methods
    let methods = vec![
        CommitmentMethod::Concatenation,
        CommitmentMethod::MerkleTreeRadix4,
    ];
    
    for test_method in methods {
        let test_commitment = TokenCommitment::multiple(revealed_tokens, test_method.clone());
        if test_commitment.commitment_hash == commitment_hash {
            println!("Commitment matches with method: {:?}", test_method);
            if method.is_some() && method.as_ref() != Some(&test_method) {
                println!("WARNING: Method mismatch - expected {:?}, found {:?}", method, test_method);
            }
            return Ok(());
        }
    }
    
    // 3. Check single token commitment
    if revealed_tokens.len() == 1 {
        let single_commitment = TokenCommitment::single(&revealed_tokens[0]);
        if single_commitment.commitment_hash == commitment_hash {
            println!("Commitment matches as single token");
            return Ok(());
        }
    }
    
    println!("No commitment method produces matching hash");
    Err(DiagnosticError::CommitmentMismatch)
}
```

**Solutions**:
```rust
// Rebuild commitment with correct method
fn fix_commitment_method(
    tokens: &[CashuToken],
    original_hash: &str
) -> Result<CommitmentMethod, GameError> {
    // Try each method to find the correct one
    for method in [CommitmentMethod::Concatenation, CommitmentMethod::MerkleTreeRadix4] {
        let commitment = TokenCommitment::multiple(tokens, method.clone());
        if commitment.commitment_hash == original_hash {
            return Ok(method);
        }
    }
    
    Err(GameError::CommitmentMethodNotFound)
}
```

### Event Chain Issues

#### Issue: "Broken event chain"

**Symptoms**: Game validation fails with sequence errors.

**Diagnosis**:
```rust
fn diagnose_event_chain(events: &[Event]) -> Result<(), DiagnosticError> {
    if events.is_empty() {
        return Err(DiagnosticError::EmptyEventChain);
    }
    
    println!("Checking event chain of {} events", events.len());
    
    // Check first event
    if !is_challenge_event(&events[0]) {
        println!("First event is not a Challenge: kind={}", events[0].kind);
        return Err(DiagnosticError::InvalidChainStart);
    }
    
    // Check chain continuity
    for i in 1..events.len() {
        let current = &events[i];
        let previous = &events[i-1];
        
        if !references_previous_event(current, previous) {
            println!("Event {} does not reference previous event {}", i, i-1);
            println!("Current event: {}", current.id);
            println!("Previous event: {}", previous.id);
            return Err(DiagnosticError::BrokenChain);
        }
        
        // Check timestamps
        if current.created_at < previous.created_at {
            println!("Event {} has earlier timestamp than previous", i);
            return Err(DiagnosticError::InvalidTimestamp);
        }
    }
    
    println!("Event chain is valid");
    Ok(())
}
```

**Solutions**:
```rust
// Collect missing events
async fn repair_event_chain(
    client: &NostrClient,
    partial_events: &[Event]
) -> Result<Vec<Event>, GameError> {
    let mut complete_events = Vec::new();
    
    // Find the challenge event
    let challenge_event = find_challenge_event(partial_events)?;
    complete_events.push(challenge_event.clone());
    
    // Collect all related events
    let filters = vec![
        Filter::new()
            .kinds(vec![
                Kind::Custom(9259), // Challenge
                Kind::Custom(9260), // ChallengeAccept
                Kind::Custom(9261), // Move
                Kind::Custom(9262), // Final
            ])
            .since(challenge_event.created_at)
    ];
    
    let all_events = client.get_events_of(filters, Some(Duration::from_secs(10))).await?;
    
    // Filter and sort events related to this game
    let related_events = filter_related_events(&all_events, &challenge_event.id);
    complete_events.extend(related_events);
    
    // Sort by timestamp
    complete_events.sort_by_key(|e| e.created_at);
    
    Ok(complete_events)
}
```

### Network Issues

#### Issue: "Failed to connect to relay"

**Symptoms**: Cannot publish or receive events.

**Diagnosis**:
```rust
async fn diagnose_relay_connection(relay_url: &str) -> Result<(), DiagnosticError> {
    println!("Testing connection to relay: {}", relay_url);
    
    // 1. Check URL format
    if !relay_url.starts_with("wss://") && !relay_url.starts_with("ws://") {
        return Err(DiagnosticError::InvalidRelayUrl);
    }
    
    // 2. Test basic connectivity
    match tokio::time::timeout(Duration::from_secs(10), test_relay_connection(relay_url)).await {
        Ok(Ok(())) => println!("Relay connection successful"),
        Ok(Err(e)) => {
            println!("Relay connection failed: {}", e);
            return Err(DiagnosticError::RelayConnectionFailed);
        }
        Err(_) => {
            println!("Relay connection timed out");
            return Err(DiagnosticError::RelayTimeout);
        }
    }
    
    Ok(())
}

async fn test_relay_connection(relay_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let keys = Keys::generate();
    let client = NostrClient::new(&keys);
    
    client.add_relay(relay_url).await?;
    client.connect().await;
    
    // Test with a simple event
    let test_event = EventBuilder::new(Kind::TextNote, "test", &[]).to_event(&keys)?;
    client.send_event(test_event).await?;
    
    Ok(())
}
```

**Solutions**:
```rust
// Use multiple relays for redundancy
async fn setup_redundant_relays(client: &NostrClient) -> Result<(), GameError> {
    let relays = vec![
        "wss://relay.damus.io",
        "wss://nos.lol",
        "wss://relay.nostr.band",
        "wss://nostr-pub.wellorder.net",
    ];
    
    let mut connected_count = 0;
    
    for relay in relays {
        match client.add_relay(relay).await {
            Ok(_) => {
                connected_count += 1;
                println!("Connected to relay: {}", relay);
            }
            Err(e) => {
                println!("Failed to connect to relay {}: {}", relay, e);
            }
        }
    }
    
    if connected_count == 0 {
        return Err(GameError::NoRelaysAvailable);
    }
    
    println!("Connected to {} relays", connected_count);
    Ok(())
}
```

## Error Messages

### Detailed Error Analysis

#### "GameValidation" Errors

```rust
fn analyze_game_validation_error(error: &GameProtocolError) -> String {
    match error {
        GameProtocolError::GameValidation(msg) => {
            if msg.contains("invalid move") {
                format!("Move validation failed: {}. Check if the move is legal in the current game state.", msg)
            } else if msg.contains("sequence") {
                format!("Game sequence validation failed: {}. Check event chain integrity.", msg)
            } else if msg.contains("timeout") {
                format!("Game timeout occurred: {}. Check if all players are responding within time limits.", msg)
            } else {
                format!("Game validation error: {}. Check game rules and event sequence.", msg)
            }
        }
        _ => "Not a game validation error".to_string(),
    }
}
```

#### "InvalidCommitment" Errors

```rust
fn analyze_commitment_error(error: &GameProtocolError) -> String {
    match error {
        GameProtocolError::InvalidCommitment(msg) => {
            if msg.contains("hash mismatch") {
                "Commitment hash doesn't match revealed tokens. Check commitment construction method.".to_string()
            } else if msg.contains("method") {
                "Commitment method not specified or incorrect. Ensure Final events include commitment method.".to_string()
            } else if msg.contains("format") {
                "Commitment hash format is invalid. Should be 64-character hex string.".to_string()
            } else {
                format!("Commitment validation failed: {}. Check token-commitment relationship.", msg)
            }
        }
        _ => "Not a commitment error".to_string(),
    }
}
```

## Debugging Tools

### Event Inspector

```rust
pub struct EventInspector;

impl EventInspector {
    pub fn inspect_event(event: &Event) -> EventReport {
        EventReport {
            id: event.id,
            kind: event.kind,
            pubkey: event.pubkey,
            created_at: event.created_at,
            content_length: event.content.len(),
            tags_count: event.tags.len(),
            signature_valid: event.verify().unwrap_or(false),
            content_type: Self::detect_content_type(&event.content),
        }
    }
    
    pub fn inspect_game_sequence(events: &[Event]) -> SequenceReport {
        let mut report = SequenceReport::new();
        
        for (i, event) in events.iter().enumerate() {
            let event_report = Self::inspect_event(event);
            report.events.push(event_report);
            
            // Check chain continuity
            if i > 0 {
                let references_previous = Self::check_event_reference(event, &events[i-1]);
                report.chain_breaks.push(!references_previous);
            }
        }
        
        report
    }
    
    fn detect_content_type(content: &str) -> ContentType {
        if let Ok(parsed) = serde_json::from_str::<ChallengeContent>(content) {
            ContentType::Challenge
        } else if let Ok(parsed) = serde_json::from_str::<MoveContent>(content) {
            ContentType::Move
        } else if let Ok(parsed) = serde_json::from_str::<FinalContent>(content) {
            ContentType::Final
        } else {
            ContentType::Unknown
        }
    }
}

#[derive(Debug)]
pub struct EventReport {
    pub id: EventId,
    pub kind: Kind,
    pub pubkey: PublicKey,
    pub created_at: u64,
    pub content_length: usize,
    pub tags_count: usize,
    pub signature_valid: bool,
    pub content_type: ContentType,
}

#[derive(Debug)]
pub enum ContentType {
    Challenge,
    ChallengeAccept,
    Move,
    Final,
    Reward,
    Unknown,
}
```

### Token Analyzer

```rust
pub struct TokenAnalyzer;

impl TokenAnalyzer {
    pub fn analyze_token(token: &GameToken) -> TokenReport {
        let cdk_token = token.as_cdk_token();
        
        TokenReport {
            token_type: token.game_type.clone(),
            proof_count: cdk_token.proofs.len(),
            total_amount: cdk_token.proofs.iter().map(|p| p.amount).sum(),
            c_values: token.extract_c_values(),
            is_p2pk_locked: token.is_p2pk_locked(),
            entropy_analysis: Self::analyze_entropy(&token.extract_c_values()),
        }
    }
    
    fn analyze_entropy(c_values: &[[u8; 32]]) -> EntropyReport {
        let mut total_entropy = 0.0;
        let mut min_entropy = f64::MAX;
        let mut max_entropy = 0.0;
        
        for c_value in c_values {
            let entropy = calculate_shannon_entropy(c_value);
            total_entropy += entropy;
            min_entropy = min_entropy.min(entropy);
            max_entropy = max_entropy.max(entropy);
        }
        
        EntropyReport {
            average_entropy: total_entropy / c_values.len() as f64,
            min_entropy,
            max_entropy,
            sufficient_randomness: min_entropy > 7.0, // Threshold for good entropy
        }
    }
}

#[derive(Debug)]
pub struct TokenReport {
    pub token_type: GameTokenType,
    pub proof_count: usize,
    pub total_amount: u64,
    pub c_values: Vec<[u8; 32]>,
    pub is_p2pk_locked: bool,
    pub entropy_analysis: EntropyReport,
}

#[derive(Debug)]
pub struct EntropyReport {
    pub average_entropy: f64,
    pub min_entropy: f64,
    pub max_entropy: f64,
    pub sufficient_randomness: bool,
}
```

### Game State Debugger

```rust
pub struct GameStateDebugger<G: Game> {
    game: G,
}

impl<G: Game> GameStateDebugger<G> {
    pub fn new(game: G) -> Self {
        Self { game }
    }
    
    pub async fn debug_game_sequence(&self, events: &[Event]) -> DebugReport {
        let mut report = DebugReport::new();
        
        // Validate each step
        for (i, event) in events.iter().enumerate() {
            let step_result = self.debug_game_step(event, &events[..i]).await;
            report.steps.push(step_result);
        }
        
        // Overall validation
        match self.game.validate_sequence(events) {
            Ok(result) => report.overall_result = Some(result),
            Err(e) => report.validation_error = Some(e),
        }
        
        report
    }
    
    async fn debug_game_step(&self, event: &Event, previous_events: &[Event]) -> StepDebugResult {
        StepDebugResult {
            event_id: event.id,
            event_type: Self::classify_event(event),
            is_valid_format: Self::validate_event_format(event),
            references_previous: Self::check_previous_reference(event, previous_events),
            game_state_valid: self.validate_game_state_transition(event, previous_events).await,
        }
    }
}

#[derive(Debug)]
pub struct DebugReport {
    pub steps: Vec<StepDebugResult>,
    pub overall_result: Option<ValidationResult>,
    pub validation_error: Option<GameError>,
}

#[derive(Debug)]
pub struct StepDebugResult {
    pub event_id: EventId,
    pub event_type: GameEventType,
    pub is_valid_format: bool,
    pub references_previous: bool,
    pub game_state_valid: bool,
}
```

## Performance Issues

### Slow Game Validation

**Symptoms**: Game validation takes too long, timeouts occur.

**Solutions**:

```rust
// Optimize validation with caching
pub struct CachedGameValidator<G: Game> {
    game: G,
    state_cache: HashMap<EventId, G::GameState>,
    validation_cache: HashMap<Vec<EventId>, ValidationResult>,
}

impl<G: Game> CachedGameValidator<G> {
    pub async fn validate_with_cache(&mut self, events: &[Event]) -> Result<ValidationResult, GameError> {
        // Check cache first
        let event_ids: Vec<EventId> = events.iter().map(|e| e.id).collect();
        if let Some(cached_result) = self.validation_cache.get(&event_ids) {
            return Ok(cached_result.clone());
        }
        
        // Validate incrementally
        let result = self.incremental_validation(events).await?;
        
        // Cache result
        self.validation_cache.insert(event_ids, result.clone());
        
        Ok(result)
    }
    
    async fn incremental_validation(&mut self, events: &[Event]) -> Result<ValidationResult, GameError> {
        // Find longest cached prefix
        let mut validated_count = 0;
        for i in 1..=events.len() {
            let prefix_ids: Vec<EventId> = events[..i].iter().map(|e| e.id).collect();
            if self.validation_cache.contains_key(&prefix_ids) {
                validated_count = i;
            } else {
                break;
            }
        }
        
        // Validate only new events
        if validated_count < events.len() {
            let new_events = &events[validated_count..];
            // Validate new events...
        }
        
        // Return final result
        self.game.validate_sequence(events)
    }
}
```

### Memory Usage Issues

```rust
// Optimize memory usage for large games
pub struct MemoryEfficientValidator<G: Game> {
    game: G,
    max_cache_size: usize,
}

impl<G: Game> MemoryEfficientValidator<G> {
    pub fn with_memory_limit(game: G, max_memory_mb: usize) -> Self {
        Self {
            game,
            max_cache_size: max_memory_mb * 1024 * 1024,
        }
    }
    
    pub async fn validate_streaming(&self, events: &[Event]) -> Result<ValidationResult, GameError> {
        // Process events in chunks to limit memory usage
        const CHUNK_SIZE: usize = 100;
        
        for chunk in events.chunks(CHUNK_SIZE) {
            self.validate_chunk(chunk).await?;
            
            // Force garbage collection periodically
            if chunk.len() == CHUNK_SIZE {
                tokio::task::yield_now().await;
            }
        }
        
        self.game.validate_sequence(events)
    }
}
```

## Recovery Procedures

### Recovering from Failed Games

```rust
pub struct GameRecovery;

impl GameRecovery {
    /// Attempt to recover a failed game by collecting missing events
    pub async fn recover_game(
        client: &NostrClient,
        challenge_id: EventId
    ) -> Result<RecoveryResult, GameError> {
        println!("Attempting to recover game: {}", challenge_id);
        
        // 1. Collect all possible related events
        let events = Self::collect_all_related_events(client, challenge_id).await?;
        
        // 2. Analyze what we have
        let analysis = Self::analyze_event_completeness(&events);
        
        // 3. Determine if recovery is possible
        if analysis.is_recoverable {
            Ok(RecoveryResult::Success(events))
        } else {
            Ok(RecoveryResult::Partial {
                events,
                missing_events: analysis.missing_events,
            })
        }
    }
    
    /// Recover from commitment verification failures
    pub async fn recover_commitment_failure(
        events: &[Event],
        failed_commitment: &str
    ) -> Result<CommitmentRecovery, GameError> {
        // Try to find the correct commitment method
        for event in events {
            if let Ok(move_content) = serde_json::from_str::<MoveContent>(&event.content) {
                if let Some(tokens) = &move_content.revealed_tokens {
                    // Try different methods
                    for method in [CommitmentMethod::Concatenation, CommitmentMethod::MerkleTreeRadix4] {
                        let test_commitment = TokenCommitment::multiple(tokens, method.clone());
                        if test_commitment.commitment_hash == failed_commitment {
                            return Ok(CommitmentRecovery::MethodFound(method));
                        }
                    }
                }
            }
        }
        
        Ok(CommitmentRecovery::Unrecoverable)
    }
}

#[derive(Debug)]
pub enum RecoveryResult {
    Success(Vec<Event>),
    Partial {
        events: Vec<Event>,
        missing_events: Vec<String>,
    },
    Failed(String),
}

#[derive(Debug)]
pub enum CommitmentRecovery {
    MethodFound(CommitmentMethod),
    Unrecoverable,
}
```

### Emergency Procedures

```rust
/// Emergency procedures for critical failures
pub struct EmergencyProcedures;

impl EmergencyProcedures {
    /// Emergency token recovery
    pub async fn emergency_token_recovery(
        player: &PlayerClient,
        failed_tokens: &[GameToken]
    ) -> Result<Vec<GameToken>, GameError> {
        println!("Initiating emergency token recovery...");
        
        // 1. Check if tokens can be swapped
        let mut recovered_tokens = Vec::new();
        
        for token in failed_tokens {
            match player.attempt_token_swap(token).await {
                Ok(new_token) => {
                    recovered_tokens.push(new_token);
                    println!("Recovered token: {}", token.as_cdk_token().id);
                }
                Err(e) => {
                    println!("Failed to recover token {}: {}", token.as_cdk_token().id, e);
                }
            }
        }
        
        // 2. If swapping fails, try melting to Lightning
        if recovered_tokens.is_empty() {
            println!("Swap recovery failed, attempting Lightning melt...");
            // Implementation depends on Lightning integration
        }
        
        Ok(recovered_tokens)
    }
    
    /// Emergency game termination
    pub async fn emergency_game_termination(
        player: &PlayerClient,
        game_events: &[Event],
        reason: &str
    ) -> Result<EventId, GameError> {
        println!("Initiating emergency game termination: {}", reason);
        
        // Publish emergency termination event
        let termination_content = serde_json::json!({
            "type": "emergency_termination",
            "reason": reason,
            "game_root": game_events[0].id,
            "timestamp": chrono::Utc::now().timestamp()
        });
        
        let termination_event = EventBuilder::new(
            Kind::Custom(9264), // Emergency termination kind
            serde_json::to_string(&termination_content)?,
            &[]
        ).to_event(&player.keys)?;
        
        player.nostr_client.send_event(termination_event).await
            .map_err(GameError::from)
    }
}
```

This troubleshooting guide provides comprehensive diagnostic tools and recovery procedures for common Kirk issues. Use these tools to identify problems quickly and implement appropriate solutions.
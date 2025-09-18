//! Comprehensive usage guide for the Kirk gaming protocol
//! 
//! This example shows how to implement and use games with the Kirk framework,
//! including complete integration with Cashu mints and Nostr relays.

mod games;

use games::{CoinFlipGame, DiceGame};
use kirk::{Game, GameProtocolError};

/// Demonstrates how to implement a custom game
fn demonstrate_custom_game_implementation() -> Result<(), GameProtocolError> {
    println!("=== Custom Game Implementation Guide ===\n");

    println!("1. Define your game structures:");
    println!("   - Game configuration struct");
    println!("   - Game piece struct (decoded from C values)");
    println!("   - Game state struct (for validation)");
    println!("   - Move data struct (for player actions)");

    println!("\n2. Implement the Game trait:");
    println!("   - decode_c_value: Extract game pieces from token C values");
    println!("   - validate_sequence: Validate complete game event chain");
    println!("   - is_sequence_complete: Check if game is finished");
    println!("   - determine_winner: Calculate winner from final state");
    println!("   - required_final_events: How many Final events needed");

    // Show CoinFlip as example
    let coin_game = CoinFlipGame::new();
    println!("\n3. Example: CoinFlip Game");
    println!("   Configuration: {:?}", coin_game.config);
    
    // Demonstrate C value decoding
    let example_c_value = [42u8, 150u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8];
    let pieces = coin_game.decode_c_value(&example_c_value)?;
    println!("   C value [42, 150, ...] decodes to: {:?}", pieces[0]);

    println!("\n4. Game trait provides uniform interface:");
    println!("   - Same validation and coordination logic for all games");
    println!("   - Type-safe game pieces and moves");
    println!("   - Flexible winner determination");

    Ok(())
}

/// Shows the complete player workflow
async fn demonstrate_player_workflow() -> Result<(), GameProtocolError> {
    println!("=== Player Workflow Guide ===\n");

    println!("1. Player Setup:");
    println!("   - Generate Nostr keys for identity");
    println!("   - Connect to Nostr relays for event coordination");
    println!("   - Connect to Cashu mint for token operations");
    println!("   - Create PlayerClient with all components");

    // Mock setup (in real usage, these would be actual clients)
    println!("\n   Example setup code:");
    println!("   let keys = Keys::generate();");
    println!("   let nostr_client = Client::new(&keys);");
    println!("   let cashu_wallet = Wallet::new(mint_url);");
    println!("   let player = PlayerClient::new(nostr_client, cashu_wallet, keys);");

    println!("\n2. Token Preparation:");
    println!("   - Mint game tokens from Cashu mint");
    println!("   - Tokens provide C values for game piece randomness");
    println!("   - Amount determines stake in the game");

    println!("\n3. Challenge Creation:");
    println!("   - Choose game type and parameters");
    println!("   - Create hash commitments for your tokens");
    println!("   - Publish Challenge event to Nostr");
    println!("   - Set expiry time for challenge acceptance");

    println!("\n   Example challenge code:");
    println!("   let game = CoinFlipGame::new();");
    println!("   let challenge_id = player.create_challenge(&game, &tokens, Some(3600)).await?;");

    println!("\n4. Challenge Acceptance:");
    println!("   - Find interesting challenges on Nostr");
    println!("   - Prepare your own tokens for the game");
    println!("   - Accept challenge with your commitments");

    println!("\n   Example acceptance code:");
    println!("   let accept_id = player.accept_challenge(challenge_id, &game, &my_tokens).await?;");

    println!("\n5. Gameplay:");
    println!("   - Make moves by revealing tokens and choices");
    println!("   - Use commit-and-reveal for simultaneous decisions");
    println!("   - Follow game-specific rules and sequences");

    println!("\n   Example move code:");
    println!("   let move_data = CoinFlipMove {{");
    println!("       chosen_side: CoinSide::Heads,");
    println!("       confidence: 200,");
    println!("   }};");
    println!("   let move_id = player.make_move(prev_id, MoveType::Move, move_data, Some(tokens)).await?;");

    println!("\n6. Game Finalization:");
    println!("   - Publish Final events when game is complete");
    println!("   - Include commitment methods for multi-token games");
    println!("   - Wait for mint validation and reward distribution");

    println!("\n   Example finalization code:");
    println!("   let final_id = player.finalize_game(");
    println!("       challenge_id,");
    println!("       None, // Single token, no commitment method");
    println!("       serde_json::json!({{\"state\": \"complete\"}})");
    println!("   ).await?;");

    Ok(())
}

/// Shows how mints validate games and distribute rewards
async fn demonstrate_mint_workflow() -> Result<(), GameProtocolError> {
    println!("=== Mint Operator Workflow Guide ===\n");

    println!("1. Mint Setup:");
    println!("   - Run Cashu mint with game token support");
    println!("   - Connect to Nostr relays for event monitoring");
    println!("   - Configure game validation rules");
    println!("   - Set up reward distribution policies");

    println!("\n2. Game Monitoring:");
    println!("   - Subscribe to game-related Nostr events");
    println!("   - Track game sequences from Challenge to Final");
    println!("   - Collect all events for complete validation");

    println!("\n3. Sequence Validation:");
    println!("   - Verify all revealed tokens are valid");
    println!("   - Check hash commitments match revealed tokens");
    println!("   - Validate game rules and move sequences");
    println!("   - Detect fraud or rule violations");

    println!("\n   Example validation code:");
    println!("   let game = CoinFlipGame::new();");
    println!("   let validation_result = game.validate_sequence(&all_events)?;");
    println!("   if validation_result.is_valid {{");
    println!("       // Process winner and rewards");
    println!("   }}");

    println!("\n4. Winner Determination:");
    println!("   - Use game-specific logic to determine winner");
    println!("   - Handle ties, forfeits, and edge cases");
    println!("   - Calculate reward amounts based on stakes");

    println!("\n5. Reward Distribution:");
    println!("   - Melt losing player's game tokens");
    println!("   - Mint P2PK-locked reward tokens for winner");
    println!("   - Publish Reward event to Nostr");
    println!("   - Enable winner to unlock tokens later");

    println!("\n   Example reward code:");
    println!("   let reward_tokens = mint.mint_reward_tokens(total_amount, winner_pubkey).await?;");
    println!("   mint.publish_game_result(&events, winner_pubkey, reward_tokens).await?;");

    println!("\n6. Error Handling:");
    println!("   - Publish ValidationFailure events for system errors");
    println!("   - Handle timeout scenarios");
    println!("   - Manage dispute resolution");

    Ok(())
}

/// Shows how third parties can validate games
fn demonstrate_validator_workflow() -> Result<(), GameProtocolError> {
    println!("=== Third-Party Validator Workflow Guide ===\n");

    println!("1. Validator Setup:");
    println!("   - Connect to Nostr relays (read-only)");
    println!("   - No Cashu mint connection required");
    println!("   - Create ValidationClient for game verification");

    println!("\n2. Event Collection:");
    println!("   - Query Nostr for game event sequences");
    println!("   - Filter by game types of interest");
    println!("   - Collect complete event chains");

    println!("\n3. Independent Validation:");
    println!("   - Verify event signatures and timestamps");
    println!("   - Check hash commitments against revealed tokens");
    println!("   - Validate game rules and sequences");
    println!("   - Determine winners independently");

    println!("\n   Example validation code:");
    println!("   let validator = ValidationClient::new(nostr_client);");
    println!("   let events = validator.collect_game_events(challenge_id).await?;");
    println!("   let result = validator.validate_game_sequence(&game, &events)?;");

    println!("\n4. Verification Results:");
    println!("   - Compare results with mint's determination");
    println!("   - Detect potential fraud or errors");
    println!("   - Publish verification reports if needed");

    println!("\n5. Use Cases:");
    println!("   - Audit mint behavior for fairness");
    println!("   - Provide dispute resolution services");
    println!("   - Create game analytics and statistics");
    println!("   - Build trust in the gaming ecosystem");

    Ok(())
}

/// Demonstrates different C value decoding strategies
fn demonstrate_c_value_strategies() -> Result<(), GameProtocolError> {
    println!("=== C Value Decoding Strategies ===\n");

    println!("1. Single Value Extraction:");
    println!("   - Use one or few bytes from C value");
    println!("   - Simple modulo operations for ranges");
    println!("   - Good for basic randomness needs");

    let coin_game = CoinFlipGame::new();
    let c_value = [42u8, 150u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8];
    let coin_pieces = coin_game.decode_c_value(&c_value)?;
    println!("   CoinFlip example: {:?}", coin_pieces[0]);

    println!("\n2. Multiple Value Extraction:");
    println!("   - Use multiple bytes for different attributes");
    println!("   - Create complex game pieces");
    println!("   - Combine bytes for larger value ranges");

    let dice_game = DiceGame::new();
    let dice_pieces = dice_game.decode_c_value(&c_value)?;
    println!("   Dice example: {:?}", dice_pieces);

    println!("\n3. Advanced Strategies:");
    println!("   - Hash C value for uniform distribution");
    println!("   - Use bit manipulation for flags");
    println!("   - Combine multiple C values for complex pieces");
    println!("   - Apply game-specific transformations");

    println!("\n4. Security Considerations:");
    println!("   - C values provide cryptographic randomness");
    println!("   - Players cannot predict or control values");
    println!("   - Use sufficient entropy for game balance");
    println!("   - Avoid patterns that could be exploited");

    Ok(())
}

/// Shows commit-and-reveal mechanics
fn demonstrate_commit_reveal() -> Result<(), GameProtocolError> {
    println!("=== Commit-and-Reveal Mechanics ===\n");

    println!("1. Commitment Phase:");
    println!("   - Players create hash commitments of their tokens");
    println!("   - Commitments hide actual token values");
    println!("   - Published in Challenge/ChallengeAccept events");

    println!("\n2. Commitment Methods:");
    println!("   - Single token: SHA256(token_hash)");
    println!("   - Multiple tokens (concatenation): SHA256(token1 || token2 || ...)");
    println!("   - Multiple tokens (merkle tree): Merkle root of token hashes");

    println!("\n3. Reveal Phase:");
    println!("   - Players reveal actual tokens in Move events");
    println!("   - Validators verify tokens match commitments");
    println!("   - Game pieces extracted from revealed C values");

    println!("\n4. Strategic Benefits:");
    println!("   - Prevents information leakage before decisions");
    println!("   - Enables simultaneous decision making");
    println!("   - Maintains game balance and fairness");

    println!("\n5. Fraud Prevention:");
    println!("   - Mismatched commitments result in forfeit");
    println!("   - Cryptographic proof of cheating");
    println!("   - Automatic penalty enforcement");

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Kirk Gaming Protocol - Complete Usage Guide ===\n");

    demonstrate_custom_game_implementation()?;
    println!("\n{}\n", "=".repeat(60));

    demonstrate_player_workflow().await?;
    println!("\n{}\n", "=".repeat(60));

    demonstrate_mint_workflow().await?;
    println!("\n{}\n", "=".repeat(60));

    demonstrate_validator_workflow()?;
    println!("\n{}\n", "=".repeat(60));

    demonstrate_c_value_strategies()?;
    println!("\n{}\n", "=".repeat(60));

    demonstrate_commit_reveal()?;

    println!("\n=== Usage Guide Complete ===");
    println!("\nNext steps:");
    println!("1. Study the example games in examples/games/");
    println!("2. Implement your own game using the Game trait");
    println!("3. Test with the provided mock infrastructure");
    println!("4. Deploy with real Cashu mints and Nostr relays");

    Ok(())
}
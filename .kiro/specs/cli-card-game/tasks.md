# Implementation Plan

- [x] 1. Set up project structure and core dependencies

  - Create new Cargo project for CLI card game
  - Add Bevy ECS minimal dependencies (no rendering/audio)
  - Add Kirk protocol library dependency
  - Add key derivation dependencies (hkdf, rand)
  - Add CLI dependencies (clap, rustyline, colored)
  - _Requirements: 1.1, 1.5_

- [ ] 2. Implement master key management system

  - [ ] 2.1 Create MasterKeyManager resource with HKDF key derivation

    - Implement master seed generation using cryptographically secure RNG
    - Implement HKDF-based key derivation for Nostr and mint keys
    - Add methods for deterministic key generation from master seed
    - _Requirements: 10.1, 10.2_

  - [ ] 2.2 Add seed persistence and recovery functionality
    - Implement optional master seed file storage
    - Add seed loading from disk on application startup
    - Add secure seed backup and recovery mechanisms
    - Write unit tests for key derivation consistency
    - _Requirements: 12.4, 12.5_

- [ ] 3. Create Bevy ECS components and resources

  - [ ] 3.1 Define core ECS components for game entities

    - Implement Player component with pubkey and balance tracking
    - Implement GameToken component with CDK token integration
    - Implement PlayingCard component with suit/rank from C values
    - Implement Challenge component for game challenge tracking
    - _Requirements: 8.1, 8.2_

  - [ ] 3.2 Define ECS components for game state management

    - Implement ActiveGame component for ongoing games
    - Implement GameSequence component for event chain tracking
    - Implement RewardToken component for P2PK locked rewards
    - Implement PendingReward component for reward processing queue
    - _Requirements: 3.1, 5.1_

  - [ ] 3.3 Create ECS resources for global state
    - Implement EmbeddedMint resource wrapping CDK mint
    - Implement NostrClient resource for event communication
    - Implement command and event queues for thread communication
    - Implement GameStatusDisplay resource for REPL feedback
    - _Requirements: 1.1, 1.5, 6.1_

- [ ] 4. Implement card game logic using Kirk protocol

  - [ ] 4.1 Create CardGame implementation of Game trait

    - Implement C value to playing card conversion (52-card deck)
    - Implement game sequence validation for card reveals
    - Implement winner determination by card comparison
    - Add tie-breaking rules and edge case handling
    - _Requirements: 3.1, 3.2, 8.1_

  - [ ] 4.2 Add card game specific data structures
    - Define PlayingCard struct with Suit and Rank enums
    - Implement card comparison and ordering logic
    - Add card display formatting for REPL output
    - Write unit tests for card derivation from C values
    - _Requirements: 3.1, 6.1_

- [ ] 5. Create Bevy ECS systems for game logic

  - [ ] 5.1 Implement REPL command processing system

    - Create system to process ReplCommandQueue events
    - Handle challenge creation, acceptance, and listing commands
    - Handle token minting and balance query commands
    - Add proper error handling and user feedback
    - _Requirements: 2.1, 2.2, 7.1_

  - [ ] 5.2 Implement Nostr event handling system

    - Create system to process GameEventQueue events
    - Handle Challenge, ChallengeAccept, Move, and Final events
    - Update game state components based on received events
    - Add event validation and fraud detection
    - _Requirements: 3.1, 3.2, 4.1_

  - [ ] 5.3 Implement token minting and management system

    - Create system to handle MintTokenRequest components
    - Integrate with CDK mint for Game token creation
    - Generate hash commitments for token privacy
    - Update player balances and token ownership
    - _Requirements: 2.1, 2.2, 6.2_

  - [ ] 5.4 Implement game validation and reward system
    - Create system to validate completed game sequences
    - Determine winners using CardGame trait implementation
    - Mint P2PK locked Reward tokens for winners
    - Publish Reward events to Nostr for transparency
    - _Requirements: 5.1, 5.2, 11.3, 11.4_

- [ ] 6. Create embedded Cashu mint integration

  - [ ] 6.1 Set up CDK mint with derived keys

    - Initialize CDK mint instance using derived mint keys
    - Configure mint for Game and Reward token types
    - Implement NUT-11 P2PK locking for reward tokens
    - Add mint startup and configuration validation
    - _Requirements: 1.1, 6.1, 6.2_

  - [ ] 6.2 Implement mint operations as ECS systems
    - Create systems for minting, validating, and melting tokens
    - Add game sequence processing and validation
    - Implement automatic reward distribution for winners
    - Add mint status monitoring and error handling
    - _Requirements: 6.3, 6.4, 11.1, 11.2_

- [ ] 7. Create embedded Nostr relay

  - [ ] 7.1 Implement basic in-memory Nostr relay

    - Create WebSocket server for Nostr protocol (NIP-01)
    - Implement event storage and retrieval
    - Add subscription handling and event broadcasting
    - Support game event kinds (9259-9263)
    - _Requirements: 1.1, 1.4_

  - [ ] 7.2 Integrate relay with ECS event processing
    - Connect relay to GameEventQueue for event injection
    - Add event filtering for game-related events
    - Implement proper event validation and signature checking
    - Add relay status monitoring and connection management
    - _Requirements: 1.4, 3.1_

- [ ] 8. Implement REPL interface

  - [ ] 8.1 Create interactive command-line interface

    - Set up rustyline for command input and history
    - Implement command parsing for all game operations
    - Add colored output for better user experience
    - Create help system with command documentation
    - _Requirements: 1.5, 2.1, 6.1_

  - [ ] 8.2 Add game status display and feedback

    - Implement real-time status updates from ECS world
    - Show current games, challenges, and token balances
    - Display player and mint public keys
    - Add progress indicators for ongoing operations
    - _Requirements: 6.1, 6.2, 7.1_

  - [ ] 8.3 Integrate REPL with Bevy ECS world
    - Create methods to inject commands into ECS world
    - Add status polling from ECS resources
    - Implement proper error handling and user feedback
    - Add graceful shutdown and state persistence
    - _Requirements: 1.5, 7.2, 12.5_

- [ ] 9. Create main application and thread coordination

  - [ ] 9.1 Implement GameApp with Bevy ECS integration

    - Set up Bevy App with minimal plugins (no rendering)
    - Add all ECS systems with proper scheduling and ordering
    - Implement async resource initialization
    - Add startup systems for mint and client setup
    - _Requirements: 1.1, 1.4, 10.1_

  - [ ] 9.2 Create main application entry point

    - Implement CLI argument parsing with clap
    - Set up logging and configuration management
    - Launch REPL thread and ECS world concurrently
    - Add proper error handling and graceful shutdown
    - _Requirements: 1.5, 12.1, 12.2_

  - [ ] 9.3 Add embedded relay thread management
    - Launch embedded Nostr relay in separate thread
    - Connect relay to ECS world through event queues
    - Add relay health monitoring and restart capability
    - Implement proper thread synchronization and cleanup
    - _Requirements: 1.1, 1.4, 9.1_

- [ ] 10. Implement configuration and persistence

  - [ ] 10.1 Create configuration management system

    - Define GameConfig structure with all settings
    - Support configuration via CLI args, env vars, and files
    - Add validation for configuration parameters
    - Implement configuration file loading and saving
    - _Requirements: 12.1, 12.2, 12.3_

  - [ ] 10.2 Add game state persistence
    - Implement optional game state saving between sessions
    - Add recovery mechanisms for interrupted games
    - Create backup and restore functionality for master seed
    - Add data migration support for future versions
    - _Requirements: 12.4, 12.5_

- [ ] 11. Add comprehensive error handling and resilience

  - [ ] 11.1 Implement network error handling

    - Add automatic reconnection for Nostr client with exponential backoff
    - Handle mint unavailability with operation queuing
    - Implement retry logic for failed event publishing
    - Add timeout handling for network operations
    - _Requirements: 9.1, 9.2, 9.3_

  - [ ] 11.2 Add game timeout and forfeit handling
    - Implement timeout detection for player moves
    - Add automatic forfeit processing for unresponsive players
    - Handle edge cases like simultaneous timeouts
    - Add grace periods and timeout configuration
    - _Requirements: 9.4, 9.5_

- [ ] 12. Create comprehensive test suite

  - [ ] 12.1 Write unit tests for core components

    - Test master key derivation and consistency
    - Test card game logic and C value conversion
    - Test ECS component creation and manipulation
    - Test REPL command parsing and validation
    - _Requirements: 8.1, 8.2, 8.3_

  - [ ] 12.2 Create integration tests for full game flows
    - Test complete game from challenge to reward distribution
    - Test multi-threaded communication between REPL and ECS
    - Test embedded mint and relay functionality
    - Test error recovery and timeout scenarios
    - _Requirements: 8.4, 8.5, 8.6_

- [ ] 13. Create documentation and examples

  - [ ] 13.1 Write comprehensive README and usage guide

    - Document installation and setup procedures
    - Provide step-by-step game playing instructions
    - Add troubleshooting guide for common issues
    - Include configuration examples and best practices
    - _Requirements: 8.1, 8.2_

  - [ ] 13.2 Create developer documentation
    - Document ECS architecture and system interactions
    - Provide examples of extending the game with new features
    - Document the Kirk protocol integration patterns
    - Add API documentation for key components
    - _Requirements: 8.3, 8.4, 8.5_

- [ ] 14. Final integration and polish

  - [ ] 14.1 Perform end-to-end testing and bug fixes

    - Test all REPL commands and game scenarios
    - Validate proper error messages and user feedback
    - Test configuration persistence and recovery
    - Fix any remaining bugs and edge cases
    - _Requirements: 6.1, 6.2, 7.1, 7.2_

  - [ ] 14.2 Add final polish and user experience improvements
    - Improve REPL output formatting and colors
    - Add progress indicators for long-running operations
    - Optimize performance for responsive user interaction
    - Add final validation of all requirements
    - _Requirements: 6.1, 6.2, 12.1, 12.2_

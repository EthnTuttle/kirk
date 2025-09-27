# Requirements Document

## Introduction

This document outlines the requirements for a CLI-based card game that demonstrates the Kirk gaming protocol capabilities. The game will be a simple two-player card game (similar to War or High Card) where players use Cashu ecash tokens to commit to their cards, play through Nostr events, and receive rewards based on game outcomes. This serves as both a practical demonstration of the Kirk protocol and a reference implementation for other game developers.

## Requirements

### Requirement 1

**User Story:** As a player, I want to run a CLI application with embedded mint and relay services, so that I can participate in trustless card games without external dependencies.

#### Acceptance Criteria

1. WHEN the CLI application starts THEN it SHALL launch an embedded Cashu mint and Nostr relay in separate threads within the same process
2. WHEN the application initializes THEN it SHALL generate or load Nostr keys for the player identity
3. WHEN the application starts THEN it SHALL initialize a CDK wallet that connects to the embedded mint
4. WHEN the embedded services start THEN they SHALL run independently and be sufficiently decoupled as separate executables would be
5. WHEN the application runs THEN it SHALL provide a REPL (Read-Eval-Print Loop) interface for issuing commands and viewing information

### Requirement 2

**User Story:** As a player, I want to create or join card game challenges through REPL commands, so that I can find opponents and start games.

#### Acceptance Criteria

1. WHEN I issue a challenge command THEN the REPL SHALL allow me to specify the number of Game tokens to wager
2. WHEN I create a challenge THEN the system SHALL mint the required Game tokens from the embedded Cashu mint
3. WHEN I create a challenge THEN the system SHALL publish a Challenge event to the embedded Nostr relay with my token commitments
4. WHEN I issue a list command THEN the REPL SHALL show me available challenges from other players
5. WHEN I accept a challenge THEN the system SHALL mint matching Game tokens and publish a ChallengeAccept event
6. WHEN challenges expire THEN the system SHALL handle cleanup and token recovery appropriately

### Requirement 3

**User Story:** As a player, I want to play a simple card game where my card is derived from my Cashu token, so that the game outcome is cryptographically verifiable.

#### Acceptance Criteria

1. WHEN the game starts THEN each player SHALL have their card value derived from their Game token's C value
2. WHEN C values are processed THEN they SHALL be converted to standard playing card values (Ace through King, with suits)
3. WHEN both players have committed THEN the game SHALL use a simple comparison rule (highest card wins, with tie-breaking rules)
4. WHEN players make moves THEN they SHALL reveal their Game tokens through Move events
5. WHEN all moves are complete THEN players SHALL publish Final events to complete the game sequence
6. WHEN the game is complete THEN the winner SHALL be determined by comparing the revealed card values

### Requirement 4

**User Story:** As a player, I want the CLI to handle the commit-and-reveal mechanics automatically, so that I don't need to understand the cryptographic details.

#### Acceptance Criteria

1. WHEN I participate in a game THEN the CLI SHALL automatically create hash commitments for my Game tokens
2. WHEN it's time to reveal THEN the CLI SHALL automatically publish my tokens in Move events
3. WHEN the game progresses THEN the CLI SHALL show me the current game state and what actions are needed
4. WHEN moves are made THEN the CLI SHALL validate that revealed tokens match the original commitments
5. WHEN commitment validation fails THEN the CLI SHALL clearly indicate cheating has been detected

### Requirement 5

**User Story:** As a player, I want to receive rewards when I win games, so that successful gameplay is incentivized.

#### Acceptance Criteria

1. WHEN I win a game THEN the mint SHALL issue Reward tokens locked to my Nostr public key using NUT-11 P2PK
2. WHEN Reward tokens are issued THEN the mint SHALL publish a Reward event to Nostr
3. WHEN I receive Reward tokens THEN the CLI SHALL show my reward balance and allow me to manage these tokens
4. WHEN I want to use Reward tokens THEN the CLI SHALL support unlocking them for general use
5. WHEN I lose a game THEN my Game tokens SHALL be forfeited to the winner as part of the reward calculation

### Requirement 6

**User Story:** As a player, I want clear feedback about game progress and outcomes, so that I understand what's happening during gameplay.

#### Acceptance Criteria

1. WHEN the game is in progress THEN the CLI SHALL display the current game state, including my card and opponent information
2. WHEN events are published THEN the CLI SHALL show confirmation that events were successfully sent to Nostr
3. WHEN I win or lose THEN the CLI SHALL clearly display the game outcome and final card values
4. WHEN errors occur THEN the CLI SHALL provide helpful error messages and suggested actions
5. WHEN waiting for opponent actions THEN the CLI SHALL show appropriate waiting messages and timeouts

### Requirement 7

**User Story:** As a player, I want to manage my tokens and view my game history, so that I can track my gameplay and token balances.

#### Acceptance Criteria

1. WHEN I start the CLI THEN it SHALL show my current Game token and Reward token balances
2. WHEN I want to mint more tokens THEN the CLI SHALL provide commands to mint additional Game tokens
3. WHEN I want to view history THEN the CLI SHALL show my recent games, outcomes, and token transactions
4. WHEN I have Reward tokens THEN the CLI SHALL show options to unlock, swap, or melt them
5. WHEN I want to exit THEN the CLI SHALL safely save any pending state and close connections

### Requirement 8

**User Story:** As a developer studying the Kirk protocol, I want the CLI game to demonstrate key protocol features, so that I can understand how to implement my own games.

#### Acceptance Criteria

1. WHEN examining the code THEN it SHALL demonstrate proper implementation of the Game trait for card games
2. WHEN reviewing the implementation THEN it SHALL show how to decode C values into game pieces (cards)
3. WHEN studying the validation logic THEN it SHALL demonstrate sequence validation and winner determination
4. WHEN looking at client usage THEN it SHALL show proper use of PlayerClient for all game operations
5. WHEN examining error handling THEN it SHALL demonstrate proper handling of all Kirk protocol error types
6. WHEN reviewing the architecture THEN it SHALL serve as a reference implementation for other card-based games

### Requirement 9

**User Story:** As a player, I want the game to handle network issues and timeouts gracefully, so that temporary connectivity problems don't ruin my gaming experience.

#### Acceptance Criteria

1. WHEN network connections fail THEN the CLI SHALL attempt to reconnect automatically with exponential backoff
2. WHEN Nostr events fail to publish THEN the CLI SHALL retry publishing with appropriate delays
3. WHEN opponents don't respond within reasonable timeouts THEN the CLI SHALL handle forfeit scenarios
4. WHEN the mint is temporarily unavailable THEN the CLI SHALL queue operations and retry when possible
5. WHEN connectivity is restored THEN the CLI SHALL resume normal operations and sync any missed events

### Requirement 10

**User Story:** As a system architect, I want the embedded mint and game validator to communicate through a well-defined channel interface, so that the system maintains proper separation of concerns.

#### Acceptance Criteria

1. WHEN the game validator processes Nostr events THEN it SHALL communicate with the mint through an async MPSC channel
2. WHEN the MPSC channel is designed THEN it SHALL replicate the CDK mint public web API for consistency
3. WHEN the validator needs mint operations THEN it SHALL send requests through the channel that mirror HTTP API calls
4. WHEN the mint receives channel requests THEN it SHALL process them using the same logic as web API endpoints
5. WHEN the validator and mint communicate THEN they SHALL maintain independence as if they were separate executables

### Requirement 11

**User Story:** As a game authority, I want the embedded mint to act as a Nostr-enabled game validator, so that it can automatically process game sequences and distribute rewards.

#### Acceptance Criteria

1. WHEN the mint starts THEN it SHALL connect to the embedded Nostr relay as a client
2. WHEN game events are published THEN the mint SHALL subscribe to and process Challenge, Move, and Final events
3. WHEN game sequences are complete THEN the mint SHALL validate the entire sequence and determine winners
4. WHEN validation succeeds THEN the mint SHALL automatically mint Reward tokens for winners using NUT-11 P2PK
5. WHEN rewards are distributed THEN the mint SHALL publish Reward events to Nostr with the locked tokens

### Requirement 12

**User Story:** As a player, I want to configure game parameters and preferences, so that I can customize my gaming experience.

#### Acceptance Criteria

1. WHEN I want to set preferences THEN the CLI SHALL support configuration of default wager amounts
2. WHEN I configure the application THEN it SHALL allow me to set embedded service parameters
3. WHEN I want to customize gameplay THEN the CLI SHALL support setting timeout preferences for moves
4. WHEN I use the application regularly THEN it SHALL remember my preferences across sessions
5. WHEN I want to reset settings THEN the CLI SHALL provide options to restore default configurations
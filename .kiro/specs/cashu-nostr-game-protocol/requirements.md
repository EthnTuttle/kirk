# Requirements Document

## Introduction

This document outlines the requirements for a game library that combines Cashu ecash tokens with nostr events to create a trustless, cryptographically-secured gaming protocol. The system enables players to commit game pieces (derived from Cashu token C values) through nostr events, play sequential moves, and receive rewards validated by a Cashu mint acting as game authority.

## Requirements

### Requirement 1

**User Story:** As a game developer, I want to integrate Cashu CDK and nostr libraries, so that I can build games using cryptographic commitments and decentralized event coordination.

#### Acceptance Criteria

1. WHEN the library is initialized THEN it SHALL integrate the latest versions of cashubtc/cdk and rust-nostr/nostr libraries
2. WHEN existing functionality is available in these libraries THEN the system SHALL use the existing implementation rather than creating new code
3. IF modifications to these libraries are needed THEN the system SHALL implement them as branched submodules within the project
4. WHEN game pieces need to be decoded from tokens THEN the system SHALL use Rust traits to define game-specific decoding logic

### Requirement 2

**User Story:** As a player, I want to create and accept game challenges using hash commitments, so that I can initiate games with cryptographic proof of my game pieces.

#### Acceptance Criteria

1. WHEN a player creates a challenge THEN the system SHALL publish a Challenge nostr event containing one or more hash commitments of their Game Tokens
2. WHEN another player wants to accept a challenge THEN they SHALL reply with a ChallengeAccept event containing their own hash commitment(s) of Game Tokens
3. WHEN committing to a single Game Token THEN players SHALL publish a hash of that token's data
4. WHEN committing to multiple Game Tokens THEN players SHALL publish a single hash encompassing all tokens that MUST be used in the game
5. WHEN games allow optional token usage THEN players MAY publish several separate hash commitments, with no requirement to burn all committed tokens
6. WHEN players need Game tokens THEN they SHALL mint Game-type ecash tokens normally through the Cashu mint

### Requirement 3

**User Story:** As a player, I want to make sequential moves in a game, so that gameplay progresses in a verifiable chain of events.

#### Acceptance Criteria

1. WHEN players begin gameplay THEN they SHALL create Move events that follow the sequence: Challenge -> ChallengeAccept -> Move
2. WHEN a Move event is created THEN it SHALL reference the nostr event id of the previous event in the sequence
3. WHEN multiple Move events exist THEN they SHALL be sequential and create a verifiable chain of hashes
4. WHEN a Move is made THEN players SHALL reveal the complete Cashu Game Token (including all proof data), making it publicly visible and "burned"
5. WHEN tokens are revealed THEN the mint SHALL be able to validate the complete token data during game sequence verification
6. WHEN the sequence is complete THEN any individual SHALL be able to verify the entire game by accumulating all related nostr events

### Requirement 4

**User Story:** As a player, I want to use commit-and-reveal mechanics during moves, so that I can make strategic decisions without revealing information prematurely.

#### Acceptance Criteria

1. WHEN players need to make simultaneous decisions THEN they SHALL use Move events to publish Commit actions
2. WHEN all players have published Commit events THEN any player SHALL be able to publish Reveal events containing the complete Cashu Game Token
3. WHEN a player fails to Reveal after Commit THEN the other player SHALL be able to publish a FinalForfeit event
4. WHEN a FinalForfeit is published THEN the non-revealing player SHALL forfeit their burned tokens to the other player
5. WHEN game timelines are needed THEN the game implementation SHALL define publishing deadlines

### Requirement 5

**User Story:** As a player, I want to finalize completed games, so that the game sequence can be validated and rewards distributed.

#### Acceptance Criteria

1. WHEN all moves are completed THEN players SHALL publish Final events to indicate sequence completion
2. WHEN Final events are published THEN observers SHALL be able to verify the complete game sequence
3. WHEN a game implementation determines moves are complete THEN it SHALL define the finalization criteria
4. WHEN players committed to multiple tokens with a single hash THEN they MUST publish the method used to arrive at the original commitment (concatenation order or merkle tree structure) in Final events
5. WHEN cheating is detected during validation THEN the cheating player SHALL forfeit their tokens to the honest player

### Requirement 5a

**User Story:** As a validator, I want to verify hash commitments against revealed tokens, so that I can ensure players used the tokens they committed to.

#### Acceptance Criteria

1. WHEN validating single token commitments THEN the system SHALL verify the hash matches the revealed Game Token
2. WHEN validating multi-token commitments THEN the system SHALL use the commitment method published in Final events to reconstruct and verify the original hash
3. WHEN commitment methods are provided THEN they SHALL specify concatenation order or merkle tree structure used for hashing
4. WHEN hash verification fails THEN the system SHALL treat it as cheating and forfeit the player's tokens

### Requirement 6

**User Story:** As a Cashu mint operator, I want to act as game authority and manage two types of ecash, so that I can facilitate gameplay and reward winners.

#### Acceptance Criteria

1. WHEN the mint operates THEN it SHALL issue Game-type ecash for gameplay and Reward-type ecash for winners
2. WHEN players request Game ecash THEN the mint SHALL allow normal minting operations
3. WHEN Game tokens are used THEN they SHALL be valid for gameplay commitments in Challenge or ChallengeAccept events and for burning during Move events
4. WHEN Reward ecash is managed THEN the mint SHALL allow swapping, melting, and unlocking operations to make tokens generally useful
5. WHEN a game sequence is validated THEN the mint SHALL melt the burned Game tokens and mint Reward ecash for the winner
6. WHEN determining reward amounts THEN the specific game implementation SHALL dictate the ratio of Game tokens melted to Reward tokens minted
7. WHEN a game is completed THEN the mint SHALL issue Reward ecash locked to the winning player's nostr public key

### Requirement 7

**User Story:** As a mint operator, I want to validate game sequences and distribute rewards, so that only legitimate game outcomes result in reward distribution.

#### Acceptance Criteria

1. WHEN Final events are published THEN the mint SHALL process and validate the complete game sequence
2. WHEN validating sequences THEN the mint SHALL verify all ecash tokens published during Move events
3. WHEN validation is complete AND no fraud is detected THEN the mint SHALL determine the winning player based on game rules
4. WHEN the mint detects fraud or cheating THEN it SHALL forfeit the player who published the first invalid event
5. WHEN a player is forfeited for non-compliance THEN all their burned Game Tokens SHALL be awarded to the honest player
6. WHEN a legitimate winner is determined THEN the mint SHALL issue Reward ecash initially locked to their nostr public key
7. WHEN Reward ecash is issued THEN the mint SHALL publish a Reward event to nostr containing the locked ecash for the winning player
8. WHEN Reward ecash is distributed THEN it SHALL initially be locked to the winner's npub but MAY be unlocked later for general use
8. WHEN validation fails due to system errors (not cheating) THEN the mint SHALL publish a ValidationFailure event to nostr explaining why validation failed and SHALL not issue any Reward ecash

### Requirement 8

**User Story:** As a game implementer, I want flexible game piece decoding and rule definition, so that I can create different types of games using the same protocol.

#### Acceptance Criteria

1. WHEN implementing a new game THEN the system SHALL require traits for defining game piece decoding from C values
2. WHEN implementing a new game THEN the system SHALL require move validation logic that takes multiple nostr events as input and outputs true or false
3. WHEN implementing a new game THEN the system SHALL require finalization criteria that determine when a game sequence is complete
4. WHEN C values are processed THEN they SHALL provide randomness for players' game pieces as a critical component of the system
5. WHEN timeline management is needed THEN the game implementation MAY define commit/reveal and move deadlines as part of validation logic
6. WHEN Final events are published THEN each game implementation SHALL decide its own requirements for Final events (whether all players or just one player must publish)
7. WHEN multiple games exist THEN each SHALL be able to define unique piece interpretation and rules while using the same protocol
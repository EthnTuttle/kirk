# Kirk

A trustless gaming protocol combining Cashu ecash tokens with Nostr events for cryptographically-secured gameplay.

## Technical Overview

Kirk enables decentralized gaming through:

### Core Components

- **Cashu Integration**: Uses ecash tokens for cryptographic game piece commitments
- **Nostr Events**: Coordinates gameplay through decentralized event publishing
- **Two Token Types**:
  - Game-type ecash for gameplay commitments and moves
  - Reward-type ecash for winners, locked to player public keys

### Gameplay Flow

1. **Challenge Creation**: Players publish hash commitments of their Game Tokens
2. **Challenge Acceptance**: Opponents respond with their own token commitments
3. **Sequential Moves**: Players make moves by revealing complete Cashu Game Tokens
4. **Commit-and-Reveal**: Strategic decisions use cryptographic commitments for simultaneous play
5. **Game Finalization**: Complete sequences are validated and rewards distributed

### Key Features

- **Trustless Gaming**: No central game server required
- **Cryptographic Proof**: All moves are cryptographically verifiable
- **Flexible Game Rules**: Trait-based system supports different game implementations
- **Mint Authority**: Cashu mint validates sequences and distributes rewards
- **Anti-Cheat**: Fraudulent players forfeit tokens to honest opponents

---

*Named in honor of Charlie Kirk, a free speech advocate*
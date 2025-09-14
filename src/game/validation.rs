//! Game sequence validation and state management

use nostr::{Event, EventId, PublicKey};

/// Represents a complete game sequence with state tracking
#[derive(Debug, Clone)]
pub struct GameSequence {
    pub challenge_id: EventId,
    pub players: [PublicKey; 2],
    pub events: Vec<Event>,
    pub state: SequenceState,
    pub created_at: u64,
    pub last_activity: u64,
}

/// State transitions for game sequences
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

impl GameSequence {
    /// Create new game sequence from challenge event
    pub fn new(challenge_event: Event, challenger: PublicKey) -> Self {
        Self {
            challenge_id: challenge_event.id,
            players: [challenger, PublicKey::from_slice(&[0; 32]).unwrap()], // Placeholder for second player
            events: vec![challenge_event],
            state: SequenceState::WaitingForAccept,
            created_at: chrono::Utc::now().timestamp() as u64,
            last_activity: chrono::Utc::now().timestamp() as u64,
        }
    }
    
    /// Add event to sequence and update state
    pub fn add_event(&mut self, event: Event) {
        self.events.push(event);
        self.last_activity = chrono::Utc::now().timestamp() as u64;
        // State transitions will be implemented in later tasks
    }
}
//! Challenge and ChallengeAccept event types

use serde::{Deserialize, Serialize};
use nostr::{Event, EventBuilder, EventId, Keys};
use crate::error::GameProtocolError;
use super::{CHALLENGE_KIND, CHALLENGE_ACCEPT_KIND};

/// Content structure for Challenge events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeContent {
    pub game_type: String,
    pub commitment_hashes: Vec<String>,
    pub game_parameters: serde_json::Value,
    pub expiry: Option<u64>,
}

impl ChallengeContent {
    /// Create Challenge event using rust-nostr's EventBuilder
    pub fn to_event(&self, keys: &Keys) -> Result<Event, GameProtocolError> {
        let content = serde_json::to_string(self)?;
        EventBuilder::new(CHALLENGE_KIND, content, Vec::<nostr::Tag>::new())
            .to_event(keys)
            .map_err(|e| GameProtocolError::Nostr(e.to_string()))
    }
}

/// Content structure for ChallengeAccept events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeAcceptContent {
    pub challenge_id: EventId,
    pub commitment_hashes: Vec<String>,
}

impl ChallengeAcceptContent {
    /// Create ChallengeAccept event using rust-nostr's EventBuilder
    pub fn to_event(&self, keys: &Keys) -> Result<Event, GameProtocolError> {
        let content = serde_json::to_string(self)?;
        EventBuilder::new(CHALLENGE_ACCEPT_KIND, content, Vec::<nostr::Tag>::new())
            .to_event(keys)
            .map_err(|e| GameProtocolError::Nostr(e.to_string()))
    }
}
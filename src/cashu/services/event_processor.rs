//! Event processing service for parsing and validating game events

use nostr::Event;
use tracing::{debug, warn, info};
use crate::error::{GameResult, GameProtocolError};
use crate::events::{EventParser, ChallengeContent, ChallengeAcceptContent, MoveContent, FinalContent};
use super::ServiceContext;

/// Parsed event content with type information
#[derive(Debug, Clone)]
pub enum ParsedEvent {
    Challenge {
        content: ChallengeContent,
        author: nostr::PublicKey,
    },
    ChallengeAccept {
        content: ChallengeAcceptContent,
        author: nostr::PublicKey,
    },
    Move {
        content: MoveContent,
        author: nostr::PublicKey,
    },
    Final {
        content: FinalContent,
        author: nostr::PublicKey,
    },
    Unknown {
        kind: nostr::Kind,
        author: nostr::PublicKey,
    },
}

impl ParsedEvent {
    /// Get the author of this event
    pub fn author(&self) -> nostr::PublicKey {
        match self {
            ParsedEvent::Challenge { author, .. } => *author,
            ParsedEvent::ChallengeAccept { author, .. } => *author,
            ParsedEvent::Move { author, .. } => *author,
            ParsedEvent::Final { author, .. } => *author,
            ParsedEvent::Unknown { author, .. } => *author,
        }
    }

    /// Get a description of the event type for logging
    pub fn event_type(&self) -> &'static str {
        match self {
            ParsedEvent::Challenge { .. } => "Challenge",
            ParsedEvent::ChallengeAccept { .. } => "ChallengeAccept",
            ParsedEvent::Move { .. } => "Move",
            ParsedEvent::Final { .. } => "Final",
            ParsedEvent::Unknown { .. } => "Unknown",
        }
    }

    /// Check if this is a valid game event
    pub fn is_game_event(&self) -> bool {
        !matches!(self, ParsedEvent::Unknown { .. })
    }
}

/// Service responsible for parsing and validating events
#[derive(Debug)]
pub struct EventProcessor {
    context: ServiceContext,
}

impl EventProcessor {
    /// Create a new event processor
    pub fn new(context: ServiceContext) -> Self {
        Self {
            context,
        }
    }

    /// Parse and validate an event
    pub async fn parse_event(&self, event: &Event) -> GameResult<ParsedEvent> {
        debug!(
            event_id = %event.id,
            author = %event.pubkey,
            kind = %event.kind,
            "Parsing event"
        );

        // Validate basic event structure
        self.validate_event_structure(event)?;

        // Parse content based on event kind
        let parsed = match event.kind.as_u16() {
            9259 => { // CHALLENGE_KIND
                let content = EventParser::parse_challenge(&event)
                    .map_err(|e| {
                        warn!(
                            event_id = %event.id,
                            error = %e,
                            "Failed to parse challenge content"
                        );
                        e
                    })?;

                // Validate challenge content
                self.validate_challenge_content(&content)?;

                ParsedEvent::Challenge {
                    content,
                    author: event.pubkey,
                }
            },
            9260 => { // CHALLENGE_ACCEPT_KIND
                let content = EventParser::parse_challenge_accept(&event)
                    .map_err(|e| {
                        warn!(
                            event_id = %event.id,
                            error = %e,
                            "Failed to parse challenge accept content"
                        );
                        e
                    })?;

                self.validate_challenge_accept_content(&content)?;

                ParsedEvent::ChallengeAccept {
                    content,
                    author: event.pubkey,
                }
            },
            9261 => { // MOVE_KIND
                let content = EventParser::parse_move(&event)
                    .map_err(|e| {
                        warn!(
                            event_id = %event.id,
                            error = %e,
                            "Failed to parse move content"
                        );
                        e
                    })?;

                self.validate_move_content(&content)?;

                ParsedEvent::Move {
                    content,
                    author: event.pubkey,
                }
            },
            9262 => { // FINAL_KIND
                let content = EventParser::parse_final(&event)
                    .map_err(|e| {
                        warn!(
                            event_id = %event.id,
                            error = %e,
                            "Failed to parse final content"
                        );
                        e
                    })?;

                self.validate_final_content(&content)?;

                ParsedEvent::Final {
                    content,
                    author: event.pubkey,
                }
            },
            _ => {
                debug!(
                    event_id = %event.id,
                    kind = %event.kind,
                    "Unknown event kind"
                );

                ParsedEvent::Unknown {
                    kind: event.kind,
                    author: event.pubkey,
                }
            }
        };

        info!(
            event_id = %event.id,
            event_type = parsed.event_type(),
            author = %parsed.author(),
            "Successfully parsed event"
        );

        Ok(parsed)
    }

    /// Validate basic event structure
    fn validate_event_structure(&self, event: &Event) -> GameResult<()> {
        // Check event ID is valid
        if event.id.to_hex().is_empty() {
            return Err(GameProtocolError::Validation {
                message: "Event ID is empty".to_string(),
                field: Some("id".to_string()),
                event_id: None,
            });
        }

        // Check content size limits
        if event.content.len() > self.context.constants.max_batch_size * 1024 {
            return Err(GameProtocolError::Validation {
                message: format!("Event content too large: {} bytes", event.content.len()),
                field: Some("content".to_string()),
                event_id: Some(event.id),
            });
        }

        // Validate timestamp is reasonable (not too far in past/future)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let event_time = event.created_at.as_u64();
        let max_skew = 3600; // 1 hour tolerance

        if event_time < now.saturating_sub(max_skew) || event_time > now + max_skew {
            return Err(GameProtocolError::Validation {
                message: format!("Event timestamp {} outside acceptable range", event_time),
                field: Some("created_at".to_string()),
                event_id: Some(event.id),
            });
        }

        Ok(())
    }

    /// Validate challenge content
    fn validate_challenge_content(&self, content: &ChallengeContent) -> GameResult<()> {
        // Validate basic challenge structure
        if content.game_type.is_empty() {
            return Err(GameProtocolError::Validation {
                message: "Game type cannot be empty".to_string(),
                field: Some("game_type".to_string()),
                event_id: None,
            });
        }

        if content.commitment_hashes.is_empty() {
            return Err(GameProtocolError::Validation {
                message: "At least one commitment hash is required".to_string(),
                field: Some("commitment_hashes".to_string()),
                event_id: None,
            });
        }

        // Validate timeout values
        if let Some(ref timeout_config) = content.timeout_config {
            if let Some(accept_timeout) = timeout_config.accept_timeout {
                if accept_timeout < 60 {
                    return Err(GameProtocolError::Validation {
                        message: "Accept timeout must be at least 60 seconds".to_string(),
                        field: Some("timeout_config.accept_timeout".to_string()),
                        event_id: None,
                    });
                }
            }

            if let Some(move_timeout) = timeout_config.move_timeout {
                if move_timeout < 60 {
                    return Err(GameProtocolError::Validation {
                        message: "Move timeout must be at least 60 seconds".to_string(),
                        field: Some("timeout_config.move_timeout".to_string()),
                        event_id: None,
                    });
                }
            }
        }

        Ok(())
    }

    /// Validate challenge accept content
    fn validate_challenge_accept_content(&self, _content: &ChallengeAcceptContent) -> GameResult<()> {
        // Basic validation - challenge accepts are simple
        Ok(())
    }

    /// Validate move content
    fn validate_move_content(&self, content: &MoveContent) -> GameResult<()> {
        // Validate deadline if present
        if let Some(deadline) = content.deadline {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            if deadline <= now {
                return Err(GameProtocolError::Validation {
                    message: format!("Move deadline {} has already passed", deadline),
                    field: Some("deadline".to_string()),
                    event_id: None,
                });
            }

            // Validate deadline is not too far in the future
            if deadline > now + 86400 { // 24 hours max
                return Err(GameProtocolError::Validation {
                    message: format!("Move deadline {} too far in the future", deadline),
                    field: Some("deadline".to_string()),
                    event_id: None,
                });
            }
        }

        Ok(())
    }

    /// Validate final content
    fn validate_final_content(&self, _content: &FinalContent) -> GameResult<()> {
        // Basic validation for final events
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{Keys, EventBuilder, Kind, Timestamp};
    use crate::cashu::GameMint;
    use std::sync::Arc;

    fn create_test_context() -> ServiceContext {
        let keys = Keys::generate();
        let mint = Arc::new(GameMint::new_test(keys));
        let nostr_client = nostr_sdk::Client::default();
        ServiceContext::new(mint, nostr_client)
    }

    #[tokio::test]
    async fn test_parse_valid_event() {
        let processor = EventProcessor::new(create_test_context());

        let keys = Keys::generate();
        let event = EventBuilder::new(
            Kind::from(crate::events::CHALLENGE_KIND),
            r#"{"game_type": "test", "stakes": []}"#,
            []
        ).to_event(&keys).unwrap();

        let result = processor.parse_event(&event).await;
        assert!(result.is_ok());

        let parsed = result.unwrap();
        assert!(parsed.is_game_event());
        assert_eq!(parsed.event_type(), "Challenge");
    }

    #[tokio::test]
    async fn test_parse_unknown_event() {
        let processor = EventProcessor::new(create_test_context());

        let keys = Keys::generate();
        let event = EventBuilder::new(
            Kind::from(9999u16), // Unknown kind
            "test content",
            []
        ).to_event(&keys).unwrap();

        let result = processor.parse_event(&event).await;
        assert!(result.is_ok());

        let parsed = result.unwrap();
        assert!(!parsed.is_game_event());
        assert_eq!(parsed.event_type(), "Unknown");
    }

    #[tokio::test]
    async fn test_validate_timestamp_limits() {
        let processor = EventProcessor::new(create_test_context());

        let keys = Keys::generate();

        // Create event with future timestamp
        let future_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() + 7200; // 2 hours in future

        let mut event = EventBuilder::new(
            Kind::from(crate::events::CHALLENGE_KIND),
            r#"{"game_type": "test"}"#,
            []
        ).to_event(&keys).unwrap();

        // Manually set future timestamp (this is normally not recommended)
        event.created_at = Timestamp::from(future_time);

        let result = processor.parse_event(&event).await;
        assert!(result.is_err());
    }
}
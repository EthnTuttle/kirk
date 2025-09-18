//! Mock Nostr relay for testing

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use nostr::{Event, EventId, Filter, PublicKey, Kind};
use tokio::sync::mpsc;

/// Mock Nostr relay that stores events in memory
#[derive(Debug, Clone)]
pub struct MockNostrRelay {
    events: Arc<Mutex<HashMap<EventId, Event>>>,
    subscriptions: Arc<Mutex<HashMap<String, Filter>>>,
}

impl MockNostrRelay {
    /// Create a new mock relay
    pub fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(HashMap::new())),
            subscriptions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Store an event in the relay
    pub fn store_event(&self, event: Event) -> Result<(), Box<dyn std::error::Error>> {
        let mut events = self.events.lock().unwrap();
        events.insert(event.id, event);
        Ok(())
    }

    /// Query events by filter
    pub fn query_events(&self, filter: &Filter) -> Vec<Event> {
        let events = self.events.lock().unwrap();
        events.values()
            .filter(|event| self.matches_filter(event, filter))
            .cloned()
            .collect()
    }

    /// Get event by ID
    pub fn get_event(&self, event_id: &EventId) -> Option<Event> {
        let events = self.events.lock().unwrap();
        events.get(event_id).cloned()
    }

    /// Get all events by author
    pub fn get_events_by_author(&self, pubkey: &PublicKey) -> Vec<Event> {
        let events = self.events.lock().unwrap();
        events.values()
            .filter(|event| event.pubkey == *pubkey)
            .cloned()
            .collect()
    }

    /// Get events by kind
    pub fn get_events_by_kind(&self, kind: Kind) -> Vec<Event> {
        let events = self.events.lock().unwrap();
        events.values()
            .filter(|event| event.kind == kind)
            .cloned()
            .collect()
    }

    /// Clear all stored events
    pub fn clear(&self) {
        let mut events = self.events.lock().unwrap();
        events.clear();
    }

    /// Get total number of stored events
    pub fn event_count(&self) -> usize {
        let events = self.events.lock().unwrap();
        events.len()
    }

    /// Check if an event matches a filter
    fn matches_filter(&self, event: &Event, filter: &Filter) -> bool {
        // Check kinds
        if let Some(kinds) = &filter.kinds {
            if !kinds.contains(&event.kind) {
                return false;
            }
        }

        // Check authors
        if let Some(authors) = &filter.authors {
            if !authors.contains(&event.pubkey) {
                return false;
            }
        }

        // Check since timestamp
        if let Some(since) = filter.since {
            if event.created_at.as_u64() < since.as_u64() {
                return false;
            }
        }

        // Check until timestamp
        if let Some(until) = filter.until {
            if event.created_at.as_u64() > until.as_u64() {
                return false;
            }
        }

        true
    }
}

impl Default for MockNostrRelay {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Timestamp};

    #[test]
    fn test_mock_relay_basic_operations() {
        let relay = MockNostrRelay::new();
        let keys = Keys::generate();

        // Create a test event
        let event = EventBuilder::new(Kind::TextNote, "test content", Vec::<nostr::Tag>::new())
            .to_event(&keys)
            .unwrap();

        // Store the event
        relay.store_event(event.clone()).unwrap();

        // Verify it was stored
        assert_eq!(relay.event_count(), 1);

        // Retrieve by ID
        let retrieved = relay.get_event(&event.id).unwrap();
        assert_eq!(retrieved.id, event.id);
        assert_eq!(retrieved.content, "test content");

        // Clear and verify
        relay.clear();
        assert_eq!(relay.event_count(), 0);
    }

    #[test]
    fn test_query_by_author() {
        let relay = MockNostrRelay::new();
        let keys1 = Keys::generate();
        let keys2 = Keys::generate();

        // Create events from different authors
        let event1 = EventBuilder::new(Kind::TextNote, "content1", Vec::<nostr::Tag>::new())
            .to_event(&keys1)
            .unwrap();
        let event2 = EventBuilder::new(Kind::TextNote, "content2", Vec::<nostr::Tag>::new())
            .to_event(&keys2)
            .unwrap();

        relay.store_event(event1.clone()).unwrap();
        relay.store_event(event2.clone()).unwrap();

        // Query by author
        let events_by_author1 = relay.get_events_by_author(&keys1.public_key());
        assert_eq!(events_by_author1.len(), 1);
        assert_eq!(events_by_author1[0].id, event1.id);

        let events_by_author2 = relay.get_events_by_author(&keys2.public_key());
        assert_eq!(events_by_author2.len(), 1);
        assert_eq!(events_by_author2[0].id, event2.id);
    }

    #[test]
    fn test_query_by_kind() {
        let relay = MockNostrRelay::new();
        let keys = Keys::generate();

        // Create events of different kinds
        let text_event = EventBuilder::new(Kind::TextNote, "text", Vec::<nostr::Tag>::new())
            .to_event(&keys)
            .unwrap();
        let metadata_event = EventBuilder::new(Kind::Metadata, "metadata", Vec::<nostr::Tag>::new())
            .to_event(&keys)
            .unwrap();

        relay.store_event(text_event.clone()).unwrap();
        relay.store_event(metadata_event.clone()).unwrap();

        // Query by kind
        let text_events = relay.get_events_by_kind(Kind::TextNote);
        assert_eq!(text_events.len(), 1);
        assert_eq!(text_events[0].id, text_event.id);

        let metadata_events = relay.get_events_by_kind(Kind::Metadata);
        assert_eq!(metadata_events.len(), 1);
        assert_eq!(metadata_events[0].id, metadata_event.id);
    }
}
//! Optimized data structures for high-performance lookups and caching

use std::collections::{HashMap, BTreeMap, VecDeque};
use std::hash::Hash;
use std::time::{SystemTime, Duration};
use nostr::{Event, EventId, PublicKey};
use crate::error::GameResult;
use crate::game::GameSequence;

/// Fast lookup table with O(1) access and LRU eviction
pub struct FastLookupTable<K, V> {
    data: HashMap<K, (V, SystemTime)>,
    access_order: VecDeque<K>,
    max_size: usize,
    ttl: Duration,
    hits: u64,
    misses: u64,
}

impl<K, V> FastLookupTable<K, V>
where
    K: Hash + Eq + Clone,
    V: Clone,
{
    /// Create a new fast lookup table
    pub fn new(max_size: usize, ttl: Duration) -> Self {
        Self {
            data: HashMap::new(),
            access_order: VecDeque::new(),
            max_size,
            ttl,
            hits: 0,
            misses: 0,
        }
    }

    /// Insert a value into the table
    pub fn insert(&mut self, key: K, value: V) {
        let now = SystemTime::now();

        // Remove old entry if it exists
        if self.data.contains_key(&key) {
            self.remove_from_access_order(&key);
        }

        // Check if we need to evict an entry
        if self.data.len() >= self.max_size && !self.data.contains_key(&key) {
            self.evict_lru();
        }

        // Insert the new entry
        self.data.insert(key.clone(), (value, now));
        self.access_order.push_back(key);
    }

    /// Get a value from the table
    pub fn get(&mut self, key: &K) -> Option<V> {
        let now = SystemTime::now();

        // Check if the entry exists and is not expired
        let result = if let Some((value, timestamp)) = self.data.get(key) {
            if now.duration_since(*timestamp).unwrap_or(Duration::MAX) <= self.ttl {
                Some((value.clone(), false)) // (value, is_expired)
            } else {
                Some((value.clone(), true)) // Mark as expired
            }
        } else {
            None
        };

        match result {
            Some((value, false)) => {
                // Valid entry - update access order
                self.remove_from_access_order(key);
                self.access_order.push_back(key.clone());
                self.hits += 1;
                Some(value)
            }
            Some((_, true)) => {
                // Expired entry - remove it
                self.data.remove(key);
                self.remove_from_access_order(key);
                self.misses += 1;
                None
            }
            None => {
                // Not found
                self.misses += 1;
                None
            }
        }
    }

    /// Remove a key from the access order queue
    fn remove_from_access_order(&mut self, key: &K) {
        if let Some(pos) = self.access_order.iter().position(|k| k == key) {
            self.access_order.remove(pos);
        }
    }

    /// Evict the least recently used entry
    fn evict_lru(&mut self) {
        if let Some(lru_key) = self.access_order.pop_front() {
            self.data.remove(&lru_key);
        }
    }

    /// Get cache hit rate
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    /// Clear expired entries
    pub fn cleanup_expired(&mut self) -> usize {
        let now = SystemTime::now();
        let mut expired_keys = Vec::new();

        for (key, (_, timestamp)) in &self.data {
            if now.duration_since(*timestamp).unwrap_or(Duration::MAX) > self.ttl {
                expired_keys.push(key.clone());
            }
        }

        for key in &expired_keys {
            self.data.remove(key);
            self.remove_from_access_order(key);
        }

        expired_keys.len()
    }

    /// Get current size
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// Indexed store for game sequences with multiple access patterns
pub struct IndexedSequenceStore {
    /// Primary storage by sequence ID
    sequences: HashMap<EventId, GameSequence>,

    /// Index by challenger (for finding player's games)
    challenger_index: HashMap<PublicKey, Vec<EventId>>,

    /// Index by accepter (for finding accepted games)
    accepter_index: HashMap<PublicKey, Vec<EventId>>,

    /// Index by game state (for efficient filtering)
    state_index: BTreeMap<String, Vec<EventId>>,

    /// Time-based index for cleanup (sorted by creation time)
    time_index: BTreeMap<u64, Vec<EventId>>,

    /// Cache for frequently accessed sequences
    cache: FastLookupTable<EventId, GameSequence>,
}

impl IndexedSequenceStore {
    /// Create a new indexed sequence store
    pub fn new(cache_size: usize, cache_ttl: Duration) -> Self {
        Self {
            sequences: HashMap::new(),
            challenger_index: HashMap::new(),
            accepter_index: HashMap::new(),
            state_index: BTreeMap::new(),
            time_index: BTreeMap::new(),
            cache: FastLookupTable::new(cache_size, cache_ttl),
        }
    }

    /// Insert a sequence into the store
    pub fn insert(&mut self, sequence_id: EventId, sequence: GameSequence) -> GameResult<()> {
        let challenger = sequence.players[0];
        let creation_time = sequence.events.first()
            .map(|e| e.created_at.as_u64())
            .unwrap_or(0);

        // Insert into primary storage
        self.sequences.insert(sequence_id, sequence.clone());

        // Update challenger index
        self.challenger_index
            .entry(challenger)
            .or_insert_with(Vec::new)
            .push(sequence_id);

        // Update accepter index if there's an accepter
        if sequence.players.len() > 1 {
            let accepter = sequence.players[1];
            self.accepter_index
                .entry(accepter)
                .or_insert_with(Vec::new)
                .push(sequence_id);
        }

        // Update state index
        let state_key = format!("{:?}", sequence.state);
        self.state_index
            .entry(state_key)
            .or_insert_with(Vec::new)
            .push(sequence_id);

        // Update time index
        self.time_index
            .entry(creation_time)
            .or_insert_with(Vec::new)
            .push(sequence_id);

        // Add to cache
        self.cache.insert(sequence_id, sequence);

        Ok(())
    }

    /// Get a sequence by ID (with caching)
    pub fn get(&mut self, sequence_id: &EventId) -> Option<GameSequence> {
        // Try cache first
        if let Some(cached) = self.cache.get(sequence_id) {
            return Some(cached);
        }

        // Fall back to main storage
        if let Some(sequence) = self.sequences.get(sequence_id).cloned() {
            // Add to cache for next time
            self.cache.insert(*sequence_id, sequence.clone());
            Some(sequence)
        } else {
            None
        }
    }

    /// Get sequences by challenger
    pub fn get_by_challenger(&mut self, challenger: &PublicKey) -> Vec<GameSequence> {
        let sequence_ids: Vec<EventId> = self.challenger_index.get(challenger)
            .map(|ids| ids.clone())
            .unwrap_or_default();

        sequence_ids.iter()
            .filter_map(|id| self.get(id))
            .collect()
    }

    /// Get sequences by accepter
    pub fn get_by_accepter(&mut self, accepter: &PublicKey) -> Vec<GameSequence> {
        let sequence_ids: Vec<EventId> = self.accepter_index.get(accepter)
            .map(|ids| ids.clone())
            .unwrap_or_default();

        sequence_ids.iter()
            .filter_map(|id| self.get(id))
            .collect()
    }

    /// Get sequences by state pattern
    pub fn get_by_state_pattern(&mut self, state_pattern: &str) -> Vec<GameSequence> {
        let matching_ids: Vec<EventId> = self.state_index.iter()
            .filter(|(state_key, _)| state_key.contains(state_pattern))
            .flat_map(|(_, sequence_ids)| sequence_ids.clone())
            .collect();

        matching_ids.iter()
            .filter_map(|id| self.get(id))
            .collect()
    }

    /// Get sequences created before a certain time
    pub fn get_before_time(&mut self, before_timestamp: u64) -> Vec<GameSequence> {
        let matching_ids: Vec<EventId> = self.time_index
            .range(..before_timestamp)
            .flat_map(|(_, sequence_ids)| sequence_ids.clone())
            .collect();

        matching_ids.iter()
            .filter_map(|id| self.get(id))
            .collect()
    }

    /// Remove a sequence and update all indices
    pub fn remove(&mut self, sequence_id: &EventId) -> Option<GameSequence> {
        if let Some(sequence) = self.sequences.remove(sequence_id) {
            // Remove from challenger index
            let challenger = sequence.players[0];
            if let Some(challenger_list) = self.challenger_index.get_mut(&challenger) {
                challenger_list.retain(|id| id != sequence_id);
                if challenger_list.is_empty() {
                    self.challenger_index.remove(&challenger);
                }
            }

            // Remove from accepter index
            if sequence.players.len() > 1 {
                let accepter = sequence.players[1];
                if let Some(accepter_list) = self.accepter_index.get_mut(&accepter) {
                    accepter_list.retain(|id| id != sequence_id);
                    if accepter_list.is_empty() {
                        self.accepter_index.remove(&accepter);
                    }
                }
            }

            // Remove from state index
            let state_key = format!("{:?}", sequence.state);
            if let Some(state_list) = self.state_index.get_mut(&state_key) {
                state_list.retain(|id| id != sequence_id);
                if state_list.is_empty() {
                    self.state_index.remove(&state_key);
                }
            }

            // Remove from time index
            let creation_time = sequence.events.first()
                .map(|e| e.created_at.as_u64())
                .unwrap_or(0);
            if let Some(time_list) = self.time_index.get_mut(&creation_time) {
                time_list.retain(|id| id != sequence_id);
                if time_list.is_empty() {
                    self.time_index.remove(&creation_time);
                }
            }

            Some(sequence)
        } else {
            None
        }
    }

    /// Get cache statistics
    pub fn get_cache_stats(&self) -> (usize, f64) {
        (self.cache.len(), self.cache.hit_rate())
    }

    /// Cleanup expired cache entries
    pub fn cleanup_cache(&mut self) -> usize {
        self.cache.cleanup_expired()
    }

    /// Get total number of sequences
    pub fn len(&self) -> usize {
        self.sequences.len()
    }

    /// Check if store is empty
    pub fn is_empty(&self) -> bool {
        self.sequences.is_empty()
    }
}

/// Event index for fast event lookups and queries
pub struct EventIndex {
    /// Events by ID
    events: HashMap<EventId, Event>,

    /// Events by author
    author_index: HashMap<PublicKey, Vec<EventId>>,

    /// Events by kind
    kind_index: HashMap<u16, Vec<EventId>>,

    /// Time-ordered events
    time_index: BTreeMap<u64, Vec<EventId>>,

    /// Cache for recently accessed events
    cache: FastLookupTable<EventId, Event>,
}

impl EventIndex {
    /// Create a new event index
    pub fn new(cache_size: usize, cache_ttl: Duration) -> Self {
        Self {
            events: HashMap::new(),
            author_index: HashMap::new(),
            kind_index: HashMap::new(),
            time_index: BTreeMap::new(),
            cache: FastLookupTable::new(cache_size, cache_ttl),
        }
    }

    /// Add an event to the index
    pub fn add_event(&mut self, event: Event) {
        let event_id = event.id;
        let author = event.pubkey;
        let kind = event.kind.as_u16();
        let timestamp = event.created_at.as_u64();

        // Add to primary storage
        self.events.insert(event_id, event.clone());

        // Update author index
        self.author_index
            .entry(author)
            .or_insert_with(Vec::new)
            .push(event_id);

        // Update kind index
        self.kind_index
            .entry(kind)
            .or_insert_with(Vec::new)
            .push(event_id);

        // Update time index
        self.time_index
            .entry(timestamp)
            .or_insert_with(Vec::new)
            .push(event_id);

        // Add to cache
        self.cache.insert(event_id, event);
    }

    /// Get an event by ID
    pub fn get_event(&mut self, event_id: &EventId) -> Option<Event> {
        // Try cache first
        if let Some(cached) = self.cache.get(event_id) {
            return Some(cached);
        }

        // Fall back to main storage
        if let Some(event) = self.events.get(event_id).cloned() {
            self.cache.insert(*event_id, event.clone());
            Some(event)
        } else {
            None
        }
    }

    /// Get events by author
    pub fn get_events_by_author(&mut self, author: &PublicKey) -> Vec<Event> {
        let event_ids: Vec<EventId> = self.author_index.get(author)
            .map(|ids| ids.clone())
            .unwrap_or_default();

        event_ids.iter()
            .filter_map(|id| self.get_event(id))
            .collect()
    }

    /// Get events by kind
    pub fn get_events_by_kind(&mut self, kind: u16) -> Vec<Event> {
        let event_ids: Vec<EventId> = self.kind_index.get(&kind)
            .map(|ids| ids.clone())
            .unwrap_or_default();

        event_ids.iter()
            .filter_map(|id| self.get_event(id))
            .collect()
    }

    /// Get events in time range
    pub fn get_events_in_range(&mut self, start_time: u64, end_time: u64) -> Vec<Event> {
        let matching_ids: Vec<EventId> = self.time_index
            .range(start_time..=end_time)
            .flat_map(|(_, event_ids)| event_ids.clone())
            .collect();

        matching_ids.iter()
            .filter_map(|id| self.get_event(id))
            .collect()
    }

    /// Remove an event from the index
    pub fn remove_event(&mut self, event_id: &EventId) -> Option<Event> {
        if let Some(event) = self.events.remove(event_id) {
            // Remove from author index
            if let Some(author_list) = self.author_index.get_mut(&event.pubkey) {
                author_list.retain(|id| id != event_id);
                if author_list.is_empty() {
                    self.author_index.remove(&event.pubkey);
                }
            }

            // Remove from kind index
            let kind = event.kind.as_u16();
            if let Some(kind_list) = self.kind_index.get_mut(&kind) {
                kind_list.retain(|id| id != event_id);
                if kind_list.is_empty() {
                    self.kind_index.remove(&kind);
                }
            }

            // Remove from time index
            let timestamp = event.created_at.as_u64();
            if let Some(time_list) = self.time_index.get_mut(&timestamp) {
                time_list.retain(|id| id != event_id);
                if time_list.is_empty() {
                    self.time_index.remove(&timestamp);
                }
            }

            Some(event)
        } else {
            None
        }
    }

    /// Get cache hit rate
    pub fn cache_hit_rate(&self) -> f64 {
        self.cache.hit_rate()
    }

    /// Get total number of events
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Check if index is empty
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{Keys, EventBuilder, Kind};

    #[test]
    fn test_fast_lookup_table() {
        let mut table: FastLookupTable<String, i32> = FastLookupTable::new(2, Duration::from_secs(1));

        table.insert("key1".to_string(), 100);
        table.insert("key2".to_string(), 200);

        assert_eq!(table.get(&"key1".to_string()), Some(100));
        assert_eq!(table.get(&"key2".to_string()), Some(200));
        assert_eq!(table.len(), 2);

        // Insert third item, should evict LRU
        table.insert("key3".to_string(), 300);
        assert_eq!(table.len(), 2);
        assert_eq!(table.get(&"key1".to_string()), None); // Should be evicted
        assert_eq!(table.get(&"key3".to_string()), Some(300));
    }

    #[tokio::test]
    async fn test_indexed_sequence_store() {
        let mut store = IndexedSequenceStore::new(10, Duration::from_secs(60));

        let keys = Keys::generate();
        let challenger = keys.public_key();

        let challenge_event = EventBuilder::new(
            Kind::from(9259u16),
            r#"{"game_type": "test", "commitment_hashes": ["hash1"]}"#,
            []
        ).to_event(&keys).unwrap();

        let sequence = crate::game::GameSequence::new(challenge_event, challenger).unwrap();
        let sequence_id = sequence.events[0].id;

        store.insert(sequence_id, sequence).unwrap();

        assert_eq!(store.len(), 1);
        assert!(store.get(&sequence_id).is_some());

        let challenger_sequences = store.get_by_challenger(&challenger);
        assert_eq!(challenger_sequences.len(), 1);
    }

    #[test]
    fn test_event_index() {
        let mut index = EventIndex::new(10, Duration::from_secs(60));

        let keys = Keys::generate();
        let event = EventBuilder::new(
            Kind::from(1u16),
            "test content",
            []
        ).to_event(&keys).unwrap();

        let event_id = event.id;
        let author = event.pubkey;

        index.add_event(event);

        assert_eq!(index.len(), 1);
        assert!(index.get_event(&event_id).is_some());

        let author_events = index.get_events_by_author(&author);
        assert_eq!(author_events.len(), 1);

        let kind_events = index.get_events_by_kind(1);
        assert_eq!(kind_events.len(), 1);
    }
}
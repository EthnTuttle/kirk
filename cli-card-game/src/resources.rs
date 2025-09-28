use bevy::prelude::*;
use nostr_sdk::Client as NostrClient;
use std::collections::VecDeque;

// Removed unused imports
use crate::repl::ReplCommand;

// Re-export types from the main kirk library
use kirk::cashu::mint::GameMint;

/// ECS Resources for Global State

/// EmbeddedMint resource wrapping CDK mint
#[derive(Resource)]
pub struct EmbeddedMint {
    /// The underlying Kirk GameMint instance
    pub inner: GameMint,
}

/// NostrClient resource for event communication
#[derive(Resource)]
pub struct NostrClientResource {
    /// The Nostr SDK client instance
    pub client: NostrClient,
}

/// ReplCommandQueue resource for thread communication
#[derive(Resource)]
pub struct ReplCommandQueue {
    /// Queue of commands from the REPL thread
    pub commands: VecDeque<ReplCommand>,
}

/// GameEventQueue resource for Nostr event processing
#[derive(Resource)]
pub struct GameEventQueue {
    /// Queue of Nostr events to process
    pub events: VecDeque<nostr::Event>,
}

/// GameStatusDisplay resource for REPL feedback
#[derive(Resource)]
pub struct GameStatusDisplay {
    /// Current status message
    pub current_status: String,
    /// Number of active games
    pub active_games_count: usize,
    /// Number of pending challenges
    pub pending_challenges: usize,
    /// Player's public key (truncated for display)
    pub player_pubkey: String,
    /// Mint's public key (truncated for display)
    pub mint_pubkey: String,
    /// Current Game token balance
    pub game_token_balance: u64,
    /// Current Reward token balance
    pub reward_token_balance: u64,
}

impl Default for GameStatusDisplay {
    fn default() -> Self {
        Self {
            current_status: "Starting...".to_string(),
            active_games_count: 0,
            pending_challenges: 0,
            player_pubkey: "".to_string(),
            mint_pubkey: "".to_string(),
            game_token_balance: 0,
            reward_token_balance: 0,
        }
    }
}

impl GameStatusDisplay {
    /// Update the status display with current information
    pub fn update_status(&mut self, message: &str) {
        self.current_status = message.to_string();
    }
    
    /// Update public key displays (truncated to first 8 characters)
    pub fn update_keys(&mut self, player_pubkey: &str, mint_pubkey: &str) {
        self.player_pubkey = if player_pubkey.len() > 8 {
            format!("{}...", &player_pubkey[..8])
        } else {
            player_pubkey.to_string()
        };
        
        self.mint_pubkey = if mint_pubkey.len() > 8 {
            format!("{}...", &mint_pubkey[..8])
        } else {
            mint_pubkey.to_string()
        };
    }
    
    /// Update token balances
    pub fn update_balances(&mut self, game_tokens: u64, reward_tokens: u64) {
        self.game_token_balance = game_tokens;
        self.reward_token_balance = reward_tokens;
    }
    
    /// Update game counts
    pub fn update_counts(&mut self, active_games: usize, pending_challenges: usize) {
        self.active_games_count = active_games;
        self.pending_challenges = pending_challenges;
    }
    
    /// Get a formatted status string for display
    pub fn get_formatted_status(&self) -> String {
        format!(
            "{} | Games: {} | Challenges: {} | Player: {} | Mint: {} | Game: {} | Reward: {}",
            self.current_status,
            self.active_games_count,
            self.pending_challenges,
            self.player_pubkey,
            self.mint_pubkey,
            self.game_token_balance,
            self.reward_token_balance
        )
    }
}

/// MintTokenRequest component for requesting token minting
#[derive(Component, Debug, Clone)]
pub struct MintTokenRequest {
    /// Amount of tokens to mint
    pub amount: u64,
    /// Reference to the Challenge entity this is for (if applicable)
    pub for_challenge: Option<Entity>,
    /// Reference to the Player entity requesting the tokens
    pub for_player: Entity,
    /// Timestamp when the request was created
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl MintTokenRequest {
    /// Create a new mint token request
    pub fn new(amount: u64, for_player: Entity) -> Self {
        Self {
            amount,
            for_challenge: None,
            for_player,
            created_at: chrono::Utc::now(),
        }
    }
    
    /// Create a new mint token request for a specific challenge
    pub fn for_challenge(amount: u64, for_player: Entity, challenge: Entity) -> Self {
        Self {
            amount,
            for_challenge: Some(challenge),
            for_player,
            created_at: chrono::Utc::now(),
        }
    }
}

/// RelayConnection resource for embedded relay management
#[derive(Resource)]
pub struct RelayConnection {
    /// Whether the embedded relay is running
    pub is_running: bool,
    /// Port the relay is listening on
    pub port: u16,
    /// Number of connected clients
    pub connected_clients: usize,
    /// Number of stored events
    pub stored_events: usize,
}

impl Default for RelayConnection {
    fn default() -> Self {
        Self {
            is_running: false,
            port: 7000,
            connected_clients: 0,
            stored_events: 0,
        }
    }
}

impl RelayConnection {
    /// Create a new relay connection resource
    pub fn new(port: u16) -> Self {
        Self {
            is_running: false,
            port,
            connected_clients: 0,
            stored_events: 0,
        }
    }
    
    /// Mark the relay as running
    pub fn set_running(&mut self, running: bool) {
        self.is_running = running;
    }
    
    /// Update connection statistics
    pub fn update_stats(&mut self, clients: usize, events: usize) {
        self.connected_clients = clients;
        self.stored_events = events;
    }
}

/// GameMetrics resource for tracking game statistics
#[derive(Resource, Default)]
pub struct GameMetrics {
    /// Total number of games played
    pub total_games: u64,
    /// Total number of games won
    pub games_won: u64,
    /// Total number of games lost
    pub games_lost: u64,
    /// Total Game tokens earned
    pub total_game_tokens_earned: u64,
    /// Total Reward tokens earned
    pub total_reward_tokens_earned: u64,
    /// Total Game tokens spent
    pub total_game_tokens_spent: u64,
    /// Average game duration in seconds
    pub average_game_duration: f64,
}

impl GameMetrics {
    /// Record a completed game
    pub fn record_game(&mut self, won: bool, duration_seconds: u64, tokens_wagered: u64, reward_earned: u64) {
        self.total_games += 1;
        
        if won {
            self.games_won += 1;
            self.total_reward_tokens_earned += reward_earned;
        } else {
            self.games_lost += 1;
            self.total_game_tokens_spent += tokens_wagered;
        }
        
        // Update average duration
        let total_duration = self.average_game_duration * (self.total_games - 1) as f64;
        self.average_game_duration = (total_duration + duration_seconds as f64) / self.total_games as f64;
    }
    
    /// Get win rate as a percentage
    pub fn win_rate(&self) -> f64 {
        if self.total_games == 0 {
            0.0
        } else {
            (self.games_won as f64 / self.total_games as f64) * 100.0
        }
    }
    
    /// Get net token balance (earned - spent)
    pub fn net_token_balance(&self) -> i64 {
        (self.total_reward_tokens_earned as i64) - (self.total_game_tokens_spent as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_game_status_display_default() {
        let status = GameStatusDisplay::default();
        assert_eq!(status.current_status, "Starting...");
        assert_eq!(status.active_games_count, 0);
        assert_eq!(status.pending_challenges, 0);
    }

    #[test]
    fn test_game_status_display_update_keys() {
        let mut status = GameStatusDisplay::default();
        let long_key = "1234567890abcdef1234567890abcdef";
        let short_key = "1234";
        
        status.update_keys(long_key, short_key);
        assert_eq!(status.player_pubkey, "12345678...");
        assert_eq!(status.mint_pubkey, "1234");
    }

    #[test]
    fn test_game_metrics_record_game() {
        let mut metrics = GameMetrics::default();
        
        // Record a won game
        metrics.record_game(true, 120, 100, 200);
        assert_eq!(metrics.total_games, 1);
        assert_eq!(metrics.games_won, 1);
        assert_eq!(metrics.games_lost, 0);
        assert_eq!(metrics.total_reward_tokens_earned, 200);
        assert_eq!(metrics.average_game_duration, 120.0);
        
        // Record a lost game
        metrics.record_game(false, 180, 100, 0);
        assert_eq!(metrics.total_games, 2);
        assert_eq!(metrics.games_won, 1);
        assert_eq!(metrics.games_lost, 1);
        assert_eq!(metrics.total_game_tokens_spent, 100);
        assert_eq!(metrics.average_game_duration, 150.0);
    }

    #[test]
    fn test_game_metrics_win_rate() {
        let mut metrics = GameMetrics::default();
        assert_eq!(metrics.win_rate(), 0.0);
        
        metrics.record_game(true, 120, 100, 200);
        assert_eq!(metrics.win_rate(), 100.0);
        
        metrics.record_game(false, 180, 100, 0);
        assert_eq!(metrics.win_rate(), 50.0);
    }

    #[test]
    fn test_mint_token_request_creation() {
        let player_entity = Entity::from_raw(1);
        let challenge_entity = Entity::from_raw(2);
        
        let request = MintTokenRequest::new(100, player_entity);
        assert_eq!(request.amount, 100);
        assert_eq!(request.for_player, player_entity);
        assert!(request.for_challenge.is_none());
        
        let challenge_request = MintTokenRequest::for_challenge(200, player_entity, challenge_entity);
        assert_eq!(challenge_request.amount, 200);
        assert_eq!(challenge_request.for_player, player_entity);
        assert_eq!(challenge_request.for_challenge, Some(challenge_entity));
    }

    #[test]
    fn test_relay_connection_management() {
        let mut relay = RelayConnection::new(8000);
        assert_eq!(relay.port, 8000);
        assert!(!relay.is_running);
        
        relay.set_running(true);
        assert!(relay.is_running);
        
        relay.update_stats(5, 100);
        assert_eq!(relay.connected_clients, 5);
        assert_eq!(relay.stored_events, 100);
    }
}
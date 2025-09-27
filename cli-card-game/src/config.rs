use serde::{Deserialize, Serialize};
use bevy::prelude::Resource;

#[derive(Debug, Clone, Serialize, Deserialize, Resource)]
pub struct GameConfig {
    pub mint_port: u16,
    pub relay_port: u16,
    pub default_wager: u64,
    pub game_timeout_seconds: u64,
    pub master_seed_file: Option<String>,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            mint_port: 3338,
            relay_port: 7000,
            default_wager: 100,
            game_timeout_seconds: 300,
            master_seed_file: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReplConfig {
    pub prompt: String,
    pub history_file: Option<String>,
    pub max_history: usize,
}

impl Default for ReplConfig {
    fn default() -> Self {
        Self {
            prompt: "kirk-cards> ".to_string(),
            history_file: Some(".kirk_cards_history".to_string()),
            max_history: 1000,
        }
    }
}
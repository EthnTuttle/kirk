use anyhow::Result;
use bevy::prelude::*;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::config::GameConfig;
use crate::repl::{GameStatus, ReplCommand, ReplInterface};

pub struct GameApp {
    bevy_app: App,
    config: GameConfig,
    command_receiver: mpsc::UnboundedReceiver<ReplCommand>,
    status_sender: mpsc::UnboundedSender<GameStatus>,
}

impl GameApp {
    pub async fn new(config: GameConfig) -> Result<Self> {
        // Create communication channels
        let (command_sender, command_receiver) = mpsc::unbounded_channel();
        let (status_sender, status_receiver) = mpsc::unbounded_channel();
        
        // Create Bevy app with minimal plugins (no rendering/audio)
        let mut bevy_app = App::new();
        bevy_app.add_plugins(MinimalPlugins);
        
        // Insert configuration as resource
        bevy_app.insert_resource(config.clone());
        
        // Insert command queue resource
        bevy_app.insert_resource(ReplCommandQueue {
            commands: std::collections::VecDeque::new(),
        });
        
        // Insert status display resource
        bevy_app.insert_resource(GameStatusDisplay {
            current_status: "Starting...".to_string(),
            active_games_count: 0,
            pending_challenges: 0,
            player_pubkey: "".to_string(),
            mint_pubkey: "".to_string(),
            game_token_balance: 0,
            reward_token_balance: 0,
        });
        
        // Add startup systems
        bevy_app.add_systems(Startup, (
            setup_logging,
            initialize_placeholder_systems,
        ));
        
        // Add update systems (placeholder for now)
        bevy_app.add_systems(Update, (
            process_commands_placeholder,
            update_status_display_placeholder,
        ));
        
        // Start REPL in separate task
        let repl_config = crate::config::ReplConfig::default();
        let mut repl = ReplInterface::new(command_sender, status_receiver, repl_config)?;
        
        tokio::spawn(async move {
            if let Err(e) = repl.run().await {
                eprintln!("REPL error: {}", e);
            }
        });
        
        Ok(Self {
            bevy_app,
            config,
            command_receiver,
            status_sender,
        })
    }
    
    pub async fn run(&mut self) -> Result<()> {
        println!("Starting Kirk Cards CLI...");
        println!("Mint port: {}", self.config.mint_port);
        println!("Relay port: {}", self.config.relay_port);
        
        // Main game loop
        loop {
            // Process any incoming REPL commands
            while let Ok(command) = self.command_receiver.try_recv() {
                if let Some(mut queue) = self.bevy_app.world_mut().get_resource_mut::<ReplCommandQueue>() {
                    queue.commands.push_back(command);
                }
            }
            
            // Update Bevy ECS world
            self.bevy_app.update();
            
            // Send status updates to REPL
            if let Some(status_display) = self.bevy_app.world().get_resource::<GameStatusDisplay>() {
                let status = GameStatus {
                    current_status: status_display.current_status.clone(),
                    active_games_count: status_display.active_games_count,
                    pending_challenges: status_display.pending_challenges,
                    player_pubkey: status_display.player_pubkey.clone(),
                    mint_pubkey: status_display.mint_pubkey.clone(),
                    game_token_balance: status_display.game_token_balance,
                    reward_token_balance: status_display.reward_token_balance,
                };
                
                let _ = self.status_sender.send(status);
            }
            
            // Small delay to prevent busy waiting
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

// Bevy ECS Resources
#[derive(Resource)]
pub struct ReplCommandQueue {
    pub commands: std::collections::VecDeque<ReplCommand>,
}

#[derive(Resource)]
pub struct GameStatusDisplay {
    pub current_status: String,
    pub active_games_count: usize,
    pub pending_challenges: usize,
    pub player_pubkey: String,
    pub mint_pubkey: String,
    pub game_token_balance: u64,
    pub reward_token_balance: u64,
}

// Placeholder systems (will be implemented in later tasks)
fn setup_logging() {
    println!("Setting up logging...");
}

fn initialize_placeholder_systems() {
    println!("Initializing placeholder systems...");
}

fn process_commands_placeholder(
    mut queue: ResMut<ReplCommandQueue>,
    mut status: ResMut<GameStatusDisplay>,
) {
    while let Some(command) = queue.commands.pop_front() {
        match command {
            ReplCommand::Help => {
                println!("Available commands:");
                println!("  challenge [amount] - Create a new challenge");
                println!("  accept <id>        - Accept a challenge");
                println!("  list               - List available challenges");
                println!("  status             - Show current status");
                println!("  balance            - Show token balances");
                println!("  mint [amount]      - Mint new Game tokens");
                println!("  unlock <token_id>  - Unlock Reward tokens");
                println!("  help               - Show this help");
                println!("  quit               - Exit the application");
            }
            ReplCommand::Status => {
                status.current_status = "Ready for commands".to_string();
            }
            ReplCommand::Balance => {
                println!("Game tokens: {}", status.game_token_balance);
                println!("Reward tokens: {}", status.reward_token_balance);
            }
            _ => {
                println!("Command received: {:?} (not yet implemented)", command);
            }
        }
    }
}

fn update_status_display_placeholder(mut status: ResMut<GameStatusDisplay>) {
    // Placeholder - will be replaced with actual status updates
    if status.current_status == "Starting..." {
        status.current_status = "Ready".to_string();
        status.player_pubkey = "placeholder_player_key".to_string();
        status.mint_pubkey = "placeholder_mint_key".to_string();
    }
}